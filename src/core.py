"""Cinnamon/Nemo plugin marketplace engine for SigmaOS-plugin-marketplace
(inspired by cinnamon-spices-applets/desklets/extensions, nemo-extensions).

Principles: typed catalog model with strict validation, pure search and
install planning (never downloads or writes into ~/.local), enablement is a
small auditable JSON store, fail-fast on incompatible Cinnamon versions.
"""
import argparse, json, os, re, sys

ENGINE = {
    "name": "sigma-plugin-market",
    "summary": "Search, install-plan, enable and list desktop plugins",
    "language": "python",
    "verbs": ["search", "install-plan", "enable", "list"],
    "principles": ["Abstraction", "Plan Before Act", "Fail-Fast",
                   "Defensive Programming", "KISS", "DRY",
                   "Deterministic Testing"],
}

cli_doc = """\
$ sigma-plugin-market search clock --type applet
clock@sigmaos  Sigma Clock  3.2.1
$ sigma-plugin-market install-plan clock@sigmaos --cinnamon 6.4 \\
    --installed clock@sigmaos=3.2.1
status: already-installed
$ sigma-plugin-market enable clock@sigmaos --store enabled.json --installed clock@sigmaos=3.2.1
$ sigma-plugin-market list --store enabled.json
"""

_UUID_RE = re.compile(r"^[a-z0-9][a-z0-9_.-]*@[a-z0-9][a-z0-9_.-]*$")
_VERSION_RE = re.compile(r"^\d+\.\d+\.\d+$")
_TYPES = ("applet", "desklet", "extension", "nemo-extension")


class PluginError(ValueError):
    pass


def _read(path):
    if path == "-":
        return sys.stdin.read()
    with open(path, "r", encoding="utf-8") as fh:
        return fh.read()


def _version_tuple(text):
    return tuple(int(n) for n in text.split("."))


def parse_catalog(text):
    """Parse and validate the plugin catalog JSON (fails fast)."""
    try:
        data = json.loads(text)
    except json.JSONDecodeError as e:
        raise PluginError(f"catalog is not valid JSON: {e}") from e
    if not isinstance(data, list):
        raise PluginError("catalog must be a JSON list")
    seen = set()
    for entry in data:
        uuid = entry.get("uuid", "")
        if not _UUID_RE.match(str(uuid)):
            raise PluginError(f"bad or missing uuid: {uuid!r}")
        if uuid in seen:
            raise PluginError(f"duplicate uuid: {uuid}")
        seen.add(uuid)
        if entry.get("type") not in _TYPES:
            raise PluginError(
                f"{uuid}: type must be one of {_TYPES}, "
                f"got {entry.get('type')!r}")
        if not entry.get("name"):
            raise PluginError(f"{uuid}: missing name")
        if not _VERSION_RE.match(str(entry.get("version", ""))):
            raise PluginError(f"{uuid}: bad version "
                              f"{entry.get('version')!r}")
        try:
            float(entry.get("cinnamon_min", 0))
        except (TypeError, ValueError):
            raise PluginError(f"{uuid}: bad cinnamon_min "
                              f"{entry.get('cinnamon_min')!r}")
        if not isinstance(entry.get("tags", []), list):
            raise PluginError(f"{uuid}: tags must be a list")
    return sorted(data, key=lambda e: e["uuid"])


def search(catalog, query, ptype=None):
    """AND search over uuid/name/tags; optional type filter."""
    if ptype is not None and ptype not in _TYPES:
        raise PluginError(f"bad plugin type: {ptype!r}")
    tokens = [t for t in query.lower().split() if t]
    hits = []
    for entry in catalog:
        if ptype is not None and entry["type"] != ptype:
            continue
        hay = " ".join([entry["uuid"], entry["name"],
                        " ".join(entry.get("tags", []))]).lower()
        if all(t in hay for t in tokens):
            hits.append(entry)
    return hits


def _install_dir(ptype, uuid):
    base = {"applet": "applets", "desklet": "desklets",
            "extension": "extensions",
            "nemo-extension": "nemo-extension"}[ptype]
    return os.path.join("~", ".local", "share", "cinnamon", base, uuid)


def install_plan(catalog, uuid, installed, cinnamon_version):
    """Pure install plan; refuses incompatible or duplicate installs."""
    by_uuid = {e["uuid"]: e for e in catalog}
    if uuid not in by_uuid:
        raise PluginError(f"unknown plugin {uuid!r} "
                          f"(known: {', '.join(sorted(by_uuid)) or 'none'})")
    entry = by_uuid[uuid]
    minimum = float(entry.get("cinnamon_min", 0))
    if cinnamon_version < minimum:
        raise PluginError(
            f"{uuid} requires Cinnamon >= {minimum}, "
            f"you have {cinnamon_version}")
    dest = _install_dir(entry["type"], uuid)
    if installed.get(uuid) == entry["version"]:
        return {"status": "already-installed", "entry": entry,
                "dest": dest, "steps": []}
    status = "upgrade" if uuid in installed else "install"
    return {"status": status, "entry": entry, "dest": dest, "steps": [
        f"fetch  https://spices.sigmaos.dev/{entry['type']}/{uuid}-"
        f"{entry['version']}.tar.gz",
        f"extract -> {dest}",
        f"enable via: sigma-plugin-market enable {uuid}",
    ]}


def _load_store(path):
    if not os.path.exists(path):
        return []
    try:
        data = json.loads(_read(path))
    except json.JSONDecodeError as e:
        raise PluginError(f"corrupt enablement store: {e}") from e
    if not isinstance(data, list):
        raise PluginError("corrupt enablement store: expected a list")
    return data


def enable(store_path, uuid, installed, disable=False):
    """Enable/disable a plugin in the store (must be installed first)."""
    if not _UUID_RE.match(uuid):
        raise PluginError(f"bad uuid: {uuid!r}")
    if uuid not in installed and not disable:
        raise PluginError(f"{uuid} is not installed; install-plan first")
    store = _load_store(store_path)
    if disable:
        if uuid not in store:
            raise PluginError(f"{uuid} is not enabled")
        store.remove(uuid)
        action = "disabled"
    else:
        if uuid in store:
            raise PluginError(f"{uuid} is already enabled")
        store.append(uuid)
        action = "enabled"
    with open(store_path, "w", encoding="utf-8") as fh:
        json.dump(sorted(store), fh, indent=2)
    return action


def list_installed(installed, store_path):
    """Merge the installed map with the enablement store for display."""
    enabled = set(_load_store(store_path))
    return [{"uuid": uuid, "version": installed[uuid],
             "enabled": uuid in enabled}
            for uuid in sorted(installed)]


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog=ENGINE["name"],
                                description=ENGINE["summary"])
    p.add_argument("--catalog", default=os.path.expanduser(
        "~/.config/sigma-plugin-market/catalog.json"))
    p.add_argument("--store", default=os.path.expanduser(
        "~/.config/sigma-plugin-market/enabled.json"))
    sub = p.add_subparsers(dest="cmd", required=True)
    se = sub.add_parser("search", help="search the catalog")
    se.add_argument("query", nargs="+")
    se.add_argument("--type", choices=list(_TYPES), default=None,
                    dest="ptype")
    ip = sub.add_parser("install-plan", help="plan an install (dry run)")
    ip.add_argument("uuid")
    ip.add_argument("--cinnamon", type=float, required=True)
    ip.add_argument("--installed", action="append", default=[],
                    metavar="UUID=VERSION")
    en = sub.add_parser("enable", help="enable an installed plugin")
    en.add_argument("uuid")
    en.add_argument("--installed", action="append", default=[],
                    metavar="UUID=VERSION")
    en.add_argument("--disable", action="store_true")
    ls = sub.add_parser("list", help="list installed plugins")
    ls.add_argument("--installed", action="append", default=[],
                    metavar="UUID=VERSION")
    return p


def _parse_installed(specs):
    installed = {}
    for spec in specs:
        if "=" not in spec:
            raise PluginError(f"--installed needs UUID=VERSION, got {spec!r}")
        uuid, _, version = spec.partition("=")
        if not _UUID_RE.match(uuid) or not _VERSION_RE.match(version):
            raise PluginError(f"bad --installed spec: {spec!r}")
        installed[uuid] = version
    return installed


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.cmd == "search":
            catalog = parse_catalog(_read(args.catalog))
            hits = search(catalog, " ".join(args.query), ptype=args.ptype)
            if not hits:
                print("no plugins matched")
            for e in hits:
                print(f"{e['uuid']:<28} {e['name']:<24} {e['version']}")
            return 0
        if args.cmd == "list":
            installed = _parse_installed(args.installed)
            rows = list_installed(installed, args.store)
            if not rows:
                print("no plugins installed")
            for row in rows:
                state = "enabled" if row["enabled"] else "disabled"
                print(f"{row['uuid']:<28} {row['version']:<10} {state}")
            return 0
        installed = _parse_installed(args.installed)
        if args.cmd == "install-plan":
            catalog = parse_catalog(_read(args.catalog))
            plan = install_plan(catalog, args.uuid, installed,
                                args.cinnamon)
            print(f"status: {plan['status']}")
            for step in plan["steps"]:
                print(f"  {step}")
        elif args.cmd == "enable":
            action = enable(args.store, args.uuid, installed,
                            disable=args.disable)
            print(f"{args.uuid}: {action}")
        return 0
    except PluginError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1


def self_test() -> None:
    import tempfile
    catalog_text = json.dumps([
        {"uuid": "clock@sigmaos", "name": "Sigma Clock", "type": "applet",
         "version": "3.2.1", "cinnamon_min": 5.8, "tags": ["clock", "time"]},
        {"uuid": "weather@sigmaos", "name": "Sigma Weather", "type": "applet",
         "version": "1.4.0", "cinnamon_min": 6.0, "tags": ["weather"]},
        {"uuid": "sysmonitor@sigmaos", "name": "Sys Monitor",
         "type": "desklet", "version": "2.0.0", "cinnamon_min": 6.4,
         "tags": ["system", "monitor"]},
        {"uuid": "gtk3-translator@sigmaos", "name": "GTK3 Translator",
         "type": "nemo-extension", "version": "0.9.2", "cinnamon_min": 4.0,
         "tags": ["nemo", "translate"]},
    ])
    catalog = parse_catalog(catalog_text)
    assert [e["uuid"] for e in catalog] == \
        ["clock@sigmaos", "gtk3-translator@sigmaos", "sysmonitor@sigmaos",
         "weather@sigmaos"]

    hits = search(catalog, "sigma")
    assert {e["uuid"] for e in hits} == \
        {"clock@sigmaos", "weather@sigmaos", "sysmonitor@sigmaos",
         "gtk3-translator@sigmaos"}          # all uuids carry the scope
    assert [e["uuid"] for e in search(catalog, "sigma clock")] == \
        ["clock@sigmaos"]                    # AND semantics narrow it down
    assert [e["uuid"] for e in search(catalog, "clock", ptype="applet")] == \
        ["clock@sigmaos"]
    assert search(catalog, "clock", ptype="desklet") == []
    assert search(catalog, "nope nothing here") == []
    try:
        search(catalog, "x", ptype="gadget")
    except PluginError:
        pass
    else:
        raise AssertionError("bad type accepted")

    installed = {"clock@sigmaos": "3.2.1"}
    plan = install_plan(catalog, "clock@sigmaos", installed, 6.4)
    assert plan["status"] == "already-installed" and plan["steps"] == []
    plan = install_plan(catalog, "weather@sigmaos", installed, 6.4)
    assert plan["status"] == "install"
    assert plan["dest"].endswith("applets/weather@sigmaos")
    assert any("spices.sigmaos.dev" in s for s in plan["steps"])
    plan = install_plan(catalog, "clock@sigmaos",
                        {"clock@sigmaos": "3.0.0"}, 6.4)
    assert plan["status"] == "upgrade"
    plan = install_plan(catalog, "gtk3-translator@sigmaos", {}, 6.4)
    assert "nemo-extension" in plan["dest"]
    # incompatible Cinnamon version is refused
    try:
        install_plan(catalog, "sysmonitor@sigmaos", {}, 6.0)
    except PluginError as e:
        assert "requires Cinnamon >= 6.4" in str(e)
    else:
        raise AssertionError("incompatible plugin accepted")
    try:
        install_plan(catalog, "ghost@sigmaos", {}, 6.4)
    except PluginError:
        pass
    else:
        raise AssertionError("unknown uuid accepted")

    # catalog validation
    broken = json.loads(catalog_text)
    for mutate, fragment in (
        (lambda r: r[0].pop("uuid"), "bad or missing uuid"),
        (lambda r: r[0].update(type="gadget"), "type must be"),
        (lambda r: r[0].update(version="3.2"), "bad version"),
        (lambda r: r[0].update(cinnamon_min="newest"), "bad cinnamon_min"),
        (lambda r: r.append(dict(r[0])), "duplicate uuid"),
    ):
        mutate(broken)
        try:
            parse_catalog(json.dumps(broken))
        except PluginError as e:
            assert fragment in str(e), (fragment, str(e))
        else:
            raise AssertionError(f"broken catalog accepted: {fragment}")
        broken = json.loads(catalog_text)
    try:
        parse_catalog("{\"not\": \"list\"}")
    except PluginError:
        pass
    else:
        raise AssertionError("non-list catalog accepted")

    # enable / list roundtrip
    with tempfile.TemporaryDirectory() as d:
        store = os.path.join(d, "enabled.json")
        for bad in (lambda: enable(store, "ghost@x", installed),
                    lambda: enable(store, "Not A Uuid", installed)):
            try:
                bad()
            except PluginError:
                pass
            else:
                raise AssertionError("bad enable accepted")
        assert enable(store, "clock@sigmaos", installed) == "enabled"
        try:
            enable(store, "clock@sigmaos", installed)
        except PluginError as e:
            assert "already enabled" in str(e)
        else:
            raise AssertionError("double enable accepted")
        rows = list_installed(
            {"clock@sigmaos": "3.2.1", "weather@sigmaos": "1.4.0"}, store)
        assert rows == [{"uuid": "clock@sigmaos", "version": "3.2.1",
                         "enabled": True},
                        {"uuid": "weather@sigmaos", "version": "1.4.0",
                         "enabled": False}]
        assert enable(store, "clock@sigmaos", installed,
                      disable=True) == "disabled"
        try:
            enable(store, "clock@sigmaos", installed, disable=True)
        except PluginError:
            pass
        else:
            raise AssertionError("double disable accepted")


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
