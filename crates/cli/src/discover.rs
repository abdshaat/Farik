//! `catervas discover`: what Catervas found in the Claude Code history.

use std::path::{Path, PathBuf};

use catervas_core::claude_services::{ServiceUse, Work};
use catervas_core::jobs::JobKind;
use catervas_core::pricing::Usage;
use catervas_runtime::computer::program_output;
use catervas_store::claude_history::{ReadSummary, place_sessions, read_transcripts};
use catervas_store::claude_services::{
    SettingsPaths, WindowsClaude, read_service_settings, windows_claude_dirs,
};
use catervas_store::jobs::{JobPlaces, find_jobs};
use catervas_store::machine::{
    Activity, ClaudeAi, FormedFound, Found, FoundProject, FoundServices, GoneFolder, JobRow,
    MACHINE_DB, Ownership, forget_machine_index, open_machine_index,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};

use crate::printable::printable;
use crate::state::{claude_desktop_dirs, claude_dir, claude_json, make_state_dir, state_dir};
use crate::{CliIo, Report};

/// Running it is the owner's consent (the screen's consent is a later step's), so it says what it is
/// about to do rather than asking.
const FIRST_LINE: &str = "reading your Claude Code history on this computer; nothing leaves it";

/// Reads the Claude Code history on this computer into the machine index and says what it found, or,
/// with `forget`, deletes the index and reads nothing.
///
/// # Errors
///
/// A sentence saying there is no state folder or no Claude folder to be found, or what the index or
/// git refused.
pub(crate) fn discover(io: &CliIo<'_>, forget: bool) -> Result<Report, String> {
    let state = state_dir(&io.env).ok_or("Catervas has no state folder; set HOME")?;
    let homes = homes_of(io);
    if forget {
        return forget_index(&state.join(MACHINE_DB), &homes);
    }
    let claude = claude_dir(&io.env)
        .ok_or("Catervas can't tell where Claude Code keeps its history; set HOME")?;
    let first = FIRST_LINE.to_string();
    if !claude.join("projects").is_dir() {
        let none = Found {
            projects: Vec::new(),
            gone: Vec::new(),
            not_a_project: Activity::default(),
            unreadable_sessions: 0,
            unreadable_lines: 0,
            claude_ai: ClaudeAi::default(),
        };
        return Ok(Report {
            lines: vec![first, "no Claude Code history on this computer".to_string()],
            json: found_json(
                ReadSummary::default(),
                &none,
                &JobsFound::default(),
                &ServicesView::default(),
            ),
            json_lines: None,
        });
    }
    make_state_dir(&state)?;
    let mut index =
        open_machine_index(&state.join(MACHINE_DB), io.clock.now()).map_err(|e| e.to_string())?;
    let summary =
        read_transcripts(&mut index, &claude, io.clock.now()).map_err(|e| e.to_string())?;
    place_sessions(&mut index, &not_projects(io), io.clock.now()).map_err(|e| e.to_string())?;
    let now = io.clock.now();
    let home = io
        .env
        .get("HOME")
        .filter(|home| !home.is_empty())
        .map(|home| {
            Path::new(home)
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(home))
        });
    let places = JobPlaces {
        home,
        claude_dir: claude.clone(),
    };
    let run_program = |program: &str, args: &[&str]| program_output(&io.env, program, args);
    let searched = find_jobs(&mut index, &places, &run_program, now).map_err(|e| e.to_string())?;
    let (paths, several_windows_users) = settings_paths(io, &claude);
    read_service_settings(&mut index, &paths).map_err(|e| e.to_string())?;
    let found = index.found().map_err(|e| e.to_string())?;
    let services = ServicesView {
        found: index.found_services().map_err(|e| e.to_string())?,
        several_windows_users,
    };
    let jobs = JobsFound {
        rows: index.found_jobs(now).map_err(|e| e.to_string())?,
        unreadable: searched.unreadable,
        now,
    };
    let mut lines = vec![first];
    lines.extend(report_lines(summary, &found, &jobs, &services, &homes));
    Ok(Report {
        lines,
        json: found_json(summary, &found, &jobs, &services),
        json_lines: None,
    })
}

/// Where Claude's settings are: the settings file, Claude Code's folder, Claude Desktop's folders, and
/// whether a WSL machine has several Windows users with Claude Desktop (none of theirs is read).
fn settings_paths(io: &CliIo<'_>, claude: &Path) -> (SettingsPaths, bool) {
    let mut desktop_dirs = claude_desktop_dirs(&io.env, std::env::consts::OS);
    let mut several = false;
    let on_wsl = std::env::consts::OS == "linux"
        && io
            .env
            .get("WSL_DISTRO_NAME")
            .is_some_and(|name| !name.is_empty());
    if on_wsl {
        match windows_claude_dirs(Path::new("/mnt/c/Users")) {
            WindowsClaude::One(folders) => desktop_dirs.extend(folders),
            WindowsClaude::Several => several = true,
            WindowsClaude::None => {}
        }
    }
    (
        SettingsPaths {
            claude_json: claude_json(&io.env),
            claude_dir: claude.to_path_buf(),
            desktop_dirs,
        },
        several,
    )
}

fn forget_index(path: &Path, homes: &[String]) -> Result<Report, String> {
    let forgotten = forget_machine_index(path).map_err(|e| e.to_string())?;
    let line = if forgotten {
        let shown = shown(&path.to_string_lossy(), homes);
        format!("forgot everything Catervas found: {shown} deleted")
    } else {
        "nothing to forget".to_string()
    };
    Ok(Report {
        lines: vec![line],
        json: json!({ "forgotten": forgotten, "path": path.to_string_lossy() }),
        json_lines: None,
    })
}

/// Home as the harness's environment names it, raw and canonical, for showing `~`.
fn homes_of(io: &CliIo<'_>) -> Vec<String> {
    let Some(home) = io.env.get("HOME").filter(|home| !home.is_empty()) else {
        return Vec::new();
    };
    let mut homes = vec![home.clone()];
    if let Ok(real) = Path::new(home).canonicalize() {
        homes.push(real.to_string_lossy().into_owned());
    }
    homes
}

/// Home and `TMPDIR`: folders that are never a project.
fn not_projects(io: &CliIo<'_>) -> Vec<PathBuf> {
    ["HOME", "TMPDIR"]
        .iter()
        .filter_map(|name| io.env.get(*name).filter(|value| !value.is_empty()))
        .map(PathBuf::from)
        .collect()
}

/// A path as a person reads it: `~/…` inside home, and as text a terminal will not obey.
fn shown(path: &str, homes: &[String]) -> String {
    let relative = homes.iter().find_map(|home| {
        if path == home {
            Some(String::new())
        } else {
            path.strip_prefix(home.as_str())
                .filter(|rest| rest.starts_with('/'))
                .map(str::to_string)
        }
    });
    match relative {
        Some(rest) => format!("~{}", printable(&rest)),
        None => printable(path).into_owned(),
    }
}

/// Tokens as `950`, `12.3k`, `1.2M`: one decimal, a trailing `.0` dropped.
fn short(count: u64) -> String {
    let (unit, suffix) = if count < 1_000 {
        return count.to_string();
    } else if count < 999_950 {
        (1_000, "k")
    } else {
        (1_000_000, "M")
    };
    let tenths = count.saturating_mul(10).saturating_add(unit / 2) / unit;
    if tenths.is_multiple_of(10) {
        format!("{}{suffix}", tenths / 10)
    } else {
        format!("{}.{}{suffix}", tenths / 10, tenths % 10)
    }
}

fn plural(count: u64, one: &str) -> String {
    if count == 1 {
        format!("{count} {one}")
    } else {
        format!("{count} {one}s")
    }
}

/// What an activity cost in tokens: what went in (input, cache read and cache write together), and
/// what came out.
fn totals(activity: &Activity) -> (u64, u64) {
    activity
        .tokens
        .values()
        .fold((0, 0), |(put_in, out), usage| {
            (
                put_in + usage.input_tokens + usage.cache_read_tokens + usage.cache_write_tokens,
                out + usage.output_tokens,
            )
        })
}

fn activity_text(activity: &Activity) -> String {
    let (put_in, out) = totals(activity);
    format!(
        "{}  {} in  {} out",
        plural(activity.sessions, "session"),
        short(put_in),
        short(out)
    )
}

fn day(time: DateTime<Utc>) -> String {
    time.format("%Y-%m-%d").to_string()
}

fn name_of(root: &str) -> String {
    Path::new(root).file_name().map_or_else(
        || root.to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The jobs a scan found, and when it ran.
#[derive(Default)]
struct JobsFound {
    rows: Vec<JobRow>,
    unreadable: u64,
    now: DateTime<Utc>,
}

impl JobsFound {
    fn of(&self, root: &str) -> Vec<&JobRow> {
        self.rows
            .iter()
            .filter(|job| job.project_root.as_deref() == Some(root))
            .collect()
    }

    /// The jobs whose project is not one of `shown`.
    fn elsewhere(&self, shown: &[&FoundProject]) -> Vec<&JobRow> {
        self.rows
            .iter()
            .filter(|job| {
                !shown
                    .iter()
                    .any(|project| job.project_root.as_deref() == Some(project.root.as_str()))
            })
            .collect()
    }
}

/// The services found, and whether Windows' Claude Desktop folders were left unread.
struct ServicesView {
    found: FoundServices,
    several_windows_users: bool,
}

impl Default for ServicesView {
    fn default() -> Self {
        Self {
            found: FoundServices {
                services: Vec::new(),
                by_root: std::collections::BTreeMap::new(),
                formed: Vec::new(),
                bookkeeping_roots: std::collections::BTreeSet::new(),
                not_a_project: Activity::default(),
            },
            several_windows_users: false,
        }
    }
}

impl ServicesView {
    fn of(&self, root: &str) -> &[ServiceUse] {
        self.found.by_root.get(root).map_or(&[], Vec::as_slice)
    }
}

/// A line of the services a project's sessions called, or none.
fn services_line(uses: &[ServiceUse]) -> Option<String> {
    if uses.is_empty() {
        return None;
    }
    let parts: Vec<String> = uses
        .iter()
        .map(|used| {
            format!(
                "{} {}, last {}",
                printable(&used.name),
                plural(used.calls, "call"),
                day(used.last_used_at)
            )
        })
        .collect();
    Some(format!("    services: {}", parts.join("; ")))
}

fn uses_json(uses: &[ServiceUse], view: &ServicesView) -> Vec<Value> {
    uses.iter()
        .map(|used| {
            let kind = view
                .found
                .services
                .iter()
                .find(|service| service.key == used.key)
                .map_or("other", |service| service.kind.as_wire());
            json!({
                "id": used.key,
                "name": used.name,
                "kind": kind,
                "calls": used.calls,
                "last_used_at": moment(used.last_used_at),
            })
        })
        .collect()
}

/// A row among the projects: a folder, or a project formed from connector work.
enum Row<'a> {
    Project(&'a FoundProject),
    Formed(&'a FormedFound),
}

impl Row<'_> {
    fn last_at(&self) -> DateTime<Utc> {
        match self {
            Self::Project(project) => project.last_at,
            Self::Formed(formed) => formed.last_at,
        }
    }

    fn key(&self) -> &str {
        match self {
            Self::Project(project) => &project.root,
            Self::Formed(formed) => &formed.name,
        }
    }
}

/// The projects and formed projects, newest first.
fn rows<'a>(found: &'a Found, services: &'a ServicesView) -> Vec<Row<'a>> {
    let mut rows: Vec<Row<'a>> = found
        .projects
        .iter()
        .map(Row::Project)
        .chain(services.found.formed.iter().map(Row::Formed))
        .collect();
    rows.sort_by(|a, b| {
        b.last_at()
            .cmp(&a.last_at())
            .then_with(|| a.key().cmp(b.key()))
    });
    rows
}

fn is_repository_job(kind: JobKind) -> bool {
    kind <= JobKind::Terraform
}

fn job_line(job: &JobRow, now: DateTime<Utc>, indent: &str) -> String {
    let label = job.kind.label().to_string();
    let schedules = printable(&job.schedules.join("; ")).into_owned();
    let name = printable(&job.display_name).into_owned();
    let parts: Vec<String> = match job.kind {
        JobKind::SessionCron | JobKind::SavedSessionCron => {
            let expiry = job.expires_at.map(|at| {
                let word = if at > now { "until" } else { "expired" };
                format!("{word} {}", day(at))
            });
            vec![label, schedules, expiry.unwrap_or_default()]
        }
        JobKind::BackgroundJob => vec![
            name,
            label,
            printable(job.state.as_deref().unwrap_or_default()).into_owned(),
            job.state_at.map(day).unwrap_or_default(),
        ],
        kind if is_repository_job(kind) => {
            vec![name, label, printable(&job.file).into_owned(), schedules]
        }
        _ => vec![name, label, schedules],
    };
    let parts: Vec<String> = parts.into_iter().filter(|part| !part.is_empty()).collect();
    format!("{indent}{}", parts.join("  "))
}

fn project_lines(
    project: &FoundProject,
    jobs: &JobsFound,
    services: &ServicesView,
    homes: &[String],
) -> Vec<String> {
    let bookkeeping = if services.found.bookkeeping_roots.contains(&project.root) {
        "  bookkeeping"
    } else {
        ""
    };
    let mut lines = vec![format!(
        "  {}  {}{bookkeeping}  last worked {}  {}",
        printable(&name_of(&project.root)),
        shown(&project.root, homes),
        day(project.last_at),
        activity_text(&project.own)
    )];
    if project.catervas.sessions > 0 {
        lines.push(format!(
            "    Catervas's agents  {}",
            activity_text(&project.catervas)
        ));
    }
    let own = jobs.of(&project.root);
    if !own.is_empty() {
        lines.push(format!("    jobs: {}", own.len()));
        lines.extend(own.iter().map(|job| job_line(job, jobs.now, "      ")));
    }
    lines.extend(services_line(services.of(&project.root)));
    lines
}

fn formed_lines(formed: &FormedFound) -> Vec<String> {
    let mut lines = vec![format!(
        "  {}  last worked {}  {}",
        printable(&formed.name),
        day(formed.last_at),
        activity_text(&formed.activity)
    )];
    lines.extend(services_line(&formed.services));
    lines
}

fn gone_line(gone: &GoneFolder, homes: &[String]) -> String {
    format!(
        "  {}  last worked {}  {}",
        shown(&gone.folder, homes),
        day(gone.last_at),
        activity_text(&gone.activity)
    )
}

fn report_lines(
    summary: ReadSummary,
    found: &Found,
    jobs: &JobsFound,
    services: &ServicesView,
    homes: &[String],
) -> Vec<String> {
    let mut lines = vec![format!(
        "read {}: {}, {} new",
        plural(summary.files, "transcript file"),
        plural(summary.sessions, "session"),
        summary.new_sessions
    )];
    if summary.refused > 0 {
        lines.push(format!(
            "refused {} outside Claude Code's transcripts",
            plural(summary.refused, "file")
        ));
    }
    if found.unreadable_sessions > 0 {
        lines.push(format!(
            "couldn't read {} from this version of Claude Code",
            plural(found.unreadable_sessions, "session")
        ));
    }
    if found.unreadable_lines > 0 {
        lines.push(format!(
            "skipped {} it couldn't read",
            plural(found.unreadable_lines, "line")
        ));
    }
    if !jobs.rows.is_empty() {
        lines.push(format!("found {}", plural(jobs.rows.len() as u64, "job")));
    }
    if jobs.unreadable > 0 {
        lines.push(format!(
            "couldn't read jobs from {}",
            plural(jobs.unreadable, "place")
        ));
    }
    let all = rows(found, services);
    for (heading, ownership) in [
        ("projects", Ownership::Yours),
        ("explored", Ownership::Explored),
    ] {
        let section: Vec<&Row<'_>> = all
            .iter()
            .filter(|row| match row {
                Row::Project(project) => project.ownership == ownership,
                Row::Formed(_) => ownership == Ownership::Yours,
            })
            .collect();
        if section.is_empty() {
            continue;
        }
        lines.push(format!("{heading}: {}", section.len()));
        for row in section {
            match row {
                Row::Project(project) => {
                    lines.extend(project_lines(project, jobs, services, homes));
                }
                Row::Formed(formed) => lines.extend(formed_lines(formed)),
            }
        }
    }
    if !found.gone.is_empty() {
        lines.push(format!("no longer on this computer: {}", found.gone.len()));
        lines.extend(found.gone.iter().map(|gone| gone_line(gone, homes)));
    }
    if services.found.not_a_project.sessions > 0 {
        lines.push(format!(
            "not in a project: {}",
            plural(services.found.not_a_project.sessions, "session")
        ));
    }
    let printed: Vec<&FoundProject> = found.projects.iter().collect();
    let elsewhere = jobs.elsewhere(&printed);
    if !elsewhere.is_empty() {
        lines.push(format!(
            "this computer: {}",
            plural(elsewhere.len() as u64, "job")
        ));
        lines.extend(elsewhere.iter().map(|job| job_line(job, jobs.now, "  ")));
    }
    if !services.found.services.is_empty() {
        let names: Vec<String> = services
            .found
            .services
            .iter()
            .map(|service| printable(&service.name).into_owned())
            .collect();
        lines.push(format!(
            "services you use with Claude: {}",
            names.join(", ")
        ));
    }
    lines
}

fn job_json(job: &JobRow) -> Value {
    json!({
        "kind": job.kind.as_wire(),
        "name": job.display_name,
        "file": is_repository_job(job.kind).then_some(&job.file),
        "schedules": job.schedules,
        "expires_at": job.expires_at.map(moment),
        "state": job.state,
        "state_at": job.state_at.map(moment),
        "last_seen": moment(job.last_seen),
    })
}

fn usage_json(usage: &Usage) -> Value {
    json!({
        "input_tokens": usage.input_tokens,
        "output_tokens": usage.output_tokens,
        "cache_read_tokens": usage.cache_read_tokens,
        "cache_write_tokens": usage.cache_write_tokens,
    })
}

fn activity_json(activity: &Activity) -> Value {
    let tokens: serde_json::Map<String, Value> = activity
        .tokens
        .iter()
        .map(|(model, usage)| (model.clone(), usage_json(usage)))
        .collect();
    json!({ "sessions": activity.sessions, "tokens": tokens })
}

fn moment(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn found_json(
    summary: ReadSummary,
    found: &Found,
    jobs: &JobsFound,
    services: &ServicesView,
) -> Value {
    let projects: Vec<Value> = rows(found, services)
        .iter()
        .map(|row| match row {
            Row::Project(project) => json!({
                "root": project.root,
                "name": name_of(&project.root),
                "is_git": project.is_git,
                "ownership": match project.ownership {
                    Ownership::Yours => "yours",
                    Ownership::Explored => "explored",
                },
                "work": services
                    .found
                    .bookkeeping_roots
                    .contains(&project.root)
                    .then_some(Work::Bookkeeping.as_wire()),
                "last_worked_at": moment(project.last_at),
                "own": activity_json(&project.own),
                "catervas": activity_json(&project.catervas),
                "jobs": jobs.of(&project.root).into_iter().map(job_json).collect::<Vec<_>>(),
                "services": uses_json(services.of(&project.root), services),
            }),
            Row::Formed(formed) => json!({
                "root": Value::Null,
                "name": formed.name,
                "is_git": false,
                "ownership": "yours",
                "work": formed.work.as_wire(),
                "last_worked_at": moment(formed.last_at),
                "own": activity_json(&formed.activity),
                "catervas": activity_json(&Activity::default()),
                "jobs": Vec::<Value>::new(),
                "services": uses_json(&formed.services, services),
            }),
        })
        .collect();
    let service_list: Vec<Value> = services
        .found
        .services
        .iter()
        .map(|service| {
            let mut sources: Vec<&str> = service.sources.iter().map(|s| s.as_wire()).collect();
            sources.sort_unstable();
            json!({
                "id": service.key,
                "name": service.name,
                "kind": service.kind.as_wire(),
                "host": service.host,
                "sources": sources,
            })
        })
        .collect();
    let gone: Vec<Value> = found
        .gone
        .iter()
        .map(|gone| {
            let mut object = activity_json(&gone.activity);
            object["folder"] = json!(gone.folder);
            object["last_worked_at"] = json!(moment(gone.last_at));
            object
        })
        .collect();
    json!({
        "files": summary.files,
        "sessions": summary.sessions,
        "new_sessions": summary.new_sessions,
        "refused": summary.refused,
        "unreadable_sessions": found.unreadable_sessions,
        "unreadable_lines": found.unreadable_lines,
        "projects": projects,
        "gone": gone,
        "not_a_project": activity_json(&services.found.not_a_project),
        "services": service_list,
        "desktop": services.several_windows_users.then_some("several_windows_users"),
        "jobs_found": jobs.rows.len(),
        "unreadable_job_sources": jobs.unreadable,
        "this_computer": jobs
            .elsewhere(&found.projects.iter().collect::<Vec<_>>())
            .into_iter()
            .map(job_json)
            .collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::short;

    #[test]
    fn writes_tokens_short() {
        assert_eq!(short(950), "950");
        assert_eq!(short(0), "0");
        assert_eq!(short(999), "999");
        assert_eq!(short(1_000), "1k");
        assert_eq!(short(1_049), "1k");
        assert_eq!(short(1_050), "1.1k");
        assert_eq!(short(2_000), "2k");
        assert_eq!(short(12_300), "12.3k");
        assert_eq!(short(310_000), "310k");
        assert_eq!(short(999_949), "999.9k");
        assert_eq!(short(999_950), "1M");
        assert_eq!(short(1_215_000), "1.2M");
        assert_eq!(short(41_200_000), "41.2M");
    }
}
