//! The hand-over of posts to Buffer (`docs/SPEC.md` 6.5, ADR 0042): a rule with no model, which
//! every tick that names no task runs, right after the ends the plans' dates bring. A post the
//! owner's plan approved, or the owner allowed, goes to Buffer an hour before its time with
//! Farik's own `create_post`, made with the connection of the agent that wrote it. A post is
//! handed over once: a failure is recorded and never tried again, since Buffer may already have
//! the post.
//!
//! The hand-over runs between sessions, and a session runs to its end before the next tick, so a
//! post can be handed over late by one running session.

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use farik_core::marketing::PostDetails;
use farik_protocol::event::EventBody;
use farik_store::marketing::{PostState, SocialPost, social_posts};
use serde_json::{Map, Value, json};

use super::{OrchestratorDeps, OrchestratorError};
use crate::daemon::own_calls::{OwnCallError, call_as};
use crate::marketing::{append, claim_post, hold_plans};
use crate::pause::{key_refused, paused};
use crate::tools::ToolDeps;
use crate::tools::media::{MediaKind, media_answers};

/// How long before its time a post is handed to Buffer.
const HAND_OVER_BEFORE: Duration = Duration::hours(1);
/// A post this close to its time, or past it, is not handed over any more.
const TOO_LATE: Duration = Duration::minutes(5);
/// The most characters of Buffer's own words a failure keeps.
const WORDS_CAP: usize = 300;

/// Why a post was missed, outside the claim, which finds a post past its time itself.
#[derive(Clone, Copy)]
enum Missed {
    Paused,
    Undecided,
}

impl Missed {
    fn word(self) -> &'static str {
        match self {
            Self::Paused => "paused",
            Self::Undecided => "undecided",
        }
    }

    /// The state a post must be in for this to be what happened to it.
    fn from(self) -> PostState {
        match self {
            Self::Paused => PostState::Scheduled,
            Self::Undecided => PostState::Requested,
        }
    }
}

fn recorded(detail: impl std::fmt::Display) -> OrchestratorError {
    OrchestratorError::Refused {
        reason: format!("social_post_not_recorded: {detail}"),
    }
}

/// Whether the post's time is within five minutes or past.
pub(crate) fn is_too_late(post: &SocialPost, now: DateTime<Utc>) -> bool {
    post.at.with_timezone(&Utc) <= now + TOO_LATE
}

/// When Farik hands the post over: an hour before its time, or, for a post the owner allowed
/// later than that, when they allowed it.
pub(crate) fn hand_over_time(post: &SocialPost) -> DateTime<Utc> {
    let hour_before = post.at.with_timezone(&Utc) - HAND_OVER_BEFORE;
    post.decided_at
        .map_or(hour_before, |decided| decided.max(hour_before))
}

/// Hands over every post whose time has come, and records those that were missed. Only a pause by
/// the owner holds it: Farik's own pause for a refused key does not, since Buffer needs no model.
///
/// # Errors
///
/// What the log refused. A post Buffer would not take is not an error: it is recorded as failed.
pub(crate) async fn hand_over_posts(deps: &OrchestratorDeps) -> Result<(), OrchestratorError> {
    let tools = &deps.tools;
    let held = paused(&tools.log)? && !key_refused(&tools.log)?;
    let now = tools.clock.now();
    let mut due: Vec<(DateTime<Utc>, u64)> = Vec::new();
    for post in social_posts(&tools.log)? {
        match post.state {
            // An undecided request whose time is almost here will not go out.
            PostState::Requested if is_too_late(&post, now) => {
                miss(tools, post.post, Missed::Undecided)?;
            }
            PostState::Scheduled if held && is_too_late(&post, now) => {
                miss(tools, post.post, Missed::Paused)?;
            }
            PostState::Scheduled if !held && hand_over_time(&post) <= now => {
                due.push((hand_over_time(&post), post.post));
            }
            _ => {}
        }
    }
    due.sort_unstable();
    for (_, number) in due {
        hand_over(deps, number).await?;
    }
    Ok(())
}

/// Records that post `post` was missed for `why`, unless it is not in the state that makes it so.
fn miss(tools: &ToolDeps, post: u64, why: Missed) -> Result<(), OrchestratorError> {
    let _held = hold_plans();
    let current = social_posts(&tools.log)?
        .into_iter()
        .find(|found| found.post == post);
    if current.is_none_or(|found| found.state != why.from()) {
        return Ok(());
    }
    let body =
        serde_json::from_value(json!({ "post": post, "why": why.word() })).map_err(recorded)?;
    append(tools, None, EventBody::SocialPostMissed(body)).map_err(recorded)?;
    Ok(())
}

/// Hands post `number` to Buffer and records what came of it, when it is still to be handed over.
async fn hand_over(deps: &OrchestratorDeps, number: u64) -> Result<(), OrchestratorError> {
    let Some((post, claim)) = claim(&deps.tools, number)? else {
        return Ok(());
    };
    let outcome = deliver(deps, &post).await;
    let _held = hold_plans();
    drop(claim);
    let body = match outcome {
        Ok(buffer_post) => EventBody::SocialPostSent(
            serde_json::from_value(json!({ "post": number, "buffer_post": buffer_post }))
                .map_err(recorded)?,
        ),
        Err(reason) => EventBody::SocialPostFailed(
            serde_json::from_value(json!({ "post": number, "reason": reason }))
                .map_err(recorded)?,
        ),
    };
    append(&deps.tools, None, body).map_err(recorded)?;
    Ok(())
}

/// Claims post `number` for the hand-over, when it is still scheduled, and misses it when its time
/// is too near. The claim and the check are one step under the plans' lock, so that a Stop of the
/// post is either before it, and the post is not handed over, or after it, and is refused.
fn claim(
    tools: &ToolDeps,
    number: u64,
) -> Result<Option<(SocialPost, crate::marketing::Claimed)>, OrchestratorError> {
    let held = hold_plans();
    let Some(post) = social_posts(&tools.log)?
        .into_iter()
        .find(|found| found.post == number && found.state == PostState::Scheduled)
    else {
        return Ok(None);
    };
    if is_too_late(&post, tools.clock.now()) {
        let body = serde_json::from_value(json!({ "post": number, "why": "not_running" }))
            .map_err(recorded)?;
        append(tools, None, EventBody::SocialPostMissed(body)).map_err(recorded)?;
        return Ok(None);
    }
    let claim = claim_post(&held, tools, number);
    Ok(Some((post, claim)))
}

/// What Farik asks Buffer to create for `post`.
fn create_input(post: &SocialPost) -> Map<String, Value> {
    let mut input = Map::new();
    input.insert("channelId".to_string(), json!(post.buffer_channel));
    input.insert("text".to_string(), json!(post.text));
    input.insert("schedulingType".to_string(), json!("automatic"));
    input.insert("mode".to_string(), json!("customScheduled"));
    input.insert(
        "dueAt".to_string(),
        json!(post.at.to_rfc3339_opts(SecondsFormat::AutoSi, true)),
    );
    if !post.media.is_empty() {
        input.insert(
            "assets".to_string(),
            json!(
                post.media
                    .iter()
                    .map(|media| if media.video {
                        json!({ "video": { "url": media.url } })
                    } else {
                        json!({ "image": { "url": media.url } })
                    })
                    .collect::<Vec<_>>()
            ),
        );
    }
    match &post.details {
        Some(PostDetails::Youtube { title, category_id }) => {
            input.insert(
                "metadata".to_string(),
                json!({ "youtube": { "title": title, "categoryId": category_id } }),
            );
        }
        Some(PostDetails::Pinterest { board }) => {
            input.insert(
                "metadata".to_string(),
                json!({ "pinterest": { "boardServiceId": board } }),
            );
        }
        None => {}
    }
    input
}

/// Buffer's id for the post it answered with: the answer's `id`, else its `post.id`.
fn buffer_id(answer: &Value) -> Option<String> {
    let id = answer.get("id").or_else(|| answer.pointer("/post/id"))?;
    let text = match id {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => return None,
    };
    (!text.is_empty()).then(|| text.chars().take(200).collect())
}

/// Hands `post` to Buffer: Buffer's id for it, or the sentence that says why not.
async fn deliver(deps: &OrchestratorDeps, post: &SocialPost) -> Result<String, String> {
    for media in &post.media {
        let kind = if media.video {
            MediaKind::Video
        } else {
            MediaKind::Image
        };
        if media_answers(&media.url, kind).await.is_err() {
            return Err("Its picture or clip no longer opens.".to_string());
        }
    }
    let when = post.at.format("%a %-d %b %H:%M");
    match call_as(
        &deps.daemon,
        &post.agent_id,
        "buffer",
        "create_post",
        create_input(post),
    )
    .await
    {
        Ok(answer) => buffer_id(&answer).ok_or_else(|| {
            format!("Buffer answered without the post's id: look in Buffer's queue before {when}.")
        }),
        Err(OwnCallError::Tool(words)) => Err(format!(
            "Buffer did not take it: \u{201c}{}\u{201d}",
            words.chars().take(WORDS_CAP).collect::<String>()
        )),
        Err(OwnCallError::Timeout) => Err(format!(
            "Buffer did not answer in time and may have the post: look in Buffer's queue before {when}."
        )),
        Err(OwnCallError::NotConnected(agent)) => Err(format!(
            "{agent}'s Buffer connection is not there; connect Buffer again."
        )),
        Err(OwnCallError::SignInAgain) => Err(format!(
            "{}'s Buffer connection is not there; connect Buffer again.",
            post.agent_id
        )),
        Err(OwnCallError::Failed(_) | OwnCallError::NotListed) => {
            Err("Farik could not reach Buffer.".to_string())
        }
    }
}

/// Kai's project with Buffer at an OAuth fixture, a server of pictures, and an orchestrator whose
/// clock the test moves, for the tests of the hand-over and of Stop.
#[cfg(test)]
pub(crate) mod fixtures {
    use std::sync::Arc;

    use chrono::{DateTime, Duration, Utc};
    use farik_protocol::clock::MovableClock;
    use farik_protocol::event::EventKind;
    use serde_json::{Map, Value, json};

    use crate::daemon::own_calls::fixtures::{buffer_kit, connect_buffer, keep_a_sign_in};
    use crate::oauth_fixture::{Fixture, ToolAnswer};
    use crate::orchestrator::fixtures::Harness;
    use crate::orchestrator::rules;
    use crate::tools::fixtures::{at, with_the_marketing_specialist};
    use crate::tools::media::fixtures::serving;

    /// Kai's project with Buffer at an OAuth fixture that answers `create_post` with the post
    /// `buf-1`, a server of pictures, and an orchestrator whose clock the test moves.
    pub(crate) struct Handing {
        pub(crate) harness: Harness,
        pub(crate) fixture: Fixture,
        pub(crate) pictures: String,
        pub(crate) clock: Arc<MovableClock>,
        pub(crate) orchestrator: crate::orchestrator::Orchestrator,
    }

    impl Handing {
        pub(crate) async fn new(name: &str) -> Handing {
            let fixture = Fixture::start().await;
            fixture.set(|flags| {
                flags.tool_answers.insert(
                    "create_post".to_string(),
                    ToolAnswer::Json(json!({ "id": "buf-1" })),
                );
                flags.tool_answers.insert(
                    "delete_post".to_string(),
                    ToolAnswer::Json(json!({ "id": "buf-1" })),
                );
            });
            let harness = Harness::new(name, with_the_marketing_specialist);
            let kit = buffer_kit(&fixture.mcp_url, false);
            let (server, kept_at, store) = connect_buffer(&harness.project, &harness.daemon, &kit);
            keep_a_sign_in(
                &store,
                (&server, &kept_at),
                &fixture,
                chrono::Duration::hours(1),
            );
            let (pictures, _) = serving().await;
            let clock = Arc::new(MovableClock::new(at()));
            let orchestrator =
                harness.orchestrator_on(harness.recorded(Vec::new()), Arc::clone(&clock));
            Handing {
                harness,
                fixture,
                pictures,
                clock,
                orchestrator,
            }
        }

        pub(crate) fn now(&self, now: DateTime<Utc>) {
            self.clock.set(now);
        }

        /// A scheduled post of Kai's in plan MP-1, slot `post-1`, going out at `going_out`
        /// (RFC 3339), with one picture the server of pictures answers.
        pub(crate) fn a_post(&self, going_out: &str) -> Value {
            json!({
                "channel": "instagram",
                "buffer_channel": "chan-1",
                "text": "We open on Wednesday.",
                "media": [{ "url": format!("{}/ok.png", self.pictures), "kind": "image" }],
                "at": going_out,
                "approved_by": "plan",
                "plan": "MP-1",
                "slot": "post-1",
            })
        }

        /// Records Kai's `kind` event with `body`, and answers its number.
        pub(crate) fn records(&self, kind: &str, body: &Value) -> u64 {
            self.harness
                .project
                .record_by(Some("kai"), at(), "FRK-1", kind, body)
                .envelope
                .seq
        }

        /// Kai schedules a post going out at `going_out`.
        pub(crate) fn schedules(&self, going_out: &str) -> u64 {
            self.records("social_post.scheduled", &self.a_post(going_out))
        }

        /// Records what Farik or the owner did to a post: no agent and no session.
        pub(crate) fn happens(&self, kind: &str, body: &Value) {
            self.harness.project.record("", kind, body);
        }

        /// The owner allows a request: the event names its task, and no agent and no session.
        pub(crate) fn allows(&self, request: u64, body: &Value) {
            let mut allowed = body.clone();
            allowed["post"] = json!(request);
            allowed["approved_by"] = json!("owner");
            self.harness
                .project
                .record("FRK-1", "social_post.scheduled", &allowed);
        }

        /// One tick of the rules that start no session.
        pub(crate) async fn hands_over(&self) {
            rules::hand_over_posts(&self.orchestrator.deps)
                .await
                .expect("the hand-over runs");
        }

        pub(crate) fn events(&self, kind: EventKind) -> Vec<farik_protocol::event::FarikEvent> {
            self.harness.project.events(&[kind])
        }

        pub(crate) fn created(&self) -> Vec<Map<String, Value>> {
            self.fixture.calls("create_post")
        }

        /// The one `social_post.` event of `kind`, as JSON.
        pub(crate) fn the_event(&self, kind: EventKind) -> Value {
            let events = self.events(kind);
            assert_eq!(events.len(), 1, "{kind:?}: {events:?}");
            farik_protocol::event::event_to_value(&events[0])["body"].clone()
        }
    }

    pub(crate) fn after(minutes: i64) -> String {
        (at() + Duration::minutes(minutes)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use farik_protocol::event::EventKind;
    use serde_json::json;

    use super::fixtures::{Handing, after};
    use crate::oauth_fixture::ToolAnswer;
    use crate::tools::fixtures::at;
    use crate::tools::media::LoopbackAllowed;

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn hands_a_post_to_buffer_an_hour_before() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("hand-over-hour").await;
        // An hour and a minute before: not yet.
        let post = handing.schedules(&after(61));
        handing.hands_over().await;
        assert!(
            handing.created().is_empty(),
            "61 minutes ahead is too early"
        );
        assert!(handing.events(EventKind::SocialPostSent).is_empty());

        // Fifty-nine minutes before: handed over, once, with the input of the design.
        handing.clock.set(at());
        let mut sooner = handing.a_post(&after(59));
        sooner["slot"] = json!("post-2");
        let second = handing.records("social_post.scheduled", &sooner);
        handing.hands_over().await;

        assert_eq!(
            handing.created(),
            [json!({
                "channelId": "chan-1",
                "text": "We open on Wednesday.",
                "schedulingType": "automatic",
                "mode": "customScheduled",
                "dueAt": after(59),
                "assets": [{ "image": { "url": format!("{}/ok.png", handing.pictures) } }],
            })
            .as_object()
            .cloned()
            .expect("an object")],
            "one create_post, with no metadata for Instagram"
        );
        assert_eq!(
            handing.the_event(EventKind::SocialPostSent),
            json!({ "post": second, "buffer_post": "buf-1" })
        );

        // Nothing is handed over twice.
        handing.hands_over().await;
        assert_eq!(handing.created().len(), 1);
        // The first goes when its hour comes.
        handing.now(at() + Duration::minutes(2));
        handing.hands_over().await;
        assert_eq!(handing.created().len(), 2);
        assert_eq!(handing.events(EventKind::SocialPostSent).len(), 2);
        let _ = post;
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn adds_the_metadata_youtube_and_pinterest_need() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("hand-over-metadata").await;
        let mut youtube = handing.a_post(&after(59));
        youtube["channel"] = json!("youtube");
        youtube["slot"] = json!("post-yt");
        youtube["media"] =
            json!([{ "url": format!("{}/clip.mp4", handing.pictures), "kind": "video" }]);
        youtube["details"] = json!({ "title": "Opening day", "category_id": "22" });
        handing.records("social_post.scheduled", &youtube);
        let mut pinterest = handing.a_post(&after(58));
        pinterest["channel"] = json!("pinterest");
        pinterest["slot"] = json!("post-pin");
        pinterest["details"] = json!({ "board": "board-2" });
        handing.records("social_post.scheduled", &pinterest);

        handing.hands_over().await;

        // The oldest hand-over goes first: the Pinterest pin, a minute before the clip.
        let created = handing.created();
        assert_eq!(created.len(), 2);
        assert_eq!(
            created[0]["metadata"],
            json!({ "pinterest": { "boardServiceId": "board-2" } })
        );
        assert_eq!(
            created[1]["metadata"],
            json!({ "youtube": { "title": "Opening day", "categoryId": "22" } })
        );
        assert_eq!(
            created[1]["assets"],
            json!([{ "video": { "url": format!("{}/clip.mp4", handing.pictures) } }])
        );
        let mut plain = handing.a_post(&after(57));
        plain["slot"] = json!("post-2");
        handing.records("social_post.scheduled", &plain);
        handing.hands_over().await;
        assert!(
            handing.created()[2].get("metadata").is_none(),
            "no metadata for the other nine"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn sends_a_late_hand_over_and_misses_a_past_one() {
        let _here = LoopbackAllowed::new();
        // Farik was not ticking until ten minutes before: the post is still sent.
        let late = Handing::new("hand-over-late").await;
        let post = late.schedules(&after(10));
        late.hands_over().await;
        assert_eq!(late.created().len(), 1);
        assert_eq!(
            late.the_event(EventKind::SocialPostSent),
            json!({ "post": post, "buffer_post": "buf-1" })
        );

        // Until four minutes before: too late, and Buffer is not asked.
        let missed = Handing::new("hand-over-missed").await;
        let post = missed.schedules(&after(4));
        missed.hands_over().await;
        assert!(missed.created().is_empty());
        assert_eq!(
            missed.the_event(EventKind::SocialPostMissed),
            json!({ "post": post, "why": "not_running" })
        );
        // Five minutes exactly is within the five.
        let edge = Handing::new("hand-over-edge").await;
        edge.schedules(&after(5));
        edge.hands_over().await;
        assert!(edge.created().is_empty());
        assert_eq!(edge.events(EventKind::SocialPostMissed).len(), 1);
        // A post that was missed is not looked at again.
        missed.hands_over().await;
        assert_eq!(missed.events(EventKind::SocialPostMissed).len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one failure after another, each with its sentence"
    )]
    async fn a_failure_is_recorded_and_not_retried() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("hand-over-failures").await;
        let failed = |post: u64| -> String {
            handing
                .events(EventKind::SocialPostFailed)
                .iter()
                .map(farik_protocol::event::event_to_value)
                .find(|event| event["body"]["post"] == post)
                .expect("the post failed")["body"]["reason"]
                .as_str()
                .expect("a reason")
                .to_string()
        };
        let answers = |answer: ToolAnswer| {
            handing.fixture.set(|flags| {
                flags.tool_answers.insert("create_post".to_string(), answer);
            });
        };
        let post = |slot: &str, minutes: i64| {
            let mut body = handing.a_post(&after(minutes));
            body["slot"] = json!(slot);
            handing.records("social_post.scheduled", &body)
        };

        // Buffer says no: its words, cut at 300 characters, in curly quotes.
        answers(ToolAnswer::Error(format!(
            "Channel is disconnected. {}",
            "x".repeat(400)
        )));
        let refused = post("post-1", 59);
        handing.hands_over().await;
        let said = failed(refused);
        let kept = format!("Channel is disconnected. {}", "x".repeat(400))
            .chars()
            .take(300)
            .collect::<String>();
        assert_eq!(
            said,
            format!("Buffer did not take it: \u{201c}{kept}\u{201d}")
        );

        // Over its rate limit, Buffer's words are kept the same way.
        answers(ToolAnswer::Error(
            "Rate limit exceeded (429). Retry after 60 seconds.".to_string(),
        ));
        let limited = post("post-2", 58);
        handing.hands_over().await;
        assert_eq!(
            failed(limited),
            "Buffer did not take it: \u{201c}Rate limit exceeded (429). Retry after 60 seconds.\u{201d}"
        );

        // An answer with no id for the post.
        answers(ToolAnswer::Json(json!({ "ok": true })));
        let nameless = post("post-x", 57);
        handing.hands_over().await;
        let when = (at() + Duration::minutes(57))
            .format("%a %-d %b %H:%M")
            .to_string();
        assert_eq!(
            failed(nameless),
            format!("Buffer answered without the post's id: look in Buffer's queue before {when}.")
        );

        // A post's id may be under `post`.
        answers(ToolAnswer::Json(json!({ "post": { "id": "buf-77" } })));
        let nested = post("post-yt", 56);
        handing.hands_over().await;
        assert_eq!(
            handing.the_event(EventKind::SocialPostSent),
            json!({ "post": nested, "buffer_post": "buf-77" })
        );

        // A picture that no longer opens.
        let calls = handing.created().len();
        let mut gone = handing.a_post(&after(55));
        gone["slot"] = json!("post-pin");
        gone["media"] =
            json!([{ "url": format!("{}/gone.png", handing.pictures), "kind": "image" }]);
        let unopened = handing.records("social_post.scheduled", &gone);
        handing.hands_over().await;
        assert_eq!(failed(unopened), "Its picture or clip no longer opens.");
        assert_eq!(
            handing.created().len(),
            calls,
            "Buffer is not asked for a post whose picture is gone"
        );

        // Nothing that failed is tried again, however many ticks come.
        let before = handing.created().len();
        for _ in 0..3 {
            handing.hands_over().await;
        }
        assert_eq!(handing.created().len(), before);
        assert_eq!(handing.events(EventKind::SocialPostFailed).len(), 4);
    }

    #[tokio::test(start_paused = true)]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_timeout_may_have_posted_and_is_not_retried() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("hand-over-timeout").await;
        let mut body = handing.a_post("2026-09-22T07:59:00-05:00");
        body["slot"] = json!("post-1");
        let post = handing.records("social_post.scheduled", &body);
        handing.fixture.hold("tool:create_post");

        // Paused time does not move while a blocking task runs: this one holds it until the call
        // has reached Buffer, so the thirty seconds are Buffer's.
        let ((), reached) = tokio::join!(handing.hands_over(), async {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while handing.created().is_empty() {
                if std::time::Instant::now() > deadline {
                    return false;
                }
                tokio::task::spawn_blocking(|| {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                });
                tokio::task::yield_now().await;
            }
            true
        });

        assert!(reached, "the call never reached Buffer");
        assert_eq!(
            handing.the_event(EventKind::SocialPostFailed),
            json!({
                "post": post,
                "reason": "Buffer did not answer in time and may have the post: look in Buffer's queue before Tue 22 Sep 07:59."
            })
        );
        handing.fixture.release("tool:create_post");
        handing.hands_over().await;
        assert_eq!(handing.created().len(), 1, "a timeout is never retried");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn says_when_the_connection_is_not_there() {
        let _here = LoopbackAllowed::new();
        // Not kept at all.
        let handing = Handing::new("hand-over-no-connection").await;
        let kai = handing
            .harness
            .daemon
            .secret_at(handing.harness.project.deps.files.root(), "kai", "buffer")
            .expect("an address");
        handing
            .harness
            .daemon
            .connector_secrets()
            .delete(&kai)
            .expect("forgotten");
        handing.harness.daemon.forget_kept(&kai);
        let post = handing.schedules(&after(59));
        handing.hands_over().await;
        assert_eq!(
            handing.the_event(EventKind::SocialPostFailed),
            json!({ "post": post, "reason": "kai's Buffer connection is not there; connect Buffer again." })
        );
        assert!(handing.created().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn only_the_owner_s_pause_holds_the_hand_over() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("hand-over-pause").await;
        let due = handing.schedules(&after(30));
        let mut soon = handing.a_post(&after(4));
        soon["slot"] = json!("post-2");
        let near = handing.records("social_post.scheduled", &soon);

        // The owner paused the team: nothing goes, and the post that is about to go out is missed.
        handing.happens("team.paused", &json!({ "by": "human" }));
        handing.hands_over().await;
        assert!(handing.created().is_empty());
        assert_eq!(
            handing.the_event(EventKind::SocialPostMissed),
            json!({ "post": near, "why": "paused" })
        );

        // Farik's own pause, for a refused key, does not stop Buffer, which needs no model.
        handing.happens("team.resumed", &json!({ "by": "human" }));
        handing.happens(
            "team.paused",
            &json!({ "by": "farik", "reason": "credential_refused", "detail": "refused" }),
        );
        handing.hands_over().await;
        assert_eq!(
            handing.the_event(EventKind::SocialPostSent),
            json!({ "post": due, "buffer_post": "buf-1" })
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn an_undecided_request_is_missed() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("hand-over-undecided").await;
        let request = |minutes: i64| {
            let mut body = handing.a_post(&after(minutes));
            let object = body.as_object_mut().expect("an object");
            for key in ["approved_by", "plan", "slot"] {
                object.remove(key);
            }
            handing.records("social_post.requested", &body)
        };
        let waiting = request(30);
        let late = request(4);

        handing.hands_over().await;

        assert_eq!(
            handing.the_event(EventKind::SocialPostMissed),
            json!({ "post": late, "why": "undecided" })
        );
        assert!(
            handing.created().is_empty(),
            "a request is never handed over undecided"
        );
        let _ = waiting;
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn hands_over_a_post_the_owner_allowed_at_once_when_its_hour_has_passed() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("hand-over-allowed").await;
        let mut body = handing.a_post(&after(30));
        let object = body.as_object_mut().expect("an object");
        for key in ["approved_by", "plan", "slot"] {
            object.remove(key);
        }
        let request = handing.records("social_post.requested", &body);
        // The owner allows it ten minutes later, which is after the hour before: it goes then.
        handing.now(at() + Duration::minutes(10));
        handing.allows(request, &body);
        handing.hands_over().await;
        assert_eq!(handing.created().len(), 1);
        // One allowed while its hour has not come is held until then.
        let waits = Handing::new("hand-over-allowed-early").await;
        let mut far = waits.a_post(&after(180));
        let object = far.as_object_mut().expect("an object");
        for key in ["approved_by", "plan", "slot"] {
            object.remove(key);
        }
        let request = waits.records("social_post.requested", &far);
        waits.allows(request, &far);
        waits.hands_over().await;
        assert!(waits.created().is_empty());
        waits.now(at() + Duration::minutes(130));
        waits.hands_over().await;
        assert_eq!(waits.created().len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_last_day_evening_post_outlives_the_plan_s_expiry() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("hand-over-expiry").await;
        handing
            .harness
            .project
            .plan_proposed("FRK-1", "MP-1", "2026-10-20", "2026-11-03");
        handing.harness.project.plan_approved("FRK-1", "MP-1", "");
        // The plan's last day, in the evening in New York, which is already the next day in UTC.
        let post = handing.schedules("2026-11-03T20:00:00-05:00");
        handing.now("2026-11-04T00:00:00Z".parse().expect("a time"));

        handing.orchestrator.tick().await.expect("the tick runs");

        let ended = handing.the_event(EventKind::MarketingPlanEnded);
        assert_eq!(ended["why"], "expired");
        assert_eq!(
            handing.the_event(EventKind::SocialPostSent),
            json!({ "post": post, "buffer_post": "buf-1" }),
            "the plan's expiry does not stop a post checked against its slot day"
        );
        assert!(handing.events(EventKind::SocialPostStopped).is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(clippy::too_many_lines, reason = "each way a plan ends, side by side")]
    async fn ending_the_plan_stops_only_what_it_should() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("hand-over-plan-end").await;
        // MP-1, from 2026-09-22 to 2026-11-30, with a slot on 5 November and one on 12 November.
        let project = &handing.harness.project;
        let mut plan =
            farik_protocol::event::fixtures::a_body_wire(EventKind::MarketingPlanProposed);
        plan["starts_on"] = json!("2026-09-22");
        plan["ends_on"] = json!("2026-11-30");
        plan["campaigns"] = json!([]);
        plan["budget"] = json!({ "total": "2000", "google_ads": "0" });
        plan["posts"] = json!([
            { "key": "post-1", "channel": "instagram", "on": "2026-11-05", "topic": "Early" },
            { "key": "post-2", "channel": "instagram", "on": "2026-11-12", "topic": "Late" },
            { "key": "post-x", "channel": "instagram", "on": "2026-11-20", "topic": "Later" },
            { "key": "post-yt", "channel": "instagram", "on": "2026-11-21", "topic": "With Buffer" },
        ]);
        project.record_by(Some("kai"), at(), "FRK-1", "marketing_plan.proposed", &plan);
        project.plan_approved("FRK-1", "MP-1", "");
        let post_for = |slot: &str, day: &str| {
            let mut body = handing.a_post(&format!("{day}T15:00:00Z"));
            body["slot"] = json!(slot);
            handing.records("social_post.scheduled", &body)
        };
        let early = post_for("post-1", "2026-11-05");
        let late = post_for("post-2", "2026-11-12");
        let handed = post_for("post-x", "2026-11-20");
        let with_buffer = post_for("post-yt", "2026-11-21");
        handing.happens(
            "social_post.sent",
            &json!({ "post": with_buffer, "buffer_post": "buf-9" }),
        );
        // One that Farik is handing to Buffer this moment is left alone.
        let claimed = crate::marketing::claim_post(
            &crate::marketing::hold_plans(),
            &handing.harness.project.deps,
            handed,
        );

        // The owner ends the plan: every post not yet with Buffer stops, but the one being handed
        // over and the one Buffer has.
        crate::marketing::end_plan(&handing.harness.project.deps, "MP-1", None).expect("ended");
        let stopped: Vec<(u64, String)> = handing
            .events(EventKind::SocialPostStopped)
            .iter()
            .map(farik_protocol::event::event_to_value)
            .map(|event| {
                (
                    event["body"]["post"].as_u64().expect("a post"),
                    event["body"]["by"].as_str().expect("a word").to_string(),
                )
            })
            .collect();
        assert_eq!(
            stopped,
            [
                (early, "plan_ended".to_string()),
                (late, "plan_ended".to_string())
            ]
        );
        drop(claimed);

        // A plan replaced by a newer one from 10 November stops the posts of its slots from that
        // day and keeps the ones before it; a plan that merely expires stops none.
        let other = Handing::new("hand-over-plan-replaced").await;
        let project = &other.harness.project;
        project.record_by(Some("kai"), at(), "FRK-1", "marketing_plan.proposed", &plan);
        project.plan_approved("FRK-1", "MP-1", "");
        let mut newer = plan.clone();
        newer["plan"] = json!("MP-2");
        newer["starts_on"] = json!("2026-11-10");
        newer["ends_on"] = json!("2026-12-31");
        newer["posts"] = json!([]);
        project.record_by(
            Some("kai"),
            at(),
            "FRK-1",
            "marketing_plan.proposed",
            &newer,
        );
        project.plan_approved("FRK-1", "MP-2", "");
        let early = {
            let mut body = other.a_post("2026-11-05T15:00:00Z");
            body["slot"] = json!("post-1");
            other.records("social_post.scheduled", &body)
        };
        let late = {
            let mut body = other.a_post("2026-11-12T15:00:00Z");
            body["slot"] = json!("post-2");
            other.records("social_post.scheduled", &body)
        };
        other.now("2026-11-10T08:00:00Z".parse().expect("a time"));
        other.orchestrator.tick().await.expect("the tick runs");
        let ended = other.the_event(EventKind::MarketingPlanEnded);
        assert_eq!(ended["why"], "replaced");
        let stopped: Vec<u64> = other
            .events(EventKind::SocialPostStopped)
            .iter()
            .map(|event| {
                farik_protocol::event::event_to_value(event)["body"]["post"]
                    .as_u64()
                    .expect("a post")
            })
            .collect();
        assert_eq!(
            stopped,
            [late],
            "the slot of 12 November goes, the one of 5 November stays"
        );
        assert!(!stopped.contains(&early));

        let expiring = Handing::new("hand-over-plan-expired").await;
        let project = &expiring.harness.project;
        project.record_by(Some("kai"), at(), "FRK-1", "marketing_plan.proposed", &plan);
        project.plan_approved("FRK-1", "MP-1", "");
        let waiting = {
            let mut body = expiring.a_post("2026-11-30T23:00:00-05:00");
            body["slot"] = json!("post-2");
            expiring.records("social_post.scheduled", &body)
        };
        expiring.now("2026-12-01T01:00:00Z".parse().expect("a time"));
        expiring.orchestrator.tick().await.expect("the tick runs");
        assert_eq!(
            expiring.the_event(EventKind::MarketingPlanEnded)["why"],
            "expired"
        );
        assert!(expiring.events(EventKind::SocialPostStopped).is_empty());
        let _ = waiting;
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn hands_over_the_oldest_hand_over_first() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("hand-over-order").await;
        // A post in the plan, due at 11:30; and one the owner allowed at 11:45, whose own hour
        // before (11:20) is earlier, but which could not go before the owner's yes.
        let mut plan = handing.a_post(&after(30));
        plan["text"] = json!("In the plan");
        handing.records("social_post.scheduled", &plan);
        let mut asked = handing.a_post(&after(20));
        asked["text"] = json!("Allowed later");
        let object = asked.as_object_mut().expect("an object");
        for key in ["approved_by", "plan", "slot"] {
            object.remove(key);
        }
        let request = handing.records("social_post.requested", &asked);
        asked["post"] = json!(request);
        asked["approved_by"] = json!("owner");
        handing.harness.project.record_at(
            at() - Duration::minutes(15),
            "FRK-1",
            "social_post.scheduled",
            &asked,
        );

        handing.hands_over().await;

        let texts: Vec<String> = handing
            .created()
            .iter()
            .map(|input| input["text"].as_str().expect("a text").to_string())
            .collect();
        assert_eq!(texts, ["In the plan", "Allowed later"]);
    }
}
