//! Synthetic claude.ai export files for tests, in this crate and in others. None holds a real
//! conversation or a real person: every text field is `NEVER-KEPT-TEXT`, and the shape is inferred,
//! since Anthropic documents none.

use serde_json::json;

/// The files of a hand-written export in the shape this version reads, as `(entry name, text)`:
/// four conversations (the third has no id, the fourth no readable name or date) and two Projects.
pub const EXPORT_2026_10: [(&str, &str); 3] = [
    (
        "conversations.json",
        include_str!("fixtures/2026_10/conversations.json"),
    ),
    (
        "projects.json",
        include_str!("fixtures/2026_10/projects.json"),
    ),
    ("users.json", include_str!("fixtures/2026_10/users.json")),
];

/// One conversation as `conversations.json` holds it, with `messages` messages of fixed text.
#[must_use]
pub fn a_conversation_json(
    uuid: &str,
    name: &str,
    created_at: &str,
    updated_at: &str,
    messages: usize,
) -> String {
    let chat_messages: Vec<_> = (0..messages)
        .map(|_| {
            json!({
                "sender": "human", "text": "NEVER-KEPT-TEXT",
                "content": [{"type": "text", "text": "NEVER-KEPT-TEXT"}],
                "attachments": [{"file_name": "NEVER-KEPT-TEXT", "extracted_content": "NEVER-KEPT-TEXT"}],
            })
        })
        .collect();
    json!({
        "uuid": uuid, "name": name, "created_at": created_at, "updated_at": updated_at,
        "chat_messages": chat_messages,
    })
    .to_string()
}

/// One Project as `projects.json` holds it, each document's content `NEVER-KEPT-TEXT`.
#[must_use]
pub fn a_project_json(uuid: &str, name: &str, updated_at: &str, doc_names: &[&str]) -> String {
    let docs: Vec<_> = doc_names
        .iter()
        .map(|filename| json!({"filename": filename, "content": "NEVER-KEPT-TEXT"}))
        .collect();
    json!({
        "uuid": uuid, "name": name, "description": "", "created_at": updated_at,
        "updated_at": updated_at, "prompt_template": "NEVER-KEPT-TEXT", "docs": docs,
    })
    .to_string()
}

/// `users.json` for one account, whose name and email are fixed fakes.
#[must_use]
pub fn a_users_json(uuid: &str) -> String {
    json!([{
        "uuid": uuid, "full_name": "NEVER-KEPT-TEXT", "email_address": "never-kept@example.invalid",
    }])
    .to_string()
}
