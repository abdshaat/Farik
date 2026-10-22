//! Finding projects: the one scan `catervas discover` and the hub's finding both run
//! (`docs/SPEC.md` 4.1, 4.4; ADR 0059).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use catervas_protocol::clock::Clock;
use catervas_runtime::computer::program_output;
use catervas_runtime::connectors::ConnectorSecrets;
use catervas_runtime::daemon::{FindingHost, FindingProgress};
use catervas_runtime::github::{GithubPlaces, load_connection, read_into_index};
use catervas_store::claude_history::{
    ReadProgress, ReadSummary, find_copies, place_sessions, read_transcripts_while,
};
use catervas_store::claude_services::read_service_settings;
use catervas_store::jobs::{JobPlaces, find_jobs};
use catervas_store::machine::{Found, MACHINE_DB, MachineIndex, open_machine_index};

use crate::CliIo;
use crate::discover::{JobsFound, ServicesView, not_projects, settings_paths};
use crate::state::{make_state_dir, read_state, write_state};

/// Where a scan looks, and the clock it stamps what it writes with.
pub(crate) struct ScanPlaces {
    /// The environment the harness runs in: `HOME`, `TMPDIR`, and the `PATH` the schedulers' programs
    /// are found on.
    pub env: BTreeMap<String, String>,
    /// The time a scan is stamped with.
    pub clock: Arc<dyn Clock + Send + Sync>,
    /// Claude Code's folder.
    pub claude: PathBuf,
}

impl ScanPlaces {
    /// The places of a command run in `io`, reading Claude Code's history in `claude`.
    pub(crate) fn of(io: &CliIo<'_>, claude: &Path) -> Self {
        Self {
            env: io.env.clone(),
            clock: Arc::clone(&io.clock),
            claude: claude.to_path_buf(),
        }
    }
}

/// How far a scan has got, told to the scan's caller as it goes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ScanProgress {
    /// Transcript files to read.
    pub files_total: u64,
    /// Transcript files passed so far.
    pub files_read: u64,
    /// Sessions in the index so far.
    pub sessions: u64,
    /// Projects placed so far.
    pub projects: u64,
    /// Jobs found so far.
    pub jobs: u64,
    /// What is being read, such as "sessions from August".
    pub reading: Option<String>,
    /// Transcripts with lines this version of Claude Code's format could not be read from.
    pub unreadable: u64,
}

/// What a finished or stopped scan found.
pub(crate) struct ScanTotals {
    /// What the transcripts added.
    pub summary: ReadSummary,
    /// What the index holds now.
    pub found: Found,
    /// The jobs.
    pub jobs: JobsFound,
    /// The services.
    pub services: ServicesView,
}

/// Reads the transcripts, places and compares the sessions, and finds the jobs and the services,
/// telling `report` how far it has got. `stop`, set while it runs, ends the reading between two
/// files; what was read stays in the index, so the next scan goes on from there.
///
/// # Errors
///
/// A sentence saying what the index, git or a file refused.
pub(crate) fn scan(
    places: &ScanPlaces,
    index: &mut MachineIndex,
    stop: &AtomicBool,
    report: &dyn Fn(ScanProgress),
) -> Result<ScanTotals, String> {
    let now = places.clock.now();
    let mut progress = ScanProgress::default();
    let summary = read_transcripts_while(index, &places.claude, now, &mut |read: &ReadProgress| {
        progress.files_total = read.files_total;
        progress.files_read = read.files_read;
        progress.reading = read
            .next_modified
            .map(|at| format!("sessions from {}", at.format("%B")));
        progress.sessions = read.sessions;
        report(progress.clone());
        !stop.load(Ordering::SeqCst)
    })
    .map_err(|e| e.to_string())?;
    progress.reading = None;
    place_sessions(index, &not_projects(&places.env), now).map_err(|e| e.to_string())?;
    find_copies(index).map_err(|e| e.to_string())?;
    progress.projects = index.found().map_err(|e| e.to_string())?.projects.len() as u64;
    report(progress.clone());
    if summary.stopped {
        return Ok(ScanTotals {
            summary,
            found: index.found().map_err(|e| e.to_string())?,
            jobs: JobsFound {
                now,
                ..JobsFound::default()
            },
            services: ServicesView::default(),
        });
    }
    let home = places
        .env
        .get("HOME")
        .filter(|home| !home.is_empty())
        .map(|home| {
            Path::new(home)
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(home))
        });
    let job_places = JobPlaces {
        home,
        claude_dir: places.claude.clone(),
    };
    let run_program = |program: &str, args: &[&str]| program_output(&places.env, program, args);
    let searched = find_jobs(index, &job_places, &run_program, now).map_err(|e| e.to_string())?;
    let (paths, several_windows_users) = settings_paths(&places.env, &places.claude);
    read_service_settings(index, &paths).map_err(|e| e.to_string())?;
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
    progress.jobs = jobs.rows.len() as u64;
    progress.unreadable = found.unreadable_sessions;
    report(progress);
    Ok(ScanTotals {
        summary,
        found,
        jobs,
        services,
    })
}

/// What the hub's finding asks of the command line: the scan of this computer, and the state
/// `state.json` keeps for it.
pub(crate) struct CliFinder {
    /// The environment `catervas serve` was given.
    pub env: BTreeMap<String, String>,
    /// The time a scan is stamped with.
    pub clock: Arc<dyn Clock + Send + Sync>,
    /// Catervas's state folder.
    pub state: PathBuf,
    /// Claude Code's folder.
    pub claude: PathBuf,
    /// Home.
    pub home: PathBuf,
    /// Where this computer's own keys are kept: the GitHub token among them.
    pub secrets: Arc<dyn ConnectorSecrets>,
    /// Where GitHub is, and how a repository is cloned from it.
    pub github: GithubPlaces,
}

impl CliFinder {
    /// Keeps a change to what `state.json` holds for finding.
    fn keep(&self, change: impl FnOnce(&mut crate::state::ServeState)) -> Result<(), String> {
        let mut state = read_state(&self.state);
        change(&mut state);
        write_state(&self.state, &state)
    }
}

impl FindingHost for CliFinder {
    fn scan(&self, stop: &AtomicBool, report: &dyn Fn(FindingProgress)) -> Result<(), String> {
        make_state_dir(&self.state)?;
        let mut index = open_machine_index(&self.state.join(MACHINE_DB), self.clock.now())
            .map_err(|e| e.to_string())?;
        let places = ScanPlaces {
            env: self.env.clone(),
            clock: Arc::clone(&self.clock),
            claude: self.claude.clone(),
        };
        scan(&places, &mut index, stop, &|progress| {
            report(FindingProgress {
                files_total: progress.files_total,
                files_read: progress.files_read,
                sessions: progress.sessions,
                projects: progress.projects,
                jobs: progress.jobs,
                reading: progress.reading,
                unreadable: progress.unreadable,
            });
        })?;
        // The GitHub phase, once the owner connected it. GitHub saying no (a limit, a token that
        // expired) leaves what was read before, and is not a failed scan.
        if !stop.load(Ordering::SeqCst)
            && let Ok(Some((token, _))) = load_connection(self.secrets.as_ref())
        {
            let _ = read_into_index(&self.github.api, &token, &mut index, self.clock.now(), stop);
        }
        Ok(())
    }

    fn secrets(&self) -> Arc<dyn ConnectorSecrets> {
        Arc::clone(&self.secrets)
    }

    fn github(&self) -> GithubPlaces {
        self.github.clone()
    }

    fn state_dir(&self) -> PathBuf {
        self.state.clone()
    }

    fn claude_dir(&self) -> PathBuf {
        self.claude.clone()
    }

    fn home(&self) -> PathBuf {
        self.home.clone()
    }

    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        self.clock.now()
    }

    fn not_projects(&self) -> Vec<PathBuf> {
        not_projects(&self.env)
    }

    fn paused(&self) -> bool {
        read_state(&self.state).finding_paused
    }

    fn set_paused(&self, paused: bool) -> Result<(), String> {
        self.keep(|state| state.finding_paused = paused)
    }

    fn export_dismissed(&self) -> bool {
        read_state(&self.state).export_dismissed
    }

    fn set_export_dismissed(&self, dismissed: bool) -> Result<(), String> {
        self.keep(|state| state.export_dismissed = dismissed)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use catervas_core::claude_history::fixtures::{a_reply_line, a_turn_line};
    use catervas_core::pricing::Usage;
    use catervas_protocol::clock::FixedClock;
    use catervas_store::files::fixtures::TempProject;
    use catervas_store::machine::{MACHINE_DB, open_machine_index};
    use chrono::{DateTime, Utc};

    use super::{ScanPlaces, scan};

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-12T12:00:00Z")
            .expect("a moment")
            .with_timezone(&Utc)
    }

    /// A Claude folder with five transcripts, one session each, and the places to scan it.
    fn five_transcripts(name: &str) -> (TempProject, ScanPlaces) {
        let temp = TempProject::new(name);
        let claude = temp.root.join("home/.claude");
        let folder = claude.join("projects/-gone-shop");
        std::fs::create_dir_all(&folder).expect("a projects folder");
        for number in 1..=5 {
            let id = format!("1111111{number}-1111-4111-8111-111111111111");
            let at = format!("2026-10-12T09:0{number}:00Z");
            let usage = Usage {
                input_tokens: 10,
                output_tokens: 5,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
            };
            let text = format!(
                "{}\n{}\n",
                a_turn_line(&id, "/gone/shop", &at),
                a_reply_line(
                    &id,
                    "/gone/shop",
                    &at,
                    &format!("msg_{number}"),
                    "claude-opus-5-5",
                    usage
                )
            );
            std::fs::write(folder.join(format!("{id}.jsonl")), text).expect("a transcript");
        }
        let places = ScanPlaces {
            env: BTreeMap::from([(
                "HOME".to_string(),
                temp.root.join("home").to_string_lossy().into_owned(),
            )]),
            clock: Arc::new(FixedClock::new(now())),
            claude,
        };
        (temp, places)
    }

    #[test]
    fn scans_with_progress_and_stops_between_files() {
        let (temp, places) = five_transcripts("scan-progress");
        let mut index =
            open_machine_index(&temp.root.join("state").join(MACHINE_DB), now()).expect("index");
        let seen = RefCell::new(Vec::new());
        let stop = AtomicBool::new(false);
        let totals = scan(&places, &mut index, &stop, &|progress| {
            seen.borrow_mut().push(progress.files_read);
        })
        .expect("a scan");
        let read = seen.borrow().clone();
        assert_eq!((read.first(), read.iter().max()), (Some(&0), Some(&5)));
        assert!(read.windows(2).all(|pair| pair[0] <= pair[1]), "{read:?}");
        assert!(!totals.summary.stopped);
        assert_eq!(totals.summary.files, 5);
        assert_eq!(totals.summary.sessions, 5);
        assert_eq!(totals.found.gone[0].activity.sessions, 5);

        let mut fresh =
            open_machine_index(&temp.root.join("state2").join(MACHINE_DB), now()).expect("index");
        let stop = AtomicBool::new(false);
        let stopped = scan(&places, &mut fresh, &stop, &|progress| {
            if progress.files_read == 2 {
                stop.store(true, Ordering::SeqCst);
            }
        })
        .expect("a stopped scan");
        assert!(stopped.summary.stopped);
        assert_eq!(stopped.summary.files, 2);
        assert_eq!(stopped.summary.sessions, 2);
    }
}
