//! Checking a skill a user or a kit adds, against the Agent Skills format and Catervas's limits
//! (`docs/SPEC.md` 6.7, ADR 0034). Pure: the caller reads the folder.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::LazyLock;

use catervas_core::contract::Role;
use serde::Deserialize;
use serde_json::Value;

/// A skill that passed `check_skill`, ready to be copied into a session's plugin folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedSkill {
    /// The skill's name, equal to its folder's.
    pub name: String,
    /// When the skill applies, as its frontmatter says.
    pub description: String,
    /// The frontmatter's other keys, in file order: Claude Code would act on them, so the session's
    /// copy drops them and the person is told.
    pub ignored_fields: Vec<String>,
    /// Every file as text: `SKILL.md` with its frontmatter rewritten to `name` and `description`
    /// alone, the rest unchanged.
    pub session_files: BTreeMap<String, String>,
}

/// Why a skill is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillRefusal {
    /// No `SKILL.md`, no frontmatter, a frontmatter that is not a mapping, or no `name`.
    FrontmatterInvalid,
    /// The name is not lower-case words joined by single hyphens, or is over 64 characters.
    NameInvalid,
    /// The name is not the folder's.
    NameMismatch,
    /// The description is missing, not text, empty, or over 1024 characters.
    DescriptionInvalid,
    /// `SKILL.md` runs a command when loaded.
    RunsCommands,
    /// `SKILL.md` attaches a file with `@`.
    AttachesFiles,
    /// `SKILL.md`, a file or the folder is over its limit.
    TooLarge,
    /// More than 16 files.
    TooManyFiles,
    /// A file that is not UTF-8 text without NUL.
    FileNotText(String),
    /// A path that is not allowed.
    PathInvalid(String),
}

impl SkillRefusal {
    /// The refusal's wire code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::FrontmatterInvalid => "skill_frontmatter_invalid",
            Self::NameInvalid => "skill_name_invalid",
            Self::NameMismatch => "skill_name_mismatch",
            Self::DescriptionInvalid => "skill_description_invalid",
            Self::RunsCommands => "skill_runs_commands",
            Self::AttachesFiles => "skill_attaches_files",
            Self::TooLarge => "skill_too_large",
            Self::TooManyFiles => "skill_too_many_files",
            Self::FileNotText(_) => "skill_file_not_text",
            Self::PathInvalid(_) => "skill_path_invalid",
        }
    }
}

impl fmt::Display for SkillRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = self.code();
        match self {
            Self::FrontmatterInvalid => write!(
                formatter,
                "{code}: SKILL.md must open with a --- line, a frontmatter of name and description, \
                 and a closing --- line."
            ),
            Self::NameInvalid => write!(
                formatter,
                "{code}: a skill's name is lower-case letters and digits joined by single hyphens, \
                 at most 64 characters."
            ),
            Self::NameMismatch => write!(
                formatter,
                "{code}: the name in SKILL.md must be the skill folder's name."
            ),
            Self::DescriptionInvalid => write!(
                formatter,
                "{code}: a skill needs a description of 1 to 1024 characters saying when it applies."
            ),
            Self::RunsCommands => write!(
                formatter,
                "{code}: SKILL.md runs a command when it loads (!` or a ```! block). Catervas does not \
                 load skills that do."
            ),
            Self::AttachesFiles => write!(
                formatter,
                "{code}: SKILL.md has an @ that would attach a file. Name the file as a path or a \
                 Markdown link instead."
            ),
            Self::TooLarge => write!(
                formatter,
                "{code}: SKILL.md may be 32 KiB, a file 64 KiB, and the folder 256 KiB."
            ),
            Self::TooManyFiles => {
                write!(formatter, "{code}: a skill folder holds 16 files at most.")
            }
            Self::FileNotText(path) => write!(
                formatter,
                "{code}: {path} is not UTF-8 text, or holds a NUL."
            ),
            Self::PathInvalid(path) => write!(
                formatter,
                "{code}: {path} is not an allowed path: up to 3 parts of letters, digits, dots, \
                 hyphens and underscores, none starting with a dot."
            ),
        }
    }
}

impl std::error::Error for SkillRefusal {}

const SKILL_MD_MAX: usize = 32 * 1024;
const FILE_MAX: usize = 64 * 1024;
const TOTAL_MAX: usize = 256 * 1024;
const FILES_MAX: usize = 16;

/// Checks the files of a skill folder (relative path to bytes) for the skill `name`.
///
/// # Errors
///
/// The first refusal, in this order: paths, the file count, sizes, text, `SKILL.md` and its
/// frontmatter, the name, the description, commands, attached files.
pub fn check_skill(
    name: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<CheckedSkill, SkillRefusal> {
    if let Some(path) = files.keys().find(|path| !path_ok(path)) {
        return Err(SkillRefusal::PathInvalid(path.clone()));
    }
    if files.len() > FILES_MAX {
        return Err(SkillRefusal::TooManyFiles);
    }
    let too_big = files
        .get("SKILL.md")
        .is_some_and(|f| f.len() > SKILL_MD_MAX)
        || files.values().any(|f| f.len() > FILE_MAX)
        || files.values().map(Vec::len).sum::<usize>() > TOTAL_MAX;
    if too_big {
        return Err(SkillRefusal::TooLarge);
    }
    let mut texts = BTreeMap::new();
    for (path, bytes) in files {
        match std::str::from_utf8(bytes) {
            Ok(text) if !text.contains('\0') => {
                texts.insert(path.clone(), text.to_string());
            }
            _ => return Err(SkillRefusal::FileNotText(path.clone())),
        }
    }
    let text = texts
        .get("SKILL.md")
        .ok_or(SkillRefusal::FrontmatterInvalid)?;
    let (front, body) = split_frontmatter(text).ok_or(SkillRefusal::FrontmatterInvalid)?;
    let value: Value = serde_saphyr::from_str_with_options(front, crate::yaml_options())
        .map_err(|_| SkillRefusal::FrontmatterInvalid)?;
    let Value::Object(map) = &value else {
        return Err(SkillRefusal::FrontmatterInvalid);
    };
    let Some(Value::String(named)) = map.get("name") else {
        return Err(SkillRefusal::FrontmatterInvalid);
    };
    if !skill_name_ok(named) {
        return Err(SkillRefusal::NameInvalid);
    }
    if named != name {
        return Err(SkillRefusal::NameMismatch);
    }
    let description = match map.get("description") {
        Some(Value::String(text)) if (1..=1024).contains(&text.chars().count()) => text.clone(),
        _ => return Err(SkillRefusal::DescriptionInvalid),
    };
    if runs_commands(text) {
        return Err(SkillRefusal::RunsCommands);
    }
    if attaches_files(text) {
        return Err(SkillRefusal::AttachesFiles);
    }
    let keys: Keys = serde_saphyr::from_str_with_options(front, crate::yaml_options())
        .map_err(|_| SkillRefusal::FrontmatterInvalid)?;
    let ignored_fields = keys
        .0
        .into_iter()
        .filter(|key| key != "name" && key != "description")
        .collect();
    let quoted = Value::String(description.clone()).to_string();
    texts.insert(
        "SKILL.md".to_string(),
        format!("---\nname: {named}\ndescription: {quoted}\n---\n{body}"),
    );
    Ok(CheckedSkill {
        name: named.clone(),
        description,
        ignored_fields,
        session_files: texts,
    })
}

/// The keys of a YAML mapping, in the order the file writes them.
struct Keys(Vec<String>);

impl<'de> Deserialize<'de> for Keys {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Keys;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a mapping")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Keys, A::Error> {
                let mut keys = Vec::new();
                while let Some((key, _)) = map.next_entry::<String, serde::de::IgnoredAny>()? {
                    keys.push(key);
                }
                Ok(Keys(keys))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

/// The frontmatter's text and the body after its closing line, which is left as it was written.
/// CRLF line ends are read as LF for finding the lines only.
fn split_frontmatter(text: &str) -> Option<(&str, &str)> {
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))?;
    let mut at = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return Some((&rest[..at], &rest[at + line.len()..]));
        }
        at += line.len();
    }
    None
}

/// A skill's name: 1 to 64 characters, lower-case letters and digits in words joined by single hyphens.
#[must_use]
pub fn skill_name_ok(name: &str) -> bool {
    name.chars().count() <= 64
        && !name.is_empty()
        && name.split('-').all(|word| {
            !word.is_empty()
                && word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

/// At most 3 parts, each `^[A-Za-z0-9][A-Za-z0-9._-]{0,99}$`.
fn path_ok(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() <= 3
        && parts.iter().all(|part| {
            let mut chars = part.chars();
            chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
                && part.len() <= 100
                && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        })
}

/// `` !` `` or ```` ```! ```` anywhere, or a line opening, after whitespace, three or more backticks
/// or tildes and then `!`.
fn runs_commands(text: &str) -> bool {
    text.contains("!`")
        || text.contains("```!")
        || text.lines().any(|line| {
            let line = line.trim_start();
            ['`', '~'].into_iter().any(|fence| {
                let rest = line.trim_start_matches(fence);
                line.len() - rest.len() >= 3 && rest.starts_with('!')
            })
        })
}

/// An `@` not inside an email-like word: Claude Code attaches `@<path>` after whitespace (JS `\s`,
/// U+FEFF among it) and after 。、？！, so only an `@` right after an ASCII letter, digit, or one of
/// `._%+-` is allowed.
fn attaches_files(text: &str) -> bool {
    let mut before: Option<char> = None;
    for c in text.chars() {
        if c == '@' && !before.is_some_and(|b| b.is_ascii_alphanumeric() || "._%+-".contains(b)) {
            return true;
        }
        before = Some(c);
    }
    false
}

/// The `name` and `description` a folder's `SKILL.md` declares, read leniently: whatever else is
/// wrong with the skill, so that a skill waiting for review can still be named and described. A
/// missing description is empty. `None` when there is no `SKILL.md`, no frontmatter, or no name.
#[must_use]
pub fn declared_name_and_description(
    files: &BTreeMap<String, Vec<u8>>,
) -> Option<(String, String)> {
    let text = std::str::from_utf8(files.get("SKILL.md")?).ok()?;
    let (front, _) = split_frontmatter(text)?;
    let value: Value = serde_saphyr::from_str_with_options(front, crate::yaml_options()).ok()?;
    let Value::String(name) = value.get("name")? else {
        return None;
    };
    let description = match value.get("description") {
        Some(Value::String(description)) => description.clone(),
        _ => String::new(),
    };
    Some((name.clone(), description))
}

/// Every agent role Catervas ships.
pub const SHIPPED_ROLES: [Role; 8] = [
    Role::ProductManager,
    Role::ScrumMaster,
    Role::Architect,
    Role::SoftwareDeveloper,
    Role::MarketingSpecialist,
    Role::UiUxDesigner,
    Role::FinanceSpecialist,
    Role::ProcurementSpecialist,
];

static CORE_SKILL_NAMES: LazyLock<BTreeSet<&'static str>> = LazyLock::new(|| {
    let roles: Vec<_> = SHIPPED_ROLES
        .into_iter()
        .filter_map(|role| crate::load_role(role).ok())
        .collect();
    let kits: Vec<_> = SHIPPED_ROLES
        .into_iter()
        .filter_map(|role| crate::load_kit(role).ok())
        .collect();
    crate::shipped_skill_names(&roles, &kits)
        .into_iter()
        // ponytail: leaks a few short names once, so the set can be 'static; a Cow set if it grows.
        .map(|name| &*Box::leak(name.into_boxed_str()))
        .collect()
});

/// The name of every skill a shipped role or its kit carries.
#[must_use]
pub fn core_skill_names() -> BTreeSet<&'static str> {
    CORE_SKILL_NAMES.clone()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use catervas_core::contract::Role;
    use serde_json::Value;

    use super::{SkillRefusal, check_skill, core_skill_names, declared_name_and_description};
    use crate::load_role;

    fn skill(entries: &[(&str, &str)]) -> BTreeMap<String, Vec<u8>> {
        entries
            .iter()
            .map(|(path, text)| ((*path).to_string(), text.as_bytes().to_vec()))
            .collect()
    }

    fn md(front: &str, body: &str) -> String {
        format!("---\n{front}\n---\n{body}")
    }

    const OK_FRONT: &str = "name: api-style\ndescription: Use when writing API handlers.";

    fn refusal(files: &BTreeMap<String, Vec<u8>>) -> SkillRefusal {
        check_skill("api-style", files).expect_err("this skill is refused")
    }

    #[test]
    fn accepts_a_skill_with_a_reference() {
        let files = skill(&[
            ("SKILL.md", &md(OK_FRONT, "Read references/a.md.\n")),
            ("references/a.md", "the details\n"),
        ]);
        let checked = check_skill("api-style", &files).expect("a good skill");
        assert_eq!(checked.name, "api-style");
        assert_eq!(checked.description, "Use when writing API handlers.");
        assert!(checked.ignored_fields.is_empty());
        let paths: Vec<&str> = checked.session_files.keys().map(String::as_str).collect();
        assert_eq!(paths, ["SKILL.md", "references/a.md"]);
        assert_eq!(checked.session_files["references/a.md"], "the details\n");
    }

    #[test]
    fn rewrites_the_frontmatter_to_name_and_description() {
        let front = "name: api-style\nallowed-tools: Bash\ndescription: \"Use: when\\nit applies\"\nhooks:\n  PreToolUse: []\nmodel: opus";
        let body = "# Title\n\nBody  with trailing space \n";
        let files = skill(&[("SKILL.md", &md(front, body))]);
        let checked = check_skill("api-style", &files).expect("a good skill");
        assert_eq!(checked.ignored_fields, ["allowed-tools", "hooks", "model"]);
        let copy = &checked.session_files["SKILL.md"];
        let rest = copy
            .strip_prefix("---\nname: api-style\ndescription: ")
            .expect("two keys");
        let (description, tail) = rest.split_once("\n---\n").expect("a closing line");
        assert_eq!(tail, body, "the body is unchanged byte for byte");
        let read: String = serde_json::from_str(description).expect("a JSON string");
        assert_eq!(read, "Use: when\nit applies");
        assert_eq!(checked.description, read);
        // The keys are reported in file order, not sorted.
        let reversed =
            "model: opus\nname: api-style\ndescription: d\nhooks: {}\nallowed-tools: Bash";
        let checked = check_skill("api-style", &skill(&[("SKILL.md", &md(reversed, "b"))]))
            .expect("a good skill");
        assert_eq!(checked.ignored_fields, ["model", "hooks", "allowed-tools"]);
    }

    #[test]
    fn refuses_a_skill_without_a_frontmatter() {
        let cases = [
            skill(&[("other.md", "x")]),
            skill(&[("SKILL.md", "name: api-style\n")]),
            skill(&[("SKILL.md", &md("[1, 2]", "body"))]),
            skill(&[("SKILL.md", &md("description: no name", "body"))]),
            skill(&[("SKILL.md", "---\nname: api-style\ndescription: d\nbody\n")]),
        ];
        for files in &cases {
            assert_eq!(
                refusal(files),
                SkillRefusal::FrontmatterInvalid,
                "{files:?}"
            );
        }
        let crlf = "---\r\nname: api-style\r\ndescription: Use it.\r\n---\r\nBody\r\n";
        let checked = check_skill("api-style", &skill(&[("SKILL.md", crlf)])).expect("CRLF passes");
        assert_eq!(checked.description, "Use it.");
        assert!(checked.session_files["SKILL.md"].ends_with("---\nBody\r\n"));
    }

    #[test]
    fn refuses_a_skill_that_runs_commands() {
        for body in [
            "Run !`ls` now.\n",
            "```!\nls\n```\n",
            "  ~~~!\nls\n~~~\n",
            "````!\nls\n",
            "Run x ```!\ntouch y\n```\n",
        ] {
            let files = skill(&[("SKILL.md", &md(OK_FRONT, body))]);
            assert_eq!(refusal(&files), SkillRefusal::RunsCommands, "{body:?}");
        }
        let fine = skill(&[("SKILL.md", &md(OK_FRONT, "Say hi!\n```sh\nls\n```\n``!\n"))]);
        assert!(check_skill("api-style", &fine).is_ok());
    }

    #[test]
    fn refuses_a_skill_that_attaches_files() {
        for body in [
            "See @~/.ssh/id_rsa\n",
            "@references/a.md\n",
            "line\n@x",
            "tab\t@x",
            "Notes\u{3002}@~/x",
            "Notes \u{feff}@~/x",
            "Notes\u{ff01}@~/x",
            "Notes\u{3001}@~/x",
            "Notes\u{ff1f}@~/x",
            "(@~/x)",
            "\"@~/x\"",
        ] {
            let files = skill(&[("SKILL.md", &md(OK_FRONT, body))]);
            assert_eq!(refusal(&files), SkillRefusal::AttachesFiles, "{body:?}");
        }
        for body in ["mail ana@example.com\n", "a.b+c_d-e%f@example.com\n"] {
            let files = skill(&[("SKILL.md", &md(OK_FRONT, body))]);
            assert!(check_skill("api-style", &files).is_ok(), "{body:?}");
        }
    }

    #[test]
    fn refuses_names_and_descriptions_out_of_bounds() {
        let with = |front: &str| skill(&[("SKILL.md", &md(front, "b"))]);
        let upper = check_skill("Has_Upper", &with("name: Has_Upper\ndescription: d"));
        assert_eq!(upper, Err(SkillRefusal::NameInvalid));
        for bad in ["a--b", "-a", "a-", "a_b", ""] {
            let files = with(&format!("name: \"{bad}\"\ndescription: d"));
            assert_eq!(
                check_skill(bad, &files),
                Err(SkillRefusal::NameInvalid),
                "{bad:?}"
            );
        }
        let long = "a".repeat(65);
        let files = with(&format!("name: {long}\ndescription: d"));
        assert_eq!(check_skill(&long, &files), Err(SkillRefusal::NameInvalid));
        let ok = "a".repeat(64);
        let files = with(&format!("name: {ok}\ndescription: d"));
        assert!(check_skill(&ok, &files).is_ok());
        assert_eq!(
            refusal(&with("name: other-name\ndescription: d")),
            SkillRefusal::NameMismatch
        );
        assert_eq!(
            refusal(&with("name: api-style")),
            SkillRefusal::DescriptionInvalid
        );
        assert_eq!(
            refusal(&with("name: api-style\ndescription: \"\"")),
            SkillRefusal::DescriptionInvalid
        );
        assert_eq!(
            refusal(&with("name: api-style\ndescription: 7")),
            SkillRefusal::DescriptionInvalid
        );
        let long = "é".repeat(1025);
        assert_eq!(
            refusal(&with(&format!("name: api-style\ndescription: {long}"))),
            SkillRefusal::DescriptionInvalid
        );
        let longest = "é".repeat(1024);
        assert!(
            check_skill(
                "api-style",
                &with(&format!("name: api-style\ndescription: {longest}"))
            )
            .is_ok()
        );
    }

    #[test]
    fn refuses_folders_out_of_bounds() {
        let head = md(OK_FRONT, "b");
        let with_extra = |extra: Vec<(String, Vec<u8>)>| {
            let mut files = skill(&[("SKILL.md", &head)]);
            files.extend(extra);
            files
        };
        let big_md = skill(&[("SKILL.md", &md(OK_FRONT, &"x".repeat(33 * 1024)))]);
        assert_eq!(refusal(&big_md), SkillRefusal::TooLarge);
        let at_limit = skill(&[("SKILL.md", &md(OK_FRONT, &"x".repeat(32 * 1024 - 80)))]);
        assert!(check_skill("api-style", &at_limit).is_ok());
        assert_eq!(
            refusal(&with_extra(vec![("a.md".into(), vec![b'x'; 65 * 1024])])),
            SkillRefusal::TooLarge
        );
        assert!(
            check_skill(
                "api-style",
                &with_extra(vec![("a.md".into(), vec![b'x'; 64 * 1024])])
            )
            .is_ok()
        );
        let five: Vec<(String, Vec<u8>)> = (0..5)
            .map(|n| (format!("f{n}.md"), vec![b'x'; 52 * 1024]))
            .collect();
        assert_eq!(
            refusal(&with_extra(five)),
            SkillRefusal::TooLarge,
            "260 KiB in all"
        );
        let many: Vec<(String, Vec<u8>)> = (0..16)
            .map(|n| (format!("f{n}.md"), b"x".to_vec()))
            .collect();
        assert_eq!(
            refusal(&with_extra(many)),
            SkillRefusal::TooManyFiles,
            "17 files"
        );
        let fifteen: Vec<(String, Vec<u8>)> = (0..15)
            .map(|n| (format!("f{n}.md"), b"x".to_vec()))
            .collect();
        assert!(
            check_skill("api-style", &with_extra(fifteen)).is_ok(),
            "16 files"
        );
        assert_eq!(
            refusal(&with_extra(vec![("n.md".into(), b"a\0b".to_vec())])),
            SkillRefusal::FileNotText("n.md".to_string())
        );
        assert_eq!(
            refusal(&with_extra(vec![("n.md".into(), vec![0xff, 0xfe])])),
            SkillRefusal::FileNotText("n.md".to_string())
        );
        for bad in [
            ".hidden",
            "a/b/c/d.md",
            "../x",
            "a//b.md",
            "/abs.md",
            "a b.md",
            "a/.x/c.md",
            "a\\b.md",
        ] {
            assert_eq!(
                refusal(&with_extra(vec![(bad.to_string(), b"x".to_vec())])),
                SkillRefusal::PathInvalid(bad.to_string()),
                "{bad}"
            );
        }
        let long_part = format!("{}.md", "a".repeat(97));
        assert!(
            check_skill("api-style", &with_extra(vec![(long_part, b"x".to_vec())])).is_ok(),
            "a part of 100 characters"
        );
        assert_eq!(
            refusal(&with_extra(vec![(
                format!("{}.md", "a".repeat(98)),
                b"x".to_vec()
            )])),
            SkillRefusal::PathInvalid(format!("{}.md", "a".repeat(98)))
        );
        assert!(
            check_skill(
                "api-style",
                &with_extra(vec![("a/b/c.md".into(), b"x".to_vec())])
            )
            .is_ok()
        );
    }

    fn shipped_dirs() -> Vec<std::path::PathBuf> {
        let roles = Path::new(env!("CARGO_MANIFEST_DIR")).join("roles");
        let mut dirs: Vec<_> = std::fs::read_dir(roles)
            .expect("the roles folder")
            .map(|entry| entry.expect("an entry").path())
            .collect();
        dirs.sort();
        dirs
    }

    fn read_folder(dir: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut files = BTreeMap::new();
        let mut stack = vec![(dir.to_path_buf(), String::new())];
        while let Some((folder, prefix)) = stack.pop() {
            for entry in std::fs::read_dir(&folder).expect("a folder") {
                let entry = entry.expect("an entry");
                let name = entry.file_name().into_string().expect("a UTF-8 name");
                let relative = format!("{prefix}{name}");
                if entry.path().is_dir() {
                    stack.push((entry.path(), format!("{relative}/")));
                } else {
                    files.insert(relative, std::fs::read(entry.path()).expect("a file"));
                }
            }
        }
        files
    }

    #[test]
    fn declares_a_name_and_description_whatever_else_is_wrong() {
        let declared = |text: &str| declared_name_and_description(&skill(&[("SKILL.md", text)]));
        assert_eq!(
            declared(&md(OK_FRONT, "run !`ls` @x")),
            Some((
                "api-style".to_string(),
                "Use when writing API handlers.".to_string()
            )),
            "a skill that would be refused is still named"
        );
        assert_eq!(
            declared("---\r\nname: a-b\r\n---\r\n"),
            Some(("a-b".to_string(), String::new())),
            "no description reads as empty, and CRLF is read"
        );
        assert_eq!(
            declared(&md("name: Has_Upper\ndescription: 7", "b")),
            Some(("Has_Upper".to_string(), String::new()))
        );
        for text in [
            "no frontmatter",
            "---\n[1]\n---\n",
            "---\ndescription: d\n---\n",
            "---\nname: 3\n---\n",
        ] {
            assert_eq!(declared(text), None, "{text:?}");
        }
        assert_eq!(
            declared_name_and_description(&skill(&[("other.md", "x")])),
            None
        );
    }

    #[test]
    fn keeping_the_books_passes_the_skill_checks() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("roles/finance_specialist/skills/keeping-the-books");
        let checked = check_skill("keeping-the-books", &read_folder(&folder))
            .expect("the Finance Specialist's skill passes");
        assert_eq!(checked.name, "keeping-the-books");
        assert!(!checked.description.trim().is_empty());
    }

    #[test]
    fn sourcing_a_product_passes_the_skill_checks() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("roles/procurement_specialist/skills/sourcing-a-product");
        let checked = check_skill("sourcing-a-product", &read_folder(&folder))
            .expect("the Procurement Specialist's skill passes");
        assert_eq!(checked.name, "sourcing-a-product");
        assert!(!checked.description.trim().is_empty());
    }

    #[test]
    fn every_shipped_role_skill_passes() {
        let mut from_roles = std::collections::BTreeSet::new();
        let mut checked = 0;
        for role_dir in shipped_dirs() {
            let id = role_dir
                .file_name()
                .and_then(|n| n.to_str())
                .expect("a name");
            let role: Role = serde_json::from_value(Value::String(id.to_string()))
                .expect("a role directory is named for its role");
            for shipped in load_role(role).expect("a shipped role").skills {
                let files = read_folder(&role_dir.join("skills").join(&shipped.name));
                let result = check_skill(&shipped.name, &files);
                assert!(result.is_ok(), "{id}/{}: {result:?}", shipped.name);
                checked += 1;
                from_roles.insert(shipped.name);
            }
            // A kit's skills ship too, and `load_kit` has already run `check_skill` over each.
            let kit = crate::load_kit(role).expect("a shipped kit loads");
            from_roles.extend(kit.skills.into_iter().map(|skill| skill.name));
        }
        assert!(checked > 0);
        let names: std::collections::BTreeSet<String> =
            core_skill_names().into_iter().map(String::from).collect();
        assert_eq!(names, from_roles);
    }
}
