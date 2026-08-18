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
| `km check` | Validate the playable mod |
| `km build` | Build under `dist/dev` |
| `km install` | Build and install under `~/Zomboid/mods` for local play |
| `km package` | Build a Workshop project under `dist/workshop` |
| `km stage` | Package and copy under `~/Zomboid/Workshop` for uploading |
| `km clean` | Remove generated artifacts |

Use `km install` while developing and testing in the game. Use `km stage` when
the mod is ready for **Workshop > Create and update items**. `km package` only
creates the upload project; it does not copy it into Zomboid's Workshop folder.

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

## Output contract

Successful command data and status messages are written to standard output.
Warnings and errors are written to standard error. `--quiet` suppresses successful
output but continues to report warnings and errors.

Pass `--format json` to emit newline-delimited JSON objects. Every object has a
stable `type` field (`status`, `path`, `warning`, or `error`). Path objects also
contain `name` and `path`; error objects contain `kind`, `message`, and
`exit_code`. Help remains human-readable. Exit code `0` means success, `1` means
a project, validation, environment, or filesystem failure, and `2` means invalid
command-line usage.

```sh
km --format json paths
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
