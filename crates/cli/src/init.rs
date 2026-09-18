//! `farik init`: make the repository this is run in a Farik project (F2).

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use farik_core::criteria::{CriteriaLibrary, CriterionTemplate};
use farik_core::team::{Team, validate_team};
use farik_protocol::event::EventBody;
use farik_protocol::generated::event::{CriteriaUpdatedBody, ProjectScannedBody, TeamUpdatedBody};
use farik_store::files::{FilesError, ProjectFiles};
use farik_store::{Git, ProjectScan, open_event_log, scan_project, seeded_library};
use serde_json::json;

use crate::project::{DATABASE, ProjectIds, directory_name, repository_root};
use crate::{HUMAN, Report};

/// Makes `.farik/`, the event log, the project scan and the criterion library, and records what it
/// found.
///
/// A second run is a rescan: the team file and everything a person has written are left alone, the
/// scan is read again, and the criteria the scan found are replaced while the ones a person wrote
/// are kept (`seeded_library`). That is what makes this the command to run after the project's test
/// command changes.
///
/// # Errors
///
/// A sentence saying this is not a git repository, what git said about one it could not read, or
/// what could not be written.
pub fn init(cwd: &Path, now: DateTime<Utc>) -> Result<Report, String> {
    let root = repository_root(cwd)?;
    let git = Git::open(root.clone());
    let scan: ProjectScan = scan_project(&git, now).map_err(|error| error.to_string())?;
    let files = ProjectFiles::open(root.clone());

    // A file that is there and cannot be read is not a file that is absent. `init` writes nothing
    // over it and refuses instead: `ProjectFiles::init` would leave a broken team file where it is
    // and answer `Ok`, so a run that swallowed the difference would report a team it did not write,
    // and `seeded_library` keeps only what it is given, so it would drop every criterion a person
    // wrote because one line of the library has a typo in it.
    let existing = absent_or(files.read_team())?;
    let kept = absent_or(files.read_criteria())?;
    // Both hand-edited files are read before anything is written, so that a run which refuses one of
    // them has written nothing. The other order leaves a team file behind that no `team.updated` will
    // ever record: the next run finds the file and takes the "kept the team" branch, and the log is
    // append-only, so the event that says where that team came from can never be added.
    let team_was_written = existing.is_none();
    let team = match existing {
        Some(team) => team,
        None => starter_team(&directory_name(&root))?,
    };
    files.init(&team).map_err(|error| error.to_string())?;

    let library = seeded_library(&scan.detected_criteria, kept.as_ref());
    files
        .write_criteria(&library)
        .map_err(|error| error.to_string())?;
    files
        .write_project_scan(&project_document(&scan, &library))
        .map_err(|error| error.to_string())?;

    let log =
        Arc::new(open_event_log(&root.join(DATABASE), now).map_err(|error| error.to_string())?);
    let ids = ProjectIds::of(&log, &team, &root)?;
    let project = crate::Project {
        root: root.clone(),
        files,
        log,
        team,
        ids,
    };

    let mut recorded = Vec::new();
    if team_was_written {
        let event = project.event(
            EventBody::TeamUpdated(TeamUpdatedBody {
                agent_ids: project
                    .team
                    .agents
                    .iter()
                    .map(|agent| agent.id.to_string())
                    .collect(),
                team_name: project.team.name.to_string(),
                updated_by: HUMAN.to_string(),
            }),
            now,
            None,
        )?;
        recorded.push(project.append(&event)?);
    }
    let event = project.event(
        EventBody::ProjectScanned(ProjectScannedBody {
            detected_criteria: names_of(&scan.detected_criteria),
            read_back: scan.read_back.clone(),
        }),
        now,
        None,
    )?;
    recorded.push(project.append(&event)?);
    let event = project.event(
        EventBody::CriteriaUpdated(CriteriaUpdatedBody {
            criterion_names: names_of(&library.criteria),
            updated_by: HUMAN.to_string(),
        }),
        now,
        None,
    )?;
    recorded.push(project.append(&event)?);

    let mut lines = vec![scan.read_back.clone()];
    if team_was_written {
        lines.push(format!(
            "wrote .farik/team.yaml: {}",
            project
                .team
                .agents
                .iter()
                .map(|agent| agent.id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    } else {
        lines.push("kept the team already in .farik/team.yaml".to_string());
    }
    lines.push(match names_of(&library.criteria).len() {
        0 => "no criteria: nothing in this repository says how it is tested".to_string(),
        count => format!(
            "{count} criteria in .farik/team/criteria.yaml: {}",
            names_of(&library.criteria).join(", ")
        ),
    });
    Ok(Report {
        lines,
        json: json!({
            "root": root.display().to_string(),
            "read_back": scan.read_back,
            "team_written": team_was_written,
            "criteria": names_of(&library.criteria),
            "events": recorded,
        }),
    })
}

/// What a file holds, nothing when there is no such file, and a refusal when there is one and it
/// cannot be read.
///
/// `.farik/team.yaml` and `.farik/team/criteria.yaml` are files 5.13 expects people to hand-edit, so
/// one of them being unreadable is the ordinary way this command meets a mistake, and the answer is
/// to say so rather than to write past it.
fn absent_or<T>(read: Result<T, FilesError>) -> Result<Option<T>, String> {
    match read {
        Ok(value) => Ok(Some(value)),
        Err(FilesError::NotFound { .. }) => Ok(None),
        Err(other) => Err(other.to_string()),
    }
}

/// Every criterion's name, in the order they are held in.
fn names_of(criteria: &[CriterionTemplate]) -> Vec<String> {
    criteria.iter().map(|one| one.name.to_string()).collect()
}

/// What `.farik/project.md` holds: the line the scan read back, and the criteria it found.
///
/// This is the file every session is given (`docs/SPEC.md` section 5.8), so it says what the scan
/// found and nothing it did not.
fn project_document(scan: &ProjectScan, library: &CriteriaLibrary) -> String {
    let mut parts = vec![format!("# The project\n\n{}", scan.read_back)];
    let names = names_of(&library.criteria);
    if !names.is_empty() {
        parts.push(format!("Criteria: {}.", names.join(", ")));
    }
    format!("{}\n", parts.join("\n\n"))
}

/// The team a project starts with: the two agents `validate_team` says a team cannot work without
/// (D18), named after their roles because the person has not named them yet.
///
/// The team editor (F1) is how a person renames them, adds the other roles, and changes the models.
/// Both get `claude-opus-5` at `high`, which is what `docs/SPEC.md` 8.2 ships as the default for the
/// Product Manager, the Architect and the Developer.
///
/// # Errors
///
/// The sentence `validate_team`'s refusal reads as, which would mean this function and the schema
/// disagree.
fn starter_team(project: &str) -> Result<Team, String> {
    let wire = json!({
        "name": if project.is_empty() { "Farik".to_string() } else { project.to_string() },
        "agents": [
            {
                "id": "product-manager",
                "display_name": "Product Manager",
                "role": "product_manager",
                "persona": "Owns the backlog and turns every request into a contract.",
                "status": "active",
                "model": { "id": "claude-opus-5", "effort": "high" }
            },
            {
                "id": "developer",
                "display_name": "Developer",
                "role": "software_developer",
                "persona": "Writes the code and the tests that hold it.",
                "status": "active",
                "model": { "id": "claude-opus-5", "effort": "high" }
            }
        ],
        "budgets": { "daily_usd": 20 },
        "policy": {
            "human_accepts_contracts": "high_risk",
            "wip_limit_per_agent": 1,
            "blocked_limit_hours": 24,
            "max_iterations": 3,
            "integration": "manual"
        },
        "rules": {}
    });
    validate_team(&wire).map_err(|errors| {
        format!(
            "the team farik init writes is not one: {}",
            errors
                .iter()
                .map(|error| format!("{} {}", error.path, error.message))
                .collect::<Vec<_>>()
                .join("; ")
        )
    })
}

#[cfg(test)]
mod tests {
    use farik_core::contract::Role;

    use super::starter_team;

    #[test]
    fn starts_a_project_with_the_two_agents_a_team_cannot_work_without() {
        let team = starter_team("notes").expect("the team farik init writes is a team");
        assert_eq!(team.name.as_str(), "notes", "named after the project");
        assert!(team.has_active(Role::ProductManager));
        assert!(team.has_active(Role::SoftwareDeveloper));
        assert_eq!(
            team.agents
                .iter()
                .map(|agent| agent.id.as_str().to_string())
                .collect::<Vec<_>>(),
            ["product-manager", "developer"],
            "named after their roles, because the person has not named them yet"
        );
        assert_eq!(
            team.agents
                .iter()
                .map(|agent| agent.model.as_ref().map(|model| (
                    model.id.as_str().to_string(),
                    model.effort.map(|effort| effort.to_string())
                )))
                .collect::<Vec<_>>(),
            [
                Some(("claude-opus-5".to_string(), Some("high".to_string()))),
                Some(("claude-opus-5".to_string(), Some("high".to_string())))
            ],
            "8.2 ships Opus 5 at high for the Product Manager, the Architect and the Developer, and \
             this team is two of those three"
        );
    }

    #[test]
    fn names_a_team_after_something_when_the_directory_name_is_nothing() {
        assert_eq!(
            starter_team("").expect("a team").name.as_str(),
            "Farik",
            "a repository at the root of a volume has no directory name, and a blank team name is \
             one validate_team refuses"
        );
    }
}
