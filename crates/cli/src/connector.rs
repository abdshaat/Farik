//! `farik connect` and `farik disconnect` (`docs/SPEC.md` 5.6, ADR 0030): this process reads the
//! keys, lists the server's tools with them and keeps them, so that a key never crosses a socket;
//! the team file and the event go through the command, which carries names only.

use std::collections::BTreeMap;
use std::io::BufRead as _;

use farik_core::contract::ValidationError;
use farik_core::team::spec_sha256;
use farik_protocol::command::Command;
use farik_runtime::claude::Secret;
use farik_runtime::connectors::{
    ConnectorEntry, ConnectorError, SecretAt, SecretStore, list_tools, working_folder,
};
use farik_runtime::credential::CredentialError;
use farik_runtime::daemon::{custom_entry, labelled};
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
    let wire = wire_of(asked)?;
    let tags = tags_of(asked.tags)?;
    let (_, server) =
        custom_entry(&project.team, asked.agent, &wire, json!({})).map_err(|e| errors(&e))?;
    let keys = read_keys(asked.keys, io)?;
    let folder = working_folder(&project.root, asked.agent, &server.name)
        .map_err(|error| format!("{} cannot be made: {error}", server.name))?;
    let listed = runtime()?
        .block_on(list_tools(&server, &keys, &folder))
        .map_err(|error| not_listed(&error))?;
    let tools = labelled(&listed, &tags)?;
    let (entry, server) = custom_entry(&project.team, asked.agent, &wire, Value::Object(tools))
        .map_err(|e| errors(&e))?;
    let spec = spec_sha256(&server);
    let at = SecretAt::of(&project.root, asked.agent, &server.name)
        .map_err(|error| format!("this project's id cannot be read: {error}"))?;
    let stored_in = io
        .connector_secrets
        .save(
            &at,
            &ConnectorEntry {
                spec_sha256: spec.clone(),
                keys,
            },
        )
        .map_err(|error| words(&error))?;
    let said = crate::human::said(command(
        project,
        Command::ConnectorConnect {
            agent: asked.agent.to_string(),
            server: entry.as_object().cloned().unwrap_or_default(),
            spec_sha256: spec,
        },
        "connect",
        io,
    ))?;
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
    Ok(Report {
        lines,
        json: json!({ "tools": entry["tools"], "stored_in": stored_in, "said": said.json["said"] }),
        json_lines: None,
    })
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
    let at = SecretAt::of(&project.root, agent, name)
        .map_err(|error| format!("this project's id cannot be read: {error}"))?;
    io.connector_secrets
        .delete(&at)
        .map_err(|error| words(&error))?;
    Ok(said)
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
