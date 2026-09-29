//! The JSON-RPC 2.0 wire between the browser and the daemon: `docs/schemas/rpc.schema.json` as
//! Rust types, and the readers that turn an untrusted frame into one.

use std::sync::LazyLock;

use jsonschema::Validator;
use serde_json::{Value, json};

pub use crate::generated::rpc::*;

const SCHEMA_JSON: &str = include_str!("../../../docs/schemas/rpc.schema.json");

/// A validator for one definition of the embedded schema.
fn validator_for(definition: &str) -> Validator {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded rpc schema is valid JSON: it is the file in docs/schemas/ \
         that typify generated this crate's types from at compile time",
    );
    let root = json!({
        "$schema": schema["$schema"],
        "$ref": format!("#/$defs/{definition}"),
        "$defs": schema["$defs"],
    });
    jsonschema::options().build(&root).expect(
        "the rpc definition compiles: it is JSON Schema 2020-12 with no external references, \
         and the generator already parsed it",
    )
}

static REQUEST: LazyLock<Validator> = LazyLock::new(|| validator_for("rpcRequest"));
static NOTIFICATION: LazyLock<Validator> = LazyLock::new(|| validator_for("rpcNotification"));

/// Every violation of `validator` in `input`, as `path: message`, or the typed value.
fn read<T: serde::de::DeserializeOwned>(
    validator: &Validator,
    input: &Value,
) -> Result<T, Vec<String>> {
    let errors: Vec<String> = validator
        .iter_errors(input)
        .map(|e| format!("{}: {e}", e.instance_path()))
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    serde_json::from_value(input.clone()).map_err(|e| {
        vec![format!(
            "the schema passed but the typed frame could not be built: {e}"
        )]
    })
}

/// Reads a request frame from the browser: the schema first, then the typed request.
///
/// # Errors
///
/// Every violation, as `path: message`.
pub fn rpc_request_from_value(v: &Value) -> Result<RpcRequest, Vec<String>> {
    read(&REQUEST, v)
}

/// Reads a notification frame: the schema for the envelope, then `event_from_value` for the event
/// inside it, so an event with no `seq` is refused although the schema calls it an object.
///
/// # Errors
///
/// Every violation, as `path: message`.
pub fn rpc_notification_from_value(v: &Value) -> Result<RpcNotification, Vec<String>> {
    let note: RpcNotification = read(&NOTIFICATION, v)?;
    crate::event::event_from_value(&v["params"]["event"]).map_err(|errors| {
        errors
            .into_iter()
            .map(|e| {
                format!(
                    "/params/event{}: {}",
                    e.path.trim_end_matches('/'),
                    e.message
                )
            })
            .collect::<Vec<String>>()
    })?;
    Ok(note)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{rpc_notification_from_value, rpc_request_from_value};

    fn request(id: u64, method: &str, params: &Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
    }

    #[test]
    fn reads_each_method_request() {
        let frames = [
            request(1, "subscribe", &json!({ "from_seq": 0 })),
            request(2, "unsubscribe", &json!({})),
            request(
                3,
                "command",
                &json!({ "command": { "command": "team_pause", "body": {} } }),
            ),
            request(
                4,
                "query",
                &json!({ "name": "events.since", "params": { "after_seq": 0, "limit": 500 } }),
            ),
            request(
                5,
                "query",
                &json!({ "name": "task.get", "params": { "task_id": "FRK-12" } }),
            ),
            request(6, "query", &json!({ "name": "serve.status", "params": {} })),
        ];
        for frame in frames {
            rpc_request_from_value(&frame).unwrap_or_else(|e| panic!("{frame} refused: {e:?}"));
        }
        // The typed request keeps what was asked.
        let read = rpc_request_from_value(&frames_subscribe()).expect("reads");
        assert_eq!(
            serde_json::to_value(&read).expect("writes"),
            frames_subscribe()
        );
    }

    fn frames_subscribe() -> Value {
        request(1, "subscribe", &json!({ "from_seq": 7 }))
    }

    #[test]
    fn refuses_a_request_without_jsonrpc_2_0() {
        let wrong = json!({ "jsonrpc": "1.0", "id": 1, "method": "unsubscribe", "params": {} });
        assert!(rpc_request_from_value(&wrong).is_err());
        let no_id = json!({ "jsonrpc": "2.0", "method": "unsubscribe", "params": {} });
        assert!(rpc_request_from_value(&no_id).is_err());
    }

    #[test]
    fn refuses_an_unknown_query_name() {
        let frame = request(1, "query", &json!({ "name": "secrets.get", "params": {} }));
        assert!(rpc_request_from_value(&frame).is_err());
        // A known name with another query's params is refused too.
        let mixed = request(1, "query", &json!({ "name": "task.get", "params": {} }));
        assert!(rpc_request_from_value(&mixed).is_err());
    }

    #[test]
    fn an_event_notification_carries_an_event_wire() {
        let event = json!({
            "seq": 3, "recorded_at": "2026-09-28T10:00:00Z",
            "team_id": "t", "project_id": "p",
            "kind": "team.paused", "body": { "by": "human" }
        });
        let note = |event: &Value| json!({ "jsonrpc": "2.0", "method": "event", "params": { "event": event } });
        rpc_notification_from_value(&note(&event)).expect("a team.paused event");
        let mut without_seq = event.clone();
        without_seq.as_object_mut().expect("object").remove("seq");
        assert!(rpc_notification_from_value(&note(&without_seq)).is_err());
    }
}
