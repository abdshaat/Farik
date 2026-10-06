//! Farik's own calls to a service (ADR 0042, the amendment on `OWN_CALLS`): an act that is not an
//! agent's, such as Farik handing the owner's approved post to Buffer, made with the agent's own
//! connection to the service. Every one goes through [`call_as`], and only the pairs of
//! [`OWN_CALLS`] may be made: a later step that has Farik call a service itself adds its pairs
//! here and calls through `call_as`, never beside it.
//!
//! Such a call is not the agent's: it passes no hook, may name a tool the kit tags `denied`, and
//! is recorded by the event of the act it serves. It uses no session, so it never counts against
//! the agent's tool-call limit.

use std::sync::Arc;
use std::time::Duration;

use farik_core::contract::Role;
use farik_core::team::{CustomServer, CustomTransport, custom_server};
use serde_json::Value;

use super::{DaemonState, Fresh, matches_kit, refreshed_entry};
use crate::claude::Secret;
use crate::connectors::{ConnectorEntry, ConnectorError, call_tool, confirmed_entry, own_program};

/// The calls Farik makes itself, as `(service, tool)`: Buffer's `get_channel`, `create_post` and
/// `delete_post`. Any other pair is [`OwnCallError::NotListed`] before anything starts.
pub(crate) const OWN_CALLS: &[(&str, &str)] = &[
    ("buffer", "get_channel"),
    ("buffer", "create_post"),
    ("buffer", "delete_post"),
];

/// Why a call of Farik's own did not give an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OwnCallError {
    /// The pair is not one of [`OWN_CALLS`].
    NotListed,
    /// The agent's connection to the service is not there as the kit has it: no entry, one the
    /// kit does not describe, none kept, or one whose keys or sign-in are gone. Holds the agent.
    NotConnected(String),
    /// The service ended the agent's sign-in.
    SignInAgain,
    /// The call could not be made or the connection could not be read, in a sentence.
    Failed(String),
    /// The service did not answer within thirty seconds.
    Timeout,
    /// The service answered that the call failed, in its own words, cut at 500 characters, which
    /// are untrusted text.
    Tool(String),
}

/// How long a sign-in must stay good for a call to be made on it: refreshed when it will not last
/// this long.
const VALID_FOR: Duration = Duration::from_secs(60);
/// How long a refresh may take before the call is made on the sign-in as it is, while that still
/// holds.
const REFRESH_WAIT: Duration = Duration::from_secs(10);

/// Calls `tool` of the service `server` with `arguments`, as Farik, with the connection of the
/// agent `agent`: its entry in the team file, which must be exactly the kit's (`matches_kit`) and
/// kept as connected, runs it in the agent's connector folder. A signed-in entry is refreshed
/// first, when it will not last a minute; an entry of keys is used as kept.
///
/// # Errors
///
/// [`OwnCallError`]: the pair is not listed, the agent's connection is not there or the service
/// ended its sign-in, the call could not be made, the service did not answer within thirty
/// seconds, or it answered that the call failed.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "step 08d task 4 makes the first call")
)]
pub(crate) async fn call_as(
    state: &Arc<DaemonState>,
    agent: &str,
    server: &str,
    tool: &str,
    arguments: serde_json::Map<String, Value>,
) -> Result<Value, OwnCallError> {
    if !OWN_CALLS.contains(&(server, tool)) {
        return Err(OwnCallError::NotListed);
    }
    let failed = |why: &dyn std::fmt::Display| OwnCallError::Failed(why.to_string());
    let not_connected = || OwnCallError::NotConnected(agent.to_string());
    let deps = state
        .deps()
        .ok_or_else(|| failed(&super::NO_PROJECT))?
        .clone();
    let team = deps.files.read_team().map_err(|error| failed(&error))?;
    let held = team
        .agents
        .iter()
        .find(|held| held.id.as_str() == agent)
        .ok_or_else(not_connected)?;
    let definition: CustomServer = held
        .mcp_servers
        .iter()
        .flatten()
        .filter(|entry| entry.name.as_str() == server)
        .find_map(custom_server)
        .ok_or_else(not_connected)?;
    let kit = (deps.kits)(Role::from(held.role)).map_err(|error| failed(&error))?;
    if !matches_kit(&kit, &definition) {
        return Err(not_connected());
    }
    let at = state
        .secret_at(deps.files.root(), agent, server)
        .map_err(|error| failed(&error))?;
    let (reader, kept_at, wanted) = (Arc::clone(state), at.clone(), definition.clone());
    let runs = tokio::task::spawn_blocking(move || reader.read_kept(&kept_at).runs(&wanted))
        .await
        .map_err(|error| failed(&error))?;
    if !runs {
        return Err(not_connected());
    }
    let signs_in = matches!(
        &definition.transport,
        CustomTransport::Http { oauth: Some(_), .. }
    );
    let entry: ConnectorEntry = if signs_in {
        refreshed_entry(state, &at, &definition, VALID_FOR, REFRESH_WAIT, true)
            .await
            .map_err(|fresh| match fresh {
                Fresh::Lapsed => OwnCallError::SignInAgain,
                Fresh::NotConfirmed => not_connected(),
                Fresh::Failed(why) | Fresh::Store(why) => OwnCallError::Failed(why),
            })?
    } else {
        let (secrets, kept_at, wanted) =
            (state.connector_secrets(), at.clone(), definition.clone());
        tokio::task::spawn_blocking(move || confirmed_entry(secrets.as_ref(), &kept_at, &wanted))
            .await
            .map_err(|error| failed(&error))?
            .map_err(|error| failed(&format!("the connection could not be read: {error:?}")))?
            .ok_or_else(not_connected)?
    };
    let folder = state
        .connector_folder(&at)
        .map_err(|error| failed(&error))?;
    let own = own_program(&definition, state.own_program())
        .map_err(|why| OwnCallError::Failed(why.to_string()))?;
    let bearer: Option<&Secret> = entry.oauth.as_ref().map(|grant| &grant.access_token);
    call_tool(
        &definition,
        &entry.keys,
        bearer,
        &folder,
        &own,
        tool,
        arguments,
    )
    .await
    .map_err(|error| match error {
        ConnectorError::Timeout => OwnCallError::Timeout,
        ConnectorError::KeyMissing(_) => not_connected(),
        ConnectorError::Failed(why) => OwnCallError::Failed(why),
        ConnectorError::ToolError { text } => OwnCallError::Tool(text),
    })
}

/// A Marketing Specialist whose Buffer is the OAuth fixture, as the tests of Farik's own calls and
/// of the tools that make them need it.
#[cfg(test)]
pub(crate) mod fixtures {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use farik_core::team::CustomServer;
    use serde_json::{Value, json};

    use crate::claude::Secret;
    use crate::connectors::{
        ConnectorEntry, ConnectorSecrets as _, MemoryConnectorSecrets, SecretAt,
    };
    use crate::daemon::DaemonState;
    use crate::oauth_fixture::Fixture;
    use crate::sign_in::OAuthGrant;
    use crate::tools::fixtures::TestProject;

    /// The Marketing Specialist's kit with one service, `buffer`, at `url`: signed in to, or, with
    /// `keyed`, reached with `Authorization: Bearer {API_KEY}`. `get_channel` is `network`; Farik's
    /// own `create_post` and `delete_post` are `denied` to the agent, as in the shipped kit.
    pub(crate) fn buffer_kit(url: &str, keyed: bool) -> farik_roles::Kit {
        let mut connector = json!({
            "name": "buffer", "transport": "http", "url": url,
            "title": "Buffer", "about": "Schedules posts.", "why": "To post.",
            "setup": "Sign in.",
            "tools": {
                "get_channel": "network", "list_channels": "network",
                "create_post": "denied", "delete_post": "denied"
            },
        });
        if keyed {
            connector["credential_keys"] = json!(["API_KEY"]);
            connector["headers"] = json!({ "Authorization": "Bearer {API_KEY}" });
            connector["key_page"] = json!("https://buffer.example/keys");
        } else {
            connector["oauth"] = json!({ "scopes": ["read"] });
        }
        let kit =
            json!({ "role": "marketing_specialist", "skills": [], "connectors": [connector] });
        farik_roles::parse_fixture_kit(
            farik_core::contract::Role::MarketingSpecialist,
            &kit.to_string(),
            &[],
            &[],
        )
        .expect("the fixture kit loads")
    }

    /// Makes `kit` the Marketing Specialist's kit and connects `kai` to its `buffer`, as the team
    /// file would hold it, in a store `daemon` keeps from now on. Answers the server and where its
    /// entry is kept, and the store.
    pub(crate) fn connect_buffer(
        project: &TestProject,
        daemon: &DaemonState,
        kit: &farik_roles::Kit,
    ) -> (CustomServer, SecretAt, Arc<MemoryConnectorSecrets>) {
        project.set_kit(kit.clone());
        let files = &project.deps.files;
        let team = files.read_team().expect("the team");
        let (entry, server) =
            crate::daemon::kit_entry(kit, &team, "kai", "buffer", &BTreeMap::new())
                .expect("the kit's service is kai's");
        let team = crate::daemon::with_server(&team, "kai", "buffer", Some(&entry))
            .expect("the entry is the team's");
        files.write_team(&team).expect("the team is written");
        let store = Arc::new(MemoryConnectorSecrets::default());
        assert!(daemon.set_connector_secrets(Arc::clone(&store) as _));
        let at = daemon
            .secret_at(files.root(), "kai", "buffer")
            .expect("an address");
        (server, at, store)
    }

    /// Keeps a sign-in `fixture` honours that ends `expires_in` from now, for `server` at `at`.
    pub(crate) fn keep_a_sign_in(
        store: &MemoryConnectorSecrets,
        (server, at): (&CustomServer, &SecretAt),
        fixture: &Fixture,
        expires_in: chrono::Duration,
    ) -> OAuthGrant {
        let now = chrono::Utc::now();
        let (access, refresh) = fixture.mint();
        let grant = OAuthGrant {
            issuer: fixture.origin.clone(),
            resource: fixture.mcp_url.clone(),
            client_id: "client-kept".to_string(),
            token_endpoint: format!("{}/token", fixture.origin),
            revocation_endpoint: Some(format!("{}/revoke", fixture.origin)),
            access_token: Secret::new(access),
            refresh_token: Some(Secret::new(refresh)),
            issued_at: now,
            expires_at: Some(now + expires_in),
            scopes: Vec::new(),
            lapsed: false,
        };
        store
            .save(
                at,
                &ConnectorEntry {
                    spec_sha256: farik_core::team::spec_sha256(server),
                    keys: BTreeMap::new(),
                    oauth: Some(grant.clone()),
                },
            )
            .expect("kept");
        grant
    }

    /// What Buffer's `get_channel` answers in the tests: the channel's service at the top.
    pub(crate) fn a_channel(service: &str) -> Value {
        json!({ "id": "chan-1", "service": service, "name": "Our shop" })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use serde_json::{Map, Value, json};

    use super::fixtures::{a_channel, buffer_kit, connect_buffer, keep_a_sign_in};
    use super::{OwnCallError, call_as};
    use crate::claude::Secret;
    use crate::connectors::{ConnectorEntry, ConnectorSecrets as _};
    use crate::oauth_fixture::{Fixture, ToolAnswer};
    use crate::orchestrator::fixtures::Harness;
    use crate::tools::fixtures::with_the_marketing_specialist;

    fn arguments(channel: &str) -> Map<String, Value> {
        json!({ "channelId": channel })
            .as_object()
            .cloned()
            .expect("an object")
    }

    fn marketing(name: &str) -> Harness {
        Harness::new(name, with_the_marketing_specialist)
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn calls_only_the_listed_tools() {
        let fixture = Fixture::start().await;
        let harness = marketing("own-calls-listed");
        let kit = buffer_kit(&fixture.mcp_url, false);
        let (server, at, store) = connect_buffer(&harness.project, &harness.daemon, &kit);
        keep_a_sign_in(&store, (&server, &at), &fixture, chrono::Duration::hours(1));

        for (service, tool) in [
            ("buffer", "list_posts"),
            ("buffer", "edit_post"),
            ("buffer", "execute_mutation"),
            ("kit", "get_channel"),
        ] {
            assert_eq!(
                call_as(&harness.daemon, "kai", service, tool, arguments("chan-1")).await,
                Err(OwnCallError::NotListed),
                "{service} {tool}"
            );
        }

        assert_eq!(fixture.count("/mcp"), 0, "nothing was started or reached");
        assert_eq!(fixture.count("/token"), 0);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn calls_with_the_agent_s_kept_sign_in() {
        let fixture = Fixture::start().await;
        fixture.set(|flags| {
            flags.tool_answers.insert(
                "get_channel".to_string(),
                ToolAnswer::Json(a_channel("instagram")),
            );
        });
        let harness = marketing("own-calls-signed-in");
        let kit = buffer_kit(&fixture.mcp_url, false);
        let (server, at, store) = connect_buffer(&harness.project, &harness.daemon, &kit);
        let kept = keep_a_sign_in(&store, (&server, &at), &fixture, chrono::Duration::hours(1));

        // A sign-in with an hour left is used as it is.
        let answer = call_as(
            &harness.daemon,
            "kai",
            "buffer",
            "get_channel",
            arguments("chan-1"),
        )
        .await
        .expect("Buffer answers");
        assert_eq!(answer, a_channel("instagram"));
        assert_eq!(
            fixture.calls("get_channel"),
            [arguments("chan-1")],
            "exactly its arguments"
        );
        assert_eq!(fixture.count("/token"), 0, "nothing needed refreshing");
        let bearer = format!("Bearer {}", kept.access_token.expose());
        let seen = fixture.requests("/mcp");
        assert!(!seen.is_empty());
        assert!(
            seen.iter()
                .all(|request| request.authorization.as_deref() == Some(bearer.as_str())),
            "every request carried the agent's access token: {seen:?}"
        );

        // One within 60 seconds of expiry is refreshed first, once, and the new token is used.
        keep_a_sign_in(
            &store,
            (&server, &at),
            &fixture,
            chrono::Duration::seconds(30),
        );
        let before = fixture.requests("/mcp").len();
        call_as(
            &harness.daemon,
            "kai",
            "buffer",
            "get_channel",
            arguments("chan-2"),
        )
        .await
        .expect("Buffer answers");
        assert_eq!(fixture.count("/token"), 1, "one refresh");
        let now = store
            .load(&at)
            .expect("the store reads")
            .and_then(|entry| entry.oauth)
            .expect("a grant is kept");
        let bearer = format!("Bearer {}", now.access_token.expose());
        let after = &fixture.requests("/mcp")[before..];
        assert!(!after.is_empty());
        assert!(
            after
                .iter()
                .all(|request| request.authorization.as_deref() == Some(bearer.as_str())),
            "the refreshed token was used: {after:?}"
        );
        assert_eq!(fixture.calls("get_channel")[1], arguments("chan-2"));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_an_entry_that_is_not_the_kit_s_or_not_kept() {
        let fixture = Fixture::start().await;
        let harness = marketing("own-calls-refuses");
        let kit = buffer_kit(&fixture.mcp_url, false);
        let (_, at, store) = connect_buffer(&harness.project, &harness.daemon, &kit);
        let named = || OwnCallError::NotConnected("kai".to_string());

        // Nothing kept.
        assert_eq!(
            call_as(
                &harness.daemon,
                "kai",
                "buffer",
                "get_channel",
                arguments("c")
            )
            .await,
            Err(named())
        );

        // The team file's entry is widened beyond the kit's, and kept as connected: it is not the
        // kit's, so it is not used.
        let mut team = serde_json::to_value(harness.project.deps.files.read_team().expect("team"))
            .expect("JSON");
        let at_kai = team["agents"]
            .as_array()
            .and_then(|agents| agents.iter().position(|agent| agent["id"] == "kai"))
            .expect("kai");
        team["agents"][at_kai]["mcp_servers"][0]["tools"]["create_post"] = json!("network");
        let widened = farik_core::team::validate_team(&team).expect("a team");
        harness
            .project
            .deps
            .files
            .write_team(&widened)
            .expect("written");
        let widened_server = widened.agents[at_kai]
            .mcp_servers
            .iter()
            .flatten()
            .find_map(farik_core::team::custom_server)
            .expect("the entry");
        keep_a_sign_in(
            &store,
            (&widened_server, &at),
            &fixture,
            chrono::Duration::hours(1),
        );
        assert_eq!(
            call_as(
                &harness.daemon,
                "kai",
                "buffer",
                "get_channel",
                arguments("c")
            )
            .await,
            Err(named()),
            "an entry that is not the kit's is not used"
        );
        assert_eq!(fixture.count("/mcp"), 0);

        // An agent the team has no Buffer for.
        assert_eq!(
            call_as(
                &harness.daemon,
                "dev-a",
                "buffer",
                "get_channel",
                arguments("c")
            )
            .await,
            Err(OwnCallError::NotConnected("dev-a".to_string()))
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_sign_in_the_service_ended_is_not_run_on() {
        let fixture = Fixture::start().await;
        let kit = buffer_kit(&fixture.mcp_url, false);
        let named = || OwnCallError::NotConnected("kai".to_string());

        // A sign-in kept as ended is not one to run on.
        let ended = marketing("own-calls-ended");
        let (server, at, store) = connect_buffer(&ended.project, &ended.daemon, &kit);
        let mut grant =
            keep_a_sign_in(&store, (&server, &at), &fixture, chrono::Duration::hours(1));
        grant.lapsed = true;
        store
            .save(
                &at,
                &ConnectorEntry {
                    spec_sha256: farik_core::team::spec_sha256(&server),
                    keys: BTreeMap::new(),
                    oauth: Some(grant),
                },
            )
            .expect("kept");
        assert_eq!(
            call_as(
                &ended.daemon,
                "kai",
                "buffer",
                "get_channel",
                arguments("c")
            )
            .await,
            Err(named())
        );
        assert_eq!(fixture.count("/mcp"), 0);

        // The service ends the sign-in when Farik refreshes it: the owner signs in again.
        let harness = marketing("own-calls-lapsed");
        fixture.set(|flags| flags.refresh_error = Some((400, "invalid_grant".to_string())));
        let (server, at, store) = connect_buffer(&harness.project, &harness.daemon, &kit);
        keep_a_sign_in(
            &store,
            (&server, &at),
            &fixture,
            chrono::Duration::seconds(-30),
        );
        assert_eq!(
            call_as(
                &harness.daemon,
                "kai",
                "buffer",
                "get_channel",
                arguments("c")
            )
            .await,
            Err(OwnCallError::SignInAgain)
        );
        assert_eq!(fixture.count("/mcp"), 0);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn uses_a_keys_entry_as_kept() {
        let fixture = Fixture::start().await;
        fixture.set(|flags| {
            flags
                .tool_answers
                .insert("get_channel".to_string(), ToolAnswer::Json(a_channel("x")));
        });
        let harness = marketing("own-calls-keys");
        let kit = buffer_kit(&fixture.mcp_url, true);
        let (server, at, store) = connect_buffer(&harness.project, &harness.daemon, &kit);
        // The key holds a token the fixture honours.
        let (token, _) = fixture.mint();
        store
            .save(
                &at,
                &ConnectorEntry {
                    spec_sha256: farik_core::team::spec_sha256(&server),
                    keys: BTreeMap::from([("API_KEY".to_string(), Secret::new(token.clone()))]),
                    oauth: None,
                },
            )
            .expect("kept");

        let answer = call_as(
            &harness.daemon,
            "kai",
            "buffer",
            "get_channel",
            arguments("chan-1"),
        )
        .await
        .expect("Buffer answers");

        assert_eq!(answer, a_channel("x"));
        let bearer = format!("Bearer {token}");
        let seen = fixture.requests("/mcp");
        assert!(!seen.is_empty());
        assert!(
            seen.iter()
                .all(|request| request.authorization.as_deref() == Some(bearer.as_str())),
            "{seen:?}"
        );
        assert_eq!(fixture.count("/token"), 0, "a key is not refreshed");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn maps_what_the_service_answers() {
        let fixture = Fixture::start().await;
        fixture.set(|flags| {
            flags.tool_answers.insert(
                "get_channel".to_string(),
                ToolAnswer::Error("Channel not found".to_string()),
            );
        });
        let harness = marketing("own-calls-maps");
        let kit = buffer_kit(&fixture.mcp_url, false);
        let (server, at, store) = connect_buffer(&harness.project, &harness.daemon, &kit);
        keep_a_sign_in(&store, (&server, &at), &fixture, chrono::Duration::hours(1));

        assert_eq!(
            call_as(
                &harness.daemon,
                "kai",
                "buffer",
                "get_channel",
                arguments("c")
            )
            .await,
            Err(OwnCallError::Tool("Channel not found".to_string()))
        );
    }

    #[tokio::test(start_paused = true)]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn gives_up_on_a_service_that_does_not_answer() {
        let fixture = Fixture::start().await;
        let harness = marketing("own-calls-timeout");
        let kit = buffer_kit(&fixture.mcp_url, false);
        let (server, at, store) = connect_buffer(&harness.project, &harness.daemon, &kit);
        keep_a_sign_in(&store, (&server, &at), &fixture, chrono::Duration::hours(1));
        fixture.hold("tool:get_channel");
        let started = tokio::time::Instant::now();

        // Paused time does not move while a blocking task runs: this one holds it until the call
        // has reached the tool, so the thirty seconds are the call's and not the connecting's.
        let held = Arc::clone(&harness.daemon);
        let (called, reached) = tokio::join!(
            call_as(&held, "kai", "buffer", "get_channel", arguments("c")),
            async {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while fixture.calls("get_channel").is_empty() {
                    if std::time::Instant::now() > deadline {
                        return false;
                    }
                    tokio::task::spawn_blocking(|| {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    })
                    .await
                    .expect("the wait ends");
                }
                true
            }
        );
        assert!(reached, "the call never reached the tool");

        assert_eq!(called, Err(OwnCallError::Timeout));
        let waited = started.elapsed();
        assert!(
            waited >= std::time::Duration::from_secs(30)
                && waited < std::time::Duration::from_secs(31),
            "{waited:?}"
        );
    }
}
