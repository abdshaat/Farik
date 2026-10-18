//! `catervas_request_connection` (ADR 0063): an agent asks the owner to connect a service its
//! kit offers and its work needs. Today lists the pair first, with the agent's reason.

use catervas_protocol::event::{ConnectionRequestedBody, EventBody};
use catervas_roles::KitConnector;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Call, ToolError, failed};

/// The fewest and the most characters of a reason.
const WHY_LENGTH: std::ops::RangeInclusive<usize> = 20..=300;

/// `catervas_request_connection`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequestConnectionInput {
    /// The service, by the name your kit gives it.
    server: String,
    /// Why your work needs it: 20 to 300 characters.
    why: String,
}

fn refused(code: &str, detail: impl std::fmt::Display) -> ToolError {
    ToolError::Refused {
        reason: format!("{code}: {detail}"),
    }
}

/// Records the ask; nothing is recorded on a refusal.
pub(super) fn request_connection(
    call: &Call<'_>,
    input: &RequestConnectionInput,
) -> Result<Value, ToolError> {
    let why = input.why.trim();
    let length = why.chars().count();
    if !WHY_LENGTH.contains(&length) {
        return Err(refused(
            "connection_why",
            format!("why is {length} characters; it must be 20 to 300"),
        ));
    }
    let server = input.server.trim();
    let kit = (call.deps().kits)(call.role()).map_err(failed)?;
    let offered = kit
        .connectors
        .iter()
        .any(|connector| matches!(connector, KitConnector::Server { entry, .. } if entry.name.as_str() == server));
    if !offered {
        return Err(refused(
            "connection_not_in_kit",
            format!("{server} is not a service your kit offers"),
        ));
    }
    if call
        .agent
        .mcp_servers
        .iter()
        .flatten()
        .any(|held| held.name.as_str() == server)
    {
        return Err(refused(
            "connection_held",
            format!("you already have {server}"),
        ));
    }
    call.append(
        call.context.task_id.as_ref(),
        EventBody::ConnectionRequested(ConnectionRequestedBody {
            server: server.parse().map_err(failed)?,
            why: why.parse().map_err(failed)?,
        }),
    )?;
    Ok(json!({
        "requested": server,
        "next": "The owner sees this on Today with Connect. Carry on without it; do not wait for it.",
    }))
}

#[cfg(test)]
mod tests {
    use catervas_protocol::event::{EventBody, EventKind};
    use serde_json::json;

    use crate::tools::ToolError;
    use crate::tools::fixtures::{TestProject, a_developer_kit, a_kit_server, a_team_of_three};

    const WHY: &str = "The plan needs the issues to triage bugs.";

    fn reason(refused: &ToolError) -> &str {
        let ToolError::Refused { reason } = refused else {
            panic!("a refusal, not {refused:?}");
        };
        reason
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn requests_a_connection_the_kit_offers() {
        let project = TestProject::new(
            "tools-request-connection",
            &a_team_of_three(|wire| wire["agents"][2]["mcp_servers"] = json!([a_kit_server()])),
        );
        project.set_kit(a_developer_kit(&[], Some("network")));
        let ask = |agent: &str, server: &str, why: &str| {
            project.call(
                agent,
                None,
                "catervas_request_connection",
                json!({ "server": server, "why": why }),
            )
        };

        let answered = ask("dev-a", "github", WHY).expect("the kit offers github");
        assert_eq!(answered["requested"], "github");
        let events = project.events(&[EventKind::ConnectionRequested]);
        assert_eq!(events.len(), 1);
        let EventBody::ConnectionRequested(body) = &events[0].body else {
            panic!("a request");
        };
        assert_eq!((body.server.as_str(), body.why.as_str()), ("github", WHY));
        assert_eq!(events[0].envelope.ids.agent_id.as_deref(), Some("dev-a"));

        let missing = ask("dev-a", "nothing", WHY).expect_err("not in the kit");
        assert!(reason(&missing).starts_with("connection_not_in_kit: "));
        let short = ask("dev-a", "github", "short").expect_err("too short");
        assert!(reason(&short).starts_with("connection_why: "));
        let held = ask("dev-b", "github", WHY).expect_err("dev-b has github");
        assert!(reason(&held).starts_with("connection_held: "));
        assert_eq!(
            project.events(&[EventKind::ConnectionRequested]).len(),
            1,
            "a refusal records nothing"
        );
    }
}
