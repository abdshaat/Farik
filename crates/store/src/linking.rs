//! The committed files that proofs of linking come from, and what the owner answered about each
//! pair of projects (`docs/SPEC.md` 4.4, 8.4; ADR 0059).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use catervas_core::linking::{
    ClaudeAiProjectText, Hint, HintKind, MAX_FILE_BYTES, MAX_FILES, ProjectFiles, ProjectHintInput,
    Proof, ProofKind, SessionNote, find_hints, find_proofs, find_shared_work, is_read_for_proof,
    still_holds,
};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use rusqlite::TransactionBehavior;

use crate::error::StoreError;
use crate::git::{Git, GitError};
use crate::machine::MachineIndex;

/// The files of the repository at `git` that proofs are read from, as `(path, text)`, at `HEAD`
/// through `git show` and never from the working tree. A file over 256 KiB is skipped, no more
/// than 300 are read (the shallowest first), and nothing under `node_modules`, `vendor`, `target`
/// or `.git` is. A folder that is no git repository, or has no commit yet, gives none.
///
/// # Errors
///
/// `NotInstalled` when the `git` program cannot be run.
pub fn committed_files(git: &Git) -> Result<Vec<(String, String)>, GitError> {
    let sizes = match git.tree_sizes("HEAD") {
        Ok(sizes) => sizes,
        Err(GitError::NotARepository | GitError::CommandFailed { .. }) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut wanted: Vec<String> = sizes
        .into_iter()
        .filter(|(path, size)| *size <= MAX_FILE_BYTES && is_read_for_proof(path))
        .map(|(path, _)| path)
        .collect();
    wanted.sort_by(|a, b| {
        let depth = |path: &str| path.matches('/').count();
        depth(a).cmp(&depth(b)).then_with(|| a.cmp(b))
    });
    wanted.truncate(MAX_FILES);
    let mut files = Vec::new();
    for path in wanted {
        match git.file_at("HEAD", &path) {
            Ok(text) => files.push((path, text)),
            Err(GitError::CommandFailed { .. }) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(files)
}

/// The files of the repository at `root`, as `committed_files` reads them; none when it cannot be
/// read, so that a project whose files cannot be read has no proof and only hints.
#[must_use]
pub fn read_from_git(root: &str) -> Vec<(String, String)> {
    committed_files(&Git::open(root.into())).unwrap_or_default()
}

/// What the owner can answer about a pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// One team.
    Yes,
    /// Keep separate.
    No,
}

impl Answer {
    /// The answer as it is written on the wire and in the index.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
        }
    }

    /// The answer a wire word names.
    #[must_use]
    pub fn from_wire(wire: &str) -> Option<Self> {
        match wire {
            "yes" => Some(Self::Yes),
            "no" => Some(Self::No),
            _ => None,
        }
    }
}

/// Why a pair could not be answered.
#[derive(Debug)]
pub enum LinkingError {
    /// The index refused.
    Store(StoreError),
    /// A Yes was given to a pair whose proof does not hold now.
    ProofNoLongerHolds,
    /// A key is not one of the projects.
    UnknownProject,
}

impl fmt::Display for LinkingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "{error}"),
            Self::ProofNoLongerHolds => {
                write!(formatter, "the files that proved it no longer say so")
            }
            Self::UnknownProject => write!(formatter, "Catervas doesn't know that project"),
        }
    }
}

impl std::error::Error for LinkingError {}

impl From<StoreError> for LinkingError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// A project among those that can be linked: what the proofs and the hints need of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkProject {
    /// Its key, as the found list gives it.
    pub key: String,
    /// The name shown.
    pub name: String,
    /// When it was last worked on.
    pub last_at: Option<DateTime<Utc>>,
    /// The folder shown, when it has one.
    pub folder: Option<String>,
    /// Its repository's normalised remote.
    pub remote_key: Option<String>,
    /// The folder its committed files are read from.
    pub root: Option<String>,
    /// The days (UTC) with sessions in it.
    pub active_days: BTreeSet<NaiveDate>,
    /// The sessions that wrote a file under its root.
    pub written_in: BTreeSet<String>,
}

/// What the files and sessions say of the projects now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evaluation {
    /// Every proof, in the order of the kinds.
    pub proofs: Vec<Proof>,
    /// The hints of pairs that have no proof.
    pub hints: Vec<Hint>,
    /// The hints of every pair, proven or not, which a proof's screen lists as "Also".
    pub all_hints: Vec<Hint>,
}

/// Reads the files of every project with `read` and finds the proofs and hints among them. The
/// shared work of the sessions in `sessions` is a proof, ahead of the files' proofs.
#[must_use]
pub fn evaluate(
    projects: &[LinkProject],
    claude_ai: &[ClaudeAiProjectText],
    sessions: &BTreeMap<String, SessionNote>,
    read: &dyn Fn(&str) -> Vec<(String, String)>,
) -> Evaluation {
    let files: Vec<ProjectFiles> = projects
        .iter()
        .map(|project| ProjectFiles {
            key: project.key.clone(),
            name: project.name.clone(),
            remote_key: project.remote_key.clone(),
            root: project.root.clone(),
            files: project.root.as_deref().map(read).unwrap_or_default(),
        })
        .collect();
    let inputs: Vec<ProjectHintInput> = projects
        .iter()
        .map(|project| ProjectHintInput {
            key: project.key.clone(),
            name: project.name.clone(),
            remote_key: project.remote_key.clone(),
            active_days: project.active_days.clone(),
            written_in: project.written_in.clone(),
        })
        .collect();
    let mut proofs = find_shared_work(&inputs, sessions);
    proofs.extend(find_proofs(&files, claude_ai));
    // By kind, then keys, as `find_proofs` orders them; the sort is stable.
    proofs.sort_by(|x, y| (x.kind, &x.a, &x.b).cmp(&(y.kind, &y.a, &y.b)));
    Evaluation {
        hints: find_hints(&inputs, &proofs),
        all_hints: find_hints(&inputs, &[]),
        proofs,
    }
}

/// An answer kept for a pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredLink {
    /// What the owner said.
    pub answer: Answer,
    /// The proofs shown when the owner said it; none for a pair with hints only.
    pub proofs: Vec<Proof>,
}

fn pair_of(a: &str, b: &str) -> (String, String) {
    if a < b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

impl MachineIndex {
    /// Every answer the owner gave about a pair of projects, by the pair's keys, the lesser first.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read or holds an answer that is not one.
    pub fn link_answers(&self) -> Result<BTreeMap<(String, String), StoredLink>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT a, b, answer, proof FROM links")?;
        let mut rows = statement.query([])?;
        let mut answers = BTreeMap::new();
        while let Some(row) = rows.next()? {
            let wire: String = row.get(2)?;
            let answer = Answer::from_wire(&wire).ok_or_else(|| StoreError::Sqlite {
                detail: "the index holds a link answer that is not one".to_string(),
            })?;
            let proofs = row
                .get::<_, Option<String>>(3)?
                .and_then(|json| serde_json::from_str::<Vec<serde_json::Value>>(&json).ok())
                .map(|values| values.iter().filter_map(Proof::from_json).collect())
                .unwrap_or_default();
            answers.insert((row.get(0)?, row.get(1)?), StoredLink { answer, proofs });
        }
        Ok(answers)
    }

    /// Keeps the owner's answer for a pair, with the proofs shown, replacing an earlier one.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index refuses.
    pub fn put_link(
        &mut self,
        a: &str,
        b: &str,
        answer: Answer,
        proofs: &[Proof],
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let (a, b) = pair_of(a, b);
        let json = (!proofs.is_empty()).then(|| {
            serde_json::Value::Array(proofs.iter().map(Proof::to_json).collect()).to_string()
        });
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO links (a, b, answer, proof, at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (a, b) DO UPDATE SET answer = excluded.answer, proof = excluded.proof,
                                              at = excluded.at",
            (
                &a,
                &b,
                answer.as_wire(),
                &json,
                now.to_rfc3339_opts(SecondsFormat::Millis, true),
            ),
        )?;
        transaction.commit()?;
        Ok(())
    }
}

/// What the sessions say of each root, for the hints.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LinkActivity {
    /// The days (UTC) with a session that started or last did something in the root.
    pub days_by_root: BTreeMap<String, BTreeSet<NaiveDate>>,
    /// The sessions of the owner that wrote a file under the root; a session placed with
    /// Catervas's agents is not among them.
    pub written_by_root: BTreeMap<String, BTreeSet<String>>,
    /// What is known of each session in `written_by_root`: its title and when it last did
    /// anything.
    pub sessions: BTreeMap<String, SessionNote>,
}

impl MachineIndex {
    /// The days with sessions and the sessions that wrote, by root.
    ///
    /// # Errors
    ///
    /// `Sqlite` when the index cannot be read.
    pub fn link_activity(&self) -> Result<LinkActivity, StoreError> {
        let mut activity = LinkActivity::default();
        let mut statement = self.connection.prepare(
            "SELECT project_root, first_at, last_at FROM sessions WHERE project_root IS NOT NULL",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let root: String = row.get(0)?;
            for column in [1, 2] {
                let moment: Option<String> = row.get(column)?;
                if let Some(day) = moment
                    .and_then(|text| DateTime::parse_from_rfc3339(&text).ok())
                    .map(|moment| moment.with_timezone(&Utc).date_naive())
                {
                    activity
                        .days_by_root
                        .entry(root.clone())
                        .or_default()
                        .insert(day);
                }
            }
        }
        let mut statement = self.connection.prepare(
            "SELECT w.session_id, w.root, s.title, s.last_at
             FROM session_roots_written w LEFT JOIN sessions s ON s.session_id = w.session_id
             WHERE s.place IS NULL OR s.place != 'catervas'",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let session: String = row.get(0)?;
            let last_at = row
                .get::<_, Option<String>>(3)?
                .and_then(|text| DateTime::parse_from_rfc3339(&text).ok())
                .map(|moment| moment.with_timezone(&Utc));
            if let Some(last_at) = last_at {
                activity
                    .sessions
                    .entry(session.clone())
                    .or_insert(SessionNote {
                        title: row.get(2)?,
                        last_at,
                    });
            }
            activity
                .written_by_root
                .entry(row.get(1)?)
                .or_default()
                .insert(session);
        }
        Ok(activity)
    }
}

/// A pair with a proof the owner is to be asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskedPair {
    /// The proof to show.
    pub proof: Proof,
    /// The sentences of the pair's other proofs.
    pub also: Vec<String>,
    /// The hints the pair has besides.
    pub hints: Vec<HintKind>,
}

/// The projects joined by the Yes answers that hold, as groups of keys (a project alone is no
/// group), each key in one group.
fn yes_groups(yes: &[(String, String)], known: &BTreeSet<&str>) -> Vec<BTreeSet<String>> {
    let mut groups: Vec<BTreeSet<String>> = Vec::new();
    for (a, b) in yes {
        if !known.contains(a.as_str()) || !known.contains(b.as_str()) {
            continue;
        }
        let (mut joined, rest): (Vec<_>, Vec<_>) = groups
            .drain(..)
            .partition(|group| group.contains(a) || group.contains(b));
        groups = rest;
        let mut merged = BTreeSet::from([a.clone(), b.clone()]);
        for group in joined.drain(..) {
            merged.extend(group);
        }
        groups.push(merged);
    }
    groups
}

/// The pairs with a proof that still holds and no answer, and the pairs answered No for which a
/// proof of a kind not shown with the No is now found. A pair whose projects are one group through
/// other Yes answers is not asked. Each project with a group comes after the pair that formed it.
///
/// # Errors
///
/// `Sqlite` when the index cannot be read.
pub fn asked_pairs(
    index: &MachineIndex,
    projects: &[LinkProject],
    evaluation: &Evaluation,
) -> Result<Vec<AskedPair>, StoreError> {
    let answers = index.link_answers()?;
    let known: BTreeSet<&str> = projects
        .iter()
        .map(|project| project.key.as_str())
        .collect();
    let yes: Vec<(String, String)> = answers
        .iter()
        .filter(|(_, link)| link.answer == Answer::Yes)
        .map(|(pair, _)| pair.clone())
        .collect();
    let groups = yes_groups(&yes, &known);
    let joined = |a: &str, b: &str| {
        groups
            .iter()
            .any(|group| group.contains(a) && group.contains(b))
    };

    let mut by_pair: BTreeMap<(String, String), Vec<&Proof>> = BTreeMap::new();
    for proof in &evaluation.proofs {
        by_pair
            .entry(pair_of(&proof.a, &proof.b))
            .or_default()
            .push(proof);
    }
    let mut asked = Vec::new();
    for (pair, proofs) in by_pair {
        if joined(&pair.0, &pair.1) {
            continue;
        }
        let mut shown: Vec<&Proof> = proofs.clone();
        match answers.get(&pair) {
            Some(link) if link.answer == Answer::Yes => continue,
            Some(link) => {
                let kinds: BTreeSet<_> = link.proofs.iter().map(|proof| proof.kind).collect();
                if !proofs.iter().any(|proof| !kinds.contains(&proof.kind)) {
                    continue;
                }
                // The proof the owner has not seen leads.
                shown.sort_by_key(|proof| kinds.contains(&proof.kind));
            }
            None => {}
        }
        let first = shown[0].clone();
        let mut also: Vec<String> = Vec::new();
        for proof in shown.iter().skip(1) {
            if proof.sentence != first.sentence && !also.contains(&proof.sentence) {
                also.push(proof.sentence.clone());
            }
        }
        let hints = evaluation
            .all_hints
            .iter()
            .find(|hint| (hint.a.clone(), hint.b.clone()) == pair)
            .map(|hint| hint.kinds.clone())
            .unwrap_or_default();
        asked.push(AskedPair {
            proof: first,
            also,
            hints,
        });
    }
    let last = |key: &str| {
        projects
            .iter()
            .find(|project| project.key == key)
            .and_then(|project| project.last_at)
    };
    let name = |key: &str| {
        projects
            .iter()
            .find(|project| project.key == key)
            .map(|project| project.name.clone())
            .unwrap_or_default()
    };
    asked.sort_by(|x, y| {
        let newest = |pair: &AskedPair| last(&pair.proof.a).max(last(&pair.proof.b));
        newest(y)
            .cmp(&newest(x))
            .then_with(|| name(&x.proof.a).cmp(&name(&y.proof.a)))
            .then_with(|| name(&x.proof.b).cmp(&name(&y.proof.b)))
    });
    // Shared work leads: its pairs come before the rest, each part in the order above.
    asked.sort_by_key(|pair| pair.proof.kind != ProofKind::SharedWork);
    // A pair that touches a project already asked about follows the pair that reached it.
    let mut ordered = Vec::new();
    while !asked.is_empty() {
        let first = asked.remove(0);
        let mut seen = BTreeSet::from([first.proof.a.clone(), first.proof.b.clone()]);
        ordered.push(first);
        while let Some(at) = asked
            .iter()
            .position(|pair| seen.contains(&pair.proof.a) || seen.contains(&pair.proof.b))
        {
            let next = asked.remove(at);
            seen.insert(next.proof.a.clone());
            seen.insert(next.proof.b.clone());
            ordered.push(next);
        }
    }
    Ok(ordered)
}

/// The pairs with hints only that the owner has not answered, listed as "Maybe related".
///
/// # Errors
///
/// `Sqlite` when the index cannot be read.
pub fn maybe_pairs(index: &MachineIndex, evaluation: &Evaluation) -> Result<Vec<Hint>, StoreError> {
    let answers = index.link_answers()?;
    Ok(evaluation
        .hints
        .iter()
        .filter(|hint| !answers.contains_key(&(hint.a.clone(), hint.b.clone())))
        .cloned()
        .collect())
}

/// Keeps the owner's answer about the pair of projects with keys `pair`. A Yes needs a proof that
/// holds now; a pair with hints only may be answered Yes when `from_maybe` says the owner chose it
/// from the list of such pairs, and is then kept with no proof.
///
/// # Errors
///
/// `UnknownProject` for a key that is not among `projects`; `ProofNoLongerHolds` for a Yes with no
/// proof now (and no such choice); `Store` when the index refuses.
#[allow(
    clippy::too_many_arguments,
    reason = "the projects and what reads them, then the pair and what was said about it"
)]
pub fn answer_pair(
    index: &mut MachineIndex,
    projects: &[LinkProject],
    claude_ai: &[ClaudeAiProjectText],
    sessions: &BTreeMap<String, SessionNote>,
    read: &dyn Fn(&str) -> Vec<(String, String)>,
    pair: (&str, &str),
    answer: Answer,
    from_maybe: bool,
    now: DateTime<Utc>,
) -> Result<(), LinkingError> {
    let keys: BTreeSet<&str> = projects
        .iter()
        .map(|project| project.key.as_str())
        .collect();
    if pair.0 == pair.1 || !keys.contains(pair.0) || !keys.contains(pair.1) {
        return Err(LinkingError::UnknownProject);
    }
    let evaluation = evaluate(projects, claude_ai, sessions, read);
    let wanted = pair_of(pair.0, pair.1);
    let proofs: Vec<Proof> = evaluation
        .proofs
        .iter()
        .filter(|proof| pair_of(&proof.a, &proof.b) == wanted)
        .cloned()
        .collect();
    if answer == Answer::Yes && proofs.is_empty() {
        let hinted = evaluation
            .hints
            .iter()
            .any(|hint| (hint.a.clone(), hint.b.clone()) == wanted);
        if !(from_maybe && hinted) {
            return Err(LinkingError::ProofNoLongerHolds);
        }
    }
    index.put_link(pair.0, pair.1, answer, &proofs, now)?;
    Ok(())
}

/// Projects the owner said belong together, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The keys, in order.
    pub projects: Vec<String>,
    /// One sentence for each Yes, then for each other proof among the group's projects.
    pub why: Vec<String>,
}

/// The groups the Yes answers form, and only those: a hint joins nothing. A Yes whose proofs no
/// longer hold (none of them found again, of the same kind in the same files) joins nothing.
///
/// # Errors
///
/// `Sqlite` when the index cannot be read.
pub fn confirmed_groups(
    index: &MachineIndex,
    projects: &[LinkProject],
    evaluation: &Evaluation,
) -> Result<Vec<Group>, StoreError> {
    let answers = index.link_answers()?;
    let known: BTreeSet<&str> = projects
        .iter()
        .map(|project| project.key.as_str())
        .collect();
    let name = |key: &str| {
        projects
            .iter()
            .find(|project| project.key == key)
            .map_or_else(|| key.to_string(), |project| project.name.clone())
    };
    let mut yes: Vec<((String, String), String)> = Vec::new();
    for (pair, link) in &answers {
        if link.answer != Answer::Yes
            || !known.contains(pair.0.as_str())
            || !known.contains(pair.1.as_str())
        {
            continue;
        }
        let holding = link
            .proofs
            .iter()
            .find(|stored| still_holds(&evaluation.proofs, stored));
        if !link.proofs.is_empty() && holding.is_none() {
            continue;
        }
        let why = match holding {
            Some(stored) => evaluation
                .proofs
                .iter()
                .find(|now| now.kind == stored.kind && now.a == stored.a && now.b == stored.b)
                .map_or_else(|| stored.sentence.clone(), |now| now.sentence.clone()),
            None => format!(
                "You said {} and {} belong together.",
                name(&pair.0),
                name(&pair.1)
            ),
        };
        yes.push((pair.clone(), why));
    }
    let pairs: Vec<(String, String)> = yes.iter().map(|(pair, _)| pair.clone()).collect();
    let mut groups: Vec<Group> = yes_groups(&pairs, &known)
        .into_iter()
        .map(|members| {
            let mut why: Vec<String> = Vec::new();
            for (pair, sentence) in &yes {
                if members.contains(&pair.0) && !why.contains(sentence) {
                    why.push(sentence.clone());
                }
            }
            for proof in &evaluation.proofs {
                if members.contains(&proof.a)
                    && members.contains(&proof.b)
                    && !why.contains(&proof.sentence)
                {
                    why.push(proof.sentence.clone());
                }
            }
            Group {
                projects: members.into_iter().collect(),
                why,
            }
        })
        .collect();
    groups.sort_by(|a, b| a.projects.cmp(&b.projects));
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use catervas_core::linking::{HintKind, ProofKind};
    use chrono::{DateTime, Duration, TimeZone, Utc};

    use super::*;
    use crate::files::fixtures::TempProject;
    use crate::git::fixtures::TempRepo;
    use crate::machine::{MACHINE_DB, MachineIndex, open_machine_index};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 22, 12, 0, 0)
            .single()
            .expect("a valid time")
    }

    /// A project that is the repository `repository`, last worked in `days` days ago.
    fn project(name: &str, repository: &TempRepo, days: i64) -> LinkProject {
        LinkProject {
            key: format!("github.com/linking-test/{name}"),
            name: name.to_string(),
            last_at: Some(now() - Duration::days(days)),
            folder: Some(repository.path.to_string_lossy().into_owned()),
            remote_key: Some(format!("github.com/linking-test/{name}")),
            root: Some(repository.path.to_string_lossy().into_owned()),
            active_days: BTreeSet::new(),
            written_in: BTreeSet::new(),
        }
    }

    /// The folder name of `repository`, as a sibling is reached from another repository.
    fn sibling(repository: &TempRepo) -> String {
        let name = repository.path.file_name().expect("a name");
        format!("../{}", name.to_string_lossy())
    }

    fn uses(repository: &TempRepo, others: &[&TempRepo]) {
        let dependencies: Vec<String> = others
            .iter()
            .enumerate()
            .map(|(at, other)| format!("    \"dep-{at}\": \"file:{}\"", sibling(other)))
            .collect();
        repository.write(
            "package.json",
            &format!(
                "{{\n  \"dependencies\": {{\n{}\n  }}\n}}\n",
                dependencies.join(",\n")
            ),
        );
        repository.commit("the dependencies");
    }

    struct World {
        _temp: TempProject,
        index: MachineIndex,
    }

    impl World {
        fn new(name: &str) -> Self {
            let temp = TempProject::new(name);
            let index = open_machine_index(&temp.root.join(MACHINE_DB), now()).expect("an index");
            Self { _temp: temp, index }
        }

        fn pairs(&self, projects: &[LinkProject]) -> Vec<(String, String)> {
            let evaluation = evaluate(projects, &[], &BTreeMap::new(), &read_from_git);
            asked_pairs(&self.index, projects, &evaluation)
                .expect("pairs")
                .into_iter()
                .map(|pair| (pair.proof.a, pair.proof.b))
                .collect()
        }

        fn say(
            &mut self,
            projects: &[LinkProject],
            a: &LinkProject,
            b: &LinkProject,
            answer: Answer,
        ) -> Result<(), LinkingError> {
            answer_pair(
                &mut self.index,
                projects,
                &[],
                &BTreeMap::new(),
                &read_from_git,
                (&a.key, &b.key),
                answer,
                false,
                now(),
            )
        }

        /// Says `answer` about the projects named `a` and `b`, as the shared-work test names them.
        fn say_with(
            &mut self,
            projects: &[LinkProject],
            a: &str,
            b: &str,
            answer: Answer,
        ) -> Result<(), LinkingError> {
            let sessions = self.index.link_activity()?.sessions;
            let key = |name: &str| format!("github.com/shared-test/{name}");
            answer_pair(
                &mut self.index,
                projects,
                &[],
                &sessions,
                &files_of,
                (&key(a), &key(b)),
                answer,
                false,
                now(),
            )
        }

        fn groups(&self, projects: &[LinkProject]) -> Vec<(Vec<String>, Vec<String>)> {
            let evaluation = evaluate(projects, &[], &BTreeMap::new(), &read_from_git);
            confirmed_groups(&self.index, projects, &evaluation)
                .expect("groups")
                .into_iter()
                .map(|group| (group.projects, group.why))
                .collect()
        }
    }

    fn keys(projects: &[&LinkProject]) -> Vec<String> {
        let mut keys: Vec<String> = projects.iter().map(|project| project.key.clone()).collect();
        keys.sort();
        keys
    }

    /// A session in the index: its title, when it last did anything, where it was placed, and the
    /// roots it wrote under.
    fn session(index: &MachineIndex, id: &str, title: &str, day: u32, place: &str, roots: &[&str]) {
        index
            .connection
            .execute(
                "INSERT INTO sessions (session_id, title, last_at, place) VALUES (?1, ?2, ?3, ?4)",
                (id, title, format!("2026-10-{day:02}T09:00:00.000Z"), place),
            )
            .expect("a session");
        for root in roots {
            index
                .connection
                .execute(
                    "INSERT INTO session_roots_written (session_id, root) VALUES (?1, ?2)",
                    (id, root),
                )
                .expect("a root written");
        }
    }

    /// Projects named `names`, each with the folder `/work/<name>`, newest first by `days`, and
    /// the sessions the index says wrote under them.
    fn projects_at(index: &MachineIndex, names: &[(&str, i64)]) -> Vec<LinkProject> {
        let activity = index.link_activity().expect("activity");
        names
            .iter()
            .map(|(name, days)| {
                let root = format!("/work/{name}");
                LinkProject {
                    key: format!("github.com/shared-test/{name}"),
                    name: (*name).to_string(),
                    last_at: Some(now() - Duration::days(*days)),
                    folder: Some(root.clone()),
                    remote_key: Some(format!("github.com/shared-test/{name}")),
                    written_in: activity
                        .written_by_root
                        .get(&root)
                        .cloned()
                        .unwrap_or_default(),
                    root: Some(root),
                    active_days: BTreeSet::new(),
                }
            })
            .collect()
    }

    /// The files `/work/<name>` holds: `c-app` uses `d-lib`'s code, and `e-app` uses `f-lib`'s.
    fn files_of(root: &str) -> Vec<(String, String)> {
        let uses = |other: &str| {
            vec![(
                "package.json".to_string(),
                format!("{{\n  \"dependencies\": {{\n    \"dep\": \"file:../{other}\"\n  }}\n}}\n"),
            )]
        };
        match root {
            "/work/c-app" => uses("d-lib"),
            "/work/e-app" => uses("f-lib"),
            _ => Vec::new(),
        }
    }

    fn asked(world: &World, projects: &[LinkProject]) -> (Vec<AskedPair>, Evaluation) {
        let sessions = world.index.link_activity().expect("activity").sessions;
        let evaluation = evaluate(projects, &[], &sessions, &files_of);
        let pairs = asked_pairs(&world.index, projects, &evaluation).expect("pairs");
        (pairs, evaluation)
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one world of sessions told in order: asked first, shown, checked again, kept"
    )]
    fn asks_shared_work_first_and_rechecks_it() {
        let mut world = World::new("asks-shared-work-first-and-rechecks-it");
        let index = &world.index;
        // a-app and b-lib: two sessions. c-app and d-lib: a path dependency, and newer.
        session(
            index,
            "s1",
            "Fix the login redirect",
            20,
            "owner",
            &["/work/a-app", "/work/b-lib"],
        );
        session(
            index,
            "s2",
            "",
            21,
            "owner",
            &["/work/a-app", "/work/b-lib"],
        );
        // e-app and f-lib: shared work and a path dependency.
        session(
            index,
            "s3",
            "Move the config",
            18,
            "owner",
            &["/work/e-app", "/work/f-lib"],
        );
        session(
            index,
            "s4",
            "Rename the client",
            19,
            "owner",
            &["/work/e-app", "/work/f-lib"],
        );
        // g-app and h-lib: a Catervas session wrote in both, and one session of the owner did.
        session(
            index,
            "s5",
            "Own work",
            17,
            "owner",
            &["/work/g-app", "/work/h-lib"],
        );
        session(
            index,
            "s6",
            "A role's work",
            17,
            "catervas",
            &["/work/g-app", "/work/h-lib"],
        );
        let projects = projects_at(
            index,
            &[
                ("a-app", 9),
                ("b-lib", 9),
                ("c-app", 0),
                ("d-lib", 0),
                ("e-app", 12),
                ("f-lib", 12),
                ("g-app", 1),
                ("h-lib", 1),
            ],
        );
        let key = |name: &str| format!("github.com/shared-test/{name}");

        // The pairs with shared work come first, though c-app and d-lib are newer.
        let (pairs, evaluation) = asked(&world, &projects);
        let order: Vec<(String, String, ProofKind)> = pairs
            .iter()
            .map(|pair| (pair.proof.a.clone(), pair.proof.b.clone(), pair.proof.kind))
            .collect();
        assert_eq!(
            order,
            vec![
                (key("a-app"), key("b-lib"), ProofKind::SharedWork),
                (key("e-app"), key("f-lib"), ProofKind::SharedWork),
                (key("c-app"), key("d-lib"), ProofKind::PathDependency),
            ],
            "{pairs:?}"
        );
        // The first has no Also; the second shows its file proof under Also; the session ends hold
        // the titles and days.
        assert_eq!(pairs[0].also, Vec::<String>::new());
        assert_eq!(
            pairs[0].proof.sentence,
            "You worked on a-app and b-lib together in 2 sessions."
        );
        let texts: Vec<&str> = pairs[0]
            .proof
            .ends
            .iter()
            .map(|end| end.text.as_str())
            .collect();
        assert_eq!(
            texts,
            vec![
                "A session · 2026-10-21",
                "Fix the login redirect · 2026-10-20"
            ]
        );
        assert_eq!(pairs[1].also, vec!["e-app uses f-lib's code.".to_string()]);
        // A Catervas session added nothing: g-app and h-lib have one session of the owner.
        assert!(
            evaluation
                .proofs
                .iter()
                .all(|proof| proof.a != key("g-app"))
        );
        let wrote_both: Vec<String> = evaluation
            .hints
            .iter()
            .filter(|hint| hint.kinds.contains(&HintKind::WroteBoth))
            .map(|hint| hint.a.clone())
            .collect();
        assert_eq!(wrote_both, vec![key("g-app")], "one session is a hint");

        // A Yes is kept, and holds when a third session is added: the sentence counts three.
        world
            .say_with(&projects, "a-app", "b-lib", Answer::Yes)
            .expect("yes");
        session(
            &world.index,
            "s7",
            "More",
            22,
            "owner",
            &["/work/a-app", "/work/b-lib"],
        );
        let projects = projects_at(&world.index, &[("a-app", 9), ("b-lib", 9)]);
        let sessions = world.index.link_activity().expect("activity").sessions;
        let evaluation = evaluate(&projects, &[], &sessions, &files_of);
        let groups = confirmed_groups(&world.index, &projects, &evaluation).expect("groups");
        assert_eq!(groups.len(), 1, "{groups:?}");
        assert_eq!(
            groups[0].why,
            vec!["You worked on a-app and b-lib together in 3 sessions.".to_string()]
        );

        // A No is not asked again, with more shared sessions.
        let projects = projects_at(&world.index, &[("e-app", 12), ("f-lib", 12)]);
        world
            .say_with(&projects, "e-app", "f-lib", Answer::No)
            .expect("no");
        session(
            &world.index,
            "s8",
            "Again",
            23,
            "owner",
            &["/work/e-app", "/work/f-lib"],
        );
        let projects = projects_at(&world.index, &[("e-app", 12), ("f-lib", 12)]);
        assert_eq!(asked(&world, &projects).0.len(), 0);

        // Below two shared sessions the proof is gone: the pair is not asked, a Yes is refused,
        // and the group the earlier Yes made is no more.
        world
            .index
            .connection
            .execute(
                "DELETE FROM session_roots_written WHERE session_id IN ('s2', 's7')",
                [],
            )
            .expect("removed");
        let projects = projects_at(&world.index, &[("a-app", 9), ("b-lib", 9)]);
        assert_eq!(asked(&world, &projects).0.len(), 0);
        let refused = world.say_with(&projects, "a-app", "b-lib", Answer::Yes);
        assert!(
            matches!(refused, Err(LinkingError::ProofNoLongerHolds)),
            "{refused:?}"
        );
        let sessions = world.index.link_activity().expect("activity").sessions;
        let evaluation = evaluate(&projects, &[], &sessions, &files_of);
        assert!(
            confirmed_groups(&world.index, &projects, &evaluation)
                .expect("groups")
                .is_empty()
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::many_single_char_names,
        clippy::too_many_lines,
        reason = "projects a, b, c and the pairs x, y and p, q of one scenario, told in order"
    )]
    fn asks_each_proven_pair_once() {
        let mut world = World::new("asks-each-proven-pair-once");
        let (a, b, c) = (
            TempRepo::new("link-a"),
            TempRepo::new("link-b"),
            TempRepo::new("link-c"),
        );
        uses(&a, &[&b]);
        uses(&b, &[&c]);
        let projects = vec![
            project("a", &a, 1),
            project("b", &b, 2),
            project("c", &c, 3),
        ];
        let (pa, pb, pc) = (&projects[0], &projects[1], &projects[2]);

        // Two pairs with a proof, a and b first.
        assert_eq!(
            world.pairs(&projects),
            vec![
                (pa.key.clone(), pb.key.clone()),
                (pb.key.clone(), pc.key.clone())
            ]
        );

        // Yes to both: one group of three. A later proof from a to c is not asked, and is listed
        // with the group's reasons.
        world.say(&projects, pa, pb, Answer::Yes).expect("yes");
        assert_eq!(
            world.pairs(&projects),
            vec![(pb.key.clone(), pc.key.clone())]
        );
        world.say(&projects, pb, pc, Answer::Yes).expect("yes");
        uses(&a, &[&b, &c]);
        assert_eq!(world.pairs(&projects), vec![]);
        let groups = world.groups(&projects);
        assert_eq!(groups.len(), 1, "{groups:?}");
        assert_eq!(groups[0].0, keys(&[pa, pb, pc]));
        assert!(
            groups[0].1.contains(&"a uses c's code.".to_string()),
            "{:?}",
            groups[0].1
        );

        // A No is not asked again, until a proof of a kind it was not given appears.
        let (x, y) = (TempRepo::new("link-x"), TempRepo::new("link-y"));
        uses(&x, &[&y]);
        let projects = vec![project("x", &x, 1), project("y", &y, 2)];
        let (px, py) = (&projects[0], &projects[1]);
        assert_eq!(world.pairs(&projects).len(), 1);
        world.say(&projects, px, py, Answer::No).expect("no");
        assert_eq!(world.pairs(&projects), vec![]);
        x.write(
            "compose.yaml",
            &format!("services:\n  y:\n    build: {}\n", sibling(&y)),
        );
        x.commit("deploys y");
        let again = world.pairs(&projects);
        assert_eq!(again, vec![(px.key.clone(), py.key.clone())]);
        let evaluation = evaluate(&projects, &[], &BTreeMap::new(), &read_from_git);
        let asked = asked_pairs(&world.index, &projects, &evaluation).expect("pairs");
        assert_eq!(
            asked[0].proof.kind,
            ProofKind::Deploys,
            "the new proof leads"
        );
        assert_eq!(asked[0].also, vec!["x uses y's code.".to_string()]);
        // The projects share an owner, which is a hint beside the proof.
        assert_eq!(asked[0].hints, vec![HintKind::SameOwner]);

        // A proof whose file changed so that it no longer holds is not asked, and a Yes to it is
        // refused.
        let (p, q) = (TempRepo::new("link-p"), TempRepo::new("link-q"));
        uses(&p, &[&q]);
        let projects = vec![project("p", &p, 1), project("q", &q, 2)];
        assert_eq!(world.pairs(&projects).len(), 1);
        p.write("package.json", "{\n  \"dependencies\": {}\n}\n");
        p.commit("no longer depends");
        assert_eq!(world.pairs(&projects), vec![]);
        let refused = world.say(&projects, &projects[0], &projects[1], Answer::Yes);
        assert!(
            matches!(refused, Err(LinkingError::ProofNoLongerHolds)),
            "{refused:?}"
        );
        assert!(Path::new(&p.path).exists());
    }
}
