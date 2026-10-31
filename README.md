# SigmaOS Plugin Marketplace

Python CLI for validating and searching a local plugin catalog, planning
installation steps, and maintaining a JSON enablement list for Cinnamon and
Nemo plugin types.

## Current integration limits

`install-plan` prints fetch and extraction steps but does not download or
install plugins. `enable` writes an ID to the local enablement store; it does
not load or execute the plugin. The caller supplies the installed-version map,
so the command does not independently inspect the host. Catalog validation
checks structure and compatibility fields, not publisher identity, artifact
signatures, or code safety.

Before connecting a real installer, verify the artifact's origin and digest,
stage extraction safely, enforce permissions, and preserve a rollback path.

## Development

`make test` exercises catalog validation, search, planning, and enablement-store
updates using local fixtures. References include
[Cinnamon Spices](https://github.com/linuxmint/cinnamon-spices-applets) and
[Omarchy's marketplace](https://github.com/omacom/omarchy-plugin-marketplace);
this project does not claim compatibility with either.
