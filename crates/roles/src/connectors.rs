//! The connectors Farik runs itself (`docs/SPEC.md` 5.6): each one's pinned image, its arguments, and
//! every tool's tag, read from the kit that ships it.

use std::collections::BTreeMap;

use farik_core::governor::permissions::ConnectorTag;
use serde::Deserialize;

/// A built-in connector, as the UI/UX Designer's kit defines it.
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

/// The connector Farik ships under `name`, or `None` for a name Farik does not ship: the
/// `container` connector of the UI/UX Designer's kit (`roles/ui_ux_designer/kit.yaml`).
#[must_use]
pub fn builtin_connector(name: &str) -> Option<ConnectorDefinition> {
    // The Designer's kit is the only one that ships a container, and the tests over every shipped
    // kit rule out its failing to load.
    let kit = crate::load_kit(farik_core::contract::Role::UiUxDesigner).ok()?;
    kit.connectors
        .into_iter()
        .find_map(|connector| match connector {
            crate::KitConnector::Container(definition) if definition.name == name => {
                Some(definition)
            }
            _ => None,
        })
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
