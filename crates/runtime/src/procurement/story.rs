//! The story the tests of the procurement mailbox tell: Ivo, the Procurement Specialist of Corner
//! Bakery, writes from `buying@bakery.test` to Dana of Pie Box Pros, against `GreenMail`.

use std::sync::Arc;

use farik_protocol::command::Command;
use farik_protocol::event::EventKind;
use serde_json::{Value, json};

use crate::claude::Secret;
use crate::connectors::MemoryConnectorSecrets;
use crate::greenmail::{Account, BUYING, GreenMail};
use crate::mailbox::{MailboxAt, ProviderChoice, Security, Server, Trust};
use crate::orchestrator::fixtures::Harness;
use crate::orchestrator::{CommandError, Orchestrator};
use crate::procurement::{MailboxConnect, connect_mailbox};

/// Dana of Pie Box Pros, whose mailbox the fixture holds.
pub(crate) const DANA: Account = Account {
    login: "sales",
    password: "seller-word",
    address: "sales@pieboxpros.test",
};

/// The story: Ivo writes from `buying@bakery.test` to Dana, in a project whose mailbox is
/// connected to the fixture.
pub(crate) struct Story {
    pub(crate) fixture: GreenMail,
    pub(crate) harness: Harness,
    pub(crate) store: Arc<MemoryConnectorSecrets>,
    pub(crate) at: MailboxAt,
}

/// A draft of Ivo's through the tool, on a thread of its own: the tool runs a runtime, and the
/// test is on one already. Answers the message's number.
pub(crate) fn drafted(project: &crate::tools::fixtures::TestProject, input: Value) -> u64 {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                project
                    .call("proc", Some("FRK-1"), "farik_draft_seller_message", input)
                    .expect("a draft")["message"]
                    .as_u64()
                    .expect("a number")
            })
            .join()
            .expect("the draft ends")
    })
}

pub(crate) fn secret(word: &str) -> Secret {
    Secret::new(word.to_string())
}

pub(crate) fn connection(fixture: &GreenMail, disclose_ai: bool) -> MailboxConnect {
    let server = |port: u16| Server {
        host: "localhost".to_string(),
        port,
        security: Security::Tls,
    };
    MailboxConnect {
        address: BUYING.address.to_string(),
        name: "Sam Ortiz".to_string(),
        provider: ProviderChoice::Other,
        imap: server(fixture.imaps),
        smtp: server(fixture.smtps),
        username: BUYING.login.to_string(),
        folder: "INBOX".to_string(),
        signature: "Corner Bakery".to_string(),
        disclose_ai,
    }
}

impl Story {
    pub(crate) async fn new(name: &str) -> Story {
        let fixture = GreenMail::start(name, &[&BUYING, &DANA]);
        let harness = Harness::with_procurement(&format!("send-{name}"));
        harness.procurement_task("FRK-1", Some("in_progress"));
        let keys = Arc::new(MemoryConnectorSecrets::default());
        assert!(harness.daemon.set_connector_secrets(keys.clone()));
        assert!(
            harness
                .daemon
                .set_mail_trust(Trust::Root(fixture.ca_der.clone()))
        );
        let at = harness
            .daemon
            .mailbox_at(harness.project.deps.files.root())
            .expect("the project's id");
        let story = Story {
            fixture,
            harness,
            store: keys,
            at,
        };
        story.connect(true, BUYING.password).await;
        story
    }

    pub(crate) async fn connect(&self, disclose_ai: bool, password: &str) {
        connect_mailbox(
            &self.harness.project.deps,
            &*self.store,
            &self.at,
            connection(&self.fixture, disclose_ai),
            &secret(password),
            &Trust::Root(self.fixture.ca_der.clone()),
        )
        .await
        .expect("the mailbox connects");
    }

    /// A quote request of Ivo's to Dana, drafted as the tool drafts it; answers its number.
    pub(crate) fn draft(&self, subject: &str, body: &str) -> u64 {
        self.draft_as(json!({
            "seller": "Pie Box Pros", "to": DANA.address, "subject": subject,
            "body": body, "purpose": "quote_request"
        }))
    }

    pub(crate) fn draft_as(&self, input: Value) -> u64 {
        drafted(&self.harness.project, input)
    }

    pub(crate) fn orchestrator(&self) -> Orchestrator {
        self.harness.orchestrator(self.harness.recorded(Vec::new()))
    }

    pub(crate) async fn send(
        &self,
        message: u64,
        subject: &str,
        body: &str,
    ) -> Result<String, String> {
        self.orchestrator()
            .handle(Command::SellerMessageSend {
                message,
                subject: subject.to_string(),
                body: body.to_string(),
            })
            .await
            .map(|report| report.said)
            .map_err(reason)
    }

    pub(crate) fn events(&self, kinds: &[EventKind]) -> Vec<farik_protocol::event::FarikEvent> {
        self.harness.events(kinds)
    }

    pub(crate) fn out(&self, name: &str) -> String {
        std::fs::read_to_string(
            self.harness
                .procurement_folder()
                .join("mail/out")
                .join(name),
        )
        .unwrap_or_default()
    }

    /// What Dana received, whole.
    pub(crate) fn dana_has(&self) -> Vec<String> {
        self.fixture.inbox(&DANA)
    }
}

pub(crate) fn reason(error: CommandError) -> String {
    match error {
        CommandError::Refused { reason } => reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// The purchase orders that wait for the owner, as the store lists them.
pub(crate) fn orders_waiting(story: &Story) -> Vec<farik_store::waiting::OrderAsk> {
    let deps = &story.harness.project.deps;
    let team = deps.files.read_team().expect("the team");
    farik_store::waiting::waiting(&deps.projections, &deps.log, &deps.files, &team)
        .expect("the store reads")
        .into_iter()
        .filter_map(|item| item.order)
        .collect()
}

pub(crate) fn lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}
