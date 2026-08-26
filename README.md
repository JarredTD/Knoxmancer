# Knoxmancer

[![CI](https://github.com/JarredTD/Knoxmancer/actions/workflows/ci.yml/badge.svg)](https://github.com/JarredTD/Knoxmancer/actions/workflows/ci.yml)
[![License: AGPL-3.0-only](https://img.shields.io/badge/license-AGPL--3.0--only-blue.svg)](LICENSE)

Knoxmancer is a public command-line tool for creating, validating, building, installing, and staging
Project Zomboid Build 42 mods. It is unofficial and is not affiliated with The Indie Stone.

## Features

- Creates and adopts source-oriented Build 42 mod projects.
- Validates mod metadata, source layouts, Workshop inputs, and preview images.
- Builds local development copies and complete Workshop packages from the same source.
- Installs mods atomically or synchronizes an existing copy for main-menu Lua reloading.
- Detects conflicting local, staged, and Steam-managed copies without modifying subscribed content.
- Produces human-readable output or stable JSON Lines for automation.

## Setup

Install Rust 1.97 or newer, then install Knoxmancer from its repository:

```shell
cargo install --git https://github.com/JarredTD/Knoxmancer --locked
```

This installs both `knoxmancer` and the shorter `km` command. A source checkout additionally needs
`cargo-llvm-cov` 0.9.0 and `cargo-audit` 0.22.2; `cargo xtask check` runs the complete local gate.

## Usage

| Command | Result |
| --- | --- |
| `km new <directory>` | Create a Build 42 project |
| `km init` | Adopt the current source-oriented project |
| `km check` | Validate the playable mod |
| `km check --workshop` | Validate Workshop publishing inputs |
| `km build` | Build under `dist/dev` |
| `km install` | Atomically install under the resolved local mods root |
| `km install --live` | Synchronize an existing copy for main-menu Lua reloading |
| `km package` | Build a Workshop project under `dist/workshop` |
| `km stage` | Package and copy under the resolved Workshop staging root |
| `km paths` | Show resolved artifact, installation, and staging paths |
| `km copies` | Find matching local, staged, and subscribed copies |
| `km doctor` | Run read-only project and environment checks |
| `km clean` | Remove generated project artifacts |

Use `km install` for ordinary local testing and `km stage` immediately before uploading through
Project Zomboid's **Workshop > Create and update items** flow. A normal or live install removes a
matching staged mod copy because the game loads Workshop staging before local mods. It never changes
Steam-managed subscribed files.

Live installation updates an existing matching copy in place and verifies every copied file. Use it
only from the main menu, then select **Reload Lua** before loading a world. New or removed Lua files
and non-Lua changes may still require a game restart.

## Configuration

Each project owns a `knoxmancer.toml` manifest:

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

Machine-specific author, mods-root, Workshop-root, and Steam-root defaults are stored outside the
project. Manage them with `km config show`, `km config set`, and `km config unset`; command options
take precedence. Steam libraries are discovered automatically when possible.

The conventional source layout places `mod.info`, `client`, `shared`, `server`, and `media` beneath
`src`. Workshop metadata and assets live in `public`, while generated files remain under `dist`.
`{{MOD_VERSION}}` in `public/description.md` resolves to the version in `src/mod.info` during
packaging.

## Architecture

Knoxmancer separates project validation, artifact construction, Workshop rendering, machine-specific
environment discovery, and filesystem mutation. Project manifests remain portable; user paths and
discovered installations remain outside the repository. Destructive and publishing-adjacent actions
are explicit, and Steam-managed content is treated as read-only.

Successful output is written to standard output; warnings and errors use standard error. `--quiet`
hides successful output, while `--format json` emits JSON Lines with stable event types. Exit codes
are `0` for success, `1` for operational failure, and `2` for invalid usage.

## License

Knoxmancer is licensed under the [GNU Affero General Public License v3.0 only](LICENSE).
