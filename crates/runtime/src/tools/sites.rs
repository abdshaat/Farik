//! `catervas_request_sites` and `catervas_read_sites` (`docs/SPEC.md` 6.10, ADR 0039): the Procurement
//! Specialist reads only Catervas's approved sites and the sites its owner allowed, asks for another
//! with its reason and ends its turn, and reads where it may read.

use std::collections::BTreeSet;
use std::sync::Mutex;

use catervas_core::contract::TaskId;
use catervas_core::governor::sites::{WebAccess, site_of, web_access};
use catervas_protocol::event::{EventBody, SiteRequestedBody};
use catervas_roles::sites::catervas_sites;
use catervas_store::sites::{CatervasEntry, approved_sites, declined_sites, site_requests};
use catervas_store::{EventLog, StoreError};
use chrono::{DateTime, SecondsFormat, Utc};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::sheets::in_its_own_implement_session;
use super::{Call, ToolError, failed};

/// The most sites one call asks for.
const MOST_SITES_A_CALL: usize = 10;
/// The most requests that wait for the owner in a project at once.
const MOST_WAITING: usize = 20;
/// The most characters a reason has.
const MOST_WHY: usize = 300;
/// The most characters an address has.
const MOST_URL: usize = 2_048;
/// The most characters of an address an answer repeats.
const SHOWN_URL: usize = 200;
/// What an agent whose request was recorded is told to do.
const NEXT: &str = "end your turn: the owner's decision starts the next session";
/// Held while a request's place is counted and taken, one lock for every project in the process.
static ASKING: Mutex<()> = Mutex::new(());

/// `catervas_request_sites`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequestSitesInput {
    /// 1 to 10 sites you want to read and may not yet, each with the first page you want and why.
    sites: Vec<SiteAsk>,
}

/// One site `catervas_request_sites` asks the owner to allow.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SiteAsk {
    /// The first page you want to read, as an address that starts with https:// and names the
    /// site, at most 2048 characters. The owner is shown it as text.
    url: String,
    /// Why you need this site, 1 to 300 characters on one line, in your own words.
    why: String,
}

fn refused(code: &'static str, detail: impl Into<String>) -> ToolError {
    Refusal::Sites {
        code,
        detail: detail.into(),
    }
    .into()
}

/// Catervas's own hosts, from the list the running release ships.
fn catervas_hosts() -> BTreeSet<String> {
    catervas_sites()
        .iter()
        .map(|site| site.host.clone())
        .collect()
}

/// The sites a held role may read now: Catervas's, and the owner's, from the project's log.
///
/// # Errors
///
/// What the log refused.
pub(crate) fn approved_set(log: &EventLog) -> Result<BTreeSet<String>, StoreError> {
    approved_sites(log, &catervas_hosts())
}

/// The sites as `sites.list` and `catervas site list --json` answer: Catervas's with whether each is on
/// and when the owner last turned it off or on, the owner's own, and the requests that wait.
///
/// # Errors
///
/// What the log refused.
pub fn site_list(log: &EventLog) -> Result<Value, StoreError> {
    let entries: Vec<CatervasEntry> = catervas_sites()
        .iter()
        .map(|site| CatervasEntry {
            host: site.host.clone(),
            shop: site.shop.clone(),
            category: site.category.to_string(),
        })
        .collect();
    let list = catervas_store::sites::site_list(log, &entries)?;
    let time = |at: DateTime<Utc>| at.to_rfc3339_opts(SecondsFormat::AutoSi, true);
    let catervas: Vec<Value> = list
        .catervas
        .iter()
        .map(|row| {
            let mut wire = json!({
                "host": row.host, "shop": row.shop, "category": row.category, "on": row.on,
            });
            if let Some(at) = row.at {
                wire["at"] = json!(time(at));
            }
            wire
        })
        .collect();
    let owner: Vec<Value> = list
        .owner
        .iter()
        .map(|row| {
            let mut wire = json!({ "host": row.host, "at": time(row.at) });
            if let Some(request) = row.request {
                wire["request"] = json!(request);
            }
            wire
        })
        .collect();
    let waiting: Vec<Value> = list
        .waiting
        .iter()
        .map(|asked| {
            json!({
                "request": asked.request, "host": asked.host, "url": asked.url,
                "why": asked.why, "task_id": asked.task_id, "agent_id": asked.agent_id,
                "at": time(asked.at),
            })
        })
        .collect();
    Ok(json!({ "catervas": catervas, "owner": owner, "waiting": waiting }))
}

/// An address cut for an answer or a refusal, so that what an agent wrote never fills the log.
pub(crate) fn shown(url: &str) -> String {
    if url.chars().count() > SHOWN_URL {
        format!("{}...", url.chars().take(SHOWN_URL).collect::<String>())
    } else {
        url.to_string()
    }
}

/// The site an entry names, or why it names none; and its reason, held to its rules.
fn checked(ask: &SiteAsk) -> Result<String, ToolError> {
    if ask.url.chars().count() > MOST_URL {
        return Err(refused(
            "site_invalid",
            format!(
                "{} its address is longer than {MOST_URL} characters",
                shown(&ask.url)
            ),
        ));
    }
    let host = site_of(&ask.url)
        .map_err(|fault| refused("site_invalid", format!("{} {fault}", shown(&ask.url))))?;
    let length = ask.why.chars().count();
    if ask.why.trim().is_empty() || length > MOST_WHY || ask.why.contains(['\r', '\n']) {
        return Err(refused(
            "site_why_invalid",
            format!(
                "the reason for {host} is 1 to {MOST_WHY} characters on one line, and is not blank"
            ),
        ));
    }
    Ok(host)
}

/// `catervas_request_sites`: answers each entry, in order, `allowed` for a site that is approved,
/// `waiting` for one this task asked for already, `declined` (with the owner's note) for one the
/// owner did not allow for this task, `full` for one that would be the twenty-first to wait in the
/// project, and otherwise records `site.requested` and answers `asked` with its number. A call is
/// refused whole, with nothing recorded, for one bad entry. When something was recorded the answer
/// ends with what to do: end the turn.
///
/// # Errors
///
/// `site_request_refused` outside the implement session of the held role's own task,
/// `site_count_invalid` for no entry or more than ten, `site_invalid` and `site_why_invalid` for an
/// entry that breaks its rule, `too_many_site_requests` when nothing was recorded because twenty
/// requests wait already; `Failed` when the log cannot be read or written.
pub(super) fn request_sites(
    call: &Call<'_>,
    input: &RequestSitesInput,
) -> Result<Value, ToolError> {
    let refuse = |why: &str| refused("site_request_refused", format!("only {why}"));
    if web_access(call.role()) != WebAccess::ApprovedSites {
        return Err(refuse("the Procurement Specialist asks for a site"));
    }
    in_its_own_implement_session(call, "a request for a site", &refuse)?;
    let task = call.task()?.clone();
    if !(1..=MOST_SITES_A_CALL).contains(&input.sites.len()) {
        return Err(refused(
            "site_count_invalid",
            format!(
                "ask for 1 to {MOST_SITES_A_CALL} sites in a call, not {}",
                input.sites.len()
            ),
        ));
    }
    let hosts = input
        .sites
        .iter()
        .map(checked)
        .collect::<Result<Vec<_>, _>>()?;
    // The count of what waits and the requests that raise it are one step, since two sessions of
    // the role may ask at once and the cap is the project's.
    let _asking = ASKING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let log = &call.deps().log;
    let approved = approved_set(log).map_err(failed)?;
    let requests = site_requests(log).map_err(failed)?;
    let mut waiting_here: BTreeSet<String> = requests
        .iter()
        .filter(|asked| asked.decision.is_none() && asked.task_id == task)
        .map(|asked| asked.host.clone())
        .collect();
    let mut waiting_in_all = requests
        .iter()
        .filter(|asked| asked.decision.is_none())
        .count();
    let declined = declined_sites(log, &task).map_err(failed)?;
    let (mut asked, mut full) = (0, 0);
    let mut answers = Vec::new();
    for (entry, host) in input.sites.iter().zip(hosts) {
        let mut answer = json!({ "url": entry.url, "host": host });
        if approved.contains(&host) {
            answer["answer"] = json!("allowed");
        } else if waiting_here.contains(&host) {
            answer["answer"] = json!("waiting");
        } else if let Some(refusal) = declined.iter().find(|site| site.host == host) {
            answer["answer"] = json!("declined");
            answer["note"] = json!(refusal.note.clone().unwrap_or_default());
        } else if waiting_in_all >= MOST_WAITING {
            answer["answer"] = json!("full");
            full += 1;
        } else {
            let body = SiteRequestedBody {
                host: host.clone().try_into().map_err(failed)?,
                url: entry.url.clone().try_into().map_err(failed)?,
                why: entry.why.clone().try_into().map_err(failed)?,
            };
            let event = call.append(Some(&task), EventBody::SiteRequested(body))?;
            waiting_here.insert(host);
            waiting_in_all += 1;
            asked += 1;
            answer["answer"] = json!("asked");
            answer["request"] = json!(event.envelope.seq);
        }
        answers.push(answer);
    }
    if asked == 0 && full > 0 {
        return Err(refused(
            "too_many_site_requests",
            format!(
                "{MOST_WAITING} requests wait for the owner already, so none was recorded; read \
                 catervas_read_sites for what you may read, and go on without the rest"
            ),
        ));
    }
    let mut answer = json!({ "sites": answers });
    if asked > 0 {
        answer["next"] = json!(NEXT);
    }
    Ok(answer)
}

/// `catervas_read_sites`: the sites the held role may read, Catervas's with their shop and category and
/// then the owner's; and, for the session's task, the sites waiting for the owner and those the
/// owner did not allow, with their notes. Records nothing.
///
/// # Errors
///
/// `sites_refused` for a role that is not held to approved sites; `Failed` when the log cannot be
/// read.
pub(super) fn read_sites(call: &Call<'_>) -> Result<Value, ToolError> {
    if web_access(call.role()) != WebAccess::ApprovedSites {
        return Err(refused(
            "sites_refused",
            "only the Procurement Specialist reads the sites it may read",
        ));
    }
    let log = &call.deps().log;
    let approved = approved_set(log).map_err(failed)?;
    let mut listed: Vec<Value> = catervas_sites()
        .iter()
        .filter(|site| approved.contains(&site.host))
        .map(|site| {
            json!({
                "host": site.host,
                "shop": site.shop,
                "category": site.category.to_string(),
            })
        })
        .collect();
    let catervas = catervas_hosts();
    listed.extend(
        approved
            .iter()
            .filter(|host| !catervas.contains(*host))
            .map(|host| json!({ "host": host })),
    );
    let (waiting, declined) = match &call.context.task_id {
        Some(task) => (
            waiting_hosts(log, task).map_err(failed)?,
            declined_sites(log, task).map_err(failed)?,
        ),
        None => (Vec::new(), Vec::new()),
    };
    Ok(json!({
        "approved": listed,
        "waiting": waiting,
        "declined": declined
            .iter()
            .map(|site| json!({ "host": site.host, "note": site.note.clone().unwrap_or_default() }))
            .collect::<Vec<_>>(),
    }))
}

/// The hosts `task` asked for that wait for the owner, oldest request first.
fn waiting_hosts(log: &EventLog, task: &TaskId) -> Result<Vec<String>, StoreError> {
    Ok(site_requests(log)?
        .into_iter()
        .filter(|asked| asked.decision.is_none() && &asked.task_id == task)
        .map(|asked| asked.host)
        .collect())
}

/// Refuses, as `site_request_waiting`, the task's own site request while it waits for the owner:
/// its assignee does not hand the task in meanwhile (ADR 0039).
///
/// # Errors
///
/// `site_request_waiting` naming the first host that waits; `Failed` when the log cannot be read.
pub(super) fn refuse_while_waiting(call: &Call<'_>, task: &TaskId) -> Result<(), ToolError> {
    match waiting_hosts(&call.deps().log, task)
        .map_err(failed)?
        .first()
    {
        Some(host) => Err(refused(
            "site_request_waiting",
            format!("{host} waits for the owner; end your turn"),
        )),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use catervas_core::contract::Role;
    use catervas_protocol::event::{EventBody, EventKind};
    use catervas_roles::sites::catervas_sites;
    use serde_json::{Value, json};

    use crate::session::SessionPurpose;
    use crate::tools::ToolError;
    use crate::tools::fixtures::{
        TestProject, a_team_of_three, at, run, with_the_finance_specialist,
        with_the_marketing_specialist, with_the_procurement_specialist,
    };

    const NEXT: &str = "end your turn: the owner's decision starts the next session";

    /// A project with the Finance Specialist `fin`, the Marketing Specialist `kai` and the
    /// Procurement Specialist `proc`, whose tasks FRK-1 and FRK-4 are in progress, a finance task
    /// FRK-2 and a Developer's task FRK-3.
    fn a_project(name: &str) -> TestProject {
        let project = TestProject::new(
            name,
            &a_team_of_three(|wire| {
                with_the_finance_specialist(wire);
                with_the_marketing_specialist(wire);
                with_the_procurement_specialist(wire);
            }),
        );
        for (task, role, assignee) in [
            ("FRK-1", "procurement_specialist", "proc"),
            ("FRK-2", "finance_specialist", "fin"),
            ("FRK-4", "procurement_specialist", "proc"),
        ] {
            project.filed_with(task, "assigned", "task", None, |wire| {
                wire["assignee_role"] = json!(role);
                wire["reviewer_role"] = json!("product_manager");
            });
            project.moved(
                task,
                "assigned",
                "in_progress",
                &json!({ "assignee": assignee, "reviewer": "pm" }),
            );
        }
        project.filed("FRK-3", "assigned", "task", None);
        project.moved(
            "FRK-3",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
        );
        project
    }

    /// `catervas_request_sites` as `proc` in its implement session of FRK-1.
    fn request(project: &TestProject, sites: &[(&str, &str)]) -> Result<Value, ToolError> {
        let sites: Vec<Value> = sites
            .iter()
            .map(|(url, why)| json!({ "url": url, "why": why }))
            .collect();
        project.call(
            "proc",
            Some("FRK-1"),
            "catervas_request_sites",
            json!({ "sites": sites }),
        )
    }

    fn refusal_of(result: Result<Value, ToolError>) -> String {
        match result {
            Err(ToolError::Refused { reason }) => reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// What each entry of an answer says: its `answer` word.
    fn answers(answer: &Value) -> Vec<&str> {
        answer["sites"]
            .as_array()
            .expect("a list of answers")
            .iter()
            .map(|entry| entry["answer"].as_str().expect("an answer word"))
            .collect()
    }

    /// One of Catervas's own hosts, taken from the shipped list so that the launch review edits the
    /// YAML alone.
    fn a_catervas_host() -> &'static str {
        &catervas_sites()[0].host
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn asks_for_each_new_site() {
        let project = a_project("sites-ask");
        project.record("", "site.approved", &json!({ "host": "owner.example" }));
        let catervas = format!("https://www.{}/", a_catervas_host());

        let answer = request(
            &project,
            &[
                ("https://owner.example/prices", "It sells the boxes."),
                (&catervas, "A big shop."),
                ("https://shop.example/boxes", "A maker of boxes."),
                ("https://WWW.New.example/Boxes?x=1", "Another maker."),
            ],
        )
        .expect("the request is taken");

        let asked = project.events(&[EventKind::SiteRequested]);
        assert_eq!(asked.len(), 2, "only the two new sites are recorded");
        for event in &asked {
            let ids = &event.envelope.ids;
            assert_eq!(
                ids.task_id.as_ref().map(|task| task.as_str()),
                Some("FRK-1")
            );
            assert_eq!(ids.agent_id.as_deref(), Some("proc"));
            assert_eq!(ids.session_id.as_deref(), Some("session-1"));
        }
        let EventBody::SiteRequested(second) = &asked[1].body else {
            panic!("a site.requested event carries its body");
        };
        assert_eq!(second.host.to_string(), "new.example");
        assert_eq!(
            second.url.to_string(),
            "https://WWW.New.example/Boxes?x=1",
            "the address is kept exactly as the agent wrote it"
        );
        assert_eq!(second.why.to_string(), "Another maker.");
        assert_eq!(answers(&answer), ["allowed", "allowed", "asked", "asked"]);
        assert_eq!(answer["sites"][0]["host"], "owner.example");
        assert_eq!(answer["sites"][1]["host"], a_catervas_host());
        assert_eq!(answer["sites"][2]["request"], asked[0].envelope.seq);
        assert_eq!(answer["sites"][3]["request"], asked[1].envelope.seq);
        assert_eq!(answer["next"], NEXT);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn says_nothing_to_wait_for_when_nothing_was_asked() {
        let project = a_project("sites-ask-nothing");
        let before = project.event_count();

        let answer = request(
            &project,
            &[(
                &format!("https://{}/", a_catervas_host()),
                "It is on the list.",
            )],
        )
        .expect("an approved site is taken");

        assert_eq!(answers(&answer), ["allowed"]);
        assert!(answer.get("next").is_none(), "{answer}");
        assert_eq!(project.event_count(), before);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn asks_for_a_catervas_site_the_owner_turned_off() {
        let project = a_project("sites-ask-turned-off");
        project.record("", "site.removed", &json!({ "host": a_catervas_host() }));

        let answer = request(
            &project,
            &[(
                &format!("https://{}/", a_catervas_host()),
                "I need it again.",
            )],
        )
        .expect("the request is taken");

        assert_eq!(answers(&answer), ["asked"]);
        assert_eq!(project.events(&[EventKind::SiteRequested]).len(), 1);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn does_not_ask_twice() {
        let project = a_project("sites-twice");
        let first = request(&project, &[("https://shop.example/", "A maker.")]).expect("asked");
        assert_eq!(answers(&first), ["asked"]);
        let refused_one =
            request(&project, &[("https://other.example/", "Another maker.")]).expect("asked");
        let other = refused_one["sites"][0]["request"]
            .as_u64()
            .expect("a number");
        project.record(
            "FRK-1",
            "site.declined",
            &json!({ "request": other, "host": "other.example", "note": "Not that one." }),
        );
        let before = project.event_count();

        let again = request(
            &project,
            &[
                ("https://www.shop.example/other-page", "Still wanted."),
                ("https://other.example/", "Asking once more."),
            ],
        )
        .expect("answered");

        assert_eq!(answers(&again), ["waiting", "declined"]);
        assert_eq!(again["sites"][1]["note"], "Not that one.");
        assert!(again.get("next").is_none(), "nothing was recorded: {again}");
        assert_eq!(
            project.event_count(),
            before,
            "nothing is recorded for either"
        );

        // A later approval of the site, by any route, ends the refusal.
        project.record("", "site.approved", &json!({ "host": "other.example" }));
        let allowed =
            request(&project, &[("https://other.example/", "Allowed now.")]).expect("answered");
        assert_eq!(answers(&allowed), ["allowed"]);
        // Two entries for one site in one call ask once.
        let twice = request(
            &project,
            &[
                ("https://new.example/a", "First."),
                ("https://new.example/b", "Second."),
            ],
        )
        .expect("answered");
        assert_eq!(answers(&twice), ["asked", "waiting"]);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_bad_site_or_why() {
        let project = a_project("sites-refuse");
        let good = ("https://shop.example/", "A maker.");
        let long = "w".repeat(301);
        let too_long_url = format!("https://shop.example/{}", "a".repeat(2_049));
        for (url, why, code) in [
            ("http://a.com", "A maker.", "site_invalid: http://a.com "),
            (
                "https://127.0.0.1/",
                "A maker.",
                "site_invalid: https://127.0.0.1/ ",
            ),
            ("a.com", "A maker.", "site_invalid: a.com "),
            (
                "https://u:p@a.com/",
                "A maker.",
                "site_invalid: https://u:p@a.com/ ",
            ),
            (&too_long_url, "A maker.", "site_invalid: "),
            ("https://a.com/", "", "site_why_invalid: "),
            ("https://a.com/", "   ", "site_why_invalid: "),
            ("https://a.com/", &long, "site_why_invalid: "),
            ("https://a.com/", "one\ntwo", "site_why_invalid: "),
            ("https://a.com/", "one\rtwo", "site_why_invalid: "),
        ] {
            // The good entry first: a call is refused whole, and nothing is recorded.
            let reason = refusal_of(request(&project, &[good, (url, why)]));
            assert!(reason.starts_with(code), "{url} {why:?}: {reason}");
            assert_eq!(
                project.events(&[EventKind::SiteRequested]).len(),
                0,
                "{reason}"
            );
        }
        // The bounds themselves are within the rules.
        let longest = "w".repeat(300);
        let answered = request(
            &project,
            &[("https://a.com/", "x"), ("https://b.com/", &longest)],
        )
        .expect("one character and 300 are a why");
        assert_eq!(answers(&answered), ["asked", "asked"]);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn takes_one_to_ten_sites_a_call() {
        let project = a_project("sites-count");
        let none = refusal_of(project.call(
            "proc",
            Some("FRK-1"),
            "catervas_request_sites",
            json!({ "sites": [] }),
        ));
        assert!(none.starts_with("site_count_invalid: "), "{none}");
        let urls: Vec<String> = (0..11).map(|n| format!("https://s{n}.example/")).collect();
        let entries: Vec<(&str, &str)> = urls.iter().map(|url| (url.as_str(), "Why.")).collect();
        let eleven = refusal_of(request(&project, &entries));
        assert!(eleven.starts_with("site_count_invalid: "), "{eleven}");
        assert_eq!(project.events(&[EventKind::SiteRequested]).len(), 0);
        let ten = request(&project, &entries[..10]).expect("ten are taken");
        assert_eq!(ten["sites"].as_array().map(Vec::len), Some(10));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn caps_the_requests_waiting() {
        let project = a_project("sites-cap");
        // Nineteen wait in the project: ten on FRK-1 and nine on FRK-4.
        for number in 0..19 {
            let task = if number < 10 { "FRK-1" } else { "FRK-4" };
            project.record_by(
                Some("proc"),
                at(),
                task,
                "site.requested",
                &json!({
                    "host": format!("w{number}.example"),
                    "url": format!("https://w{number}.example/"),
                    "why": "Waiting."
                }),
            );
        }

        let answer = request(
            &project,
            &[
                ("https://new-1.example/", "One."),
                ("https://new-2.example/", "Two."),
                ("https://new-3.example/", "Three."),
            ],
        )
        .expect("one is taken");

        assert_eq!(answers(&answer), ["asked", "full", "full"]);
        assert_eq!(answer["next"], NEXT);
        assert_eq!(project.events(&[EventKind::SiteRequested]).len(), 20);
        // At the cap, a call that records nothing because of it is refused.
        let reason = refusal_of(request(&project, &[("https://new-4.example/", "Four.")]));
        assert!(reason.starts_with("too_many_site_requests: "), "{reason}");
        assert_eq!(project.events(&[EventKind::SiteRequested]).len(), 20);
        // A call that needs no place is not refused: a site waiting, or approved.
        let none_needed = request(
            &project,
            &[
                ("https://w3.example/", "Already waiting."),
                (&format!("https://{}/", a_catervas_host()), "On the list."),
            ],
        )
        .expect("nothing is asked, so nothing is full");
        assert_eq!(answers(&none_needed), ["waiting", "allowed"]);
        // Deciding one makes room.
        let seq = project.events(&[EventKind::SiteRequested])[0].envelope.seq;
        project.record(
            "FRK-1",
            "site.declined",
            &json!({ "request": seq, "host": "w0.example", "note": "" }),
        );
        let room = request(&project, &[("https://new-5.example/", "Five.")]).expect("a place");
        assert_eq!(answers(&room), ["asked"]);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_where_it_may_read() {
        let project = a_project("sites-read");
        let catervas = catervas_sites();
        // The owner turned off one of Catervas's, allowed one of their own, and the task asked for
        // one, was refused one, and a second task asked for another.
        project.record("", "site.removed", &json!({ "host": catervas[1].host }));
        project.record("", "site.approved", &json!({ "host": "owner.example" }));
        request(
            &project,
            &[
                ("https://wait.example/", "A maker."),
                ("https://no.example/", "Another."),
            ],
        )
        .expect("asked");
        let no = project.events(&[EventKind::SiteRequested])[1].envelope.seq;
        project.record(
            "FRK-1",
            "site.declined",
            &json!({ "request": no, "host": "no.example", "note": "Too many bad reviews." }),
        );
        project.record_by(
            Some("proc"),
            at(),
            "FRK-4",
            "site.requested",
            &json!({ "host": "else.example", "url": "https://else.example/", "why": "Elsewhere." }),
        );

        let read = project
            .call("proc", Some("FRK-1"), "catervas_read_sites", json!({}))
            .expect("read");

        let approved = read["approved"].as_array().expect("a list");
        let expected: Vec<Value> = catervas
            .iter()
            .filter(|site| site.host != catervas[1].host)
            .map(|site| {
                json!({ "host": site.host, "shop": site.shop, "category": site.category.to_string() })
            })
            .chain([json!({ "host": "owner.example" })])
            .collect();
        assert_eq!(approved, &expected);
        assert_eq!(read["waiting"], json!(["wait.example"]));
        assert_eq!(
            read["declined"],
            json!([{ "host": "no.example", "note": "Too many bad reviews." }])
        );
        // A chat has no task, so nothing waits or was refused for it.
        let mut chat = project.context("proc", None);
        chat.purpose = SessionPurpose::Chat;
        let read = run(&chat, "catervas_read_sites", json!({})).expect("read in a chat");
        assert_eq!(
            read["approved"].as_array().map(Vec::len),
            Some(expected.len())
        );
        assert_eq!(read["waiting"], json!([]));
        assert_eq!(read["declined"], json!([]));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn only_a_held_roles_own_implement_session_asks() {
        let project = a_project("sites-only");
        let input = json!({ "sites": [{ "url": "https://shop.example/", "why": "A maker." }] });
        let context = |who: &str, task: Option<&str>, purpose: SessionPurpose| {
            let mut context = project.context(who, task);
            context.purpose = purpose;
            context
        };
        for (who, context) in [
            (
                "the role's chat, with no task",
                context("proc", None, SessionPurpose::Chat),
            ),
            (
                "the role's chat about its task",
                context("proc", Some("FRK-1"), SessionPurpose::Chat),
            ),
            (
                "the role in a session about no task",
                context("proc", None, SessionPurpose::Implement),
            ),
            (
                "the role in a task that is not its own",
                context("proc", Some("FRK-3"), SessionPurpose::Implement),
            ),
            (
                "the role reviewing its task",
                context("proc", Some("FRK-1"), SessionPurpose::Verify),
            ),
            (
                "a Finance Specialist in its own task",
                context("fin", Some("FRK-2"), SessionPurpose::Implement),
            ),
            (
                "a Marketing Specialist",
                context("kai", Some("FRK-1"), SessionPurpose::Implement),
            ),
            (
                "a Developer in its own task",
                context("dev-a", Some("FRK-3"), SessionPurpose::Implement),
            ),
            (
                "the Product Manager",
                context("pm", Some("FRK-1"), SessionPurpose::Implement),
            ),
        ] {
            let reason = refusal_of(run(&context, "catervas_request_sites", input.clone()));
            assert!(
                reason.starts_with("site_request_refused: "),
                "{who}: {reason}"
            );
        }
        assert_eq!(project.events(&[EventKind::SiteRequested]).len(), 0);
        // Another role reads no list of sites either.
        for who in ["fin", "kai", "dev-a", "pm"] {
            let reason = refusal_of(project.call(who, None, "catervas_read_sites", json!({})));
            assert!(reason.starts_with("sites_refused: "), "{who}: {reason}");
        }
        assert_eq!(
            catervas_core::governor::sites::web_access(Role::ProcurementSpecialist),
            catervas_core::governor::sites::WebAccess::ApprovedSites
        );
    }
}
