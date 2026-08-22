# Knoxmancer

[![CI](https://github.com/JarredTD/Knoxmancer/actions/workflows/ci.yml/badge.svg)](https://github.com/JarredTD/Knoxmancer/actions/workflows/ci.yml)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)

Project Zomboid mod development CLI. Knoxmancer is an unofficial project and
is not affiliated with The Indie Stone.

## Install

Requires Rust 1.97 or newer.

```sh
cargo install --git https://github.com/JarredTD/Knoxmancer --locked
```

This installs `knoxmancer` and its `km` alias.

## Commands

| Command | Result |
| --- | --- |
| `km new <directory>` | Create a Build 42 project |
| `km init` | Adopt the current source-oriented project |
| `km paths` | Show resolved artifact, installation, and staging paths |
| `km copies` | Find matching local, staged, and Steam Workshop copies |
| `km check` | Validate the playable mod |
| `km check --workshop` | Validate all Workshop publishing inputs without building |
| `km build` | Build under `dist/dev` |
| `km install` | Build and install under `~/Zomboid/mods` for local play |
| `km install --live` | Synchronize an existing local copy for main-menu Lua reloading |
| `km package` | Build a Workshop project under `dist/workshop` |
| `km stage` | Package and copy under `~/Zomboid/Workshop` for uploading |
| `km clean` | Remove generated artifacts |
| `km config show/set/unset` | Manage machine-specific defaults |
| `km completions <shell>` | Generate a shell completion script |
| `km doctor` | Run read-only project and environment readiness checks |
| `km open <target>` | Open an existing artifact or game-facing directory |

Use `km install` for local testing and `km stage` before uploading through
**Workshop > Create and update items**. Knoxmancer reports conflicting local,
staged, and subscribed copies, but never modifies Steam-managed files.

### Live installation

`km install` atomically replaces the local copy and is the default. If the game
locks that directory, exit to the main menu, run `km install --live`, then use
**Reload Lua** before loading the world.

Live mode updates an existing matching install in place, verifies copied files,
and reports partial failures. Never run it inside a loaded world. New or removed
Lua files and non-Lua changes may still require a game restart. Knoxmancer never
falls back to live mode automatically.

## Manifest

`knoxmancer.toml` defines the game build, project directories, and optional
Workshop package files:

```toml
manifest_version = 1

[project]
build = "42"

[paths]
source = "src"
public = "public"
output = "dist"

[package]
include = []
```

Paths are relative to the project root. The conventional source layout is:

```text
src/
|-- mod.info
|-- client/
|-- shared/
|-- server/
`-- media/
```

Lua folders map into `42/media/lua`; `media` maps into `42/media`. Workshop
metadata and assets live in `public`. Generated files go to `dist`.
Use `{{MOD_VERSION}}` in `public/description.md` to insert the current
`modversion` from `src/mod.info` whenever a Workshop package is created.

## User defaults

Machine-specific defaults stay outside the project manifest. Command options
such as `install --root` and `stage --root` take precedence.

```sh
km config set author "Your Name"
km config set mods-root "C:\Users\you\Zomboid\mods"
km config set workshop-root "C:\Users\you\Zomboid\Workshop"
km config set steam-root "D:\Steam"
km config show
km config unset author
```

Steam roots and additional libraries are discovered automatically. Configure a
root only when discovery is insufficient. Completion scripts can be printed or
written directly:

```sh
km completions powershell > _km.ps1
km completions bash --output km.bash
km completions zsh --bin knoxmancer --output _knoxmancer
```

## Output contract

Success goes to standard output; warnings and errors go to standard error.
`--quiet` hides successful output. `--format json` emits JSON Lines with stable
event types, including per-file live-install operations. Exit codes are `0` for
success, `1` for operational failure, and `2` for invalid usage.

```sh
km --format json paths
km --format json copies
km --quiet check
```

## License

Knoxmancer is licensed under the [GNU Affero General Public License v3](LICENSE).
