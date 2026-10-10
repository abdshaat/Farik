//! `catervas_write_evaluation` (`docs/SPEC.md` 6.10): the Procurement Specialist writes a comparison
//! as a note, `evaluations/<name>.md`, in its private folder, keeping every previous version.

use catervas_core::contract::Role;
use catervas_core::team::private_folder;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::sheets::{in_its_own_implement_session, private_path, store_file};
use super::{Call, ToolError};

/// The most characters an evaluation's name has.
const MOST_NAME: usize = 64;
/// The most bytes an evaluation's text has.
const MOST_TEXT: usize = 64 * 1024;

/// `catervas_write_evaluation`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteEvaluationInput {
    /// The comparison's name: lower-case letters and digits in words joined by single hyphens, 1
    /// to 64 characters, such as `email-sending`. The note is `evaluations/<name>.md`.
    name: String,
    /// The comparison, as Markdown text of 1 byte to 64 KiB with no NUL. It replaces the note of
    /// that name, and the previous version is kept. Catervas never shows it as a page, so no link in
    /// it runs.
    text: String,
}

fn refused(code: &'static str, detail: impl Into<String>) -> ToolError {
    Refusal::Finance {
        code,
        detail: detail.into(),
    }
    .into()
}

/// Whether `name` is words of lower-case letters and digits joined by single hyphens, at most 64
/// characters: `^[a-z0-9]+(-[a-z0-9]+)*$`.
pub(super) fn is_a_name(name: &str) -> bool {
    name.len() <= MOST_NAME
        && name.split('-').all(|word| {
            !word.is_empty()
                && word
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

/// `catervas_write_evaluation`: writes `evaluations/<name>.md` in the Procurement Specialist's
/// private folder, in its implement session of a task it is the assignee of, copying the note
/// there already to `.history/` first, and answers the note's path, its bytes and whether it
/// replaced one. It reports no path to the permission check and holds the folder line itself.
///
/// # Errors
///
/// `evaluation_refused` outside that session, `evaluation_name_invalid`, `evaluation_not_text` for
/// a text that is empty or holds a NUL, `evaluation_too_large` past 64 KiB, `private_path_refused`
/// for a path that passes through a link or is not a file; `Failed` when a file cannot be written.
/// Nothing is written for a refusal.
pub(super) fn write_evaluation(
    call: &Call<'_>,
    input: &WriteEvaluationInput,
) -> Result<Value, ToolError> {
    let refuse = |why: &str| refused("evaluation_refused", format!("only {why}"));
    let Some(folder) =
        private_folder(call.role()).filter(|_| call.role() == Role::ProcurementSpecialist)
    else {
        return Err(refuse("the Procurement Specialist writes an evaluation"));
    };
    in_its_own_implement_session(call, "an evaluation", &refuse)?;
    if !is_a_name(&input.name) {
        return Err(refused(
            "evaluation_name_invalid",
            format!(
                "{:?} is not a name: write words of lower-case letters and digits joined by \
                 single hyphens, at most {MOST_NAME} characters, such as email-sending",
                input.name
            ),
        ));
    }
    if input.text.is_empty() || input.text.contains('\0') {
        return Err(refused(
            "evaluation_not_text",
            "the text is empty or holds a NUL, and a comparison is text",
        ));
    }
    if input.text.len() > MOST_TEXT {
        return Err(refused(
            "evaluation_too_large",
            format!(
                "the text is {} bytes, and an evaluation is at most {MOST_TEXT}; write less, or \
                 split it into two evaluations",
                input.text.len()
            ),
        ));
    }
    let relative = format!("evaluations/{}.md", input.name);
    let root = call.deps().files.root();
    let target = private_path(root, folder, &relative)?;
    let replaced = store_file(
        &root.join(folder),
        &target,
        input.text.as_bytes(),
        call.deps().clock.now(),
    )?;
    Ok(json!({ "path": relative, "bytes": input.text.len(), "replaced": replaced }))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    use std::path::{Path, PathBuf};

    use serde_json::{Value, json};

    use crate::session::SessionPurpose;
    use crate::tools::ToolError;
    use crate::tools::fixtures::{
        TestProject, a_team_of_three, run, with_the_finance_specialist,
        with_the_procurement_specialist,
    };

    /// A project with the Finance Specialist `fin` and the Procurement Specialist `proc`, whose
    /// task CTV-1 is in progress, a finance task CTV-2 and a Developer's task CTV-3.
    fn a_project(name: &str) -> TestProject {
        let project = TestProject::new(
            name,
            &a_team_of_three(|wire| {
                with_the_finance_specialist(wire);
                with_the_procurement_specialist(wire);
            }),
        );
        for (task, role, assignee, reviewer) in [
            ("CTV-1", "procurement_specialist", "proc", "pm"),
            ("CTV-2", "finance_specialist", "fin", "pm"),
        ] {
            project.filed_with(task, "assigned", "task", None, |wire| {
                wire["assignee_role"] = json!(role);
                wire["reviewer_role"] = json!("product_manager");
            });
            project.moved(
                task,
                "assigned",
                "in_progress",
                &json!({ "assignee": assignee, "reviewer": reviewer }),
            );
        }
        project.filed("CTV-3", "assigned", "task", None);
        project.moved(
            "CTV-3",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        project
    }

    fn folder(project: &TestProject) -> PathBuf {
        project.repo.path.join(".catervas/local/procurement")
    }

    /// `catervas_write_evaluation` as `proc` in its implement session of CTV-1.
    fn write(project: &TestProject, name: &str, text: &str) -> Result<Value, ToolError> {
        project.call(
            "proc",
            Some("CTV-1"),
            "catervas_write_evaluation",
            json!({ "name": name, "text": text }),
        )
    }

    fn refusal_of(result: Result<Value, ToolError>) -> String {
        match result {
            Err(ToolError::Refused { reason }) => reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// Every file under `folder`, as paths relative to it, sorted.
    fn files_under(folder: &Path) -> Vec<String> {
        fn walk(folder: &Path, at: &Path, found: &mut Vec<String>) {
            let Ok(entries) = fs::read_dir(at) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(folder, &path, found);
                } else if let Ok(relative) = path.strip_prefix(folder) {
                    found.push(relative.display().to_string());
                }
            }
        }
        let mut found = Vec::new();
        walk(folder, folder, &mut found);
        found.sort();
        found
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn writes_an_evaluation_in_the_folder() {
        let project = a_project("evaluation-write");
        let text = "# Email sending\n\nAcme: 12 USD a month, read 2026-10-07 at https://acme.example.\n\n[a link](https://x.example) and `code` and <b>html</b>\n";

        let answer = write(&project, "email-sending", text).expect("the evaluation is written");

        assert_eq!(
            answer,
            json!({ "path": "evaluations/email-sending.md", "bytes": text.len(), "replaced": false })
        );
        let file = folder(&project).join("evaluations/email-sending.md");
        assert_eq!(
            fs::read(&file).expect("the file is there"),
            text.as_bytes(),
            "byte for byte"
        );
        let mode = |path: &Path| fs::metadata(path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode(&folder(&project)), 0o700, "the folder is private");
        assert_eq!(mode(&file), 0o600);
        assert_eq!(
            files_under(&folder(&project)),
            ["evaluations/email-sending.md"],
            "nothing else is left, no temporary file"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn keeps_the_previous_evaluation() {
        let project = a_project("evaluation-history");
        write(&project, "email-sending", "first").expect("the first write");

        let answer = write(&project, "email-sending", "second").expect("the second write");

        assert_eq!(answer["replaced"], true);
        assert_eq!(
            fs::read_to_string(folder(&project).join("evaluations/email-sending.md")).ok(),
            Some("second".to_string())
        );
        let files = files_under(&folder(&project));
        let kept: Vec<&String> = files
            .iter()
            .filter(|path| path.starts_with(".history/"))
            .collect();
        let [only] = kept.as_slice() else {
            panic!("one previous version, got {files:?}");
        };
        assert!(
            only.starts_with(".history/evaluations%2Femail-sending.md.2026")
                && only.ends_with("Z.md"),
            "{only}"
        );
        assert_eq!(
            fs::read_to_string(folder(&project).join(only)).ok(),
            Some("first".to_string())
        );
        // A workbook's copy still ends in `.xlsx`, whatever the notes' end in.
        let workbook =
            json!({ "path": "vendors.xlsx", "sheets": [{ "name": "Vendors", "rows": [["a"]] }] });
        for _ in 0..2 {
            project
                .call(
                    "proc",
                    Some("CTV-1"),
                    "catervas_write_sheet",
                    workbook.clone(),
                )
                .expect("the register is written");
        }
        let copies: Vec<String> = files_under(&folder(&project))
            .into_iter()
            .filter(|path| path.starts_with(".history/vendors.xlsx."))
            .collect();
        assert_eq!(copies.len(), 1, "{copies:?}");
        assert!(copies[0].ends_with("Z.xlsx"), "{copies:?}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_an_evaluations_folder_that_is_a_link() {
        let project = a_project("evaluation-link");
        let outside = project.repo.path.join("outside");
        fs::create_dir_all(&outside).expect("a folder outside");
        fs::create_dir_all(folder(&project)).expect("the folder");
        symlink(&outside, folder(&project).join("evaluations")).expect("a link out");

        let reason = refusal_of(write(&project, "email-sending", "a comparison"));

        assert!(reason.starts_with("private_path_refused: "), "{reason}");
        assert_eq!(
            files_under(&outside),
            Vec::<String>::new(),
            "nothing is written outside"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_bad_name() {
        let project = a_project("evaluation-names");
        let long = "a".repeat(65);
        for name in [
            "../x",
            "Email",
            "a--b",
            "a.md",
            "",
            "-a",
            "a-",
            "a b",
            "a/b",
            long.as_str(),
        ] {
            let reason = refusal_of(write(&project, name, "a comparison"));
            assert!(
                reason.starts_with("evaluation_name_invalid: "),
                "{name:?}: {reason}"
            );
        }
        assert_eq!(files_under(&folder(&project)), Vec::<String>::new());
        // The most it takes: 64 characters, and digits and single hyphens inside.
        write(&project, &"a".repeat(64), "x").expect("64 characters");
        write(&project, "q3-2026-email-1", "x").expect("a name of words and numbers");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_bad_body() {
        let project = a_project("evaluation-body");
        let empty = refusal_of(write(&project, "email-sending", ""));
        assert!(empty.starts_with("evaluation_not_text: "), "{empty}");
        let nul = refusal_of(write(&project, "email-sending", "a\0b"));
        assert!(nul.starts_with("evaluation_not_text: "), "{nul}");
        let large = refusal_of(write(&project, "email-sending", &"a".repeat(64 * 1024 + 1)));
        assert!(large.starts_with("evaluation_too_large: "), "{large}");
        assert_eq!(
            files_under(&folder(&project)),
            Vec::<String>::new(),
            "nothing is written for a refusal"
        );
        // One byte, and 64 KiB, are within the bounds.
        write(&project, "one", "a").expect("one byte");
        write(&project, "most", &"a".repeat(64 * 1024)).expect("64 KiB");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn only_procurement_writes_evaluations() {
        let project = a_project("evaluation-only");
        let input = json!({ "name": "email-sending", "text": "a comparison" });
        let context = |who: &str, task: Option<&str>, purpose: SessionPurpose| {
            let mut context = project.context(who, task);
            context.purpose = purpose;
            context
        };
        for (who, context) in [
            (
                "the Finance Specialist in its own task",
                context("fin", Some("CTV-2"), SessionPurpose::Implement),
            ),
            (
                "the Product Manager",
                context("pm", Some("CTV-1"), SessionPurpose::Implement),
            ),
            (
                "a Developer",
                context("dev-a", Some("CTV-3"), SessionPurpose::Implement),
            ),
            (
                "the Procurement Specialist in a chat, with no task",
                context("proc", None, SessionPurpose::Chat),
            ),
            (
                "the Procurement Specialist in a chat about its task",
                context("proc", Some("CTV-1"), SessionPurpose::Chat),
            ),
            (
                "the Procurement Specialist in a session about no task",
                context("proc", None, SessionPurpose::Implement),
            ),
            (
                "the Procurement Specialist in a task that is not its own",
                context("proc", Some("CTV-3"), SessionPurpose::Implement),
            ),
            (
                "the Procurement Specialist when it is a task's reviewer",
                context("proc", Some("CTV-1"), SessionPurpose::Verify),
            ),
        ] {
            let reason = refusal_of(run(&context, "catervas_write_evaluation", input.clone()));
            assert!(
                reason.starts_with("evaluation_refused: "),
                "{who}: {reason}"
            );
        }
        assert_eq!(files_under(&folder(&project)), Vec::<String>::new());
        write(&project, "email-sending", "a comparison")
            .expect("its own task's implement session writes");
    }
}
