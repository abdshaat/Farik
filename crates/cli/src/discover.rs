//! `catervas discover`: what Catervas found in the Claude Code history.

use std::path::{Path, PathBuf};

use catervas_core::pricing::Usage;
use catervas_store::claude_history::{ReadSummary, place_sessions, read_transcripts};
use catervas_store::machine::{
    Activity, Found, FoundProject, GoneFolder, MACHINE_DB, Ownership, forget_machine_index,
    open_machine_index,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};

use crate::printable::printable;
use crate::state::{claude_dir, make_state_dir, state_dir};
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
        };
        return Ok(Report {
            lines: vec![first, "no Claude Code history on this computer".to_string()],
            json: found_json(ReadSummary::default(), &none),
            json_lines: None,
        });
    }
    make_state_dir(&state)?;
    let mut index =
        open_machine_index(&state.join(MACHINE_DB), io.clock.now()).map_err(|e| e.to_string())?;
    let summary =
        read_transcripts(&mut index, &claude, io.clock.now()).map_err(|e| e.to_string())?;
    place_sessions(&mut index, &not_projects(io), io.clock.now()).map_err(|e| e.to_string())?;
    let found = index.found().map_err(|e| e.to_string())?;
    let mut lines = vec![first];
    lines.extend(report_lines(summary, &found, &homes));
    Ok(Report {
        lines,
        json: found_json(summary, &found),
        json_lines: None,
    })
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

fn project_lines(project: &FoundProject, homes: &[String]) -> Vec<String> {
    let mut lines = vec![format!(
        "  {}  {}  last worked {}  {}",
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

fn report_lines(summary: ReadSummary, found: &Found, homes: &[String]) -> Vec<String> {
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
    for (heading, ownership) in [
        ("projects", Ownership::Yours),
        ("explored", Ownership::Explored),
    ] {
        let section: Vec<&FoundProject> = found
            .projects
            .iter()
            .filter(|project| project.ownership == ownership)
            .collect();
        if section.is_empty() {
            continue;
        }
        lines.push(format!("{heading}: {}", section.len()));
        for project in section {
            lines.extend(project_lines(project, homes));
        }
    }
    if !found.gone.is_empty() {
        lines.push(format!("no longer on this computer: {}", found.gone.len()));
        lines.extend(found.gone.iter().map(|gone| gone_line(gone, homes)));
    }
    if found.not_a_project.sessions > 0 {
        lines.push(format!(
            "not in a project: {}",
            plural(found.not_a_project.sessions, "session")
        ));
    }
    lines
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

fn found_json(summary: ReadSummary, found: &Found) -> Value {
    let projects: Vec<Value> = found
        .projects
        .iter()
        .map(|project| {
            json!({
                "root": project.root,
                "name": name_of(&project.root),
                "is_git": project.is_git,
                "ownership": match project.ownership {
                    Ownership::Yours => "yours",
                    Ownership::Explored => "explored",
                },
                "last_worked_at": moment(project.last_at),
                "own": activity_json(&project.own),
                "catervas": activity_json(&project.catervas),
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
        "not_a_project": activity_json(&found.not_a_project),
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
