//! What claude.ai's data export says about the owner's conversations and Projects: metadata only.
//!
//! The files are inferred (Anthropic documents none). A reader here declares only the fields it
//! keeps, each read leniently, so no message text and no document content is ever materialised, even
//! in memory.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde::de::{self, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};

use crate::claude_history::TITLE_LIMIT;

/// The export shapes with a fixture and a test of their own.
pub const EXPORT_SHAPES: [&str; 1] = ["2026-10"];

/// How many characters of a Project's description are kept.
pub const DESCRIPTION_LIMIT: usize = 2000;

/// The longest id kept.
const UUID_LIMIT: usize = 64;

fn drain_map<'de, A: MapAccess<'de>>(mut map: A) -> Result<(), A::Error> {
    while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
    Ok(())
}

fn drain_seq<'de, A: SeqAccess<'de>>(mut seq: A) -> Result<(), A::Error> {
    while seq.next_element::<IgnoredAny>()?.is_some() {}
    Ok(())
}

/// A text, or nothing for anything that is not one.
struct TextVisitor;

impl<'de> Visitor<'de> for TextVisitor {
    type Value = Option<String>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("anything")
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Some(value.to_string()))
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(self)
    }
    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        drain_map(map).map(|()| None)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
        drain_seq(seq).map(|()| None)
    }
}

fn text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    deserializer.deserialize_any(TextVisitor)
}

/// How many elements a list has, none of them materialised; zero for anything that is not a list.
struct CountVisitor;

impl<'de> Visitor<'de> for CountVisitor {
    type Value = u64;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("anything")
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(0)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(0)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(0)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(0)
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok(0)
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(0)
    }
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(0)
    }
    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(self)
    }
    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        drain_map(map).map(|()| 0)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut count = 0;
        while seq.next_element::<IgnoredAny>()?.is_some() {
            count += 1;
        }
        Ok(count)
    }
}

fn count<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    deserializer.deserialize_any(CountVisitor)
}

/// One document of a Project: its file name alone.
struct DocName(Option<String>);

struct DocNameVisitor;

impl<'de> Visitor<'de> for DocNameVisitor {
    type Value = DocName;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("anything")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut name = None;
        while let Some(key) = map.next_key::<String>()? {
            if key == "filename" {
                name = map.next_value_seed(TextSeed)?;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(DocName(name))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
        drain_seq(seq).map(|()| DocName(None))
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok(DocName(None))
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(DocName(None))
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(DocName(None))
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(DocName(None))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(DocName(None))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(DocName(None))
    }
}

impl<'de> Deserialize<'de> for DocName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(DocNameVisitor)
    }
}

struct TextSeed;

impl<'de> de::DeserializeSeed<'de> for TextSeed {
    type Value = Option<String>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        text(deserializer)
    }
}

/// The file names of a list of documents; none for anything that is not a list.
struct NamesVisitor;

impl<'de> Visitor<'de> for NamesVisitor {
    type Value = Vec<String>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("anything")
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }
    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(self)
    }
    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        drain_map(map).map(|()| Vec::new())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut names = Vec::new();
        while let Some(DocName(name)) = seq.next_element::<DocName>()? {
            names.extend(name);
        }
        Ok(names)
    }
}

fn names<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    deserializer.deserialize_any(NamesVisitor)
}

/// A conversation as `conversations.json` holds it, as far as Catervas reads it. A field that is
/// missing, `null` or of another type reads as absent. Its messages are counted, never read.
#[derive(Deserialize)]
pub struct ExportConversation {
    #[serde(default, deserialize_with = "text")]
    uuid: Option<String>,
    #[serde(default, deserialize_with = "text")]
    name: Option<String>,
    #[serde(default, deserialize_with = "text")]
    created_at: Option<String>,
    #[serde(default, deserialize_with = "text")]
    updated_at: Option<String>,
    #[serde(default, deserialize_with = "count")]
    chat_messages: u64,
}

/// A Project as `projects.json` holds it. Its documents' names are read; their content is not.
#[derive(Deserialize)]
pub struct ExportProject {
    #[serde(default, deserialize_with = "text")]
    uuid: Option<String>,
    #[serde(default, deserialize_with = "text")]
    name: Option<String>,
    #[serde(default, deserialize_with = "text")]
    description: Option<String>,
    #[serde(default, deserialize_with = "text")]
    created_at: Option<String>,
    #[serde(default, deserialize_with = "text")]
    updated_at: Option<String>,
    #[serde(default, deserialize_with = "names")]
    docs: Vec<String>,
}

/// An account as `users.json` holds it: its id alone.
#[derive(Deserialize)]
pub struct ExportUser {
    #[serde(default, deserialize_with = "text")]
    uuid: Option<String>,
}

/// One conversation: its id, title, dates and how many messages it has. Never what was said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationRecord {
    /// Its id, trimmed.
    pub uuid: String,
    /// Its title, trimmed and cut to `TITLE_LIMIT` characters.
    pub title: Option<String>,
    /// When it was started.
    pub created_at: Option<DateTime<Utc>>,
    /// When it was last changed.
    pub updated_at: Option<DateTime<Utc>>,
    /// How many messages it holds.
    pub messages: u64,
}

/// One claude.ai Project: its id, name, description and its documents' file names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeProjectRecord {
    /// Its id, trimmed.
    pub uuid: String,
    /// Its name, trimmed and cut to `TITLE_LIMIT` characters.
    pub name: Option<String>,
    /// Its description, trimmed and cut to `DESCRIPTION_LIMIT` characters.
    pub description: Option<String>,
    /// Its documents' file names, each cut to `TITLE_LIMIT` characters. Never their content.
    pub doc_names: Vec<String>,
    /// When it was started.
    pub created_at: Option<DateTime<Utc>>,
    /// When it was last changed.
    pub updated_at: Option<DateTime<Utc>>,
}

fn uuid_of(raw: Option<String>) -> Option<String> {
    let uuid = raw?.trim().to_string();
    (!uuid.is_empty() && uuid.chars().count() <= UUID_LIMIT).then_some(uuid)
}

fn cut(raw: Option<String>, limit: usize) -> Option<String> {
    let cut: String = raw?.trim().chars().take(limit).collect();
    (!cut.is_empty()).then_some(cut)
}

fn time_of(raw: Option<&str>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw?.trim())
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

/// A conversation's record; `None` when it has no usable id.
#[must_use]
pub fn conversation_record(raw: ExportConversation) -> Option<ConversationRecord> {
    Some(ConversationRecord {
        uuid: uuid_of(raw.uuid)?,
        title: cut(raw.name, TITLE_LIMIT),
        created_at: time_of(raw.created_at.as_deref()),
        updated_at: time_of(raw.updated_at.as_deref()),
        messages: raw.chat_messages,
    })
}

/// A Project's record; `None` when it has no usable id.
#[must_use]
pub fn project_record(raw: ExportProject) -> Option<ClaudeProjectRecord> {
    Some(ClaudeProjectRecord {
        uuid: uuid_of(raw.uuid)?,
        name: cut(raw.name, TITLE_LIMIT),
        description: cut(raw.description, DESCRIPTION_LIMIT),
        doc_names: raw
            .docs
            .into_iter()
            .filter_map(|name| cut(Some(name), TITLE_LIMIT))
            .collect(),
        created_at: time_of(raw.created_at.as_deref()),
        updated_at: time_of(raw.updated_at.as_deref()),
    })
}

/// The account an export belongs to: the first user's id, else `""`.
#[must_use]
pub fn account_of(users: &[ExportUser]) -> String {
    users
        .first()
        .and_then(|user| uuid_of(user.uuid.clone()))
        .unwrap_or_default()
}

/// An export's time is its content's: the latest `updated_at` (else `created_at`) of its items.
#[must_use]
pub fn export_content_at(
    conversations: &[ConversationRecord],
    projects: &[ClaudeProjectRecord],
) -> Option<DateTime<Utc>> {
    conversations
        .iter()
        .map(|c| c.updated_at.or(c.created_at))
        .chain(projects.iter().map(|p| p.updated_at.or(p.created_at)))
        .flatten()
        .max()
}

/// Synthetic export files for tests, in this crate and in others.
pub mod fixtures;

#[cfg(test)]
mod tests {
    use super::fixtures::{EXPORT_2026_10, a_conversation_json};
    use super::*;
    use crate::claude_history::TITLE_LIMIT;
    use chrono::{DateTime, Utc};

    const SHAPES: [(&str, [(&str, &str); 3]); 1] = [("2026-10", EXPORT_2026_10)];

    fn entry<'a>(files: &'a [(&str, &str); 3], name: &str) -> &'a str {
        files.iter().find(|(n, _)| *n == name).expect("an entry").1
    }

    #[allow(clippy::unnecessary_wraps, reason = "compared with optional dates")]
    fn at(text: &str) -> Option<DateTime<Utc>> {
        Some(
            DateTime::parse_from_rfc3339(text)
                .expect("a time")
                .with_timezone(&Utc),
        )
    }

    fn conversations(files: &[(&str, &str); 3]) -> Vec<Option<ConversationRecord>> {
        let raw: Vec<ExportConversation> =
            serde_json::from_str(entry(files, "conversations.json")).expect("a list");
        raw.into_iter().map(conversation_record).collect()
    }

    fn projects(files: &[(&str, &str); 3]) -> Vec<Option<ClaudeProjectRecord>> {
        let raw: Vec<ExportProject> =
            serde_json::from_str(entry(files, "projects.json")).expect("a list");
        raw.into_iter().map(project_record).collect()
    }

    #[test]
    fn reads_each_record_of_a_known_export_shape() {
        assert_eq!(
            SHAPES.map(|(name, _)| name),
            EXPORT_SHAPES,
            "every shape has a fixture"
        );
        for (_, files) in SHAPES {
            let convs = conversations(&files);
            let c1 = convs[0].as_ref().expect("C1");
            assert_eq!(c1.uuid, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
            assert_eq!(c1.title.as_deref(), Some("Plan the shop launch"));
            assert_eq!(c1.created_at, at("2026-09-03T10:00:00Z"));
            assert_eq!(c1.updated_at, at("2026-10-02T08:00:00Z"));
            assert_eq!(c1.messages, 3);
            let c2 = convs[1].as_ref().expect("C2");
            assert_eq!((c2.title.clone(), c2.messages), (None, 0));
            assert_eq!(c2.updated_at, at("2026-10-05T12:00:00Z"));
            assert_eq!(convs[2], None);
            let c4 = convs[3].as_ref().expect("C4");
            assert_eq!(
                (c4.title.clone(), c4.created_at, c4.messages),
                (None, None, 1)
            );
            let projs = projects(&files);
            let p1 = projs[0].as_ref().expect("P1");
            assert_eq!(p1.name.as_deref(), Some("Shop launch"));
            assert_eq!(p1.description.as_deref(), Some("Launch github.com/me/shop"));
            assert_eq!(p1.doc_names, ["brief.md", "prices.csv"]);
            assert_eq!(p1.updated_at, at("2026-10-01T09:00:00Z"));
            let p2 = projs[1].as_ref().expect("P2");
            assert!(p2.doc_names.is_empty());
            let users: Vec<ExportUser> =
                serde_json::from_str(entry(&files, "users.json")).expect("a list");
            assert_eq!(account_of(&users), "ffffffff-ffff-4fff-8fff-ffffffffffff");
            let kept: Vec<ConversationRecord> = convs.into_iter().flatten().collect();
            let kept_projects: Vec<ClaudeProjectRecord> = projs.into_iter().flatten().collect();
            assert_eq!(
                export_content_at(&kept, &kept_projects),
                at("2026-10-05T12:00:00Z")
            );
        }
    }

    /// Guard: checked by mutation (keep a text field), not watched to fail.
    #[test]
    fn keeps_no_export_text() {
        for (_, files) in SHAPES {
            let text = format!("{:?}{:?}", conversations(&files), projects(&files));
            for secret in ["NEVER-KEPT-TEXT", "never-kept@example.invalid"] {
                assert!(!text.contains(secret), "{secret} in {text}");
            }
            let users: Vec<ExportUser> =
                serde_json::from_str(entry(&files, "users.json")).expect("a list");
            assert!(!account_of(&users).contains("never-kept"));
        }
    }

    fn one_conversation(json: &str) -> Option<ConversationRecord> {
        conversation_record(serde_json::from_str(json).expect("an object"))
    }

    #[test]
    fn reads_an_export_leniently() {
        let odd = r#"{"uuid":"u1","chat_messages":"x"}"#;
        assert_eq!(one_conversation(odd).expect("kept").messages, 0);
        let projects = |json: &str| {
            project_record(serde_json::from_str(json).expect("an object")).expect("kept")
        };
        assert!(projects(r#"{"uuid":"p","docs":{}}"#).doc_names.is_empty());
        assert!(one_conversation(&format!(r#"{{"uuid":"{}"}}"#, "x".repeat(65))).is_none());
        assert!(one_conversation(r#"{"uuid":"  "}"#).is_none());
        assert!(one_conversation(r#"{"uuid":7}"#).is_none());
        assert!(one_conversation(&format!(r#"{{"uuid":"{}"}}"#, "x".repeat(64))).is_some());
        let long = a_conversation_json("u", &"n".repeat(300), "2026-01-01T00:00:00Z", "x", 0);
        assert_eq!(
            one_conversation(&long).expect("kept").title,
            Some("n".repeat(TITLE_LIMIT))
        );
        let described = format!(r#"{{"uuid":"p","description":"{}"}}"#, "d".repeat(3000));
        assert_eq!(
            projects(&described).description,
            Some("d".repeat(DESCRIPTION_LIMIT))
        );
        assert_eq!(account_of(&[]), "");
        let undated = vec![one_conversation(r#"{"uuid":"u"}"#).expect("kept")];
        assert_eq!(export_content_at(&undated, &[]), None);
        let created = vec![
            one_conversation(r#"{"uuid":"u","created_at":"2026-03-01T00:00:00Z"}"#).expect("kept"),
        ];
        assert_eq!(export_content_at(&created, &[]), at("2026-03-01T00:00:00Z"));
        let both = vec![
            one_conversation(
                r#"{"uuid":"u","created_at":"2026-03-01T00:00:00Z","updated_at":"2026-04-01T00:00:00+02:00"}"#,
            )
            .expect("kept"),
        ];
        assert_eq!(export_content_at(&both, &[]), at("2026-03-31T22:00:00Z"));
    }
}
