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
builds = ["42"]

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

## License

Knoxmancer is licensed under the [GNU Affero General Public License v3](LICENSE).
