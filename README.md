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

```text
km new <directory>       Create a Build 42 project
km init                  Adopt the current project
km doctor                Inspect the local environment
km check                 Validate a project
km build                 Build a readable artifact
km build --release       Build a release artifact
km install               Install a local development build
km package               Create a Workshop-ready tree
km package --stage       Stage it for the Zomboid Workshop uploader
km clean                 Remove generated artifacts
```

## Manifest

`knoxmancer.toml` defines the supported game builds, project directories, and
files included with release artifacts:

```toml
[project]
builds = ["42"]

[paths]
source = "src"
public = "public"
output = "dist"

[release]
include = ["CHANGELOG.md", "LICENSE"]
```

All configured paths are relative to the project root. The source tree uses a
development-oriented layout:

```text
src/
├── mod.info
├── client/
├── shared/
├── server/
└── media/
```

Knoxmancer maps `client`, `shared`, and `server` into the corresponding
`42/media/lua` directories. Files under `media` are copied into `42/media`.
`public` contains `description.md`, `preview.png`, and `workshop.txt`; `output`
receives generated artifacts. Release includes must be files inside the project.

Game-facing metadata remains in `mod.info` and `workshop.txt`. Artifacts are
written under `dist/dev`, `dist/release`, and `dist/workshop`.

`km package --stage` detects the Steam library containing Project Zomboid and
copies the Workshop artifact into the game's `mods` directory. Override the
detected directory with `--root <path>` when needed.

Use `--format json` for versioned newline-delimited JSON output.

## License

Knoxmancer is licensed under the [GNU Affero General Public License v3](LICENSE).
