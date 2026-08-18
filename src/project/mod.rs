//! Mod project configuration, paths, metadata, validation, and tests.

pub(crate) mod config;
mod diagnostic;
mod layout;
mod metadata;
pub(crate) mod preview;
pub(crate) mod test_runner;
pub(crate) mod validation;

pub(crate) use config::Project;
pub(crate) use diagnostic::Diagnostic;
pub(crate) use layout::ProjectLayout;
pub(crate) use metadata::ModMetadata;
pub(crate) use validation::ValidatedProject;
