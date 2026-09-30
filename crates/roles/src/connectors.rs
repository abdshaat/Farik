//! The connectors Farik ships (`docs/SPEC.md` 5.6): each one's pinned image, its arguments, and
//! every tool's tag, embedded in the binary.

use std::collections::BTreeMap;

use farik_core::governor::permissions::ConnectorTag;
use serde::Deserialize;

/// A built-in connector, as its file under `connectors/` defines it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorDefinition {
    /// The name an agent's `mcp_servers` gives it, and its tools' `mcp__<name>__` prefix.
    pub name: String,
    /// The image the server runs in, pinned by digest.
    pub image: String,
    /// The arguments after the image's entrypoint.
    pub args: Vec<String>,
    /// Where the image keeps its Node modules, Playwright among them.
    pub module_root: String,
    /// Every tool the pinned image lists, and its tag.
    pub tools: BTreeMap<String, ConnectorTag>,
}

/// The connector Farik ships under `name`, or `None` for a name it does not ship.
///
/// # Panics
///
/// When a shipped file is not a connector's, which the tests over every shipped file rule out.
#[must_use]
pub fn builtin_connector(name: &str) -> Option<ConnectorDefinition> {
    let text = match name {
        "playwright" => include_str!("../connectors/playwright.yaml"),
        _ => return None,
    };
    Some(
        serde_saphyr::from_str_with_options(text, crate::yaml_options())
            .expect("a shipped connector file is a connector's"),
    )
}

#[cfg(test)]
mod tests {
    use farik_core::team::BUILTIN_CONNECTORS;

    use super::builtin_connector;

    #[test]
    fn ships_every_connector_the_team_file_may_name() {
        for name in BUILTIN_CONNECTORS {
            let definition = builtin_connector(name).expect("a shipped connector");
            assert_eq!(definition.name, name);
            assert!(
                definition.image.contains("@sha256:"),
                "{}",
                definition.image
            );
        }
        assert_eq!(builtin_connector("selenium"), None);
    }
}
