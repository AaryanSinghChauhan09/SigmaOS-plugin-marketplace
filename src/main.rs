//! SigmaOS-plugin-marketplace — plugin store CLI.
//!
//! Usage:
//!   sigmaos-market status | search <q> | browse <kind> | info <id>
//!   sigmaos-market install <id> [version] | remove <id>
//!   sigmaos-market installed | updates | update-all | pin <id> | unpin <id>
//!
//! Install delegates to `sigmaos-plugins` on PATH (plugin-registry).
//! State: ~/.local/share/sigmaos/marketplace.json

use sigmaos_plugin_marketplace::{HostBackend, SigmaOSPluginMarketplace};
use std::process::exit;

fn usage() -> ! {
    eprintln!("usage: sigmaos-market <status|search|browse|info|install|remove|installed|updates|update-all|pin|unpin> [args]");
    exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() { usage(); }
    let db = SigmaOSPluginMarketplace::default_path();
    let mut m = SigmaOSPluginMarketplace::load(&db).unwrap_or_else(|_| SigmaOSPluginMarketplace::new());
    let be = HostBackend;
    let cmd = args[0].as_str();
    let rest = &args[1..];
    let arg = |i: usize| -> &str { rest.get(i).map(|s| s.as_str()).unwrap_or_else(|| usage()) };

    let result: Result<(), String> = (|| -> Result<(), String> { match cmd {
        "status" => { println!("{}", m.status_line()); Ok(()) }
        "search" => {
            for e in m.search(arg(0)) {
                println!("{:<16} {:<10} {:<5}★ {:<8} {}", e.id, e.version, e.kind,
                    format!("{:.1}", e.rating), e.name);
            }
            Ok(())
        }
        "browse" => {
            for e in m.browse(arg(0)) {
                println!("{:<16} {:<10} {:<5}★ {:<8} {}", e.id, e.version, e.kind,
                    format!("{:.1}", e.rating), e.name);
            }
            Ok(())
        }
        "info" => {
            let e = m.entry(arg(0)).ok_or_else(|| format!("not in catalog: {}", arg(0)))?;
            println!("id: {}", e.id);
            println!("name: {}", e.name);
            println!("version: {} (all: {})", e.version, e.versions.join(", "));
            println!("kind: {}", e.kind);
            println!("author: {}", e.author);
            println!("license: {}", e.license);
            println!("rating: {:.1} ({} downloads)", e.rating, e.downloads);
            println!("size: {} KB", e.size_bytes/1024);
            println!("source: {}", e.source);
            println!("tags: {}", e.tags.join(", "));
            println!("description: {}", e.description);
            Ok(())
        }
        "install" => {
            let ver = rest.get(1).map(|s| s.as_str());
            m.install(&be, arg(0), ver).map_err(|e| e.to_string())?;
            println!("installed {}", arg(0)); Ok(())
        }
        "remove" => { m.uninstall(&be, arg(0)).map_err(|e| e.to_string())?; println!("removed"); Ok(()) }
        "installed" => {
            for i in m.installed() {
                println!("{:<16} {:<10} {}", i.id, i.version, if i.pinned {"(pinned)"} else {""});
            }
            Ok(())
        }
        "updates" => {
            for (id, old, new) in m.updates_available() {
                println!("{}: {} → {}", id, old, new);
            }
            Ok(())
        }
        "update-all" => {
            match m.update_all(&be) {
                Ok(u) => { for x in u { println!("updated {}", x); } Ok(()) }
                Err(e) => Err(e.to_string()),
            }
        }
        "pin" => {
            if let Some(i) = m.installed.get_mut(arg(0)) { i.pinned=true; println!("pinned"); Ok(()) }
            else { Err(format!("not installed: {}", arg(0))) }
        }
        "unpin" => {
            if let Some(i) = m.installed.get_mut(arg(0)) { i.pinned=false; println!("unpinned"); Ok(()) }
            else { Err(format!("not installed: {}", arg(0))) }
        }
        _ => usage(),
    } })();

    match result {
        Ok(()) => { if let Err(e)=m.save(&db) { eprintln!("sigmaos-market: warn: save: {e}"); } }
        Err(e) => { eprintln!("sigmaos-market: {e}"); exit(1); }
    }
}
