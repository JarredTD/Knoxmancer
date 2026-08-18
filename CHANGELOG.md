# Changelog

## Unreleased

- Stage upload projects in the user Zomboid Workshop directory used by the game.
- Clarify the local-play install and Workshop-project workflows.

## 0.3.0

- Detect Steam and stage Workshop packages for the Project Zomboid uploader.
- Build Workshop packages directly without a redundant release artifact.

## 0.2.0

- Author mods from flat `src/client`, `src/shared`, `src/server`, and `src/media` directories.
- Map the source tree into the Project Zomboid Build 42 artifact layout.

## 0.1.0

- Scaffold and adopt Project Zomboid Build 42 mod projects.
- Validate mod metadata, translations, publishing assets, and release inputs.
- Reject unknown configuration, unsafe paths, and conflicting package files.
- Build, install, package, and clean isolated artifacts with atomic replacement.
- Render Workshop descriptions from Markdown.
- Emit human-readable or versioned newline-delimited JSON diagnostics.
