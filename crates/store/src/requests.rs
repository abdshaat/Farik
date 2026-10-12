//! Filing a request (F3, `docs/SPEC.md` section 5.16): one piece of code for the command line and
//! for the agents' `catervas_create_task`, so that a request is filed exactly one way whoever files it.
//! Beside it, the JSON the board, the rules, and the criterion library are shown as, for the same
//! reason: `catervas board --json` and `catervas_read_board` say one thing.

use std::fmt;
use std::num::NonZeroU64;
use std::sync::{Mutex, PoisonError};

use catervas_core::contract::{Role, TaskContract, TaskId, TaskKind, TaskStatus};
use catervas_core::criteria::{CriteriaLibrary, TemplateVerification};
use catervas_core::folders::DocumentFormat;
use catervas_core::governor::gates::{
    ContractWriteActor, ContractWriteRefusal, FIELDS_FIXED_AT_CREATION,
    FIELDS_ONLY_THE_HUMAN_WRITES, FIELDS_THE_GOVERNOR_WRITES, FIELDS_THE_STORE_OWNS,
    check_contract_write, check_human_triage,
};
use catervas_core::governor::team_rules::TeamRules;
use catervas_core::governor::transition_table::TransitionActor;
use catervas_protocol::command::{Command, RequestSize, command_from_value};
use catervas_protocol::event::{
    CatervaficationStartedBody, CatervaficationStartedBodyProject, CatervasEvent,
    ContractLockedBody, ContractSummary, ContractUnlockedBody, EventBody, EventIds, EventKind,
    RequestTriagedBody, RequestTriagedBodySize, TaskCreatedBody, TaskCreatedBodyFiledAs, new_event,
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use crate::files::{FilesError, ProjectFiles};
use crate::git::Git;
use crate::{EventLog, EventQuery, Projections, StoreError, TaskProjection};

/// Who the human is in the log: the id every act of theirs is recorded under.
const HUMAN: &str = "human";

/// Every text of a request the Product Manager replaces while refining it.
pub const PLACEHOLDER: &str = "(placeholder, for the Product Manager to write)";
/// The budget a request is filed with when the team caps no task's budget (ADR 0015). The Product
/// Manager rewrites it while refining, as it rewrites every placeholder.
pub const PLACEHOLDER_MAX_COST_USD: f64 = 20.0;
/// A request a person files from a brief: its title and its intent, the brief, are theirs, and
/// every other field is a placeholder the refine session replaces. The placeholder path is a glob
/// no file matches.
///
/// # Errors
///
/// A sentence saying the title is under three characters or the brief under twenty, the schema's
/// minimums.
pub fn request_from_brief(title: &str, brief: &str, max_cost_usd: f64) -> Result<Value, String> {
    if title.trim().chars().count() < 3 {
        return Err(format!(
            "{title:?} is too short a title: a contract's title is three characters or more"
        ));
    }
    if brief.trim().chars().count() < 20 {
        return Err(format!(
            "{brief:?} is too short a brief: it is the contract's intent, which is twenty \
             characters or more"
        ));
    }
    Ok(json!({
        "title": title,
        "intent": brief,
        "scope": { "in_scope": [PLACEHOLDER], "out_of_scope": [PLACEHOLDER] },
        "requirements": [{ "id": "R1", "text": PLACEHOLDER }],
        "exit_criteria": [{
            "id": "C1",
            "text": PLACEHOLDER,
            "satisfies": ["R1"],
            "verification": { "method": "review", "rubric": [PLACEHOLDER] }
        }],
        "assignee_role": "software_developer",
        "reviewer_role": "software_developer",
        "risk": "low",
        "budget": { "max_cost_usd": max_cost_usd },
        "allowed_paths": [PLACEHOLDER]
    }))
}

/// The budget a request is filed with: the team's cap on a task's budget when it has one, and
/// otherwise `PLACEHOLDER_MAX_COST_USD`.
#[must_use]
pub fn placeholder_budget_usd(rules: &TeamRules) -> f64 {
    rules
        .max_task_budget_usd
        .unwrap_or(PLACEHOLDER_MAX_COST_USD)
}

/// Why a request in plain words is refused before anything else is said of it.
pub const TOO_SHORT: &str = "say a little more: at least 20 characters";
/// The longest title a request in plain words is given.
const WORDS_TITLE_LIMIT: usize = 80;

/// A request a person files in plain words (F3): its title is the text's first line, cut at 80
/// characters on a word boundary, and its intent the whole text; every other field is a
/// placeholder, as `request_from_brief` makes it.
///
/// # Errors
///
/// `TOO_SHORT` for a text under 20 characters, asked first; otherwise `request_from_brief`'s.
pub fn request_from_text(text: &str, max_cost_usd: f64) -> Result<Value, String> {
    if text.trim().chars().count() < 20 {
        return Err(TOO_SHORT.to_string());
    }
    let line = text.trim().lines().next().unwrap_or_default().trim();
    let title = if line.chars().count() <= WORDS_TITLE_LIMIT {
        line.to_string()
    } else {
        let kept: String = line.chars().take(WORDS_TITLE_LIMIT).collect();
        let next = line.chars().nth(WORDS_TITLE_LIMIT);
        if next.is_some_and(char::is_whitespace) {
            kept
        } else {
            kept.rsplit_once(char::is_whitespace)
                .map_or(kept.clone(), |(before, _)| before.to_string())
        }
    };
    request_from_brief(title.trim_end(), text, max_cost_usd)
}

/// Why a request was not filed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestError {
    /// The request is not one Catervas files, in words a person or an agent can act on.
    Refused {
        /// What is wrong with it.
        reason: String,
    },
    /// The contract file could not be written.
    Files(FilesError),
    /// The log refused.
    Store(StoreError),
}

impl fmt::Display for RequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused { reason } => write!(formatter, "{reason}"),
            Self::Files(error) => write!(formatter, "{error}"),
            Self::Store(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for RequestError {}

impl From<FilesError> for RequestError {
    fn from(error: FilesError) -> Self {
        Self::Files(error)
    }
}

impl From<StoreError> for RequestError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// The fields of `wire` its author may not write: the store's, the governor's, the human's lock,
/// and the two the triage and the epic's assignee decide (`docs/SPEC.md` section 5.11). Refused
/// rather than overwritten: a filing that quietly replaced what somebody wrote would make the file
/// and the contract two different things.
#[must_use]
pub fn fields_not_the_authors(wire: &Value) -> Vec<&'static str> {
    let Some(object) = wire.as_object() else {
        return Vec::new();
    };
    FIELDS_THE_STORE_OWNS
        .iter()
        .chain(&FIELDS_THE_GOVERNOR_WRITES)
        .chain(&FIELDS_ONLY_THE_HUMAN_WRITES)
        .chain(&FIELDS_FIXED_AT_CREATION)
        .copied()
        .filter(|field| object.contains_key(*field))
        .collect()
}

/// Gives the request in `wire` the next id, past the counter and every contract the repository
/// already holds (`docs/SPEC.md` section 8.4), and files it as a `draft`: the contract is created,
/// never written over a file already there, and
/// `task.created` appended, stamped with `ids` (whose `task_id` is replaced by the new one). With a
/// `parent`, it is a `task` of that epic, and since an epic's tasks are triaged by its breakdown
/// (5.16 item 3) `request.triaged { size: small }` follows, by `created_by`. Whether a child may be
/// written under that epic at all is `check_child_creation`'s question, asked by the caller.
///
/// The contract goes through `command_from_value`, so that a request filed from a terminal is held
/// to exactly the rules one arriving from an agent is.
///
/// With `from_chat_message`, the request is a chat's proposal the user sent: that seq must be an
/// agent's `chat_message.posted` carrying a `request` no `task.created` names yet, and
/// `task.created` names it. Requests are filed one at a time, the check among them, so that two
/// tabs cannot send one proposal twice.
///
/// # Errors
///
/// `Refused` when `wire` is not a mapping, sets a field that is not its author's, breaks a
/// contract rule, or links to a message that is not an unsent proposal; `Files` or `Store` when the
/// contract or the events cannot be read or written.
#[allow(clippy::too_many_arguments)]
pub fn file_request(
    files: &ProjectFiles,
    log: &EventLog,
    wire: Value,
    created_by: &str,
    parent: Option<&TaskId>,
    now: DateTime<Utc>,
    ids: &EventIds,
    from_chat_message: Option<u64>,
) -> Result<TaskContract, RequestError> {
    file(
        files,
        log,
        wire,
        Filing {
            created_by,
            parent,
            from_chat_message,
            raises: None,
            rederives: None,
            product_plan: false,
            catervafies: None,
            filed_as: None,
        },
        (now, ids),
    )
}

/// Every task filed to bring an `.agent.md` twin up to date (`task.created` with `rederives`,
/// `docs/SPEC.md` 5.17), oldest first, each with its twin. Whether one is still open is the
/// board's to say.
///
/// # Errors
///
/// What the log refused.
pub fn rederive_tasks(log: &EventLog) -> Result<Vec<(TaskId, String)>, StoreError> {
    let created = log.read(&EventQuery {
        kinds: vec![EventKind::TaskCreated],
        ..EventQuery::default()
    })?;
    Ok(created
        .iter()
        .filter_map(|event| match &event.body {
            EventBody::TaskCreated(body) => Some((
                event.envelope.ids.task_id.clone()?,
                body.rederives.as_ref()?.to_string(),
            )),
            _ => None,
        })
        .collect())
}

/// The whole contract of the task that brings `twin`, the version of `human` written for agents,
/// up to date with the owner's change to `human` (`docs/SPEC.md` 5.17): `owner` writes it and
/// `reviewer` checks it, within `max_cost_usd`, and it may change nothing but the twin.
#[must_use]
pub fn rederive_request(
    human: &str,
    twin: &str,
    owner: Role,
    reviewer: Role,
    max_cost_usd: f64,
) -> Value {
    let file = |path: &str| path.rsplit('/').next().unwrap_or(path).to_string();
    let says = format!("{twin} says what {human} says now");
    json!({
        "title": format!("Bring {} up to date with the owner's change", file(twin)),
        "intent": format!(
            "The owner changed {human} on the Files page. Rewrite {twin}, the version written for \
             agents, so that it says what {human} now says."
        ),
        "scope": {
            "in_scope": [twin],
            "out_of_scope": [format!("{human}, which stays as the owner left it")]
        },
        "requirements": [{ "id": "R1", "text": format!("{says}, written for agents.") }],
        "exit_criteria": [{
            "id": "C1",
            "text": format!("{says}."),
            "satisfies": ["R1"],
            "verification": {
                "method": "review",
                "rubric": [
                    format!("Every fact, decision and limit in {human} is in {twin}."),
                    format!("Nothing in {twin} contradicts {human}.")
                ]
            }
        }],
        "assignee_role": owner.to_string(),
        "reviewer_role": reviewer.to_string(),
        "risk": "low",
        "budget": { "max_cost_usd": max_cost_usd },
        "allowed_paths": [twin]
    })
}

/// Files the whole contract `rederive_request` makes, as `file_request` files one: `task.created`
/// names the twin in `rederives` and `catervas` filed it, and `request.triaged { small }` follows
/// at once, by `catervas`, so that no triage session runs.
///
/// # Errors
///
/// As `file_request`.
pub fn file_rederive_request(
    files: &ProjectFiles,
    log: &EventLog,
    wire: Value,
    twin: &str,
    (now, ids): (DateTime<Utc>, &EventIds),
) -> Result<TaskContract, RequestError> {
    file(
        files,
        log,
        wire,
        Filing {
            created_by: "catervas",
            parent: None,
            from_chat_message: None,
            raises: None,
            rederives: Some(twin),
            product_plan: false,
            catervafies: None,
            filed_as: None,
        },
        (now, ids),
    )
}

/// Files the request that raises the budget of marketing plan `plan` (ADR 0042, step 08g), as
/// `file_request` files one for `created_by`: `task.created` names the plan in `raises`, so that
/// the sprint policy never holds it, and `request.triaged { small }` follows at once, by `catervas`,
/// so that no triage session runs.
///
/// # Errors
///
/// As `file_request`.
pub fn file_raise_request(
    files: &ProjectFiles,
    log: &EventLog,
    wire: Value,
    (created_by, plan): (&str, &str),
    (now, ids): (DateTime<Utc>, &EventIds),
) -> Result<TaskContract, RequestError> {
    file(
        files,
        log,
        wire,
        Filing {
            created_by,
            parent: None,
            from_chat_message: None,
            raises: Some(plan),
            rederives: None,
            product_plan: false,
            catervafies: None,
            filed_as: None,
        },
        (now, ids),
    )
}

/// The whole contract of the task that drafts a new project's product plan (`docs/SPEC.md` 4.3,
/// 5.16): the Product Manager writes the spec and the roadmap and their `.agent` twins from the
/// owner's interview, in the team's `format`, `reviewer` checks them within `max_cost_usd`, and the
/// owner accepts them.
#[must_use]
pub fn product_plan_request(reviewer: Role, max_cost_usd: f64, format: DocumentFormat) -> Value {
    let x = format.extension();
    json!({
        "title": "Draft the product plan",
        "intent": format!("Write this new project's product plan from the owner's interview in your one-to-one chat, which From the human holds: docs/catervas/product/spec.{x}, the product description, and docs/catervas/product/roadmap.{x}, the roadmap, each for the owner to read, with its .agent.{x} version for the team. Nothing else is planned until the owner approves it."),
        "summary": "The Product Manager writes the product description and the roadmap from your interview, for you to read and approve. Nothing else is planned until you approve them.",
        "scope": {
            "in_scope": [format!("The product description (spec.{x}) and the roadmap (roadmap.{x}), each with the team's copy.")],
            "out_of_scope": ["Code, and every other document"]
        },
        "requirements": [
            {
                "id": "R1",
                "text": format!("spec.{x} has the product's name as its title, a paragraph on what it is and the problem it solves, and the sections Who it is for, What it must do, Not now and How we'll know it works, with Limits when the owner named any, in the owner's words where they gave them.")
            },
            {
                "id": "R2",
                "text": format!("roadmap.{x} has the sections Now, Next and Later, each item one line: its name, then one sentence.")
            },
            {
                "id": "R3",
                "text": format!("The team's copies of both (spec.agent.{x}, roadmap.agent.{x}) say the same, written for the agents.")
            }
        ],
        "exit_criteria": [
            {
                "id": "C1",
                "text": "The product description and the roadmap have every part listed above.",
                "satisfies": ["R1", "R2"],
                "verification": {
                    "method": "review",
                    "rubric": [
                        format!("spec.{x} has the title, the paragraph and the sections R1 names."),
                        format!("roadmap.{x} has Now, Next and Later, each item one line."),
                        "Both are in plain words, with no technical term the owner did not use."
                    ]
                }
            },
            {
                "id": "C2",
                "text": "The team's copies say the same as the documents you read.",
                "satisfies": ["R3"],
                "verification": {
                    "method": "review",
                    "rubric": [
                        format!("Every fact, decision and limit in spec.{x} and roadmap.{x} is in their .agent.{x} versions."),
                        format!("Nothing in an .agent.{x} version contradicts its version for the owner.")
                    ]
                }
            }
        ],
        "assignee_role": "product_manager",
        "reviewer_role": reviewer.to_string(),
        "risk": "low",
        "budget": { "max_cost_usd": max_cost_usd },
        "allowed_paths": [
            format!("docs/catervas/product/spec.{x}"),
            format!("docs/catervas/product/spec.agent.{x}"),
            format!("docs/catervas/product/roadmap.{x}"),
            format!("docs/catervas/product/roadmap.agent.{x}")
        ]
    })
}

/// Files the contract `product_plan_request` makes, as the owner's word: `created_by` is `human`,
/// `task.created` has `product_plan`, which skips the sprint queue and has the contract judged as
/// filed, and `request.triaged { small }` follows at once, by `catervas`, so that no triage
/// session runs.
///
/// # Errors
///
/// As `file_request`.
pub fn file_product_plan_request(
    files: &ProjectFiles,
    log: &EventLog,
    wire: Value,
    (now, ids): (DateTime<Utc>, &EventIds),
) -> Result<TaskContract, RequestError> {
    file(
        files,
        log,
        wire,
        Filing {
            created_by: HUMAN,
            parent: None,
            from_chat_message: None,
            raises: None,
            rederives: None,
            product_plan: true,
            catervafies: None,
            filed_as: None,
        },
        (now, ids),
    )
}

/// Files the epic of a Catervafication (`docs/SPEC.md` 5.16): `file_request` for an epic, which no
/// request sets the kind of, and `request.triaged { large }` and `catervafication.started` follow at
/// once, by `catervas`, under the one filing lock, so that no triage session runs.
///
/// # Errors
///
/// As `file_request`.
pub fn file_catervafication_epic(
    files: &ProjectFiles,
    log: &EventLog,
    wire: Value,
    created_by: &str,
    project: CatervaficationStartedBodyProject,
    (now, ids): (DateTime<Utc>, &EventIds),
) -> Result<TaskContract, RequestError> {
    file(
        files,
        log,
        wire,
        Filing {
            created_by,
            parent: None,
            from_chat_message: None,
            raises: None,
            rederives: None,
            product_plan: false,
            catervafies: Some(project),
            filed_as: None,
        },
        (now, ids),
    )
}

/// The longest title a contract has.
const TITLE_LIMIT: usize = 120;

/// The whole contract of the task that makes the change the owner asked `owner` for in `words`
/// (`docs/SPEC.md` 5.17): `owner` writes it and `reviewer` checks it, within `max_cost_usd`. It
/// may change `path` and `twin`, its agent version when it has one, and nothing else. `name` is
/// the document's name on the Files page.
#[must_use]
pub fn change_request(
    path: &str,
    twin: Option<&str>,
    name: &str,
    words: &str,
    owner: Role,
    reviewer: Role,
    max_cost_usd: f64,
) -> Value {
    let mut title = format!("Change {name}");
    if title.chars().count() > TITLE_LIMIT {
        title = title.chars().take(TITLE_LIMIT - 1).collect::<String>() + "…";
    }
    let paths: Vec<&str> = std::iter::once(path).chain(twin).collect();
    let mut rubric = vec![
        format!("{path} makes the change the owner's words ask for."),
        format!("Nothing else in {path} changes."),
    ];
    if let Some(twin) = twin {
        rubric.push(format!("{twin} says what {path} now says."));
    }
    json!({
        "title": title,
        "intent": format!(
            "The owner asked on the Files page for a change to {path}, in these words:\n\n{words}"
        ),
        "scope": { "in_scope": paths, "out_of_scope": ["Every other file"] },
        "requirements": [{ "id": "R1", "text": format!("{path} makes the change the owner asked for.") }],
        "exit_criteria": [{
            "id": "C1",
            "text": format!("{path} makes the change the owner's words ask for."),
            "satisfies": ["R1"],
            "verification": { "method": "review", "rubric": rubric }
        }],
        "assignee_role": owner.to_string(),
        "reviewer_role": reviewer.to_string(),
        "risk": "low",
        "budget": { "max_cost_usd": max_cost_usd },
        "allowed_paths": paths
    })
}

/// Files a whole contract for `created_by` that no refine session rewrites, as `file_request`
/// files one: `task.created` carries `filed_as`, and `request.triaged { small, reason }` by
/// `catervas` follows at once, so that no triage session runs.
///
/// # Errors
///
/// As `file_request`.
pub fn file_small_request(
    files: &ProjectFiles,
    log: &EventLog,
    wire: Value,
    created_by: &str,
    filed_as: TaskCreatedBodyFiledAs,
    reason: &str,
    (now, ids): (DateTime<Utc>, &EventIds),
) -> Result<TaskContract, RequestError> {
    file(
        files,
        log,
        wire,
        Filing {
            created_by,
            parent: None,
            from_chat_message: None,
            raises: None,
            rederives: None,
            product_plan: false,
            catervafies: None,
            filed_as: Some((filed_as, reason)),
        },
        (now, ids),
    )
}

/// What a filing says besides the request itself.
#[derive(Clone, Copy)]
struct Filing<'a> {
    created_by: &'a str,
    parent: Option<&'a TaskId>,
    from_chat_message: Option<u64>,
    raises: Option<&'a str>,
    rederives: Option<&'a str>,
    product_plan: bool,
    catervafies: Option<CatervaficationStartedBodyProject>,
    /// How a whole contract was filed, and why it is triaged small.
    filed_as: Option<(TaskCreatedBodyFiledAs, &'a str)>,
}

fn file(
    files: &ProjectFiles,
    log: &EventLog,
    mut wire: Value,
    filing: Filing<'_>,
    (now, ids): (DateTime<Utc>, &EventIds),
) -> Result<TaskContract, RequestError> {
    let Filing {
        created_by,
        parent,
        from_chat_message,
        ..
    } = filing;
    // ponytail: one lock for the process; a second process (the command line) never links a chat.
    let _filing = FILING.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(seq) = from_chat_message {
        check_proposal(log, seq)?;
    }
    let written = fields_not_the_authors(&wire);
    if !written.is_empty() {
        return Err(RequestError::Refused {
            reason: format!(
                "sets {}, which a request does not: Catervas assigns the id and the stamps, the \
                 governor writes the lifecycle, the lock is the human's lock to take, triage \
                 decides whether this is an epic, and a task's parent is set by the epic's \
                 assignee when it breaks the epic down (5.16)",
                written.join(", ")
            ),
        });
    }
    let Some(object) = wire.as_object_mut() else {
        return Err(RequestError::Refused {
            reason: "is not a contract: a contract is a mapping".to_string(),
        });
    };

    // Held to the rules before an id is taken, so that a refused request does not use one up. The
    // id is not the author's and a rule about it is not one the author can break, so the contract
    // is checked under a stand-in and given its real id once it passes.
    let stamp = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    object.insert("id".to_string(), json!("CTV-0"));
    object.insert("status".to_string(), json!("draft"));
    object.insert("created_by".to_string(), json!(created_by));
    object.insert("created_at".to_string(), json!(stamp));
    object.insert("updated_at".to_string(), json!(stamp));
    if let Some(parent) = parent {
        object.insert("kind".to_string(), json!("task"));
        object.insert("parent".to_string(), json!(parent.to_string()));
    }
    if filing.catervafies.is_some() {
        object.insert("kind".to_string(), json!("epic"));
    }

    let command = command_from_value(&json!({
        "command": "task_create",
        "body": { "contract": wire }
    }))
    .map_err(|errors| RequestError::Refused {
        reason: format!(
            "is not a contract Catervas can file: {}",
            errors
                .iter()
                .map(|error| format!("{} {}", error.path, error.message))
                .collect::<Vec<_>>()
                .join("; ")
        ),
    })?;
    let Command::TaskCreate { mut contract } = command else {
        return Err(RequestError::Refused {
            reason: "was read back as a command other than the one built from it".to_string(),
        });
    };

    // Past every contract the repository already holds as well as the counter: the log is
    // machine-local and the contracts are committed (8.4), so on a fresh clone the counter alone
    // would hand out CTV-1 again. `list_contracts` is in number order, so the last is the highest.
    // And past every task branch the repository holds, whatever its contracts say: a project
    // whose `.catervas/` was removed or carried elsewhere still has the branches of its old tasks. A
    // folder whose refs cannot be listed counts as having none; a collision then shows when the
    // worktree is made.
    let contracts = files
        .list_contracts()?
        .last()
        .and_then(|id| id.as_str().trim_start_matches("CTV-").parse::<u64>().ok())
        .unwrap_or(0);
    let branches = Git::open(files.root().to_path_buf())
        .task_branch_numbers()
        .unwrap_or_default()
        .into_iter()
        .max()
        .unwrap_or(0);
    let taken = contracts.max(branches);
    contract.id = log.next_task_id_above(taken)?;

    files.create_contract(&contract)?;
    let ids = EventIds {
        task_id: Some(contract.id.clone()),
        ..ids.clone()
    };
    let bodies = creation_bodies(&contract, filing)?;
    for body in bodies {
        let event = new_event(body, now, ids.clone()).map_err(|error| RequestError::Refused {
            reason: format!("cannot be recorded: {error:?}"),
        })?;
        log.append(&event)?;
    }
    Ok(*contract)
}

/// The events a request is filed with: `task.created`, and the triage that follows at once for a
/// task of an epic and for a request that raises a marketing plan's budget.
fn creation_bodies(
    contract: &TaskContract,
    filing: Filing<'_>,
) -> Result<Vec<EventBody>, RequestError> {
    let Filing {
        created_by,
        parent,
        from_chat_message,
        raises,
        rederives,
        product_plan,
        catervafies,
        filed_as,
    } = filing;
    let mut bodies = vec![EventBody::TaskCreated(TaskCreatedBody {
        created_by: created_by.to_string(),
        summary: summary_of(contract),
        product_plan: product_plan.then_some(true),
        from_chat_message: from_chat_message.and_then(NonZeroU64::new),
        raises: raises
            .map(|plan| {
                plan.to_string()
                    .try_into()
                    .map_err(|_| RequestError::Refused {
                        reason: format!("raises {plan:?}, which is no marketing plan's id"),
                    })
            })
            .transpose()?,
        rederives: rederives
            .map(|twin| {
                twin.to_string()
                    .try_into()
                    .map_err(|_| RequestError::Refused {
                        reason: format!("rederives {twin:?}, which is no agent version (.agent.md or .agent.html)"),
                    })
            })
            .transpose()?,
        filed_as: filed_as.map(|(kind, _)| kind),
    })];
    if let Some(parent) = parent {
        bodies.push(EventBody::RequestTriaged(RequestTriagedBody {
            size: RequestTriagedBodySize::Small,
            reason: format!("a task of {}", parent.as_str()),
            triaged_by: created_by.to_string(),
        }));
    }
    if let Some(plan) = raises {
        bodies.push(EventBody::RequestTriaged(RequestTriagedBody {
            size: RequestTriagedBodySize::Small,
            reason: format!("a raised marketing budget for {plan}"),
            triaged_by: "catervas".to_string(),
        }));
    }
    if let Some((_, reason)) = filed_as {
        bodies.push(EventBody::RequestTriaged(RequestTriagedBody {
            size: RequestTriagedBodySize::Small,
            reason: reason.to_string(),
            triaged_by: "catervas".to_string(),
        }));
    }
    if let Some(twin) = rederives {
        bodies.push(EventBody::RequestTriaged(RequestTriagedBody {
            size: RequestTriagedBodySize::Small,
            reason: format!("a re-derive of {twin}"),
            triaged_by: "catervas".to_string(),
        }));
    }
    if product_plan {
        bodies.push(EventBody::RequestTriaged(RequestTriagedBody {
            size: RequestTriagedBodySize::Small,
            reason: "a new project's product plan".to_string(),
            triaged_by: "catervas".to_string(),
        }));
    }
    if let Some(project) = catervafies {
        bodies.push(EventBody::RequestTriaged(RequestTriagedBody {
            size: RequestTriagedBodySize::Large,
            reason: match project {
                CatervaficationStartedBodyProject::Existing => {
                    "the Catervafication of an existing repository"
                }
                CatervaficationStartedBodyProject::New => "the Catervafication of a new project",
            }
            .to_string(),
            triaged_by: "catervas".to_string(),
        }));
        bodies.push(EventBody::CatervaficationStarted(
            CatervaficationStartedBody { project },
        ));
    }
    Ok(bodies)
}

/// Held while a request is filed, so that a proposal's check and its filing are one step.
static FILING: Mutex<()> = Mutex::new(());

/// Refused unless `seq` is an agent's chat reply that proposes a request not yet sent.
fn check_proposal(log: &EventLog, seq: u64) -> Result<(), RequestError> {
    let refused = |reason: String| Err(RequestError::Refused { reason });
    let event = log
        .read(&EventQuery {
            after_seq: Some(seq.saturating_sub(1)),
            limit: Some(1),
            ..EventQuery::default()
        })?
        .into_iter()
        .find(|event| event.envelope.seq == seq);
    let reply = match event.map(|event| event.body) {
        Some(EventBody::ChatMessagePosted(reply)) if reply.author != HUMAN => reply,
        _ => {
            return refused(format!(
                "links to message {seq}, which is not an agent's reply in a chat"
            ));
        }
    };
    if reply.request.is_none() {
        return refused(format!(
            "links to message {seq}, a reply that proposes no request"
        ));
    }
    let sent = log
        .read(&EventQuery {
            kinds: vec![EventKind::TaskCreated],
            ..EventQuery::default()
        })?
        .into_iter()
        .find(|event| match &event.body {
            EventBody::TaskCreated(body) => {
                body.from_chat_message.map(NonZeroU64::get) == Some(seq)
            }
            _ => false,
        });
    match sent.and_then(|event| event.envelope.ids.task_id) {
        Some(task_id) => refused(format!("was already sent, as {}", task_id.as_str())),
        None => Ok(()),
    }
}

/// Records the human's size of a request, and the kind that follows from it: large makes it an
/// epic and small one standalone task (`docs/SPEC.md` section 5.16). The human may record either
/// while the request is a draft, so this is both the first triage and the overrule of one. The
/// contract file is written with the kind, `request.triaged { triaged_by: human }` appended, and the
/// board brought up to it.
///
/// # Errors
///
/// `Refused` when `reason` is blank (before anything is read), when the board does not hold the
/// task, or with `check_human_triage`'s reasons; `Files` or `Store` when the contract or the event
/// cannot be read or written.
#[allow(clippy::too_many_arguments)]
pub fn triage_by_human(
    files: &ProjectFiles,
    log: &EventLog,
    projections: &Projections,
    task_id: &TaskId,
    size: RequestSize,
    reason: &str,
    now: DateTime<Utc>,
    ids: &EventIds,
) -> Result<CatervasEvent, RequestError> {
    // 5.16 says the triage records its decision with a reason, and the log is where a person reads
    // it back months later. The command schema says `reason` is a string and nothing more, so this
    // is the place that holds it to being written.
    if reason.trim().is_empty() {
        return Err(RequestError::Refused {
            reason: "a triage is recorded with a reason, and the log is where somebody reads it \
                     back: say why this is the size it is"
                .to_string(),
        });
    }
    let mut contract = files.read_contract(task_id)?;
    check_human_triage(
        status_on_board(projections, task_id)?,
        contract.parent.is_some(),
    )
    .map_err(|reasons| RequestError::Refused {
        reason: reasons.join("; "),
    })?;

    // The triage decides the kind, and its own tool is what changes it (5.11, 5.16 item 1). The
    // file is written as well as the event, because the board takes the kind from the event and the
    // file is what a person reads.
    contract.kind = match size {
        RequestSize::Large => TaskKind::Epic,
        RequestSize::Small => TaskKind::Task,
    };
    contract.updated_at = Some(now);
    files.write_contract(&contract)?;
    record(
        log,
        projections,
        EventBody::RequestTriaged(RequestTriagedBody {
            reason: reason.to_string(),
            size: match size {
                RequestSize::Large => RequestTriagedBodySize::Large,
                RequestSize::Small => RequestTriagedBodySize::Small,
            },
            triaged_by: HUMAN.to_string(),
        }),
        task_id,
        now,
        ids,
    )
}

/// Takes a contract for the human (`held`), or gives it back to the team (`docs/SPEC.md` section
/// 5.11): the file's `locked` is written, `contract.locked` or `contract.unlocked` appended, and the
/// board brought up to it.
///
/// Whether the write is allowed is `check_contract_write`'s answer, asked with `locked` as the only
/// field changing and the human as the actor, about the status the board holds (the log decides a
/// status, 8.4) and the kind and the lock the file holds (the file decides a contract's content).
/// The gate is asked before the contract is found to be held already, so that a task nothing can be
/// written to says so rather than answering about the lock.
///
/// # Errors
///
/// `Refused` with the governor's refusal in words, when the board does not hold the task, or when
/// the contract is already held (or already the team's); `Files` or `Store` when the contract or
/// the event cannot be read or written.
pub fn hold_contract(
    files: &ProjectFiles,
    log: &EventLog,
    projections: &Projections,
    task_id: &TaskId,
    held: bool,
    now: DateTime<Utc>,
    ids: &EventIds,
) -> Result<CatervasEvent, RequestError> {
    let contract = files.read_contract(task_id)?;
    check_contract_write(
        contract.kind,
        status_on_board(projections, task_id)?,
        contract.locked,
        &ContractWriteActor {
            kind: TransitionActor::Human,
            agent_id: None,
        },
        &["locked".to_string()],
    )
    .map_err(|refusal| RequestError::Refused {
        reason: contract_write(&refusal),
    })?;
    if contract.locked == held {
        return Err(RequestError::Refused {
            reason: format!(
                "{} is already {}",
                task_id.as_str(),
                if held {
                    "yours: catervas contract unlock gives it back"
                } else {
                    "the team's"
                }
            ),
        });
    }

    let mut written = contract;
    written.locked = held;
    written.updated_at = Some(now);
    files.write_contract(&written)?;
    let body = if held {
        EventBody::ContractLocked(ContractLockedBody {
            locked_by: HUMAN.to_string(),
        })
    } else {
        EventBody::ContractUnlocked(ContractUnlockedBody {
            unlocked_by: HUMAN.to_string(),
        })
    };
    record(log, projections, body, task_id, now, ids)
}

/// The status the board says the task is at, which the log decides (`docs/SPEC.md` section 8.4).
fn status_on_board(
    projections: &Projections,
    task_id: &TaskId,
) -> Result<TaskStatus, RequestError> {
    projections
        .task(task_id)?
        .map(|row| row.status)
        .ok_or_else(|| RequestError::Refused {
            reason: format!(
                "the log has never heard of {}, so there is nothing of it to change: catervas \
                 doctor says where the files and the log disagree",
                task_id.as_str()
            ),
        })
}

/// Appends one event about `task_id` and brings the board up to it.
fn record(
    log: &EventLog,
    projections: &Projections,
    body: EventBody,
    task_id: &TaskId,
    now: DateTime<Utc>,
    ids: &EventIds,
) -> Result<CatervasEvent, RequestError> {
    let ids = EventIds {
        task_id: Some(task_id.clone()),
        ..ids.clone()
    };
    let event = new_event(body, now, ids).map_err(|error| RequestError::Refused {
        reason: format!("cannot be recorded: {error:?}"),
    })?;
    let recorded = log.append(&event)?;
    projections.apply(&recorded)?;
    Ok(recorded)
}

/// Why the governor would not let a contract be written, in the words a person reads (`docs/SPEC.md`
/// sections 5.2, 5.11 and 5.16).
#[must_use]
pub fn contract_write(refusal: &ContractWriteRefusal) -> String {
    match refusal {
        ContractWriteRefusal::ContractLocked => {
            "the contract is held by the human, and a contract's content is the holder's alone \
             (5.11): catervas contract unlock gives it back to the team"
                .to_string()
        }
        ContractWriteRefusal::ContractFrozen { fields } => format!(
            "the contract is frozen: once a task leaves refining only its status, assignee, \
             reviewer, iteration, sprint and notes change (5.11), and this would change {}",
            listed(fields)
        ),
        ContractWriteRefusal::TaskTerminal { status } => format!(
            "the task is {status}, and nothing leaves that status (5.2): its notes are all that \
             still change"
        ),
        ContractWriteRefusal::LifecycleFields { fields } => format!(
            "{} {} the governor's, written when it applies a transition (5.2): ask for the \
             transition instead",
            listed(fields),
            is_or_are(fields)
        ),
        ContractWriteRefusal::HumansFields { fields } => format!(
            "{} {} the human's alone (5.11)",
            listed(fields),
            is_or_are(fields)
        ),
        ContractWriteRefusal::StoresFields { fields } => format!(
            "{} {} the store's: Catervas assigns the identifier and the stamps",
            listed(fields),
            is_or_are(fields)
        ),
        ContractWriteRefusal::CreationFields { fields } => format!(
            "{} {} fixed when a contract is created (5.16): the triage decides the kind, and a \
             task's epic is the epic that broke it down",
            listed(fields),
            is_or_are(fields)
        ),
        ContractWriteRefusal::ContentFields { fields } => format!(
            "a contract's content is the Product Manager's and the human's, and an epic's tasks are \
             its assignee's (5.16, 6.2): this actor does not write {}",
            listed(fields)
        ),
        ContractWriteRefusal::UnknownFields { fields } => format!(
            "{} {} not a field of a contract: every one is listed by name, so a field added to the \
             schema is refused until somebody says who writes it",
            listed(fields),
            is_or_are(fields)
        ),
    }
}

/// A list in the words a person would read it out in.
fn listed(fields: &[String]) -> String {
    match fields {
        [] => "nothing".to_string(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Whether the sentence about those fields takes a singular verb.
fn is_or_are(fields: &[String]) -> &'static str {
    if fields.len() == 1 { "is" } else { "are" }
}

/// The fields of the contract the board shows, taken from the contract itself so that the log can
/// be replayed into projections without the files (`docs/SPEC.md` section 8.4).
///
/// # Panics
///
/// Never: a contract's own kind, risk, status and title are the summary's, and both vocabularies
/// come from `task-contract.schema.json`, which a test in `catervas-protocol` holds to agreeing.
#[must_use]
pub fn summary_of(contract: &TaskContract) -> ContractSummary {
    let mut value = json!({
        "kind": contract.kind.to_string(),
        "risk": contract.risk.to_string(),
        "status": contract.status.to_string(),
        "title": contract.title,
    });
    // An absent `parent` is absent rather than null: the summary's schema says `parent` is a
    // string when it is there at all.
    if let Some(parent) = &contract.parent {
        value["parent"] = json!(parent.as_str());
    }
    serde_json::from_value(value).expect(
        "a contract's own kind, risk, status and title are the summary's, and both vocabularies \
         come from task-contract.schema.json, which a test in catervas-protocol holds to agreeing",
    )
}

/// One board row as JSON, as `catervas board --json` prints it.
#[must_use]
pub fn board_row_json(row: &TaskProjection) -> Value {
    json!({
        "task_id": row.task_id.as_str(),
        "kind": row.kind.to_string(),
        "parent": row.parent.as_ref().map(|parent| parent.as_str().to_string()),
        "title": row.title,
        "status": row.status.to_string(),
        "risk": row.risk.to_string(),
        "triaged": row.triaged,
        "locked": row.locked,
    })
}

/// The board as JSON: `{ "tasks": [...] }`, one row each, in the order given.
#[must_use]
pub fn board_json(rows: &[TaskProjection]) -> Value {
    json!({ "tasks": rows.iter().map(board_row_json).collect::<Vec<_>>() })
}

/// The rules the governor applies as JSON, as `catervas rules show --json` prints them.
#[must_use]
pub fn rules_json(rules: &TeamRules) -> Value {
    json!({
        "protected_paths": rules.protected_paths,
        "allowed_paths_ceiling": rules.allowed_paths_ceiling,
        "required_criteria": rules.required_criteria,
        "require_new_tests": rules.require_new_tests,
        "max_task_budget_usd": rules.max_task_budget_usd,
        "forbidden_commands": rules.forbidden_commands,
        "document_paths": rules.document_paths,
    })
}

/// The criterion library as JSON, as `catervas criteria list --json` prints it.
#[must_use]
pub fn criteria_json(library: &CriteriaLibrary) -> Value {
    json!({
        "criteria": library
            .criteria
            .iter()
            .map(|one| json!({
                "name": one.name.as_str(),
                "text": one.text.as_str(),
                "source": one.source.as_ref().map(ToString::to_string),
                "method": criterion_method(&one.verification),
                "how": criterion_how(&one.verification),
            }))
            .collect::<Vec<_>>(),
    })
}

/// How a criterion is verified, in the one word the schema's `method` carries.
#[must_use]
pub fn criterion_method(verification: &TemplateVerification) -> &'static str {
    match verification {
        TemplateVerification::Variant0 { .. } => "command",
        TemplateVerification::Variant1 { .. } => "test",
        TemplateVerification::Variant2 { .. } => "artifact",
        TemplateVerification::Variant3 { .. } => "review",
        TemplateVerification::Variant4 { .. } => "human",
    }
}

/// What is actually run, read or asked, which is the part a person checks.
#[must_use]
pub fn criterion_how(verification: &TemplateVerification) -> String {
    match verification {
        TemplateVerification::Variant0 { command, .. }
        | TemplateVerification::Variant1 { command, .. } => command.clone(),
        TemplateVerification::Variant2 { path, .. } => path.clone(),
        TemplateVerification::Variant3 { rubric, .. } => rubric.join("; "),
        TemplateVerification::Variant4 { question, .. } => question.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use catervas_core::contract::fixtures::a_contract_wire;
    use catervas_core::contract::{TaskId, TaskKind, TaskStatus};
    use catervas_protocol::event::{EventBody, EventIds, EventKind, RequestTriagedBodySize};
    use chrono::{DateTime, TimeZone, Utc};
    use serde_json::Value;

    use catervas_protocol::command::RequestSize;
    use catervas_protocol::event::{ContractLockedBody, event_from_value, fixtures::an_event_wire};

    use catervas_core::contract::validate_contract;
    use catervas_core::governor::team_rules::TeamRules;
    use serde_json::json;

    use super::{
        PLACEHOLDER, RequestError, file_catervafication_epic, file_product_plan_request,
        file_request, hold_contract, placeholder_budget_usd, product_plan_request,
        request_from_brief, triage_by_human,
    };
    use crate::files::fixtures::{TempProject, a_team};
    use crate::git::fixtures::git_in;
    use crate::{EventLog, EventQuery, IN_MEMORY, Projections, open_event_log, open_projections};

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 22, 10, 0, 0)
            .single()
            .expect("a real time")
    }

    fn ids() -> EventIds {
        EventIds {
            team_id: "catervas".to_string(),
            project_id: "catervas".to_string(),
            ..EventIds::default()
        }
    }

    /// A request as its author writes it: the fixture contract without what Catervas fills in.
    fn a_request() -> Value {
        let mut wire = a_contract_wire();
        let object = wire.as_object_mut().expect("the fixture is a mapping");
        object.remove("id");
        object.remove("status");
        wire
    }

    fn a_project(name: &str) -> (TempProject, Arc<EventLog>) {
        let project = TempProject::new(name);
        project.files().init(&a_team()).expect(".catervas/ is made");
        let log = Arc::new(open_event_log(Path::new(IN_MEMORY), at()).expect("the log opens"));
        (project, log)
    }

    fn kinds(log: &EventLog) -> Vec<EventKind> {
        log.read(&EventQuery::default())
            .expect("the log reads")
            .iter()
            .map(|event| event.body.kind())
            .collect()
    }

    /// Every string in `value` that is one of its texts, as against its ids, roles, and risk.
    fn texts(value: &Value) -> Vec<String> {
        [
            value["scope"]["in_scope"][0].clone(),
            value["scope"]["out_of_scope"][0].clone(),
            value["requirements"][0]["text"].clone(),
            value["exit_criteria"][0]["text"].clone(),
            value["exit_criteria"][0]["verification"]["rubric"][0].clone(),
            value["allowed_paths"][0].clone(),
        ]
        .iter()
        .map(|text| text.as_str().unwrap_or_default().to_string())
        .collect()
    }

    #[test]
    fn the_product_plan_task_names_the_teams_format() {
        use catervas_core::contract::Role;
        use catervas_core::folders::DocumentFormat;
        use catervas_core::governor::readiness::evaluate_readiness;
        use catervas_core::governor::readiness::fixtures::a_ready_context;

        let (project, log) = a_project("product-plan-request-html");
        let wire = product_plan_request(Role::Architect, 20.0, DocumentFormat::Html);
        assert_eq!(
            wire["allowed_paths"],
            json!([
                "docs/catervas/product/spec.html",
                "docs/catervas/product/spec.agent.html",
                "docs/catervas/product/roadmap.html",
                "docs/catervas/product/roadmap.agent.html",
            ])
        );
        assert!(!wire.to_string().contains(".md"), "{wire}");
        let contract = file_product_plan_request(&project.files(), &log, wire, (at(), &ids()))
            .expect("the store files it");
        assert_eq!(evaluate_readiness(&contract, &a_ready_context()), Ok(()));
    }

    #[test]
    fn writes_the_product_plans_contract() {
        use catervas_core::contract::Role;
        use catervas_core::governor::readiness::evaluate_readiness;
        use catervas_core::governor::readiness::fixtures::a_ready_context;

        let (project, log) = a_project("product-plan-request");
        let wire = product_plan_request(
            Role::Architect,
            20.0,
            catervas_core::folders::DocumentFormat::Markdown,
        );
        let paths = [
            "docs/catervas/product/spec.md",
            "docs/catervas/product/spec.agent.md",
            "docs/catervas/product/roadmap.md",
            "docs/catervas/product/roadmap.agent.md",
        ];
        assert_eq!(wire["title"], "Draft the product plan");
        assert_eq!(wire["allowed_paths"], json!(paths));
        assert_eq!(wire["assignee_role"], "product_manager");
        assert_eq!(wire["reviewer_role"], "architect");
        assert_eq!(wire["risk"], "low");
        assert_eq!(wire["budget"]["max_cost_usd"], 20.0);
        let criteria = wire["exit_criteria"].as_array().expect("criteria");
        assert_eq!(criteria.len(), 2);
        assert!(
            criteria
                .iter()
                .all(|criterion| criterion["verification"]["method"] == "review")
        );

        let contract = file_product_plan_request(&project.files(), &log, wire, (at(), &ids()))
            .expect("the store files it");
        assert_eq!(contract.assignee_role, Role::ProductManager);
        assert_eq!(contract.allowed_paths, paths);
        assert_eq!(evaluate_readiness(&contract, &a_ready_context()), Ok(()));
        // The owner reads and accepts the result.
        assert!(catervas_core::governor::done::result_needs_the_human(
            &contract
        ));
        // The words the owner reads are plain, and these are the ones they get.
        assert_eq!(
            contract.scope.in_scope,
            [
                "The product description (spec.md) and the roadmap (roadmap.md), each with the team's copy."
            ]
        );
        assert_eq!(
            contract.requirements[2].text.to_string(),
            "The team's copies of both (spec.agent.md, roadmap.agent.md) say the same, written for the agents."
        );
        assert_eq!(
            contract.exit_criteria[0].text.to_string(),
            "The product description and the roadmap have every part listed above."
        );
        assert_eq!(
            contract.exit_criteria[1].text.to_string(),
            "The team's copies say the same as the documents you read."
        );

        let events = log.read(&EventQuery::default()).expect("the log reads");
        assert_eq!(
            events
                .iter()
                .map(|event| event.body.kind())
                .collect::<Vec<_>>(),
            [EventKind::TaskCreated, EventKind::RequestTriaged]
        );
        let EventBody::TaskCreated(created) = &events[0].body else {
            panic!("a task.created");
        };
        assert_eq!(created.created_by, "human");
        assert_eq!(created.product_plan, Some(true));
        let EventBody::RequestTriaged(triaged) = &events[1].body else {
            panic!("a request.triaged");
        };
        assert_eq!(triaged.size, RequestTriagedBodySize::Small);
        assert_eq!(triaged.triaged_by, "catervas");
    }

    #[test]
    fn files_a_catervafication_epic_triaged_by_catervas() {
        use catervas_protocol::event::CatervaficationStartedBodyProject as Project;

        let (project, log) = a_project("catervafication-epic");
        let files = project.files();
        let epic = file_catervafication_epic(
            &files,
            &log,
            a_request(),
            "human",
            Project::Existing,
            (at(), &ids()),
        )
        .expect("the epic is filed");
        assert_eq!(epic.kind, TaskKind::Epic);
        assert_eq!(epic.status, TaskStatus::Draft);
        assert_eq!(
            files.read_contract(&epic.id).expect("held").kind,
            TaskKind::Epic
        );

        let events = log.read(&EventQuery::default()).expect("the log reads");
        assert_eq!(
            events
                .iter()
                .map(|event| event.body.kind())
                .collect::<Vec<_>>(),
            [
                EventKind::TaskCreated,
                EventKind::RequestTriaged,
                EventKind::CatervaficationStarted
            ]
        );
        assert!(
            events
                .iter()
                .all(|event| event.envelope.ids.task_id.as_ref() == Some(&epic.id))
        );
        let EventBody::TaskCreated(created) = &events[0].body else {
            panic!("a task.created");
        };
        assert_eq!(created.created_by, "human");
        let EventBody::RequestTriaged(triaged) = &events[1].body else {
            panic!("a request.triaged");
        };
        assert_eq!(triaged.size, RequestTriagedBodySize::Large);
        assert_eq!(
            triaged.reason,
            "the Catervafication of an existing repository"
        );
        assert_eq!(triaged.triaged_by, "catervas");
        let EventBody::CatervaficationStarted(started) = &events[2].body else {
            panic!("a catervafication.started");
        };
        assert_eq!(started.project, Project::Existing);
        let projections = open_projections(Arc::clone(&log)).expect("the projections open");
        let row = projections
            .task(&epic.id)
            .expect("the board reads")
            .expect("the epic is on it");
        assert_eq!(row.kind, TaskKind::Epic);
        assert!(row.triaged);

        let (project, log) = a_project("catervafication-epic-new");
        file_catervafication_epic(
            &project.files(),
            &log,
            a_request(),
            "catervas",
            Project::New,
            (at(), &ids()),
        )
        .expect("the epic is filed");
        let events = log.read(&EventQuery::default()).expect("the log reads");
        let EventBody::RequestTriaged(triaged) = &events[1].body else {
            panic!("a request.triaged");
        };
        assert_eq!(triaged.reason, "the Catervafication of a new project");
        let EventBody::CatervaficationStarted(started) = &events[2].body else {
            panic!("a catervafication.started");
        };
        assert_eq!(started.project, Project::New);
    }

    #[test]
    fn refuses_an_epic_wire_that_sets_its_kind() {
        use catervas_protocol::event::CatervaficationStartedBodyProject as Project;

        let (project, log) = a_project("catervafication-epic-kind");
        let mut wire = a_request();
        wire["kind"] = json!("epic");
        let refused = file_catervafication_epic(
            &project.files(),
            &log,
            wire,
            "human",
            Project::Existing,
            (at(), &ids()),
        )
        .expect_err("the kind is Catervas's to give");
        let RequestError::Refused { reason } = refused else {
            panic!("a refusal, not a failure: {refused:?}");
        };
        assert!(reason.starts_with("sets kind,"), "{reason}");
        assert!(kinds(&log).is_empty(), "nothing is recorded");
        assert!(
            project.files().list_contracts().expect("a list").is_empty(),
            "nothing is filed"
        );
    }

    #[test]
    fn builds_a_request_the_store_files() {
        let brief = "Add done.txt and a check that it exists";
        assert_eq!(brief.len(), 39);
        let brief = format!("{brief}.");
        let budget = placeholder_budget_usd(&TeamRules::default());
        let mut request =
            request_from_brief("Add done.txt", &brief, budget).expect("a request is built");
        assert!(texts(&request).iter().all(|text| text == PLACEHOLDER));
        request["id"] = json!("CTV-1");
        request["status"] = json!("draft");
        validate_contract(&request).expect("the store would file it");

        assert!(request_from_brief("Add done.txt", "A brief too short.!", 5.0).is_err());
        assert!(request_from_brief("Ad", &brief, 5.0).is_err());
    }

    #[test]
    fn places_a_budget_of_the_team_cap_or_twenty_dollars() {
        let capped = TeamRules {
            max_task_budget_usd: Some(12.5),
            ..TeamRules::default()
        };
        assert!((placeholder_budget_usd(&capped) - 12.5).abs() < 1e-9);
        assert!((placeholder_budget_usd(&TeamRules::default()) - 20.0).abs() < 1e-9);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn files_a_request_past_the_highest_old_task_branch() {
        let (project, log) = a_project("requests-old-branch");
        let root = project.root.clone();
        git_in(&root, &["init", "-b", "main"]);
        git_in(&root, &["config", "user.name", "Catervas Test"]);
        git_in(&root, &["config", "user.email", "test@catervas.invalid"]);
        git_in(&root, &["commit", "--allow-empty", "-m", "first"]);
        git_in(&root, &["branch", "feature/CTV-7"]);
        let files = project.files();
        let request = file_request(&files, &log, a_request(), "human", None, at(), &ids(), None)
            .expect("the request is filed");
        assert_eq!(
            request.id.as_str(),
            "CTV-8",
            "past the branch, with no contract"
        );

        // CTV-8 is a contract now; one numbered above the branch still wins.
        let mut held = request.clone();
        held.id = "CTV-10".parse().expect("a task id");
        files.create_contract(&held).expect("a contract is held");
        let fresh = Arc::new(open_event_log(Path::new(IN_MEMORY), at()).expect("the log opens"));
        let next = file_request(
            &files,
            &fresh,
            a_request(),
            "human",
            None,
            at(),
            &ids(),
            None,
        )
        .expect("the request is filed");
        assert_eq!(
            next.id.as_str(),
            "CTV-11",
            "past the contract, with the lower branch"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn files_a_request_when_git_cannot_list_branches() {
        // The folder is not a repository, so its branches cannot be listed: it counts as having none.
        let (project, log) = a_project("requests-no-repository");
        let files = project.files();
        let request = file_request(&files, &log, a_request(), "human", None, at(), &ids(), None)
            .expect("the request is filed");
        assert_eq!(request.id.as_str(), "CTV-1");
    }

    #[test]
    fn files_a_request_with_the_next_id_the_log_hands_out() {
        let (project, log) = a_project("requests-next");
        let files = project.files();
        let request = file_request(&files, &log, a_request(), "human", None, at(), &ids(), None)
            .expect("the request is filed");

        assert_eq!(request.id.as_str(), "CTV-1");
        assert_eq!(request.status, TaskStatus::Draft);
        assert_eq!(
            files.read_contract(&request.id).expect("it is on disk"),
            request
        );
        assert_eq!(kinds(&log), vec![EventKind::TaskCreated]);
    }

    #[test]
    fn files_a_child_as_a_triaged_task_of_its_epic() {
        let (project, log) = a_project("requests-child");
        let files = project.files();
        let epic = TaskId::try_from("CTV-1").expect("an id");
        let request = file_request(
            &files,
            &log,
            a_request(),
            "pm",
            Some(&epic),
            at(),
            &ids(),
            None,
        )
        .expect("the child is filed");

        assert_eq!(request.kind, TaskKind::Task);
        assert_eq!(request.parent.as_ref().map(|p| p.as_str()), Some("CTV-1"));
        assert_eq!(request.status, TaskStatus::Draft);
        let events = log.read(&EventQuery::default()).expect("the log reads");
        assert_eq!(
            kinds(&log),
            vec![EventKind::TaskCreated, EventKind::RequestTriaged]
        );
        let EventBody::RequestTriaged(body) = &events[1].body else {
            panic!("the second event is the triage");
        };
        assert_eq!(body.size, RequestTriagedBodySize::Small);
        assert_eq!(body.triaged_by, "pm");
        assert_eq!(
            events[1].envelope.ids.task_id.as_ref(),
            Some(&request.id),
            "the triage is about the child"
        );
        let board = open_projections(Arc::clone(&log))
            .expect("the projections open")
            .board()
            .expect("the board reads");
        assert!(board[0].triaged);
        assert_eq!(board[0].parent.as_ref(), Some(&epic));
    }

    #[test]
    fn refuses_a_request_that_sets_an_id() {
        let (project, log) = a_project("requests-id");
        let files = project.files();
        let mut wire = a_request();
        wire["id"] = serde_json::json!("CTV-9");
        let refused = file_request(&files, &log, wire, "human", None, at(), &ids(), None)
            .expect_err("an id is Catervas's to give");

        let RequestError::Refused { reason } = refused else {
            panic!("a refusal, not a failure: {refused:?}");
        };
        assert!(reason.starts_with("sets id,"), "{reason}");
        assert!(kinds(&log).is_empty(), "nothing is recorded");
        assert!(
            files.list_contracts().expect("a list").is_empty(),
            "nothing is filed"
        );
    }

    /// A project with one request the human filed, CTV-1, and the board over its log.
    fn a_filed_request(name: &str) -> (TempProject, Arc<EventLog>, Projections, TaskId) {
        let (project, log) = a_project(name);
        let request = file_request(
            &project.files(),
            &log,
            a_request(),
            "human",
            None,
            at(),
            &ids(),
            None,
        )
        .expect("the request is filed");
        let projections = open_projections(Arc::clone(&log)).expect("the projections open");
        (project, log, projections, request.id)
    }

    /// Records CTV-1's move from `draft` to `to`, as the governor would.
    fn moved(log: &EventLog, projections: &Projections, to: &str) {
        let mut wire = an_event_wire(EventKind::TaskTransitioned);
        wire["task_id"] = serde_json::json!("CTV-1");
        wire["body"]["from"] = serde_json::json!("draft");
        wire["body"]["to"] = serde_json::json!(to);
        let event = event_from_value(&wire).expect("the fixture is schema-valid");
        let recorded = log
            .append(&catervas_protocol::event::NewEvent {
                recorded_at: event.envelope.recorded_at,
                ids: event.envelope.ids,
                body: event.body,
            })
            .expect("the move is recorded");
        projections.apply(&recorded).expect("the board moves");
    }

    #[test]
    fn records_the_humans_triage() {
        let (project, log, projections, id) = a_filed_request("requests-human-triage");
        let files = project.files();

        let event = triage_by_human(
            &files,
            &log,
            &projections,
            &id,
            RequestSize::Large,
            "Three deliverables.",
            at(),
            &ids(),
        )
        .expect("the human may size a draft");

        assert_eq!(
            files.read_contract(&id).expect("a contract").kind,
            TaskKind::Epic
        );
        let EventBody::RequestTriaged(body) = &event.body else {
            panic!("the event is the triage: {event:?}");
        };
        assert_eq!(body.size, RequestTriagedBodySize::Large);
        assert_eq!(body.triaged_by, "human");
        assert_eq!(body.reason, "Three deliverables.");
        assert_eq!(event.envelope.ids.task_id.as_ref(), Some(&id));
        assert_eq!(
            kinds(&log),
            vec![EventKind::TaskCreated, EventKind::RequestTriaged]
        );
        let row = projections
            .task(&id)
            .expect("the board reads")
            .expect("CTV-1 is on it");
        assert_eq!(row.kind, TaskKind::Epic);
        assert!(row.triaged);
    }

    #[test]
    fn refuses_the_humans_triage_once_refining_started() {
        let (project, log, projections, id) = a_filed_request("requests-human-triage-late");
        let files = project.files();
        moved(&log, &projections, "refining");
        let before = kinds(&log).len();

        let refused = triage_by_human(
            &files,
            &log,
            &projections,
            &id,
            RequestSize::Large,
            "Too late.",
            at(),
            &ids(),
        )
        .expect_err("refining has started");

        let RequestError::Refused { reason } = refused else {
            panic!("a refusal: {refused:?}");
        };
        assert!(
            reason.contains("a triage is the first thing that happens"),
            "{reason}"
        );
        assert_eq!(kinds(&log).len(), before, "nothing is appended");

        let unknown = TaskId::try_from("CTV-9").expect("an id");
        let blank = triage_by_human(
            &files,
            &log,
            &projections,
            &unknown,
            RequestSize::Small,
            "   ",
            at(),
            &ids(),
        )
        .expect_err("a blank reason is no reason");
        let RequestError::Refused { reason } = blank else {
            panic!("refused before CTV-9 is looked for: {blank:?}");
        };
        assert!(reason.contains("recorded with a reason"), "{reason}");
    }

    #[test]
    fn locks_and_gives_back_a_contract() {
        let (project, log, projections, id) = a_filed_request("requests-lock");
        let files = project.files();

        let locked = hold_contract(&files, &log, &projections, &id, true, at(), &ids())
            .expect("the human may take a draft");
        assert!(files.read_contract(&id).expect("a contract").locked);
        assert_eq!(
            locked.body,
            EventBody::ContractLocked(ContractLockedBody {
                locked_by: "human".to_string()
            })
        );
        assert!(
            projections
                .task(&id)
                .expect("the board reads")
                .expect("on it")
                .locked
        );

        let again = hold_contract(&files, &log, &projections, &id, true, at(), &ids())
            .expect_err("it is held already");
        let RequestError::Refused { reason } = again else {
            panic!("a refusal: {again:?}");
        };
        assert!(reason.contains("already"), "{reason}");

        let given = hold_contract(&files, &log, &projections, &id, false, at(), &ids())
            .expect("the human may give it back");
        assert_eq!(given.body.kind(), EventKind::ContractUnlocked);
        assert!(!files.read_contract(&id).expect("a contract").locked);
        assert_eq!(
            kinds(&log),
            vec![
                EventKind::TaskCreated,
                EventKind::ContractLocked,
                EventKind::ContractUnlocked
            ]
        );
    }

    #[test]
    fn a_pairs_change_reaches_both_files() {
        use catervas_core::contract::Role;
        let spec = "docs/catervas/product/spec.html";
        let twin = "docs/catervas/product/spec.agent.html";
        let wire = super::change_request(
            spec,
            Some(twin),
            "Product spec",
            "Add a refund limit.",
            Role::ProductManager,
            Role::Architect,
            2.0,
        );
        assert_eq!(wire["allowed_paths"], json!([spec, twin]));
        assert_eq!(wire["scope"]["in_scope"], json!([spec, twin]));
        assert_eq!(wire["scope"]["out_of_scope"], json!(["Every other file"]));
        assert_eq!(wire["title"], "Change Product spec");
        assert_eq!(wire["assignee_role"], "product_manager");
        assert_eq!(wire["reviewer_role"], "architect");
        assert_eq!(
            wire["requirements"][0]["text"],
            format!("{spec} makes the change the owner asked for.")
        );
        assert_eq!(
            wire["exit_criteria"][0]["verification"]["rubric"],
            json!([
                format!("{spec} makes the change the owner's words ask for."),
                format!("Nothing else in {spec} changes."),
                format!("{twin} says what {spec} now says."),
            ])
        );
        assert!(
            wire["intent"]
                .as_str()
                .is_some_and(|intent| intent.contains("Add a refund limit.")),
            "{wire}"
        );

        let alone = super::change_request(
            "docs/catervas/architecture/overview.html",
            None,
            "Overview",
            "Shorter.",
            Role::Architect,
            Role::ProductManager,
            2.0,
        );
        assert_eq!(
            alone["allowed_paths"],
            json!(["docs/catervas/architecture/overview.html"])
        );
        assert_eq!(
            alone["exit_criteria"][0]["verification"]["rubric"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );
    }

    #[test]
    fn a_long_name_is_cut_to_the_titles_limit() {
        use catervas_core::contract::Role;
        let wire = super::change_request(
            "docs/catervas/product/spec.html",
            None,
            &"a".repeat(200),
            "x",
            Role::ProductManager,
            Role::Architect,
            2.0,
        );
        let title = wire["title"].as_str().expect("a title");
        assert_eq!(title.chars().count(), 120, "{title}");
        assert!(
            title.starts_with("Change aaa") && title.ends_with('…'),
            "{title}"
        );

        let whole = super::change_request(
            "docs/catervas/product/spec.html",
            None,
            &"a".repeat(113),
            "x",
            Role::ProductManager,
            Role::Architect,
            2.0,
        );
        let title = whole["title"].as_str().expect("a title");
        assert_eq!(title.chars().count(), 120);
        assert!(!title.ends_with('…'), "{title}");
    }
}
