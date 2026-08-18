//! Mod project configuration, paths, metadata, and validation.

pub(crate) mod config;
mod diagnostic;
mod layout;
mod metadata;
pub(crate) mod preview;
pub(crate) mod validation;
mod workshop;

pub(crate) use config::Project;
pub(crate) use diagnostic::Diagnostic;
pub(crate) use layout::ProjectLayout;
pub(crate) use validation::ValidatedProject;
pub(crate) use workshop::WorkshopMetadata;
#[cfg(test)]
pub(crate) use workshop::WorkshopVisibility;
