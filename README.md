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

Use `km install` while developing and testing in the game. After installation,
Knoxmancer reports every matching copy it finds and warns when a Steam
subscription could compete with the local mod. Use `km stage` when the mod is
ready for **Workshop > Create and update items**. Staging is replaced atomically
and verified against the current mod ID and version. `km package` only creates
the upload project; it does not copy it into Zomboid's Workshop folder.

`km copies` distinguishes the playable local and Steam locations from Workshop
uploader staging. Multiple playable copies, an outdated playable copy, or a
copy without version metadata produce a conflict warning. Outdated uploader
staging is reported separately because it can publish old files but is not a
normal gameplay source. Run `km stage` to refresh it. Knoxmancer never modifies
Steam-managed subscription files; unsubscribe through Steam while testing a
local copy.

### Live installation

Normal `km install` atomically replaces the complete local mod directory and is
the safest default. If Project Zomboid is holding that directory open, exit to
the main menu and use `km install --live` to synchronize the already-installed
copy in place, then select **Reload Lua** before loading the world. Live mode is
explicit and never used as an automatic fallback.

Live installation verifies every copied file byte-for-byte and reports each
created, updated, removed, failed, or skipped operation. It refuses to start
without an existing local copy whose mod ID matches the build. If an update
fails, stale files are retained and reported as skipped to avoid making the
installed tree less complete. A failed final verification makes the command
fail.

Do not run live installation inside a loaded world: its in-place synchronization
is intentionally non-atomic. Reload Lua is intended for changed Lua files; new
or removed Lua files may not be discovered, and non-Lua changes may require a
game restart. Knoxmancer warns when either limitation applies.

## Manifest

`knoxmancer.toml` defines the supported game builds, project directories, and
optional files included with Workshop packages:

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

All configured paths are relative to the project root. The source tree uses a
development-oriented layout:

```text
src/
|-- mod.info
|-- client/
|-- shared/
|-- server/
`-- media/
```

Knoxmancer maps `client`, `shared`, and `server` into the corresponding
`42/media/lua` directories. Files under `media` are copied into `42/media`.
`public` contains `description.md`, `preview.png`, and `workshop.txt`; `output`
receives generated artifacts.

Game-facing metadata remains in `mod.info` and `workshop.txt`. Package includes
retain their project-relative paths under `Contents/mods/<ModId>`; for example,
`LICENSE` becomes `Contents/mods/<ModId>/LICENSE`. `km package` and `km stage`
validate Workshop metadata, assets, and package includes. Local checks, builds,
and installs do not require Workshop files.

`km stage --root <path>` overrides the Workshop projects directory.
`km install --root <path>` similarly overrides the local mods directory.

## User defaults

Machine-specific defaults are stored outside project manifests. Roots must be
absolute paths; explicit command options continue to take precedence.

```sh
km config set author "Your Name"
km config set mods-root "C:\Users\you\Zomboid\mods"
km config set workshop-root "C:\Users\you\Zomboid\Workshop"
km config set steam-root "D:\Steam"
km config show
km config unset author
```

Knoxmancer discovers conventional Steam installations and the additional
libraries in `steamapps/libraryfolders.vdf`. Set `steam-root` only when automatic
discovery does not find the correct installation. `KNOXMANCER_CONFIG` may point
to an alternate absolute configuration file.

Completion scripts are written to standard output by default. Select either
executable name with `--bin`, or use `--output` for an atomic direct-to-file
write:

```sh
km completions powershell > _km.ps1
km completions bash --output km.bash
km completions zsh --bin knoxmancer --output _knoxmancer
```

`km doctor` performs full local and Workshop validation without creating or
replacing artifacts. It fails when installed playable copies are outdated or
ambiguous and warns when Workshop uploader staging needs to be refreshed.

`km open` accepts `artifact`, `mods`, `package`, or `workshop` and opens a
directory only after the corresponding workflow has created it.

## Output contract

Successful command data and status messages are written to standard output.
Warnings and errors are written to standard error. `--quiet` suppresses successful
output but continues to report warnings and errors.

Pass `--format json` to emit newline-delimited JSON objects. Every object has a
stable `type` field (`status`, `path`, `mod_copy`, `file_operation`, `warning`,
or `error`). Path objects also contain `name` and `path`. Mod-copy objects
contain `source`, `version`, `current`, and `path`. Live-install file-operation
objects contain stable `action`, `status`, and `path` fields plus `message` when
an operation fails or is skipped. Error objects contain `kind`, `message`, and
`exit_code`. Help remains human-readable. Exit code `0` means success, `1` means
a project, validation, environment, or filesystem failure, and `2` means invalid
command-line usage.

Completion scripts are shell source rather than status data, so `completions`
does not accept `--format json`.

```sh
km --format json paths
km --format json copies
km --quiet check
```

## Validation rules

Knoxmancer reads `name`, `id`, and `modversion` from `mod.info`. These identity
fields must each appear once; unrelated game metadata is left alone and may use
repeated keys. `modversion` uses `MAJOR.MINOR.PATCH`, and IDs use ASCII letters,
digits, and underscores.

Workshop titles are trimmed and limited to 128 UTF-8 bytes. Tags are
semicolon-delimited, trimmed, non-empty, unique ignoring ASCII case, limited to
255 UTF-8 bytes each, and may not contain control characters or commas. Every
configured game build requires its matching `Build N` tag.

Workshop descriptions support CommonMark paragraphs, headings one through three,
bold and italic text, inline and fenced code, non-nested unordered lists,
HTTP(S) links, horizontal rules, and line breaks. Unsupported constructs fail
validation instead of being silently rewritten. Preview images are fully decoded
as PNG data and must be 256x256 and under 1000 KB.

## License

Knoxmancer is licensed under the [GNU Affero General Public License v3](LICENSE).
