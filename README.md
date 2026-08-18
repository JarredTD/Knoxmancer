# Knoxmancer

Project Zomboid mod development CLI. Knoxmancer is an unofficial project and
is not affiliated with The Indie Stone.

## Install

```sh
cargo install --git https://github.com/JarredTD/Knoxmancer --tag v0.1.0 --locked
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
km clean                 Remove generated artifacts
```

Game-facing metadata remains in `mod.info` and `workshop.txt`. Build policy is
stored in `knoxmancer.toml`. Artifacts are written under `dist/dev`,
`dist/release`, and `dist/workshop`.

Use `--format json` for versioned newline-delimited JSON output.

The project is licensed under the GNU Affero General Public License v3; see
`LICENSE`.
