//! `catervas_chat_reply` (ADR 0026): an agent's one answer in its one-to-one chat, the one thing a
//! chat session writes.

use catervas_core::team::AgentStatus;
use catervas_protocol::event::{EventBody, EventKind};
use catervas_store::EventQuery;
use catervas_store::requests::{RequestError, TOO_SHORT, file_given_task};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolError, failed};
use crate::chat::{ChatError, Heard, NewChatMessage, ProposedRequest, given_request, post_chat};
use crate::session::SessionPurpose;

/// `catervas_chat_reply`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChatReplyInput {
    /// Your answer: 1 to 4,000 characters; line breaks are kept.
    pub text: String,
    /// The request you propose, when work is needed; the user sends it, or not.
    pub request: Option<ProposedRequest>,
    /// During a new project's interview, your summary of what the owner told you.
    pub heard: Option<Heard>,
}

fn refused(detail: impl Into<String>) -> ToolError {
    Refusal::ChatReplyRefused {
        detail: detail.into(),
    }
    .into()
}

/// Held around the one-reply check and the post, so two replies of one session at once record one.
// ponytail: one lock for every chat's reply; per-session locks if replies ever queue on it.
static REPLYING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Records the agent's reply in its chat, answering the user's message the session was started
/// for, and tells the agent its turn is over. Refused outside a `chat` session and after the
/// session's one reply; a reply refused for its limits does not use that one up.
pub(super) fn chat_reply(call: &Call<'_>, input: ChatReplyInput) -> Result<Value, ToolError> {
    let context = call.context;
    if context.purpose != SessionPurpose::Chat {
        return Err(refused(
            "catervas_chat_reply answers the user in a chat session, and this is not one",
        ));
    }
    let deps = call.deps();
    let session = Some(context.session_id.as_str());
    let _replying = REPLYING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let replied = deps
        .log
        .read(&EventQuery {
            agent_id: Some(call.agent_id().to_string()),
            kinds: vec![EventKind::ChatMessagePosted],
            after_seq: context.in_reply_to,
            ..EventQuery::default()
        })
        .map_err(failed)?
        .iter()
        .any(|event| event.envelope.ids.session_id.as_deref() == session);
    if replied {
        return Err(refused(
            "a chat session sends its one reply, and this one has",
        ));
    }
    let seq = post_chat(
        &deps.log,
        deps.clock.as_ref(),
        &deps.ids,
        NewChatMessage {
            chat: call.agent_id().to_string(),
            author: call.agent_id().to_string(),
            text: input.text,
            in_reply_to: context.in_reply_to,
            request: input.request,
            heard: input.heard,
            session_id: Some(context.session_id.clone()),
            as_task: false,
            attachments: Vec::new(),
        },
    )
    .map_err(|error| match error {
        ChatError::Refused { reason } => refused(reason),
        ChatError::Store(error) => failed(error),
    })?;
    Ok(json!({ "seq": seq, "said": "Sent. Your turn is over." }))
}

/// `catervas_take_task`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TakeTaskInput {
    /// The task's title in your words: one line, 3 to 80 characters. Its intent is the owner's
    /// message, never your words.
    text: String,
}

fn take_refused(kind: &str, words: &str) -> ToolError {
    ToolError::Refused {
        reason: format!("{kind}: {words}"),
    }
}

/// Files the owner's message the chat session answers as a task given to the session's own agent
/// (ADR 0063), as "Send as a task" does, but created by the agent. The intent is always that
/// message's text, read here from the log; `text` is only the title. Refused, with nothing
/// recorded, outside the agent's own chat session, for a paused agent, for a title that is not
/// one line of 3 to 80 characters, for a message under 20 characters, and for a message already
/// made a task.
pub(super) fn take_task(call: &Call<'_>, input: &TakeTaskInput) -> Result<Value, ToolError> {
    let context = call.context;
    let (SessionPurpose::Chat, Some(seq)) = (context.purpose, context.in_reply_to) else {
        return Err(take_refused(
            "take_task_refused",
            "only your chat session takes a task, for the message it answers",
        ));
    };
    if call.agent.status != AgentStatus::Active {
        return Err(take_refused(
            "agent_not_active",
            "you are paused, so you cannot take work; answer instead",
        ));
    }
    let title = input.text.trim();
    if !(3..=80).contains(&title.chars().count()) || title.contains(['\n', '\r']) {
        return Err(take_refused(
            "task_title",
            "a title is one line of 3 to 80 characters",
        ));
    }
    let deps = call.deps();
    let message = deps
        .log
        .read(&EventQuery {
            after_seq: Some(seq.saturating_sub(1)),
            limit: Some(1),
            ..EventQuery::default()
        })
        .map_err(failed)?
        .into_iter()
        .find(|event| event.envelope.seq == seq)
        .and_then(|event| match event.body {
            EventBody::ChatMessagePosted(body)
                if body.author == "human" && body.chat == call.agent_id() =>
            {
                Some(body.text)
            }
            _ => None,
        })
        .ok_or_else(|| {
            take_refused(
                "take_task_refused",
                "only your chat session takes a task, for the message it answers",
            )
        })?;
    let mut wire =
        given_request(&call.team, &call.agent, &message, &call.team.rules()).map_err(|reason| {
            if reason == TOO_SHORT {
                take_refused(
                    "task_too_short",
                    "the owner's message is under 20 characters, so answer it and ask what they need",
                )
            } else {
                take_refused("request_refused", &reason)
            }
        })?;
    wire["title"] = json!(title);
    let filed = file_given_task(
        &deps.files,
        &deps.log,
        wire,
        call.agent_id(),
        call.agent_id(),
        seq,
        deps.clock.now(),
        &call.ids(None),
        &[],
    )
    .map_err(|error| match error {
        RequestError::Refused { reason } => match reason.strip_prefix("was already sent, as ") {
            Some(task) => take_refused("already_a_task", task),
            None => take_refused("request_refused", &reason),
        },
        other => failed(other),
    })?;
    deps.projections.catch_up().map_err(failed)?;
    let task = filed.id.as_str();
    Ok(json!({
        "task_id": task,
        "next": format!(
            "Reply now in one line naming {task}: you have taken it and start once your current work allows."
        ),
    }))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use catervas_protocol::event::{EventBody, EventKind};
    use serde_json::json;

    use crate::session::SessionPurpose;
    use crate::tools::ToolError;
    use crate::tools::fixtures::{TestProject, a_team_of_three, run};

    fn refused_as_chat_reply(refused: &ToolError) -> bool {
        matches!(refused, ToolError::Refused { reason } if reason.starts_with("chat_reply_refused: "))
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_the_reply() {
        let project = TestProject::new("chat-reply", &a_team_of_three(|_| {}));
        let asked = crate::chat::post_chat(
            &project.deps.log,
            project.deps.clock.as_ref(),
            &project.deps.ids,
            crate::chat::NewChatMessage {
                heard: None,
                chat: "pm".to_string(),
                author: "human".to_string(),
                text: "Could customers also pay with Apple Pay?".to_string(),
                in_reply_to: None,
                request: None,
                session_id: None,
                as_task: false,
                attachments: Vec::new(),
            },
        )
        .expect("the question is recorded");
        let mut context = project.context("pm", None);
        context.purpose = SessionPurpose::Chat;
        context.in_reply_to = Some(asked);
        let reply = |input| run(&context, "catervas_chat_reply", input);

        // A request outside its limits is refused, and does not use up the one reply.
        let too_short = reply(json!({
            "text": "Not yet.",
            "request": { "title": "Apple Pay", "text": "Too short." }
        }))
        .expect_err("a request's text is 20 characters at least");
        assert!(refused_as_chat_reply(&too_short), "{too_short:?}");
        let answered = reply(json!({
            "text": "Not yet.\n\nI can put it on the board.",
            "request": {
                "title": "Let customers pay with Apple Pay",
                "text": "Offer Apple Pay at checkout beside the card form."
            }
        }))
        .expect("the reply is recorded");
        assert_eq!(answered["said"], "Sent. Your turn is over.", "{answered}");
        let before = project.event_count();
        let second = reply(json!({ "text": "And another thing." }))
            .expect_err("a chat session replies once");
        assert!(refused_as_chat_reply(&second), "{second:?}");
        // Not from any other session.
        let mut conversation = project.context("pm", None);
        conversation.purpose = SessionPurpose::Conversation;
        conversation.session_id = "session-2".to_string();
        let elsewhere = run(
            &conversation,
            "catervas_chat_reply",
            json!({ "text": "Hello." }),
        )
        .expect_err("only a chat session replies in a chat");
        assert!(refused_as_chat_reply(&elsewhere), "{elsewhere:?}");
        assert_eq!(project.event_count(), before, "nothing more is recorded");

        let replies = project.events(&[EventKind::ChatMessagePosted]);
        assert_eq!(replies.len(), 2, "the question and one reply: {replies:?}");
        let EventBody::ChatMessagePosted(body) = &replies[1].body else {
            panic!("a chat message");
        };
        assert_eq!((body.chat.as_str(), body.author.as_str()), ("pm", "pm"));
        assert_eq!(body.text, "Not yet.\n\nI can put it on the board.");
        assert_eq!(body.in_reply_to.map(NonZeroU64::get), Some(asked));
        assert_eq!(
            body.request.as_ref().map(|request| request.title.as_str()),
            Some("Let customers pay with Apple Pay")
        );
        assert_eq!(
            replies[1].envelope.ids.session_id.as_deref(),
            Some("session-1")
        );
        assert_eq!(replies[1].envelope.ids.agent_id.as_deref(), Some("pm"));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_a_reply_with_what_it_heard() {
        let project = TestProject::new("chat-reply-heard", &a_team_of_three(|_| {}));
        let asked = crate::chat::post_chat(
            &project.deps.log,
            project.deps.clock.as_ref(),
            &project.deps.ids,
            crate::chat::NewChatMessage {
                chat: "pm".to_string(),
                author: "human".to_string(),
                text: "That is everything.".to_string(),
                in_reply_to: None,
                request: None,
                heard: None,
                session_id: None,
                as_task: false,
                attachments: Vec::new(),
            },
        )
        .expect("the owner's message is recorded");
        let mut context = project.context("pm", None);
        context.purpose = SessionPurpose::Chat;
        context.in_reply_to = Some(asked);
        let heard = |change: &dyn Fn(&mut serde_json::Value)| {
            let mut heard = json!({
                "customers": "People who order cakes for birthdays.",
                "problem": "They cannot see what is free on a given day.",
                "must": "Show the days a cake can be ordered.",
                "not_now": "Delivery.",
                "works_when": "Ten orders arrive in the first month."
            });
            change(&mut heard);
            json!({ "text": "Here is what I heard.", "heard": heard })
        };
        let before = project.event_count();
        let blank = run(
            &context,
            "catervas_chat_reply",
            heard(&|heard| heard["problem"] = json!("  \n")),
        )
        .expect_err("a blank part is refused");
        assert!(
            matches!(&blank, ToolError::Refused { reason } if reason.starts_with("chat_reply_refused: blank_heard")),
            "{blank:?}"
        );
        let long = run(
            &context,
            "catervas_chat_reply",
            heard(&|heard| heard["must"] = json!("x".repeat(1001))),
        )
        .expect_err("a long part is refused");
        assert!(
            matches!(&long, ToolError::Refused { reason } if reason.starts_with("chat_reply_refused: heard_out_of_bounds")),
            "{long:?}"
        );
        assert_eq!(project.event_count(), before, "neither is recorded");

        run(&context, "catervas_chat_reply", heard(&|_| {})).expect("the reply is recorded");
        let replies = project.events(&[EventKind::ChatMessagePosted]);
        let EventBody::ChatMessagePosted(body) = &replies[1].body else {
            panic!("a chat message");
        };
        let recorded = serde_json::to_value(&body.heard).expect("serialises");
        assert_eq!(
            recorded,
            json!({
                "customers": "People who order cakes for birthdays.",
                "problem": "They cannot see what is free on a given day.",
                "must": "Show the days a cake can be ordered.",
                "not_now": "Delivery.",
                "works_when": "Ten orders arrive in the first month."
            })
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_one_reply_when_two_race() {
        let project = TestProject::new("chat-reply-race", &a_team_of_three(|_| {}));
        // Raced a few times, so that a check outside the lock is caught every run, not by luck.
        for round in 0..10 {
            let asked = crate::chat::post_chat(
                &project.deps.log,
                project.deps.clock.as_ref(),
                &project.deps.ids,
                crate::chat::NewChatMessage {
                    heard: None,
                    chat: "pm".to_string(),
                    author: "human".to_string(),
                    text: "Status?".to_string(),
                    in_reply_to: None,
                    request: None,
                    session_id: None,
                    as_task: false,
                    attachments: Vec::new(),
                },
            )
            .expect("the question is recorded");
            let mut context = project.context("pm", None);
            context.purpose = SessionPurpose::Chat;
            context.in_reply_to = Some(asked);
            context.session_id = format!("session-race-{round}");
            let start = std::sync::Barrier::new(2);
            let answers: Vec<_> = std::thread::scope(|scope| {
                let racers: Vec<_> = (0..2)
                    .map(|_| {
                        scope.spawn(|| {
                            start.wait();
                            run(&context, "catervas_chat_reply", json!({ "text": "On it." }))
                        })
                    })
                    .collect();
                racers
                    .into_iter()
                    .map(|racer| racer.join().expect("the racer ends"))
                    .collect()
            });
            let sent = answers.iter().filter(|answer| answer.is_ok()).count();
            assert_eq!(sent, 1, "round {round}: {answers:?}");
            let recorded = project
                .events(&[EventKind::ChatMessagePosted])
                .iter()
                .filter(|event| {
                    event.envelope.ids.session_id.as_deref() == Some(context.session_id.as_str())
                })
                .count();
            assert_eq!(recorded, 1, "round {round}");
        }
    }

    /// A project with the owner's "Please add a dark mode switch to Settings" at the first seq and
    /// "Dark mode?" at the second, in `dev-a`'s chat, and `dev-a`'s chat context answering the first.
    fn asked_in_a_chat(
        name: &str,
        team: &catervas_core::team::Team,
    ) -> (TestProject, crate::tools::ToolContext, u64, u64) {
        let project = TestProject::new(name, team);
        let post = |text: &str| {
            crate::chat::post_chat(
                &project.deps.log,
                project.deps.clock.as_ref(),
                &project.deps.ids,
                crate::chat::NewChatMessage {
                    heard: None,
                    chat: "dev-a".to_string(),
                    author: "human".to_string(),
                    text: text.to_string(),
                    in_reply_to: None,
                    request: None,
                    session_id: None,
                    as_task: false,
                    attachments: Vec::new(),
                },
            )
            .expect("the owner's message is recorded")
        };
        let (n, m) = (
            post("Please add a dark mode switch to Settings"),
            post("Dark mode?"),
        );
        let mut context = project.context("dev-a", None);
        context.purpose = SessionPurpose::Chat;
        context.in_reply_to = Some(n);
        (project, context, n, m)
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn takes_the_answered_message_as_its_task() {
        let (project, context, n, _) = asked_in_a_chat("chat-take", &a_team_of_three(|_| {}));

        let answer = run(
            &context,
            "catervas_take_task",
            json!({ "text": "Dark mode switch in Settings" }),
        )
        .expect("the task is taken");

        assert_eq!(answer["task_id"], "CTV-1");
        assert!(
            answer["next"]
                .as_str()
                .is_some_and(|next| next.contains("CTV-1")),
            "{answer}"
        );
        let created = project.events(&[EventKind::TaskCreated, EventKind::RequestTriaged]);
        let EventBody::TaskCreated(body) = &created[0].body else {
            panic!("a task.created");
        };
        assert_eq!(body.created_by, "dev-a");
        assert_eq!(
            body.given_to.as_ref().map(|agent| agent.as_str()),
            Some("dev-a")
        );
        assert_eq!(body.from_chat_message.map(NonZeroU64::get), Some(n));
        let EventBody::RequestTriaged(triaged) = &created[1].body else {
            panic!("a request.triaged");
        };
        assert_eq!(
            (triaged.reason.as_str(), triaged.triaged_by.as_str()),
            ("a task the owner gave dev-a in their chat", "catervas")
        );
        let contract = project.file("CTV-1");
        assert_eq!(contract["assignee_role"], "software_developer");
        assert_eq!(contract["title"], "Dark mode switch in Settings");
        assert_eq!(
            contract["intent"],
            "Please add a dark mode switch to Settings"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn takes_a_task_only_from_its_own_chat() {
        let (project, context, _, m) = asked_in_a_chat("chat-take-only", &a_team_of_three(|_| {}));
        let take = |context: &crate::tools::ToolContext, text: &str| {
            run(context, "catervas_take_task", json!({ "text": text }))
        };
        let refusal = |result: Result<serde_json::Value, ToolError>| match result {
            Err(ToolError::Refused { reason }) => reason,
            other => panic!("expected a refusal, got {other:?}"),
        };
        take(&context, "Dark mode switch").expect("the first call takes it");
        let created = || project.events(&[EventKind::TaskCreated]).len();
        assert_eq!(created(), 1);

        assert_eq!(
            refusal(take(&context, "Dark mode switch")),
            "already_a_task: CTV-1"
        );
        let mut implement = project.context("dev-a", None);
        implement.in_reply_to = context.in_reply_to;
        assert!(refusal(take(&implement, "Dark mode switch")).starts_with("take_task_refused: "));
        let mut unanswering = project.context("dev-a", None);
        unanswering.purpose = SessionPurpose::Chat;
        assert!(refusal(take(&unanswering, "Dark mode switch")).starts_with("take_task_refused: "));
        assert!(refusal(take(&context, "ab")).starts_with("task_title: "));
        let mut short = project.context("dev-a", None);
        short.purpose = SessionPurpose::Chat;
        short.in_reply_to = Some(m);
        assert!(refusal(take(&short, "Dark mode switch")).starts_with("task_too_short: "));
        assert_eq!(created(), 1, "nothing more is filed");

        let (project, context, _, _) = asked_in_a_chat(
            "chat-take-paused",
            &a_team_of_three(|wire| wire["agents"][1]["status"] = json!("paused")),
        );
        let paused = refusal(take(&context, "Dark mode switch"));
        assert!(paused.starts_with("agent_not_active: "), "{paused}");
        assert!(project.events(&[EventKind::TaskCreated]).is_empty());
    }
}
