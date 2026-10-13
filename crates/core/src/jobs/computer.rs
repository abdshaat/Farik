//! What the computer's schedulers say: crontab, systemd timers, launchd agents and Windows Task
//! Scheduler. A job keeps its schedules and its name, never its command line, arguments or
//! environment; the paths it names are returned only to tie it to a project.

use std::path::Path;

use serde_json::Value;

use super::JobKind;
use crate::team::sha256_bytes_hex;

/// A job a scheduler holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputerJob {
    /// Which scheduler holds it.
    pub kind: JobKind,
    /// What identifies it to its scheduler; for crontab, the first 16 hex characters of the
    /// SHA-256 of the trimmed line.
    pub unit: String,
    /// The name shown: the unit, label or task name, and for crontab the program's file name.
    pub name: String,
    /// Its schedules, as written.
    pub schedules: Vec<String>,
    /// The paths it names, to tie it to a project. Callers drop them after tying.
    pub paths: Vec<String>,
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_assignment_word(word: &str) -> bool {
    word.split_once('=')
        .is_some_and(|(name, _)| is_identifier(name))
}

/// A word that starts with a path, with home expanded; other words are not paths.
fn path_word(word: &str, home: Option<&str>) -> Option<String> {
    let word = word.trim_matches(['"', '\'']);
    if word.starts_with('/') {
        return Some(word.to_string());
    }
    let rest = ["~/", "$HOME/", "${HOME}/"]
        .iter()
        .find_map(|prefix| word.strip_prefix(prefix))?;
    Some(format!("{}/{rest}", home?.trim_end_matches('/')))
}

fn first_unescaped_percent(line: &str) -> Option<usize> {
    let mut escaped = false;
    for (index, c) in line.char_indices() {
        match c {
            '\\' if !escaped => escaped = true,
            '%' if !escaped => return Some(index),
            _ => escaped = false,
        }
    }
    None
}

/// A command cut at `&&`, `||`, `;` and `|`.
fn command_parts(command: &str) -> Vec<&str> {
    let bytes = command.as_bytes();
    let (mut parts, mut start, mut index) = (Vec::new(), 0, 0);
    while index < bytes.len() {
        let width = match (bytes[index], bytes.get(index + 1)) {
            (b'&', Some(b'&')) | (b'|', Some(b'|')) => 2,
            (b';' | b'|', _) => 1,
            _ => 0,
        };
        if width > 0 {
            parts.push(&command[start..index]);
            start = index + width;
            index += width;
        } else {
            index += 1;
        }
    }
    parts.push(&command[start..]);
    parts
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

fn looks_like_cron_field(field: &str, first: bool) -> bool {
    let starts_ok = !first || field.starts_with(|c: char| c.is_ascii_digit() || c == '*');
    starts_ok
        && field
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '*' | '/' | ',' | '-' | '?'))
}

/// Splits a crontab line into its schedule and its command: five fields, or one `@word`.
fn schedule_and_command(line: &str) -> Option<(String, &str)> {
    if let Some(rest) = line.strip_prefix('@') {
        let (word, command) = rest.split_once(char::is_whitespace)?;
        let valid = !word.is_empty() && word.chars().all(|c| c.is_ascii_alphabetic());
        return (valid && !command.trim().is_empty()).then(|| (format!("@{word}"), command.trim()));
    }
    let (mut fields, mut rest) = (Vec::new(), line);
    for _ in 0..5 {
        let trimmed = rest.trim_start();
        let end = trimmed.find(char::is_whitespace)?;
        fields.push(&trimmed[..end]);
        rest = &trimmed[end..];
    }
    let valid = fields
        .iter()
        .enumerate()
        .all(|(index, field)| looks_like_cron_field(field, index < 2));
    (valid && !rest.trim().is_empty()).then(|| (fields.join(" "), rest.trim()))
}

/// Reads `crontab -l`. Blank, comment and `NAME=value` lines are skipped; a job is five schedule
/// fields, or one `@word`, and a command. Text after the first unescaped `%` is dropped. The job's
/// name is the file name of the command's program, the first word of the first part that is not
/// `cd`; nothing else of the line is kept.
#[must_use]
pub fn read_crontab(text: &str, home: Option<&str>) -> Vec<ComputerJob> {
    text.lines()
        .map(str::trim)
        .filter_map(|line| {
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            if line
                .split_once('=')
                .is_some_and(|(name, _)| is_identifier(name.trim()))
            {
                return None;
            }
            let cut = first_unescaped_percent(line).map_or(line, |index| &line[..index]);
            let (schedule, command) = schedule_and_command(cut)?;
            let program = command_parts(command).into_iter().find_map(|part| {
                let word = part
                    .split_whitespace()
                    .find(|word| !is_assignment_word(word))?;
                (word != "cd").then(|| {
                    let word = word.trim_matches(['"', '\'']);
                    Path::new(word).file_name().map_or_else(
                        || word.to_string(),
                        |name| name.to_string_lossy().into_owned(),
                    )
                })
            })?;
            let mut paths: Vec<String> = Vec::new();
            for path in command
                .split_whitespace()
                .filter_map(|word| path_word(word, home))
            {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
            Some(ComputerJob {
                kind: JobKind::Crontab,
                unit: sha256_bytes_hex(line.as_bytes())[..16].to_string(),
                name: program,
                schedules: vec![schedule],
                paths,
            })
        })
        .collect()
}

/// The timers in `systemctl list-timers --no-legend` and the unit each starts, read from the end
/// of each line because the date columns before them hold spaces.
#[must_use]
pub fn timer_units(list: &str) -> Vec<(String, String)> {
    list.lines()
        .filter_map(|line| {
            let words: Vec<&str> = line.split_whitespace().collect();
            let [.., timer, unit] = words.as_slice() else {
                return None;
            };
            timer
                .strip_suffix(".timer")
                .is_some()
                .then(|| ((*timer).to_string(), (*unit).to_string()))
        })
        .collect()
}

/// The blocks of `systemctl cat`'s output, each with the path of its `# <path>` header.
fn unit_blocks(cat: &str) -> Vec<(&str, Vec<&str>)> {
    let mut blocks: Vec<(&str, Vec<&str>)> = Vec::new();
    for line in cat.lines() {
        if let Some(path) = line.strip_prefix("# /") {
            blocks.push((path, Vec::new()));
        } else if let Some((_, lines)) = blocks.last_mut() {
            lines.push(line.trim());
        }
    }
    blocks
}

fn systemd_path(value: &str, home: Option<&str>) -> Option<String> {
    let value = value.trim().trim_start_matches(['-', '@', ':', '+', '!']);
    let word = value.split_whitespace().next()?;
    let word = match word.strip_prefix("%h") {
        Some(rest) => format!("{}{rest}", home?.trim_end_matches('/')),
        None => word.to_string(),
    };
    word.starts_with('/').then_some(word)
}

/// Reads a timer and its unit from `systemctl cat`. `None` for a timer that came with the system
/// (its file under `/usr/lib/` or `/lib/`) and for one with no schedule. The schedules are each
/// `OnCalendar=` value, then each other `On…Sec=` line as written; the paths come from
/// `ExecStart=`, without its prefixes, and `WorkingDirectory=`, with `%h` expanded.
#[must_use]
pub fn read_systemd_timer(
    kind: JobKind,
    timer: &str,
    cat: &str,
    home: Option<&str>,
) -> Option<ComputerJob> {
    let blocks = unit_blocks(cat);
    let is_timer = |path: &str| path.strip_suffix(".timer").is_some() || path.contains(".timer.d/");
    let (first_timer, _) = blocks.iter().find(|(path, _)| is_timer(path))?;
    if first_timer.starts_with("usr/lib/") || first_timer.starts_with("lib/") {
        return None;
    }
    let timer_lines = || {
        blocks
            .iter()
            .filter(|(path, _)| is_timer(path))
            .flat_map(|(_, lines)| lines.iter().copied())
    };
    let mut schedules: Vec<String> = timer_lines()
        .filter_map(|line| line.strip_prefix("OnCalendar="))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    schedules.extend(
        timer_lines()
            .filter(|line| {
                line.split_once('=').is_some_and(|(key, value)| {
                    key.starts_with("On") && key.ends_with("Sec") && !value.trim().is_empty()
                })
            })
            .map(str::to_string),
    );
    if schedules.is_empty() {
        return None;
    }
    let service_lines = || {
        blocks
            .iter()
            .filter(|(path, _)| !is_timer(path))
            .flat_map(|(_, lines)| lines.iter().copied())
    };
    let exec = service_lines()
        .filter_map(|line| line.strip_prefix("ExecStart="))
        .find_map(|value| systemd_path(value, home));
    let directory = service_lines()
        .filter_map(|line| line.strip_prefix("WorkingDirectory="))
        .find_map(|value| systemd_path(value, home));
    Some(ComputerJob {
        kind,
        unit: timer.to_string(),
        name: timer.strip_suffix(".timer").unwrap_or(timer).to_string(),
        schedules,
        paths: exec.into_iter().chain(directory).collect(),
    })
}

fn calendar_field(entry: &Value, key: &str) -> String {
    entry
        .get(key)
        .and_then(Value::as_u64)
        .map_or_else(|| "*".to_string(), |number| number.to_string())
}

/// Reads one launchd agent from `plutil -convert json`: its `Label` (else the file's stem), its
/// `StartInterval` and `StartCalendarInterval` as schedules, and the paths of `Program` or the first
/// of `ProgramArguments`, and of `WorkingDirectory`. `None` for an agent with no interval.
#[must_use]
pub fn read_launchd_agent(file_stem: &str, plutil_json: &str) -> Option<ComputerJob> {
    let agent: Value = serde_json::from_str(plutil_json).ok()?;
    let mut schedules = Vec::new();
    if let Some(seconds) = agent.get("StartInterval").and_then(Value::as_u64) {
        schedules.push(format!("every {seconds} seconds"));
    }
    let calendar = agent.get("StartCalendarInterval");
    let entries: Vec<&Value> = match calendar {
        Some(Value::Array(entries)) => entries.iter().collect(),
        Some(entry @ Value::Object(_)) => vec![entry],
        _ => Vec::new(),
    };
    for entry in entries {
        schedules.push(
            ["Minute", "Hour", "Day", "Month", "Weekday"]
                .map(|key| calendar_field(entry, key))
                .join(" "),
        );
    }
    if schedules.is_empty() {
        return None;
    }
    let name = agent
        .get("Label")
        .and_then(Value::as_str)
        .unwrap_or(file_stem)
        .to_string();
    let program = agent
        .get("Program")
        .and_then(Value::as_str)
        .or_else(|| agent.get("ProgramArguments")?.as_array()?.first()?.as_str());
    let directory = agent.get("WorkingDirectory").and_then(Value::as_str);
    Some(ComputerJob {
        kind: JobKind::Launchd,
        unit: name.clone(),
        name,
        schedules,
        paths: program
            .into_iter()
            .chain(directory)
            .map(str::to_string)
            .collect(),
    })
}

/// A CSV text as records of fields, quote-aware: a comma, a doubled quote or a line break inside
/// quotes belongs to the field.
fn csv_records(text: &str) -> Vec<Vec<String>> {
    let (mut records, mut record, mut field) = (Vec::new(), Vec::new(), String::new());
    let (mut in_quotes, mut chars) = (false, text.chars().peekable());
    while let Some(c) = chars.next() {
        match (c, in_quotes) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => in_quotes = !in_quotes,
            (',', false) => record.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records.retain(|record| record.iter().any(|field| !field.is_empty()));
    records
}

/// A Windows path as WSL sees it: `X:\…` is `/mnt/x/…`, `\\wsl.localhost\<distro>\…` and
/// `\\wsl$\<distro>\…` are `/…`, and a word already starting `/` stays. Anything else is not a path.
fn wsl_path(word: &str) -> Option<String> {
    if word.starts_with('/') {
        return Some(word.to_string());
    }
    let slashes = |text: &str| text.replace('\\', "/");
    for prefix in [r"\\wsl.localhost\", r"\\wsl$\"] {
        if let Some(rest) = word.strip_prefix(prefix) {
            let inside = rest.split_once('\\').map_or("", |(_, inside)| inside);
            return Some(format!("/{}", slashes(inside)));
        }
    }
    let bytes = word.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
    {
        let drive = char::from(bytes[0]).to_ascii_lowercase();
        return Some(format!("/mnt/{drive}/{}", slashes(&word[3..])));
    }
    None
}

/// A command line split into words, a double-quoted run being one word.
fn command_words(command: &str) -> Vec<String> {
    let (mut words, mut word, mut in_quotes) = (Vec::new(), String::new(), false);
    for c in command.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// Reads `schtasks.exe /query /fo CSV /v`. Columns are found by their English header, so `None`
/// when there is no `TaskName` header (Windows in another language). A task's rows are one job; a
/// row's schedule is `Schedule Type`, `Start Time`, `Days` and `Repeat: Every`, joined with `, `,
/// leaving out `N/A`, `Disabled` and empty values. The paths come from `Task To Run` and
/// `Start In`; nothing else of a row is kept.
#[must_use]
pub fn read_task_scheduler(csv: &str) -> Option<Vec<ComputerJob>> {
    let records = csv_records(csv);
    let header_at = records
        .iter()
        .position(|record| record.iter().any(|field| field == "TaskName"))?;
    let header = &records[header_at];
    let column = |name: &str| header.iter().position(|field| field == name);
    let task_column = column("TaskName")?;
    let schedule_columns: Vec<usize> = ["Schedule Type", "Start Time", "Days", "Repeat: Every"]
        .iter()
        .filter_map(|name| column(name))
        .collect();
    let run_column = column("Task To Run");
    let start_column = column("Start In");
    let mut jobs: Vec<ComputerJob> = Vec::new();
    for record in &records[header_at + 1..] {
        let cell = |index: usize| record.get(index).map_or("", |field| field.trim());
        let name = cell(task_column);
        if name == "TaskName" {
            continue;
        }
        let index = if name.is_empty() {
            jobs.len().checked_sub(1)?
        } else if let Some(index) = jobs.iter().position(|job| job.name == name) {
            index
        } else {
            jobs.push(ComputerJob {
                kind: JobKind::TaskScheduler,
                unit: name.to_string(),
                name: name.to_string(),
                schedules: Vec::new(),
                paths: Vec::new(),
            });
            jobs.len() - 1
        };
        let schedule = schedule_columns
            .iter()
            .map(|index| cell(*index))
            .filter(|value| !matches!(*value, "" | "N/A" | "Disabled"))
            .collect::<Vec<_>>()
            .join(", ");
        let job = &mut jobs[index];
        if !schedule.is_empty() && !job.schedules.contains(&schedule) {
            job.schedules.push(schedule);
        }
        let words = run_column
            .map(|index| command_words(cell(index)))
            .unwrap_or_default();
        let start = start_column.map(|index| cell(index).to_string());
        for path in words
            .iter()
            .chain(start.iter())
            .filter_map(|word| wsl_path(word))
        {
            if !job.paths.contains(&path) {
                job.paths.push(path);
            }
        }
    }
    Some(jobs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::fixtures::{
        CRONTAB, LAUNCHD_KEEP_ALIVE, LAUNCHD_SHOP_SYNC, SCHTASKS_CSV, SYSTEMCTL_CAT_DB_BACKUP,
        SYSTEMCTL_CAT_LOGROTATE, SYSTEMCTL_CAT_SHOP_SYNC, SYSTEMCTL_TIMERS_SYSTEM,
        SYSTEMCTL_TIMERS_USER,
    };

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn reads_a_crontab() {
        let jobs = read_crontab(CRONTAB, Some("/home/me"));
        let seen: Vec<(&[String], &str)> = jobs
            .iter()
            .map(|job| (job.schedules.as_slice(), job.name.as_str()))
            .collect();
        let sync = strings(&["0 2 * * *"]);
        let publish = strings(&["*/15 * * * *"]);
        let daily = strings(&["@daily"]);
        let all = strings(&["0 1 * * *"]);
        assert_eq!(
            seen,
            [
                (sync.as_slice(), "sync.sh"),
                (publish.as_slice(), "publish.sh"),
                (daily.as_slice(), "backup.sh"),
                (all.as_slice(), "backup-all"),
            ]
        );
        assert!(jobs.iter().all(|job| job.kind == JobKind::Crontab));
        assert_eq!(jobs[0].paths, strings(&["/home/me/shop/bin/sync.sh"]));
        assert!(jobs[1].paths.contains(&"/home/me/blog".to_string()));
        assert_eq!(jobs[2].paths, strings(&["/home/me/notes/backup.sh"]));
        assert_eq!(jobs[3].paths, strings(&["/usr/bin/backup-all"]));
        for job in &jobs {
            assert_eq!(job.unit.len(), 16);
            assert!(job.unit.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')));
        }
        let units: std::collections::BTreeSet<&str> =
            jobs.iter().map(|job| job.unit.as_str()).collect();
        assert_eq!(units.len(), 4);
        assert_eq!(jobs, read_crontab(CRONTAB, Some("/home/me")));
    }

    /// Guard: checked by mutation (keep the line, an argument or an environment value), not
    /// watched to fail.
    #[test]
    fn never_reads_a_token_from_a_crontab_line() {
        let jobs = read_crontab(CRONTAB, Some("/home/me"));
        assert!(!format!("{jobs:?}").contains("SECRET"), "{jobs:?}");
    }

    #[test]
    fn reads_systemd_timers() {
        assert_eq!(
            timer_units(SYSTEMCTL_TIMERS_SYSTEM),
            [
                (
                    "logrotate.timer".to_string(),
                    "logrotate.service".to_string()
                ),
                (
                    "db-backup.timer".to_string(),
                    "db-backup.service".to_string()
                ),
            ]
        );
        assert_eq!(
            timer_units(SYSTEMCTL_TIMERS_USER),
            [(
                "shop-sync.timer".to_string(),
                "shop-sync.service".to_string()
            )]
        );
        assert_eq!(timer_units(""), []);
        let system = JobKind::SystemdSystem;
        assert_eq!(
            read_systemd_timer(system, "logrotate.timer", SYSTEMCTL_CAT_LOGROTATE, None),
            None
        );
        let backup = read_systemd_timer(system, "db-backup.timer", SYSTEMCTL_CAT_DB_BACKUP, None)
            .expect("a job");
        assert_eq!(backup.kind, system);
        assert_eq!(backup.unit, "db-backup.timer");
        assert_eq!(backup.name, "db-backup");
        assert_eq!(
            backup.schedules,
            strings(&["OnBootSec=15min", "OnUnitActiveSec=1d"])
        );
        assert_eq!(backup.paths, strings(&["/usr/local/bin/db-backup"]));
        let sync = read_systemd_timer(
            JobKind::SystemdUser,
            "shop-sync.timer",
            SYSTEMCTL_CAT_SHOP_SYNC,
            Some("/home/me"),
        )
        .expect("a job");
        assert_eq!(sync.name, "shop-sync");
        assert_eq!(sync.schedules, strings(&["*-*-* 02:30:00"]));
        assert_eq!(
            sync.paths,
            strings(&["/home/me/shop/bin/sync", "/home/me/shop"])
        );
        assert!(!format!("{backup:?}{sync:?}").contains("SECRET"));
        let no_schedule = "# /etc/systemd/system/x.timer\n[Timer]\nPersistent=true\n";
        assert_eq!(
            read_systemd_timer(system, "x.timer", no_schedule, None),
            None
        );
    }

    #[test]
    fn reads_a_launchd_agent() {
        let agent = read_launchd_agent("com.me.shop-sync", LAUNCHD_SHOP_SYNC).expect("a job");
        assert_eq!(agent.kind, JobKind::Launchd);
        assert_eq!(
            (agent.unit.as_str(), agent.name.as_str()),
            ("com.me.shop-sync", "com.me.shop-sync")
        );
        assert_eq!(agent.schedules, strings(&["30 2 * * *", "0 9 * * 1"]));
        assert_eq!(
            agent.paths,
            strings(&["/home/me/shop/bin/sync", "/home/me/shop"])
        );
        assert!(!format!("{agent:?}").contains("SECRET"));
        let interval = r#"{"Label":"x.y","Program":"/usr/bin/z","StartInterval":3600}"#;
        let every = read_launchd_agent("file-stem", interval).expect("a job");
        assert_eq!(every.schedules, strings(&["every 3600 seconds"]));
        assert_eq!(every.paths, strings(&["/usr/bin/z"]));
        let unlabelled = read_launchd_agent("file-stem", r#"{"StartInterval":60}"#).expect("a job");
        assert_eq!(unlabelled.name, "file-stem");
        assert_eq!(read_launchd_agent("keep", LAUNCHD_KEEP_ALIVE), None);
        assert_eq!(read_launchd_agent("bad", "not json"), None);
    }

    #[test]
    fn reads_windows_task_scheduler() {
        let jobs = read_task_scheduler(SCHTASKS_CSV).expect("a header");
        let names: Vec<&str> = jobs.iter().map(|job| job.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "\\Microsoft\\Windows\\Defrag\\ScheduledDefrag",
                "\\Adobe Acrobat Update Task",
                "\\ShopSync",
                "\\ShopReport",
            ]
        );
        assert!(
            jobs.iter()
                .all(|job| job.kind == JobKind::TaskScheduler && job.unit == job.name)
        );
        assert_eq!(
            jobs[1].schedules,
            strings(&["At logon time", "Daily, 11:00:00 AM, Every 1 day(s)"])
        );
        assert_eq!(jobs[2].schedules, strings(&["Daily, 2:30:00 AM"]));
        assert!(
            jobs[2]
                .paths
                .contains(&"/home/me/shop/bin/sync".to_string())
        );
        assert!(jobs[2].paths.contains(&"/home/me/shop".to_string()));
        assert_eq!(
            jobs[3].paths,
            strings(&["/mnt/c/Users/me/shop/report, final.exe"])
        );
        assert!(!format!("{jobs:?}").contains("SECRET"));
        let other_language = "\"Nom d'hote\",\"Nom de la tache\"\n\"PC\",\"\\X\"\n";
        assert_eq!(read_task_scheduler(other_language), None);
        assert_eq!(read_task_scheduler(""), None);
    }
}
