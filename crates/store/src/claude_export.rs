//! Reading claude.ai's data export into the machine index, from the zip, without extracting it.
//!
//! Only a `.zip` is read. Its entries are matched by `enclosed_name()`, at the root or under exactly
//! one top-level folder; a directory, a link or a name that would leave the folder never matches.
//! Each matched entry is streamed into the JSON reader through a counting reader: nothing is written
//! to disk but `machine.db`, and no message text or document content is kept, even in memory.

use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Component, Path};

use catervas_core::claude_export::{
    ClaudeProjectRecord, ConversationRecord, ExportConversation, ExportProject, ExportUser,
    account_of, conversation_record, export_content_at, project_record,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde::de::{self, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use zip::ZipArchive;
use zip::result::ZipError;

use crate::error::StoreError;
use crate::machine::MachineIndex;

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;
const GIB: u64 = 1024 * MIB;

/// How much of an export Catervas reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportLimits {
    /// The most the zip file may hold.
    pub file_bytes: u64,
    /// The most one matched entry may hold once unpacked, by its header and by the bytes read.
    pub entry_bytes: u64,
}

impl ExportLimits {
    /// 2 GiB for the zip and 1 GiB for each entry unpacked.
    pub const DEFAULT: ExportLimits = ExportLimits {
        file_bytes: 2 * GIB,
        entry_bytes: GIB,
    };
}

/// What one import did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportSummary {
    /// Conversations read.
    pub conversations: u64,
    /// Those not in the index before.
    pub new_conversations: u64,
    /// Projects read.
    pub projects: u64,
    /// Those not in the index before.
    pub new_projects: u64,
    /// Conversations and Projects left out for having no usable id.
    pub skipped: u64,
    /// How many of those were Projects.
    pub skipped_projects: u64,
    /// The latest time the export's content holds.
    pub content_at: DateTime<Utc>,
}

/// Why an export was not imported. Nothing is written when it is not.
#[derive(Debug)]
pub enum ExportError {
    /// The file is not a zip.
    NotAZip,
    /// The zip is not claude.ai's export, for the reason given.
    NotAnExport(&'static str),
    /// An entry that should be a list is not.
    NotAList {
        /// The entry's name.
        entry: &'static str,
    },
    /// An entry is not JSON, is cut short, or holds an element that is not an object.
    Unreadable {
        /// The entry's name.
        entry: &'static str,
    },
    /// An entry does not match its checksum.
    Damaged {
        /// The entry's name.
        entry: &'static str,
    },
    /// An entry uses a compression method this reader does not have.
    Unsupported {
        /// The entry's name.
        entry: &'static str,
    },
    /// An entry is password-protected.
    Encrypted {
        /// The entry's name.
        entry: &'static str,
    },
    /// The zip or an entry is larger than `ExportLimits` allows.
    TooLarge {
        /// The limit that was passed.
        limit_bytes: u64,
    },
    /// Nothing in the export has a date.
    NothingDated,
    /// The file could not be read.
    Io(std::io::Error),
    /// The index refused.
    Store(StoreError),
}

fn size_words(bytes: u64) -> String {
    for (unit, name) in [(GIB, "GiB"), (MIB, "MiB"), (KIB, "KiB")] {
        if bytes >= unit && bytes.is_multiple_of(unit) {
            return format!("{} {name}", bytes / unit);
        }
    }
    format!("{bytes} bytes")
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAZip => formatter.write_str(
                "this isn't claude.ai's export: it isn't a zip file; give Catervas the .zip claude.ai emailed you",
            ),
            Self::NotAnExport(reason) => {
                write!(formatter, "this isn't claude.ai's export: {reason}")
            }
            Self::NotAList { entry } => write!(
                formatter,
                "this isn't claude.ai's export as Catervas knows it: {entry} isn't a list"
            ),
            Self::Unreadable { entry } => write!(
                formatter,
                "this isn't claude.ai's export as Catervas knows it: {entry} couldn't be read"
            ),
            Self::Damaged { entry } => write!(
                formatter,
                "the export is damaged: {entry} doesn't match its checksum; download it again"
            ),
            Self::Unsupported { entry } => write!(
                formatter,
                "this export uses a kind of zip Catervas can't read ({entry})"
            ),
            Self::Encrypted { entry } => write!(
                formatter,
                "{entry} in this export is password-protected; give Catervas the export as claude.ai sent it"
            ),
            Self::TooLarge { limit_bytes } => write!(
                formatter,
                "this export is larger than Catervas reads (over {})",
                size_words(*limit_bytes)
            ),
            Self::NothingDated => formatter
                .write_str("this export has no conversations or Projects Catervas can read"),
            Self::Io(error) => error.fmt(formatter),
            Self::Store(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<StoreError> for ExportError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<std::io::Error> for ExportError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// A reader that counts what passes and refuses to pass more than its limit, and remembers how the
/// inner reader failed, so that a checksum failure is told from a limit and from a broken stream.
struct Counted<R> {
    inner: R,
    read: u64,
    limit: u64,
    exceeded: bool,
    checksum_failed: bool,
}

impl<R: Read> Read for Counted<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = match self.inner.read(buffer) {
            Ok(count) => count,
            Err(error) => {
                self.checksum_failed = error.to_string().contains("checksum");
                return Err(error);
            }
        };
        self.read += count as u64;
        if self.read > self.limit {
            self.exceeded = true;
            return Err(std::io::Error::other("over the limit"));
        }
        Ok(count)
    }
}

/// A list of `T`, or `None` for any other kind of value, which is drained.
struct List<T>(Option<Vec<T>>);

struct ListVisitor<T>(std::marker::PhantomData<T>);

impl<'de, T: Deserialize<'de>> Visitor<'de> for ListVisitor<T> {
    type Value = List<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("anything")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element::<T>()? {
            items.push(item);
        }
        Ok(List(Some(items)))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(List(None))
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(List(None))
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(List(None))
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(List(None))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(List(None))
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok(List(None))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(List(None))
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for List<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ListVisitor(std::marker::PhantomData))
    }
}

/// Whether a name starts at a root or a drive. The zip crate's `enclosed_name` lets such a name
/// through as if it were relative, so it is refused here, before a name is matched.
fn is_absolute(name: &str) -> bool {
    let bytes = name.as_bytes();
    name.starts_with(['/', '\\']) || (bytes.get(1) == Some(&b':') && bytes[0].is_ascii_alphabetic())
}

/// Where an entry named `target` is: at the root, else under exactly one top-level folder.
fn find_entry<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    target: &'static str,
) -> Result<Option<usize>, ExportError> {
    let (mut at_root, mut nested) = (Vec::new(), Vec::new());
    for index in 0..archive.len() {
        let Ok(file) = archive.by_index_raw(index) else {
            continue;
        };
        if file.is_dir() || file.is_symlink() || is_absolute(file.name()) {
            continue;
        }
        let Some(path) = file.enclosed_name() else {
            continue;
        };
        let names: Vec<_> = path
            .components()
            .filter_map(|part| match part {
                Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        match names.as_slice() {
            [name] if name == target => at_root.push(index),
            [_, name] if name == target => nested.push(index),
            _ => {}
        }
    }
    let found = if at_root.is_empty() { nested } else { at_root };
    match found.as_slice() {
        [] => Ok(None),
        [only] => Ok(Some(*only)),
        _ => Err(ExportError::NotAnExport(match target {
            "conversations.json" => "it has more than one conversations.json",
            "projects.json" => "it has more than one projects.json",
            _ => "it has more than one users.json",
        })),
    }
}

/// Streams one entry into a list of `T`. `None` when the entry is not in the zip.
fn read_list<R: Read + std::io::Seek, T: for<'de> Deserialize<'de>>(
    archive: &mut ZipArchive<R>,
    index: Option<usize>,
    entry: &'static str,
    limits: ExportLimits,
) -> Result<Vec<T>, ExportError> {
    let Some(index) = index else {
        return Ok(Vec::new());
    };
    let file = match archive.by_index(index) {
        Ok(file) => file,
        Err(ZipError::UnsupportedArchive(why)) if why == ZipError::PASSWORD_REQUIRED => {
            return Err(ExportError::Encrypted { entry });
        }
        Err(ZipError::InvalidPassword) => return Err(ExportError::Encrypted { entry }),
        Err(ZipError::UnsupportedArchive(_) | ZipError::CompressionMethodNotSupported(_)) => {
            return Err(ExportError::Unsupported { entry });
        }
        Err(ZipError::Io(error)) => return Err(error.into()),
        Err(_) => return Err(ExportError::Unreadable { entry }),
    };
    if file.size() > limits.entry_bytes {
        return Err(ExportError::TooLarge {
            limit_bytes: limits.entry_bytes,
        });
    }
    let mut counted = Counted {
        inner: file,
        read: 0,
        limit: limits.entry_bytes,
        exceeded: false,
        checksum_failed: false,
    };
    let parsed = serde_json::from_reader::<_, List<T>>(BufReader::new(&mut counted));
    if counted.exceeded {
        return Err(ExportError::TooLarge {
            limit_bytes: limits.entry_bytes,
        });
    }
    if counted.checksum_failed {
        return Err(ExportError::Damaged { entry });
    }
    match parsed {
        Ok(List(Some(items))) => Ok(items),
        Ok(List(None)) => Err(ExportError::NotAList { entry }),
        Err(_) => Err(ExportError::Unreadable { entry }),
    }
}

/// Reads claude.ai's data export, a `.zip`, into the index. Anything an earlier export of the same
/// account held is updated by uuid and nothing is deleted.
///
/// # Errors
///
/// An `ExportError`, with nothing written, for a file that is not a zip, not claude.ai's export,
/// damaged, encrypted, of a kind this cannot read, too large for `limits`, or with nothing dated;
/// `Io` when the file cannot be opened; `Store` when the index refuses.
pub fn import_claude_export(
    index: &mut MachineIndex,
    file: &Path,
    limits: ExportLimits,
    now: DateTime<Utc>,
) -> Result<ExportSummary, ExportError> {
    let file = File::open(file)?;
    if file.metadata()?.len() > limits.file_bytes {
        return Err(ExportError::TooLarge {
            limit_bytes: limits.file_bytes,
        });
    }
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(|error| match error {
        ZipError::Io(error) => ExportError::Io(error),
        _ => ExportError::NotAZip,
    })?;
    let conversations_at = find_entry(&mut archive, "conversations.json")?;
    let projects_at = find_entry(&mut archive, "projects.json")?;
    let users_at = find_entry(&mut archive, "users.json")?;
    if conversations_at.is_none() && projects_at.is_none() {
        return Err(ExportError::NotAnExport(
            "it has neither conversations.json nor projects.json",
        ));
    }
    let raw_conversations: Vec<ExportConversation> =
        read_list(&mut archive, conversations_at, "conversations.json", limits)?;
    let raw_projects: Vec<ExportProject> =
        read_list(&mut archive, projects_at, "projects.json", limits)?;
    let users: Vec<ExportUser> = read_list(&mut archive, users_at, "users.json", limits)?;
    let (read_conversations, read_projects) = (raw_conversations.len(), raw_projects.len());
    let conversations: Vec<ConversationRecord> = raw_conversations
        .into_iter()
        .filter_map(conversation_record)
        .collect();
    let projects: Vec<ClaudeProjectRecord> = raw_projects
        .into_iter()
        .filter_map(project_record)
        .collect();
    let content_at =
        export_content_at(&conversations, &projects).ok_or(ExportError::NothingDated)?;
    let account = account_of(&users);
    let (new_conversations, new_projects) =
        index.record_claude_export(&account, content_at, &conversations, &projects, now)?;
    let skipped_projects = (read_projects - projects.len()) as u64;
    Ok(ExportSummary {
        conversations: conversations.len() as u64,
        new_conversations,
        projects: projects.len() as u64,
        new_projects,
        skipped: (read_conversations - conversations.len()) as u64 + skipped_projects,
        skipped_projects,
        content_at,
    })
}

/// Zips of an export for tests, in this crate and in others.
pub mod fixtures;

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use catervas_core::claude_export::fixtures::{
        EXPORT_2026_10, a_conversation_json, a_project_json, a_users_json,
    };
    use chrono::{DateTime, TimeZone, Utc};
    use rusqlite::Connection;

    use super::fixtures::{
        ZipPatch, a_patched_zip, a_zip_with_a_symlink, an_export_zip, the_2026_10_export,
    };
    use super::*;
    use crate::files::fixtures::TempProject;
    use crate::machine::{MACHINE_DB, MachineIndex, open_machine_index};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 21, 12, 0, 0)
            .single()
            .expect("a valid time")
    }

    /// A folder named after the test, an index in it and a zip written next to the index.
    struct Setup {
        temp: TempProject,
        index: MachineIndex,
    }

    impl Setup {
        fn new(name: &str) -> Self {
            let temp = TempProject::new(name);
            let index = open_machine_index(&temp.root.join(MACHINE_DB), now()).expect("an index");
            Self { temp, index }
        }

        fn zip(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.temp.root.join(name);
            std::fs::write(&path, bytes).expect("a zip");
            path
        }

        fn import(&mut self, name: &str, bytes: &[u8]) -> Result<ExportSummary, ExportError> {
            self.import_within(name, bytes, ExportLimits::DEFAULT)
        }

        fn import_within(
            &mut self,
            name: &str,
            bytes: &[u8],
            limits: ExportLimits,
        ) -> Result<ExportSummary, ExportError> {
            let path = self.zip(name, bytes);
            import_claude_export(&mut self.index, &path, limits, now())
        }

        fn database(&self) -> Connection {
            Connection::open(self.temp.root.join(MACHINE_DB)).expect("the file opens")
        }

        fn rows(&self) -> i64 {
            let connection = self.database();
            ["claude_exports", "claude_conversations", "claude_projects"]
                .iter()
                .map(|table| {
                    connection
                        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                            row.get::<_, i64>(0)
                        })
                        .expect("a table")
                })
                .sum()
        }
    }

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .expect("a time")
            .with_timezone(&Utc)
    }

    const C1: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const USER: &str = "ffffffff-ffff-4fff-8fff-ffffffffffff";

    #[test]
    fn imports_a_known_export_shape() {
        let mut setup = Setup::new("imports-known-shape");
        let summary = setup
            .import("export.zip", &the_2026_10_export())
            .expect("the export imports");
        assert_eq!(
            (
                summary.conversations,
                summary.new_conversations,
                summary.projects,
                summary.new_projects,
                summary.skipped
            ),
            (3, 3, 2, 2, 1)
        );
        assert_eq!(summary.content_at, at("2026-10-05T12:00:00Z"));
        let connection = setup.database();
        let row: (
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
            String,
        ) = connection
            .query_row(
                "SELECT account, title, created_at, updated_at, messages, seen_in_export_at
                 FROM claude_conversations WHERE uuid = ?1",
                (C1,),
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .expect("C1");
        assert_eq!(
            row,
            (
                USER.to_string(),
                Some("Plan the shop launch".to_string()),
                Some("2026-09-03T10:00:00.000Z".to_string()),
                Some("2026-10-02T08:00:00.000Z".to_string()),
                3,
                "2026-10-05T12:00:00.000Z".to_string()
            )
        );
        let untitled: (Option<String>, Option<String>) = connection
            .query_row(
                "SELECT title, created_at FROM claude_conversations WHERE messages = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("C4");
        assert_eq!(untitled, (None, None));
        let project: (String, Option<String>, String, Option<String>) = connection
            .query_row(
                "SELECT name, description, doc_names, updated_at FROM claude_projects
                 WHERE name = 'Shop launch'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("P1");
        assert_eq!(
            project,
            (
                "Shop launch".to_string(),
                Some("Launch github.com/me/shop".to_string()),
                r#"["brief.md","prices.csv"]"#.to_string(),
                Some("2026-10-01T09:00:00.000Z".to_string())
            )
        );
        let exports: (String, String, String) = connection
            .query_row(
                "SELECT account, content_at, imported_at FROM claude_exports",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("one export");
        assert_eq!(exports.0, USER);
        assert_eq!(exports.1, "2026-10-05T12:00:00.000Z");
        assert_eq!(setup.rows(), 1 + 3 + 2);
        assert!(
            setup
                .index
                .applied_migrations()
                .expect("versions")
                .contains(&4),
            "the claude.ai tables come with migration 4"
        );
    }

    /// Guard: checked by mutation (keep a text field), not watched to fail.
    #[test]
    fn keeps_no_export_text() {
        let mut setup = Setup::new("keeps-no-export-text");
        setup
            .import("export.zip", &the_2026_10_export())
            .expect("the export imports");
        drop(setup.index);
        let bytes = std::fs::read(setup.temp.root.join(MACHINE_DB)).expect("the file");
        for secret in ["NEVER-KEPT-TEXT", "never-kept@example.invalid"] {
            assert!(
                !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
                "{secret} in machine.db"
            );
        }
    }

    fn entries_under<'a>(folder: &str) -> Vec<(String, &'a [u8])> {
        EXPORT_2026_10
            .iter()
            .map(|(name, text)| (format!("{folder}{name}"), text.as_bytes()))
            .collect()
    }

    fn zip_of(entries: &[(String, &[u8])]) -> Vec<u8> {
        let borrowed: Vec<(&str, &[u8])> = entries.iter().map(|(n, b)| (n.as_str(), *b)).collect();
        an_export_zip(&borrowed)
    }

    #[test]
    fn finds_the_export_inside_one_folder() {
        let mut setup = Setup::new("finds-export-in-folder");
        let summary = setup
            .import("folder.zip", &zip_of(&entries_under("claude-export/")))
            .expect("the export imports");
        assert_eq!((summary.conversations, summary.projects), (3, 2));
        let mut twice = entries_under("a/");
        twice.extend(entries_under("b/"));
        let mut other = Setup::new("finds-export-in-two-folders");
        let error = other
            .import("two.zip", &zip_of(&twice))
            .expect_err("two exports");
        assert!(matches!(
            error,
            ExportError::NotAnExport("it has more than one conversations.json")
        ));
        assert_eq!(other.rows(), 0);
    }

    fn big_conversations(count: usize) -> Vec<u8> {
        let items: Vec<String> = (0..count)
            .map(|n| {
                a_conversation_json(
                    &format!("{n:08}-0000-4000-8000-000000000000"),
                    "A title",
                    "2026-10-01T00:00:00Z",
                    "2026-10-02T00:00:00Z",
                    1,
                )
            })
            .collect();
        format!("[{}]", items.join(",")).into_bytes()
    }

    #[test]
    fn refuses_what_is_not_a_claude_export() {
        let one = |entries: &[(&str, &[u8])]| an_export_zip(entries);
        let cut = {
            let text = EXPORT_2026_10[0].1;
            text.as_bytes()[..text.len() / 2].to_vec()
        };
        let only_c3 = format!(
            "[{}]",
            a_conversation_json("", "x", "2026-10-01T00:00:00Z", "2026-10-02T00:00:00Z", 1)
        );
        let contents = EXPORT_2026_10[0].1.as_bytes();
        let cases: Vec<(&str, Vec<u8>, &str)> = vec![
            (
                "text",
                b"just some words, not a zip".to_vec(),
                "this isn't claude.ai's export: it isn't a zip file; give Catervas the .zip claude.ai emailed you",
            ),
            (
                "notes",
                one(&[("notes.txt", b"hello")]),
                "this isn't claude.ai's export: it has neither conversations.json nor projects.json",
            ),
            (
                "conversations-map",
                one(&[("conversations.json", b"{}")]),
                "this isn't claude.ai's export as Catervas knows it: conversations.json isn't a list",
            ),
            (
                "projects-map",
                one(&[("projects.json", b"{}")]),
                "this isn't claude.ai's export as Catervas knows it: projects.json isn't a list",
            ),
            (
                "users-map",
                one(&[("conversations.json", b"[]"), ("users.json", b"{}")]),
                "this isn't claude.ai's export as Catervas knows it: users.json isn't a list",
            ),
            (
                "cut",
                one(&[("conversations.json", &cut)]),
                "this isn't claude.ai's export as Catervas knows it: conversations.json couldn't be read",
            ),
            (
                "not-objects",
                one(&[("projects.json", b"[1, 2]")]),
                "this isn't claude.ai's export as Catervas knows it: projects.json couldn't be read",
            ),
            (
                "undated",
                one(&[("conversations.json", only_c3.as_bytes())]),
                "this export has no conversations or Projects Catervas can read",
            ),
            (
                "checksum",
                a_patched_zip("conversations.json", contents, ZipPatch::BadChecksum),
                "the export is damaged: conversations.json doesn't match its checksum; download it again",
            ),
            // Method 12 is bzip2, which this reader is built without. (99 is WinZip's AES marker, which
            // the zip crate refuses on opening the archive, as a zip it cannot read.)
            (
                "method",
                a_patched_zip("conversations.json", contents, ZipPatch::Method(12)),
                "this export uses a kind of zip Catervas can't read (conversations.json)",
            ),
            (
                "encrypted",
                a_patched_zip("conversations.json", contents, ZipPatch::Encrypted),
                "conversations.json in this export is password-protected; give Catervas the export as claude.ai sent it",
            ),
        ];
        for (name, bytes, sentence) in cases {
            let mut setup = Setup::new(&format!("refuses-{name}"));
            let error = setup
                .import("export.zip", &bytes)
                .expect_err(&format!("{name} is refused"));
            assert_eq!(error.to_string(), sentence, "{name}");
            assert_eq!(setup.rows(), 0, "{name} wrote nothing");
        }
    }

    fn folder_holds_only(root: &Path, zips: &[&str]) -> bool {
        let mut names: Vec<String> = std::fs::read_dir(root)
            .expect("the folder")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        let mut expected: Vec<String> = zips.iter().map(ToString::to_string).collect();
        expected.push(MACHINE_DB.to_string());
        expected.sort();
        names == expected
    }

    #[test]
    fn reads_a_malicious_zip_safely() {
        let honest = big_conversations(100);
        assert!(honest.len() > 8 * 1024);
        let small = ExportLimits {
            file_bytes: ExportLimits::DEFAULT.file_bytes,
            entry_bytes: 4 * 1024,
        };
        let name = "export.zip";
        let paths = [
            an_export_zip(&[("../conversations.json", b"[]")]),
            an_export_zip(&[("/conversations.json", b"[]")]),
            a_zip_with_a_symlink("conversations.json", "/etc/passwd"),
        ];
        for (n, bytes) in paths.iter().enumerate() {
            let mut setup = Setup::new(&format!("malicious-path-{n}"));
            let error = setup.import(name, bytes).expect_err("refused");
            assert!(
                matches!(error, ExportError::NotAnExport(_)),
                "{n}: {error:?}"
            );
            assert_eq!(setup.rows(), 0);
            assert!(folder_holds_only(&setup.temp.root, &[name]));
        }
        let too_large = [
            (
                "honest",
                an_export_zip(&[("conversations.json", &honest)]),
                small,
            ),
            (
                "lying",
                a_patched_zip("conversations.json", &honest, ZipPatch::DeclaredSize(100)),
                small,
            ),
            (
                "file",
                a_patched_zip("conversations.json", &honest, ZipPatch::DeclaredSize(100)),
                ExportLimits {
                    file_bytes: 1024,
                    entry_bytes: ExportLimits::DEFAULT.entry_bytes,
                },
            ),
        ];
        for (label, bytes, limits) in too_large {
            let mut setup = Setup::new(&format!("malicious-size-{label}"));
            let error = setup
                .import_within(name, &bytes, limits)
                .expect_err("too large");
            let limit = if label == "file" { 1024 } else { 4096 };
            assert!(
                matches!(error, ExportError::TooLarge { limit_bytes } if limit_bytes == limit),
                "{label}: {error:?}"
            );
            let words = if label == "file" { "1 KiB" } else { "4 KiB" };
            assert_eq!(
                error.to_string(),
                format!("this export is larger than Catervas reads (over {words})")
            );
            assert_eq!(setup.rows(), 0);
            assert!(folder_holds_only(&setup.temp.root, &[name]));
        }
    }

    fn conversations_json(items: &[(&str, &str, &str, &str, usize)]) -> String {
        let parts: Vec<String> = items
            .iter()
            .map(|(uuid, name, created, updated, n)| {
                a_conversation_json(uuid, name, created, updated, *n)
            })
            .collect();
        format!("[{}]", parts.join(","))
    }

    fn export_of(account: &str, conversations: &str, projects: &str) -> Vec<u8> {
        let users = a_users_json(account);
        an_export_zip(&[
            ("conversations.json", conversations.as_bytes()),
            ("projects.json", projects.as_bytes()),
            ("users.json", users.as_bytes()),
        ])
    }

    const U1: &str = "11111111-1111-4111-8111-111111111111";
    const U2: &str = "22222222-2222-4222-8222-222222222222";
    const P1: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";

    fn id(letter: char) -> String {
        let l = letter.to_string();
        format!(
            "{0}{0}{0}{0}{0}{0}{0}{0}-{0}{0}{0}{0}-4{0}{0}{0}-8{0}{0}{0}-{1}",
            l,
            l.repeat(12)
        )
    }

    fn export_a() -> Vec<u8> {
        export_of(
            U1,
            &conversations_json(&[
                (
                    &id('a'),
                    "Old title",
                    "2026-09-03T10:00:00Z",
                    "2026-10-02T08:00:00Z",
                    2,
                ),
                (
                    &id('b'),
                    "Second",
                    "2026-10-05T12:00:00Z",
                    "2026-10-05T12:00:00Z",
                    1,
                ),
            ]),
            &format!(
                "[{}]",
                a_project_json(P1, "Shop launch", "2026-10-01T09:00:00Z", &["brief.md"])
            ),
        )
    }

    fn export_b() -> Vec<u8> {
        export_of(
            U1,
            &conversations_json(&[
                (
                    &id('a'),
                    "New title",
                    "2026-09-03T10:00:00Z",
                    "2026-10-15T08:00:00Z",
                    5,
                ),
                (
                    &id('e'),
                    "Fifth",
                    "2026-10-20T12:00:00Z",
                    "2026-10-20T12:00:00Z",
                    1,
                ),
            ]),
            &format!(
                "[{}]",
                a_project_json(
                    P1,
                    "Shop launch",
                    "2026-10-16T09:00:00Z",
                    &["brief.md", "prices.csv"]
                )
            ),
        )
    }

    fn dump(connection: &Connection) -> Vec<String> {
        let mut lines = Vec::new();
        for table in ["claude_exports", "claude_conversations", "claude_projects"] {
            let mut statement = connection
                .prepare(&format!("SELECT * FROM {table} ORDER BY 1, 2"))
                .expect("a statement");
            let columns = statement.column_count();
            let mut rows = statement.query([]).expect("a query");
            while let Some(row) = rows.next().expect("a row") {
                let cells: Vec<String> = (0..columns)
                    .map(|n| format!("{:?}", row.get_ref(n).expect("a cell")))
                    .collect();
                lines.push(format!("{table}: {}", cells.join(" | ")));
            }
        }
        lines
    }

    #[test]
    fn updates_an_earlier_export_by_uuid() {
        let mut setup = Setup::new("updates-by-uuid");
        setup.import("a.zip", &export_a()).expect("A");
        let summary = setup.import("b.zip", &export_b()).expect("B");
        assert_eq!(
            (summary.new_conversations, summary.new_projects),
            (1, 0),
            "C5 is new, C1 and P1 are updated"
        );
        let title: String = setup
            .database()
            .query_row(
                "SELECT title FROM claude_conversations WHERE uuid = ?1",
                (id('a'),),
                |row| row.get(0),
            )
            .expect("C1");
        assert_eq!(title, "New title");
        let found = setup.index.found().expect("found").claude_ai;
        assert_eq!(found.conversations, 3);
        assert_eq!(found.conversations_not_in_latest, 1);
        assert_eq!(found.projects_not_in_latest, 0);
        assert_eq!(found.latest_export_at, Some(at("2026-10-20T12:00:00Z")));
        assert_eq!(found.projects.len(), 1);
        assert_eq!(found.projects[0].doc_names, ["brief.md", "prices.csv"]);
        assert!(found.projects[0].in_latest_export);
        let before = dump(&setup.database());
        let again = setup.import("b-again.zip", &export_b()).expect("B again");
        assert_eq!((again.new_conversations, again.new_projects), (0, 0));
        assert_eq!(dump(&setup.database()), before);
    }

    #[test]
    fn reads_overlapping_exports_the_same_in_either_order() {
        let mut forward = Setup::new("overlap-forward");
        forward.import("a.zip", &export_a()).expect("A");
        forward.import("b.zip", &export_b()).expect("B");
        let mut backward = Setup::new("overlap-backward");
        backward.import("b.zip", &export_b()).expect("B");
        backward.import("a.zip", &export_a()).expect("A");
        let rows = |setup: &Setup| {
            let mut lines = dump(&setup.database());
            // When an export was imported is the one thing that follows the order.
            lines.retain(|line| !line.starts_with("claude_exports"));
            for line in &mut lines {
                if let Some((head, _)) = line.rsplit_once(" | ") {
                    *line = head.to_string();
                }
            }
            lines
        };
        assert_eq!(rows(&forward), rows(&backward));
        assert_eq!(
            forward.index.found().expect("found").claude_ai,
            backward.index.found().expect("found").claude_ai
        );
    }

    #[test]
    fn marks_nothing_for_an_export_that_only_lost_items() {
        let mut setup = Setup::new("only-lost-items");
        setup.import("a.zip", &export_a()).expect("A");
        let fewer = export_of(
            U1,
            &conversations_json(&[(
                &id('b'),
                "Second",
                "2026-10-05T12:00:00Z",
                "2026-10-05T12:00:00Z",
                1,
            )]),
            &format!(
                "[{}]",
                a_project_json(P1, "Shop launch", "2026-10-01T09:00:00Z", &["brief.md"])
            ),
        );
        setup.import("fewer.zip", &fewer).expect("A without C1");
        let found = setup.index.found().expect("found").claude_ai;
        assert_eq!(found.conversations, 2);
        assert_eq!(found.conversations_not_in_latest, 0);
    }

    #[test]
    fn keeps_each_accounts_exports_apart() {
        let mut setup = Setup::new("accounts-apart");
        setup.import("a.zip", &export_a()).expect("A");
        let other = export_of(
            U2,
            &conversations_json(&[(
                &id('9'),
                "Other",
                "2026-11-01T00:00:00Z",
                "2026-11-01T00:00:00Z",
                1,
            )]),
            "[]",
        );
        setup
            .import("other.zip", &other)
            .expect("the other account");
        let found = setup.index.found().expect("found").claude_ai;
        assert_eq!(found.conversations, 3);
        assert_eq!(found.conversations_not_in_latest, 0);
        assert_eq!(found.latest_export_at, Some(at("2026-11-01T00:00:00Z")));
    }

    #[test]
    fn counts_conversations_by_month() {
        let mut setup = Setup::new("counts-by-month");
        let early = export_of(
            U1,
            &conversations_json(&[
                (
                    &id('x'),
                    "Only early",
                    "2026-07-05T00:00:00Z",
                    "2026-07-06T00:00:00Z",
                    1,
                ),
                (
                    &id('y'),
                    "Both",
                    "2026-07-20T00:00:00Z",
                    "2026-07-21T00:00:00Z",
                    1,
                ),
            ]),
            "[]",
        );
        setup.import("early.zip", &early).expect("the early export");
        let late = export_of(
            U1,
            &conversations_json(&[
                (
                    &id('y'),
                    "Both",
                    "2026-07-20T00:00:00Z",
                    "2026-07-21T00:00:00Z",
                    1,
                ),
                (
                    &id('o'),
                    "October",
                    "2026-10-02T00:00:00Z",
                    "2026-10-03T00:00:00Z",
                    1,
                ),
                (
                    &id('p'),
                    "A year ago",
                    "2025-11-30T23:59:59Z",
                    "2026-10-04T00:00:00Z",
                    1,
                ),
                (
                    &id('q'),
                    "Too old",
                    "2025-10-31T23:59:59Z",
                    "2026-10-04T00:00:00Z",
                    1,
                ),
                (&id('u'), "Undated", "", "", 1),
            ]),
            "[]",
        );
        setup.import("late.zip", &late).expect("the late export");
        let found = setup.index.found().expect("found").claude_ai;
        let months: Vec<(&str, u64)> = found
            .by_month
            .iter()
            .map(|m| (m.month.as_str(), m.conversations))
            .collect();
        assert_eq!(
            months,
            [
                ("2026-10", 1),
                ("2026-09", 0),
                ("2026-08", 0),
                ("2026-07", 2),
                ("2026-06", 0),
                ("2026-05", 0),
                ("2026-04", 0),
                ("2026-03", 0),
                ("2026-02", 0),
                ("2026-01", 0),
                ("2025-12", 0),
                ("2025-11", 1),
            ]
        );
        assert_eq!((found.earlier, found.undated), (1, 1));
        assert_eq!(found.conversations, 6);
        assert_eq!(found.conversations_not_in_latest, 1, "only the early one");
    }
}
