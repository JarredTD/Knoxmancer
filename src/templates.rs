//! Embedded files used when scaffolding a mod project.

pub(crate) const MOD_INFO: &str = include_str!("../templates/mod.info");
pub(crate) const CHANGELOG: &str = include_str!("../templates/CHANGELOG.md");
pub(crate) const README: &str = include_str!("../templates/README.md");
pub(crate) const DESCRIPTION: &str = include_str!("../templates/description.md");
pub(crate) const WORKSHOP: &str = include_str!("../templates/workshop.txt");
pub(crate) const TEST_RUNNER: &str = include_str!("../templates/run.lua");
pub(crate) const GITIGNORE: &str = include_str!("../templates/gitignore");
pub(crate) const CI: &str = include_str!("../templates/ci.yml");

pub(crate) fn render(template: &str, values: &[(&str, &str)]) -> String {
    values
        .iter()
        .fold(template.to_owned(), |rendered, (key, value)| {
            rendered.replace(&format!("{{{{{key}}}}}"), value)
        })
}
