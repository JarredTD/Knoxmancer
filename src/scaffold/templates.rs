//! Embedded files used when scaffolding a mod project.

pub(crate) const MOD_INFO: &str = include_str!("../../templates/mod.info");
pub(crate) const CHANGELOG: &str = include_str!("../../templates/CHANGELOG.md");
pub(crate) const README: &str = include_str!("../../templates/README.md");
pub(crate) const DESCRIPTION: &str = include_str!("../../templates/description.md");
pub(crate) const WORKSHOP: &str = include_str!("../../templates/workshop.txt");
pub(crate) const GITIGNORE: &str = include_str!("../../templates/gitignore");

pub(crate) fn render(template: &str, values: &[(&str, &str)]) -> String {
    let mut rendered = String::with_capacity(template.len());
    let mut remaining = template;
    while let Some(start) = remaining.find("{{") {
        rendered.push_str(&remaining[..start]);
        let marker = &remaining[start..];
        let Some(end) = marker.find("}}") else {
            rendered.push_str(marker);
            return rendered;
        };
        let key = &marker[2..end];
        if let Some((_, value)) = values.iter().find(|(candidate, _)| *candidate == key) {
            rendered.push_str(value);
        } else {
            rendered.push_str(&marker[..end + 2]);
        }
        remaining = &marker[end + 2..];
    }
    rendered.push_str(remaining);
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_each_template_marker_once() {
        let rendered = render(
            "name={{name}} id={{id}} keep={{UNKNOWN}}",
            &[("name", "{{id}}"), ("id", "Example")],
        );
        assert_eq!(rendered, "name={{id}} id=Example keep={{UNKNOWN}}");
    }
}
