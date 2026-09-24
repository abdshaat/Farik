//! The Claude Code program's `stream-json` output, one line at a time, as session events.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use farik_core::pricing::Usage;
use serde_json::Value;

use crate::session::{EndReason, RuntimeError, SessionEvent};

/// Reads a session's `stream-json` lines in order. It keeps what a later line needs from an
/// earlier one: a tool's name by its call id, which calls were denied, and the last rate limit.
#[derive(Debug, Default)]
pub struct StreamParser {
    tools_by_id: BTreeMap<String, String>,
    denied_ids: BTreeSet<String>,
    rate_limit: Option<RateLimit>,
}

/// The last `rate_limit_event`'s `rate_limit_info`: its status, and when it said it resets.
#[derive(Debug)]
struct RateLimit {
    status: String,
    resets_at: Option<DateTime<Utc>>,
}

impl StreamParser {
    /// The events one line reports; most lines report none.
    ///
    /// # Errors
    ///
    /// `RuntimeError::Protocol` when the line is not JSON, or is a kind of line the parser reads
    /// and lacks a field it needs.
    pub fn parse_line(&mut self, line: &str) -> Result<Vec<SessionEvent>, RuntimeError> {
        let value: Value = serde_json::from_str(line).map_err(|error| RuntimeError::Protocol {
            detail: format!("a line is not JSON: {error}"),
        })?;
        match value.get("type").and_then(Value::as_str) {
            Some("assistant") => self.assistant(&value),
            Some("user") => self.user(&value),
            Some("system") => self.system(&value),
            Some("rate_limit_event") => {
                self.rate_limit_event(&value);
                Ok(Vec::new())
            }
            Some("result") => result(&value, self.rate_limit.as_ref()),
            // The program adds line types between minor versions; a new one must not end a session.
            _ => Ok(Vec::new()),
        }
    }

    fn assistant(&mut self, value: &Value) -> Result<Vec<SessionEvent>, RuntimeError> {
        let mut events = Vec::new();
        for block in content_blocks(value, "assistant")? {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => events.push(SessionEvent::TextProduced(
                    string_field(block, "text", "an assistant text block")?.to_string(),
                )),
                Some("tool_use") => {
                    let id = string_field(block, "id", "a tool_use block")?;
                    let name = string_field(block, "name", "a tool_use block")?;
                    self.tools_by_id.insert(id.to_string(), name.to_string());
                    events.push(SessionEvent::ToolCalled {
                        tool: name.to_string(),
                        input: block.get("input").cloned().ok_or_else(|| {
                            RuntimeError::Protocol {
                                detail: "a tool_use block has no input".to_string(),
                            }
                        })?,
                    });
                }
                _ => {}
            }
        }
        Ok(events)
    }

    fn user(&self, value: &Value) -> Result<Vec<SessionEvent>, RuntimeError> {
        if value
            .pointer("/message/content")
            .is_some_and(Value::is_string)
        {
            return Ok(Vec::new());
        }
        let mut events = Vec::new();
        for block in content_blocks(value, "user")? {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let id = string_field(block, "tool_use_id", "a tool_result block")?;
            if self.denied_ids.contains(id) {
                continue;
            }
            let tool = self
                .tools_by_id
                .get(id)
                .ok_or_else(|| RuntimeError::Protocol {
                    detail: format!(
                        "a tool_result names tool_use_id {id}, which no tool_use called"
                    ),
                })?;
            let output = tool_output(block.get("content"));
            let is_error = block.get("is_error").and_then(Value::as_bool) == Some(true);
            // Claude Code reports a hook's deny as the call's error, with no line of its own.
            let hook_denial = format!("PreToolUse:{tool} hook error: ");
            match output.strip_prefix(&hook_denial) {
                Some(reason) if is_error => events.push(SessionEvent::ToolDenied {
                    tool: tool.clone(),
                    reason: reason.to_string(),
                }),
                _ => events.push(SessionEvent::ToolReturned {
                    tool: tool.clone(),
                    output,
                }),
            }
        }
        Ok(events)
    }

    fn rate_limit_event(&mut self, value: &Value) {
        // An event without its info says nothing, and must not end a session.
        if let Some(info) = value.get("rate_limit_info") {
            self.rate_limit = Some(RateLimit {
                status: info
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                resets_at: info
                    .get("resetsAt")
                    .and_then(Value::as_i64)
                    .and_then(|seconds| DateTime::from_timestamp(seconds, 0)),
            });
        }
    }

    fn system(&mut self, value: &Value) -> Result<Vec<SessionEvent>, RuntimeError> {
        if value.get("subtype").and_then(Value::as_str) != Some("permission_denied") {
            return Ok(Vec::new());
        }
        let what = "a permission_denied line";
        let id = string_field(value, "tool_use_id", what)?;
        let denied = SessionEvent::ToolDenied {
            tool: string_field(value, "tool_name", what)?.to_string(),
            reason: string_field(value, "decision_reason", what)?.to_string(),
        };
        self.denied_ids.insert(id.to_string());
        Ok(vec![denied])
    }
}

fn result(
    value: &Value,
    rate_limit: Option<&RateLimit>,
) -> Result<Vec<SessionEvent>, RuntimeError> {
    let usage = value.get("usage").ok_or_else(|| RuntimeError::Protocol {
        detail: "a result line has no usage".to_string(),
    })?;
    let tokens = |field: &str| {
        usage
            .get(field)
            .and_then(Value::as_u64)
            .ok_or_else(|| RuntimeError::Protocol {
                detail: format!("a result line's usage has no {field}"),
            })
    };
    let usage = Usage {
        input_tokens: tokens("input_tokens")?,
        output_tokens: tokens("output_tokens")?,
        cache_read_tokens: tokens("cache_read_input_tokens")?,
        cache_write_tokens: tokens("cache_creation_input_tokens")?,
    };
    let is_error = value.get("is_error").and_then(Value::as_bool) == Some(true);
    let reason = match string_field(value, "subtype", "a result line")? {
        // The program reports an API's refusal as a `success` that is an error.
        "success" if !is_error => EndReason::Completed,
        "error_max_turns" => EndReason::Limit,
        _ => EndReason::Error,
    };
    let errors: Vec<&str> = value
        .get("errors")
        .and_then(Value::as_array)
        .map(|errors| errors.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let detail = if errors.is_empty() {
        value
            .get("result")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    } else {
        errors.join("; ")
    };
    // No capture of a refused session exists yet, so each of the three forms counts.
    let lowered = detail.to_ascii_lowercase();
    let is_provider_limit = reason == EndReason::Error
        && (rate_limit.is_some_and(|limit| !limit.status.starts_with("allowed"))
            || value.get("api_error_status").and_then(Value::as_u64) == Some(429)
            || lowered.contains("usage limit")
            || lowered.contains("rate limit"));
    let (reason, resets_at) = if is_provider_limit {
        (
            EndReason::ProviderLimit,
            rate_limit.and_then(|limit| limit.resets_at),
        )
    } else {
        (reason, None)
    };
    Ok(vec![
        SessionEvent::UsageReported(usage),
        SessionEvent::Ended {
            reason,
            detail,
            resets_at,
        },
    ])
}

fn content_blocks<'a>(value: &'a Value, line: &str) -> Result<&'a Vec<Value>, RuntimeError> {
    value
        .pointer("/message/content")
        .and_then(Value::as_array)
        .ok_or_else(|| RuntimeError::Protocol {
            detail: format!("an {line} line has no message.content array"),
        })
}

fn string_field<'a>(value: &'a Value, field: &str, what: &str) -> Result<&'a str, RuntimeError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| RuntimeError::Protocol {
            detail: format!("{what} has no {field}"),
        })
}

fn tool_output(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use farik_core::pricing::Usage;
    use serde_json::json;

    use super::StreamParser;
    use crate::recorded::Transcript;
    use crate::recorded::fixtures::{
        hits_the_turn_limit, hook_denies_a_write, provider_limit_429, provider_limit_rejected,
        provider_limit_text, reads_a_file, success_with_is_error, write_denied,
    };
    use crate::session::{EndReason, RuntimeError, SessionEvent};

    fn events_of(transcript: &Transcript) -> Vec<SessionEvent> {
        let mut parser = StreamParser::default();
        transcript
            .lines()
            .flat_map(|line| parser.parse_line(line).expect("a recorded line parses"))
            .collect()
    }

    fn one_line(line: &str) -> Result<Vec<SessionEvent>, RuntimeError> {
        StreamParser::default().parse_line(line)
    }

    #[test]
    fn reports_a_tool_call_its_result_and_the_text_of_a_completed_session() {
        let events = events_of(&reads_a_file());
        assert_eq!(events.len(), 5, "{events:?}");
        match &events[0] {
            SessionEvent::ToolCalled { tool, input } => {
                assert_eq!(tool, "Read");
                assert_eq!(input["file_path"], json!("/workspace/note.txt"));
            }
            other => panic!("expected the Read call, got {other:?}"),
        }
        match &events[1] {
            SessionEvent::ToolReturned { tool, output } => {
                assert_eq!(tool, "Read");
                assert!(output.contains("hello fixture"), "{output}");
            }
            other => panic!("expected the Read result, got {other:?}"),
        }
        assert_eq!(
            events[2],
            SessionEvent::TextProduced("hello fixture".to_string())
        );
        assert!(matches!(events[3], SessionEvent::UsageReported(_)));
        assert!(matches!(
            events[4],
            SessionEvent::Ended {
                reason: EndReason::Completed,
                ..
            }
        ));
    }

    #[test]
    fn reads_usage_from_the_result_line_only() {
        let usage: Vec<Usage> = events_of(&reads_a_file())
            .into_iter()
            .filter_map(|event| match event {
                SessionEvent::UsageReported(usage) => Some(usage),
                _ => None,
            })
            .collect();
        assert_eq!(
            usage,
            vec![Usage {
                input_tokens: 18,
                // The assistant lines' own output_tokens sum to 10; only the result says 368.
                output_tokens: 368,
                cache_read_tokens: 33806,
                cache_write_tokens: 10215,
            }]
        );
    }

    #[test]
    fn reports_a_denial_once_with_its_reason() {
        let events = events_of(&write_denied());
        let denials: Vec<&SessionEvent> = events
            .iter()
            .filter(|event| matches!(event, SessionEvent::ToolDenied { .. }))
            .collect();
        assert_eq!(
            denials,
            vec![&SessionEvent::ToolDenied {
                tool: "Write".to_string(),
                reason: "Claude requested permissions to edit /workspace/out.txt which is a \
                         sensitive file."
                    .to_string(),
            }]
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, SessionEvent::ToolReturned { .. })),
            "{events:?}"
        );
    }

    #[test]
    fn ends_with_limit_when_the_turn_limit_is_reached() {
        let events = events_of(&hits_the_turn_limit());
        assert_eq!(
            events.last(),
            Some(&SessionEvent::Ended {
                reason: EndReason::Limit,
                detail: "Reached maximum number of turns (1)".to_string(),
                resets_at: None,
            })
        );
    }

    #[test]
    fn ends_with_error_on_any_other_error_subtype() {
        let events = one_line(
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["boom"],"usage":{"input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}"#,
        )
        .expect("a result line parses");
        assert_eq!(
            events.last(),
            Some(&SessionEvent::Ended {
                reason: EndReason::Error,
                detail: "boom".to_string(),
                resets_at: None,
            })
        );
    }

    #[test]
    fn ignores_line_types_it_does_not_know() {
        for line in [
            r#"{"type":"rate_limit_event"}"#,
            r#"{"type":"something_new"}"#,
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"","signature":""}]}}"#,
            r#"{"type":"user","message":{"content":"hi"}}"#,
        ] {
            assert_eq!(one_line(line), Ok(Vec::new()), "{line}");
        }
    }

    #[test]
    fn refuses_a_result_for_a_tool_nobody_called() {
        let refused = one_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_unknown","content":"x"}]}}"#,
        );
        match refused {
            Err(RuntimeError::Protocol { detail }) => {
                assert!(detail.contains("tool_use_id"), "{detail}");
            }
            other => panic!("expected a protocol refusal, got {other:?}"),
        }
    }

    #[test]
    fn refuses_a_line_that_is_not_json() {
        assert!(matches!(
            one_line("not json"),
            Err(RuntimeError::Protocol { .. })
        ));
    }

    #[test]
    fn refuses_a_result_without_usage() {
        match one_line(r#"{"type":"result","subtype":"success","result":"done"}"#) {
            Err(RuntimeError::Protocol { detail }) => {
                assert!(detail.contains("usage"), "{detail}");
            }
            other => panic!("expected a protocol refusal, got {other:?}"),
        }
    }

    #[test]
    fn joins_the_text_blocks_of_a_tool_result_array() {
        let mut parser = StreamParser::default();
        parser
            .parse_line(
                r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Grep","input":{}}]}}"#,
            )
            .expect("a tool call parses");
        let events = parser
            .parse_line(
                r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":[{"type":"text","text":"a"},{"type":"text","text":"b"}]}]}}"#,
            )
            .expect("a tool result parses");
        assert_eq!(
            events,
            vec![SessionEvent::ToolReturned {
                tool: "Grep".to_string(),
                output: "a\nb".to_string(),
            }]
        );
    }

    #[test]
    fn joins_several_errors_and_otherwise_ends_with_the_result_text() {
        let events = one_line(
            r#"{"type":"result","subtype":"error_during_execution","errors":["a","b"],"usage":{"input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}"#,
        )
        .expect("a result line parses");
        assert_eq!(
            events.last(),
            Some(&SessionEvent::Ended {
                reason: EndReason::Error,
                detail: "a; b".to_string(),
                resets_at: None,
            })
        );
        assert_eq!(
            events_of(&reads_a_file()).last(),
            Some(&SessionEvent::Ended {
                reason: EndReason::Completed,
                detail: "hello fixture".to_string(),
                resets_at: None,
            })
        );
    }

    #[test]
    fn refuses_a_known_line_missing_a_field_it_reads() {
        let usage = r#""usage":{"input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}"#;
        let cases = [
            (format!(r#"{{"type":"result",{usage}}}"#), "subtype"),
            (
                r#"{"type":"result","subtype":"success","usage":{"input_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}"#.to_string(),
                "output_tokens",
            ),
            (
                r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{}}]}}"#.to_string(),
                "id",
            ),
            (
                r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t","name":"Read"}]}}"#.to_string(),
                "input",
            ),
            (
                r#"{"type":"assistant","message":{"content":[{"type":"text"}]}}"#.to_string(),
                "text",
            ),
            (
                r#"{"type":"system","subtype":"permission_denied","tool_use_id":"t","tool_name":"Write"}"#.to_string(),
                "decision_reason",
            ),
        ];
        for (line, field) in cases {
            match one_line(&line) {
                Err(RuntimeError::Protocol { detail }) => {
                    assert!(detail.contains(field), "{field} not in {detail}");
                }
                other => panic!("expected a refusal naming {field} for {line}, got {other:?}"),
            }
        }
    }

    #[test]
    fn reports_the_denials_decision_reason_rather_than_its_message() {
        let events = one_line(
            r#"{"type":"system","subtype":"permission_denied","tool_use_id":"t","tool_name":"Write","decision_reason":"the hook said no","message":"something else"}"#,
        )
        .expect("a denial parses");
        assert_eq!(
            events,
            vec![SessionEvent::ToolDenied {
                tool: "Write".to_string(),
                reason: "the hook said no".to_string(),
            }]
        );
    }

    #[test]
    fn reads_a_hook_denial_as_a_denied_tool() {
        let events = events_of(&hook_denies_a_write());
        assert!(
            events.contains(&SessionEvent::ToolDenied {
                tool: "Write".to_string(),
                reason: "farik says no".to_string(),
            }),
            "{events:?}"
        );
        assert!(
            !events.iter().any(|event| matches!(
                event,
                SessionEvent::ToolReturned { tool, .. } if tool == "Write"
            )),
            "{events:?}"
        );
    }

    #[test]
    fn reads_a_successful_result_that_looks_like_a_hook_denial_as_returned() {
        let mut parser = StreamParser::default();
        parser
            .parse_line(
                r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Read","input":{}}]}}"#,
            )
            .expect("a tool call parses");
        let events = parser
            .parse_line(
                r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","is_error":false,"content":"PreToolUse:Read hook error: a file that says this"}]}}"#,
            )
            .expect("a tool result parses");
        assert_eq!(
            events,
            vec![SessionEvent::ToolReturned {
                tool: "Read".to_string(),
                output: "PreToolUse:Read hook error: a file that says this".to_string(),
            }]
        );
    }

    fn end_of(events: &[SessionEvent]) -> (EndReason, Option<String>) {
        match events.last() {
            Some(SessionEvent::Ended {
                reason, resets_at, ..
            }) => (*reason, resets_at.map(|at| at.to_rfc3339())),
            other => panic!("expected an end, got {other:?}"),
        }
    }

    #[test]
    fn ends_a_successful_result_with_an_error_as_an_error() {
        assert_eq!(
            end_of(&events_of(&success_with_is_error())),
            (EndReason::Error, None)
        );
    }

    #[test]
    fn reads_a_rejected_rate_limit_as_the_providers_limit() {
        let resets_at = DateTime::from_timestamp(1_790_119_200, 0).expect("a time");
        assert_eq!(
            end_of(&events_of(&provider_limit_rejected())),
            (EndReason::ProviderLimit, Some(resets_at.to_rfc3339()))
        );
    }

    #[test]
    fn reads_a_429_as_the_providers_limit() {
        assert_eq!(
            end_of(&events_of(&provider_limit_429())),
            (EndReason::ProviderLimit, None)
        );
    }

    #[test]
    fn reads_a_usage_limit_in_the_words() {
        assert_eq!(
            end_of(&events_of(&provider_limit_text())),
            (EndReason::ProviderLimit, None)
        );
        let rate_limited = one_line(
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["API Error: Rate Limit exceeded"],"usage":{"input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}"#,
        )
        .expect("a result line parses");
        assert_eq!(end_of(&rate_limited), (EndReason::ProviderLimit, None));
    }

    fn after_a_rate_limit(status: &str, result: &str) -> Vec<SessionEvent> {
        let mut parser = StreamParser::default();
        let event = format!(
            r#"{{"type":"rate_limit_event","rate_limit_info":{{"status":"{status}","resetsAt":1790119200}}}}"#
        );
        assert_eq!(parser.parse_line(&event), Ok(Vec::new()));
        parser.parse_line(result).expect("a result line parses")
    }

    const AN_ERROR: &str = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"api_error_status":500,"errors":["the server fell over"],"usage":{"input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}"#;

    #[test]
    fn keeps_an_ordinary_error_an_error() {
        assert_eq!(
            end_of(&after_a_rate_limit("allowed", AN_ERROR)),
            (EndReason::Error, None)
        );
    }

    #[test]
    fn keeps_a_warning_an_ordinary_error() {
        assert_eq!(
            end_of(&after_a_rate_limit("allowed_warning", AN_ERROR)),
            (EndReason::Error, None)
        );
    }

    #[test]
    fn passes_no_reset_time_without_a_limit() {
        let success = r#"{"type":"result","subtype":"success","is_error":false,"result":"done","usage":{"input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}"#;
        assert_eq!(
            end_of(&after_a_rate_limit("allowed", success)),
            (EndReason::Completed, None)
        );
    }
}
