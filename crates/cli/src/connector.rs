//! `farik connect` and `farik disconnect` (`docs/SPEC.md` 5.6, ADR 0030): this process reads the
//! keys, lists the server's tools with them and keeps them, so that a key never crosses a socket;
//! the team file and the event go through the command, which carries names only.

use std::collections::BTreeMap;
use std::io::BufRead as _;

use farik_core::contract::ValidationError;
use farik_core::team::{CustomTransport, spec_sha256};
use farik_protocol::command::Command;
use farik_runtime::claude::Secret;
use farik_runtime::connectors::{
    ConnectorEntry, ConnectorError, SecretAt, SecretStore, folder_refusal, list_tools,
    working_folder,
};
use farik_runtime::credential::CredentialError;
use farik_runtime::daemon::{custom_entry, labelled};
use farik_runtime::sign_in::{SignInError, revoke, start_sign_in};
use serde_json::{Map, Value, json};

use crate::project::Project;
use crate::start::{command, runtime};
use crate::{CliIo, Report};

/// The labels a tool can be given.
const TAGS: [&str; 3] = ["network", "external_effect", "denied"];

/// What `farik connect` was asked, as typed.
pub(crate) struct Asked<'a> {
    pub(crate) agent: &'a str,
    pub(crate) name: &'a str,
    pub(crate) command: Option<&'a str>,
    pub(crate) args: &'a [String],
    pub(crate) url: Option<&'a str>,
    pub(crate) headers: &'a [String],
    pub(crate) keys: &'a [String],
    pub(crate) tags: &'a [String],
    /// Sign in to the server's service instead of giving keys.
    pub(crate) sign_in: bool,
    pub(crate) client_id: Option<&'a str>,
    pub(crate) callback_port: Option<u16>,
    pub(crate) scopes: &'a [String],
}

/// `farik connect`: the entry held to the team's rules, each key read from standard input, the
/// server's tools listed with them and labelled, the keys kept beside the definition's hash, then
/// `connector_connect` handled here or sent. Prints each tool with its label, what the command
/// said, and where the keys were kept, last.
///
/// # Errors
///
/// A key given a value on the command line, a label or header that is not one, the team's
/// errors, a server whose tools cannot be listed, a store that cannot keep the keys, or the
/// command's refusal.
pub(crate) fn connect(
    project: &Project,
    asked: &Asked<'_>,
    io: &mut CliIo<'_>,
) -> Result<Report, String> {
    // An opener that fails is no error: the address is printed too, and the user can open it.
    let opener = std::sync::Arc::clone(&io.open_url);
    connect_with(project, asked, io, &move |url| {
        let _ = opener(url);
    })
}

/// [`connect`], opening the sign-in page with `open`, which a test passes to follow it.
///
/// # Errors
///
/// As [`connect`], and a service that does not offer, or does not complete, a sign-in.
pub(crate) fn connect_with(
    project: &Project,
    asked: &Asked<'_>,
    io: &mut CliIo<'_>,
    open: &dyn Fn(&str),
) -> Result<Report, String> {
    if asked.sign_in {
        return connect_signed_in(project, asked, io, open);
    }
    let wire = wire_of(asked)?;
    let tags = tags_of(asked.tags)?;
    let (_, server) =
        custom_entry(&project.team, asked.agent, &wire, json!({})).map_err(|e| errors(&e))?;
    let keys = read_keys(asked.keys, io)?;
    let state = state_of(io)?;
    let at = secret_at(&state, project, asked.agent, &server.name)?;
    let folder = working_folder(&state, &project.root, &at)
        .map_err(|error| format!("{}: {}", server.name, folder_refusal(&error)))?;
    let listed = runtime()?
        .block_on(list_tools(&server, &keys, None, &folder))
        .map_err(|error| not_listed(&error))?;
    let tools = labelled(&listed, &tags)?;
    let (entry, server) = custom_entry(&project.team, asked.agent, &wire, Value::Object(tools))
        .map_err(|e| errors(&e))?;
    let spec = spec_sha256(&server);
    let stored_in = io
        .connector_secrets
        .save(
            &at,
            &ConnectorEntry {
                spec_sha256: spec.clone(),
                keys,
                oauth: None,
            },
        )
        .map_err(|error| words(&error))?;
    let said = crate::human::said(command(
        project,
        Command::ConnectorConnect {
            agent: asked.agent.to_string(),
            server: entry.as_object().cloned().unwrap_or_default(),
            spec_sha256: spec,
            issuer: None,
        },
        "connect",
        io,
    ))?;
    Ok(connected_report(&listed, &entry, said, stored_in))
}

/// What `farik connect` prints once the server is kept: each tool with its label, what the
/// command said, and where the keys or the sign-in were kept, last.
fn connected_report(
    listed: &[farik_runtime::connectors::ListedTool],
    entry: &Value,
    said: Report,
    stored_in: SecretStore,
) -> Report {
    let mut lines: Vec<String> = listed
        .iter()
        .map(|tool| match entry["tools"][&tool.name].as_str() {
            Some(tag) => format!("{}: {tag}", tool.name),
            None => format!("{}: Farik can't use this tool", tool.name),
        })
        .collect();
    lines.extend(said.lines);
    lines.push(
        match stored_in {
            SecretStore::Keychain => "Kept in your computer's keychain",
            SecretStore::File => "Kept in a private file only you can read",
        }
        .to_string(),
    );
    Report {
        lines,
        json: json!({ "tools": entry["tools"], "stored_in": stored_in, "said": said.json["said"] }),
        json_lines: None,
    }
}

/// `farik connect --sign-in`: the server's sign-in found out, its page printed and opened, the
/// way back waited for up to ten minutes, the server's tools listed with the token and labelled,
/// the grant kept where keys are, then `connector_connect` handled here or sent, which names who
/// signed in and holds no token (ADR 0033).
fn connect_signed_in(
    project: &Project,
    asked: &Asked<'_>,
    io: &mut CliIo<'_>,
    open: &dyn Fn(&str),
) -> Result<Report, String> {
    let url = asked.url.unwrap_or_default();
    let host = host_of(url);
    let mut wire = wire_of(asked)?;
    let mut oauth = Map::new();
    if let Some(client) = asked.client_id {
        oauth.insert("client_id".to_string(), json!(client));
    }
    if let Some(port) = asked.callback_port {
        oauth.insert("callback_port".to_string(), json!(port));
    }
    if !asked.scopes.is_empty() {
        oauth.insert("scopes".to_string(), json!(asked.scopes));
    }
    wire["oauth"] = Value::Object(oauth);
    let tags = tags_of(asked.tags)?;
    let (_, server) =
        custom_entry(&project.team, asked.agent, &wire, json!({})).map_err(|e| errors(&e))?;
    let CustomTransport::Http {
        oauth: Some(settings),
        ..
    } = &server.transport
    else {
        return Err(format!("{} does not sign in", server.name));
    };
    let state = state_of(io)?;
    let at = secret_at(&state, project, asked.agent, &server.name)?;
    let folder = working_folder(&state, &project.root, &at)
        .map_err(|error| format!("{}: {}", server.name, folder_refusal(&error)))?;
    let runtime = runtime()?;
    let signing = runtime
        .block_on(start_sign_in(url, settings, chrono::Utc::now()))
        .map_err(|error| refused(&error, &host))?;
    // Prompts, not results: on stderr, so that `--json` leaves the output as the JSON alone.
    crate::say(
        &mut io.stderr,
        &format!(
            "Sign in to {} in your browser: {}",
            host_of(signing.issuer()),
            signing.authorize_url()
        ),
    );
    open(signing.authorize_url());
    let issuer = signing.issuer().to_string();
    let grant = runtime
        .block_on(signing.finish())
        .map_err(|error| refused(&error, &host))?;
    crate::say(&mut io.stderr, &format!("Signed in to {issuer}."));
    let listed = runtime
        .block_on(list_tools(
            &server,
            &BTreeMap::new(),
            Some(&grant.access_token),
            &folder,
        ))
        .map_err(|error| not_listed(&error))?;
    let tools = labelled(&listed, &tags)?;
    let (entry, server) = custom_entry(&project.team, asked.agent, &wire, Value::Object(tools))
        .map_err(|e| errors(&e))?;
    let spec = spec_sha256(&server);
    // A sign-in this one replaces is asked to be forgotten, so it does not linger at the service.
    let replaced = io
        .connector_secrets
        .load(&at)
        .ok()
        .flatten()
        .and_then(|old| old.oauth);
    let kept = grant.clone();
    let stored_in = io
        .connector_secrets
        .save(
            &at,
            &ConnectorEntry {
                spec_sha256: spec.clone(),
                keys: BTreeMap::new(),
                oauth: Some(grant),
            },
        )
        .map_err(|error| words(&error))?;
    // Not the grant just kept, nor one of its client's (`revocable_after`).
    if let Some(old) = replaced
        && old.revocable_after(&kept)
    {
        runtime.block_on(revoke(&old));
    }
    let said = crate::human::said(command(
        project,
        Command::ConnectorConnect {
            agent: asked.agent.to_string(),
            server: entry.as_object().cloned().unwrap_or_default(),
            spec_sha256: spec,
            issuer: Some(issuer),
        },
        "connect",
        io,
    ))?;
    Ok(connected_report(&listed, &entry, said, stored_in))
}

/// The host of `url`, for a sentence: no scheme, no userinfo, no port.
fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit('@').next().unwrap_or_default();
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.ends_with(']') && port.chars().all(|c| c.is_ascii_digit()) => {
            host.to_string()
        }
        _ => authority.to_string(),
    }
}

/// Why a sign-in did not happen, in a sentence that quotes nothing the service sent.
fn refused(error: &SignInError, host: &str) -> String {
    match error {
        SignInError::NotOffered => {
            format!("{host} does not offer signing in; give its key with --key")
        }
        SignInError::NotSupported => format!(
            "{host} does not let Farik sign in by itself yet; if it gives you a key, use --key"
        ),
        SignInError::PkceNotSupported => {
            format!("{host}'s sign-in is not one Farik will use")
        }
        SignInError::Denied(_) => format!("you said no on {host}'s page, so nothing was connected"),
        SignInError::Mismatch => format!(
            "something did not match on the way back from {host}, so Farik stopped to keep you safe"
        ),
        SignInError::TimedOut => "the sign-in took longer than 10 minutes".to_string(),
        SignInError::Lapsed => format!("{host} ended the sign-in"),
        SignInError::Failed(why) => why.clone(),
    }
}

/// `farik disconnect`: `connector_disconnect` handled here or sent, which takes the entry out of
/// the team file, then the agent's keys for it deleted, and no other agent's.
///
/// # Errors
///
/// The command's refusal, or a store that cannot delete the keys.
pub(crate) fn disconnect(
    project: &Project,
    agent: &str,
    name: &str,
    io: &mut CliIo<'_>,
) -> Result<Report, String> {
    let said = crate::human::said(command(
        project,
        Command::ConnectorDisconnect {
            agent: agent.to_string(),
            server: name.to_string(),
        },
        "disconnect",
        io,
    ))?;
    let at = secret_at(&state_of(io)?, project, agent, name)?;
    // Loaded first, so that a sign-in can be asked to be forgotten once the entry is gone.
    let grant = io
        .connector_secrets
        .load(&at)
        .ok()
        .flatten()
        .and_then(|entry| entry.oauth);
    io.connector_secrets
        .delete(&at)
        .map_err(|error| words(&error))?;
    if let Some(grant) = grant {
        runtime()?.block_on(revoke(&grant));
    }
    Ok(said)
}

/// The user's state folder, where the project's id on this machine is kept and each stdio
/// connector runs (ADR 0030).
fn state_of(io: &CliIo<'_>) -> Result<std::path::PathBuf, String> {
    crate::state::state_dir(&io.env).ok_or_else(|| {
        "XDG_CONFIG_HOME, HOME and APPDATA are all unset, so Farik has no state folder to keep \
         this project's connectors in"
            .to_string()
    })
}

/// Where `agent`'s keys for `server` are kept in `project`.
fn secret_at(
    state: &std::path::Path,
    project: &Project,
    agent: &str,
    server: &str,
) -> Result<SecretAt, String> {
    SecretAt::of(state, &project.root, agent, server)
        .map_err(|error| format!("this project's id cannot be read: {error}"))
}

/// The `mcp_servers` entry `asked` describes, without its tools.
fn wire_of(asked: &Asked<'_>) -> Result<Value, String> {
    let mut wire = json!({ "name": asked.name, "credential_keys": asked.keys });
    if let Some((name, _)) = asked.keys.iter().find_map(|key| key.split_once('=')) {
        return Err(format!(
            "--key {name}=...: give the key's name alone; its value is read from standard input, \
             never from the command line, which other programs can see"
        ));
    }
    if let Some(program) = asked.command {
        wire["transport"] = json!("stdio");
        wire["command"] = json!(program);
        wire["args"] = json!(asked.args);
    }
    if let Some(url) = asked.url {
        let mut headers = Map::new();
        for header in asked.headers {
            let (name, template) = header
                .split_once(':')
                .ok_or_else(|| format!("--header {header}: write it as 'Name: template'"))?;
            headers.insert(name.trim().to_string(), json!(template.trim()));
        }
        wire["transport"] = json!("http");
        wire["url"] = json!(url);
        wire["headers"] = Value::Object(headers);
    }
    Ok(wire)
}

/// The labels `--tag` gave, as an object of tool name to tag.
fn tags_of(tags: &[String]) -> Result<Value, String> {
    let mut labelled = Map::new();
    for tag in tags {
        match tag.split_once('=') {
            Some((tool, label)) if TAGS.contains(&label) => {
                labelled.insert(tool.to_string(), json!(label));
            }
            _ => {
                return Err(format!(
                    "--tag {tag}: write it as <tool>=network, <tool>=external_effect or \
                     <tool>=denied"
                ));
            }
        }
    }
    Ok(Value::Object(labelled))
}

/// One line of standard input for each key named, read with its echo off at a terminal.
fn read_keys(names: &[String], io: &mut CliIo<'_>) -> Result<BTreeMap<String, Secret>, String> {
    let mut keys = BTreeMap::new();
    let mut input = std::io::BufReader::new(&mut io.stdin);
    for name in names {
        let value = if io.stdin_is_terminal {
            rpassword::prompt_password(format!("{name}: "))
                .map_err(|error| format!("{name} could not be read: {error}"))?
        } else {
            let mut line = String::new();
            input
                .read_line(&mut line)
                .map_err(|error| format!("{name} could not be read: {error}"))?;
            line.trim_end_matches(['\n', '\r']).to_string()
        };
        if value.is_empty() {
            return Err(format!(
                "the key {name} has no value: give it on standard input, one line per --key"
            ));
        }
        keys.insert(name.clone(), Secret::new(value));
    }
    Ok(keys)
}

/// The team's errors, each at its field, in one sentence.
fn errors(errors: &[ValidationError]) -> String {
    errors
        .iter()
        .map(|error| format!("{}: {}", error.path, error.message))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Why a server's tools could not be listed, in a sentence that quotes no key.
fn not_listed(error: &ConnectorError) -> String {
    match error {
        ConnectorError::Timeout => "the server did not answer within thirty seconds".to_string(),
        ConnectorError::KeyMissing(name) => format!("the key {name} has no value"),
        ConnectorError::Failed(why) => format!("its tools could not be listed: {why}"),
    }
}

/// A store's refusal in words.
fn words(error: &CredentialError) -> String {
    match error {
        CredentialError::NoKeychain => "this computer has no keychain".to_string(),
        CredentialError::Failed(why) => why.clone(),
    }
}
