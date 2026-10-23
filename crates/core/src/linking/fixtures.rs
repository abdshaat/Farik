//! The tempting corpus (`docs/SPEC.md` 4.4; ADR 0059): invented projects whose files are in
//! `fixtures/<project>/`, 9 related pairs, one per proof kind plus a second `deploys`, and 12
//! unrelated pairs that look related and are not. The owner must never be asked to bundle two
//! unrelated projects, so `find_proofs` must give no proof for any unrelated pair.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{TimeZone, Utc};

use super::{ClaudeAiProjectText, ProjectFiles, ProofKind, SessionNote};

/// One project of the corpus: its name, its remote's owner, and its files as `(path, text)`.
struct Entry {
    name: &'static str,
    owner: &'static str,
    files: &'static [(&'static str, &'static str)],
}

macro_rules! file {
    ($project:literal, $path:literal) => {
        (
            $path,
            include_str!(concat!("fixtures/", $project, "/", $path)),
        )
    };
}

const ENTRIES: &[Entry] = &[
    Entry {
        name: "tidepool-web",
        owner: "tidepool-co",
        files: &[file!("tidepool-web", "web/package.json")],
    },
    Entry {
        name: "tidepool-api",
        owner: "tidepool-co",
        files: &[file!("tidepool-api", "api/client/package.json")],
    },
    Entry {
        name: "harbor-site",
        owner: "harbor-co",
        files: &[file!("harbor-site", ".gitmodules")],
    },
    Entry {
        name: "harbor-theme",
        owner: "harbor-co",
        files: &[],
    },
    Entry {
        name: "ledger-app",
        owner: "ledgerlab",
        files: &[file!("ledger-app", "package.json")],
    },
    Entry {
        name: "ledger-money",
        owner: "ledgerlab",
        files: &[file!("ledger-money", "package.json")],
    },
    Entry {
        name: "shop-stack",
        owner: "shop-co",
        files: &[file!("shop-stack", "compose.yaml")],
    },
    Entry {
        name: "shop-checkout",
        owner: "shop-co",
        files: &[],
    },
    Entry {
        name: "cluster-config",
        owner: "relay-co",
        files: &[file!("cluster-config", "k8s/relay.yaml")],
    },
    Entry {
        name: "relay-worker",
        owner: "relay-co",
        files: &[file!("relay-worker", ".github/workflows/publish.yml")],
    },
    Entry {
        name: "release-bot",
        owner: "docs-co",
        files: &[file!("release-bot", ".github/workflows/sync.yml")],
    },
    Entry {
        name: "docs-hub",
        owner: "docs-co",
        files: &[],
    },
    Entry {
        name: "pixel-api",
        owner: "pixel-co",
        files: &[file!("pixel-api", "docker-compose.yml")],
    },
    Entry {
        name: "pixel-garden",
        owner: "pixel-co",
        files: &[file!("pixel-garden", "CNAME")],
    },
    Entry {
        name: "orbit-cli",
        owner: "orbit-co",
        files: &[],
    },
    // The unrelated ones. 1: the same owner.
    Entry {
        name: "alpha-site",
        owner: "acme-labs",
        files: &[file!("alpha-site", "package.json")],
    },
    Entry {
        name: "zeta-cli",
        owner: "acme-labs",
        files: &[file!("zeta-cli", "Cargo.toml")],
    },
    // 2: the same days. 3: similar names. 4: a session that read both.
    Entry {
        name: "mint-notes",
        owner: "mint-co",
        files: &[],
    },
    Entry {
        name: "quartz-tools",
        owner: "quartz-co",
        files: &[],
    },
    Entry {
        name: "price-watch",
        owner: "price-co",
        files: &[],
    },
    Entry {
        name: "watch-tower",
        owner: "tower-co",
        files: &[],
    },
    Entry {
        name: "river-blog",
        owner: "river-co",
        files: &[file!("river-blog", ".github/workflows/ci.yml")],
    },
    Entry {
        name: "comet-lab",
        owner: "comet-co",
        files: &[],
    },
    // 5: both depend on react.
    Entry {
        name: "dune-app",
        owner: "dune-co",
        files: &[file!("dune-app", "package.json")],
    },
    Entry {
        name: "fern-app",
        owner: "fern-co",
        files: &[file!("fern-app", "package.json")],
    },
    // 6: both depend on @acme/ui, which neither declares.
    Entry {
        name: "opal-shop",
        owner: "opal-co",
        files: &[file!("opal-shop", "package.json")],
    },
    Entry {
        name: "pine-admin",
        owner: "pine-co",
        files: &[file!("pine-admin", "package.json")],
    },
    // 7: both use Stripe through Claude, and the stripe package.
    Entry {
        name: "lark-crm",
        owner: "lark-co",
        files: &[file!("lark-crm", "package.json")],
    },
    Entry {
        name: "moss-books",
        owner: "moss-co",
        files: &[file!("moss-books", "package.json")],
    },
    // 8: a compose build that resolves outside both.
    Entry {
        name: "gold-forms",
        owner: "gold-co",
        files: &[file!("gold-forms", "compose.yaml")],
    },
    Entry {
        name: "iris-pages",
        owner: "iris-co",
        files: &[
            file!("iris-pages", ".github/workflows/ci.yml"),
            file!("iris-pages", ".gitmodules"),
        ],
    },
    // 9: a CNAME of one, and the other's config naming other subdomains.
    Entry {
        name: "reed-docs",
        owner: "reed-co",
        files: &[file!("reed-docs", "CNAME")],
    },
    Entry {
        name: "sand-app",
        owner: "sand-co",
        files: &[file!("sand-app", "docker-compose.yml")],
    },
    // 10: a file: dependency to a folder neither is.
    Entry {
        name: "kelp-ui",
        owner: "kelp-co",
        files: &[file!("kelp-ui", "package.json")],
    },
    Entry {
        name: "oak-sdk",
        owner: "oak-co",
        files: &[file!("oak-sdk", "package.json")],
    },
    // Shared work: two sessions wrote in both of the first pair. One session wrote in both of the
    // second, and in the third pair sessions wrote in one and only read the other.
    Entry {
        name: "birch-api",
        owner: "birch-co",
        files: &[],
    },
    Entry {
        name: "cedar-app",
        owner: "cedar-co",
        files: &[],
    },
    Entry {
        name: "pearl-cms",
        owner: "pearl-co",
        files: &[],
    },
    Entry {
        name: "ruby-mail",
        owner: "ruby-co",
        files: &[],
    },
    Entry {
        name: "tulip-ui",
        owner: "tulip-co",
        files: &[],
    },
    Entry {
        name: "vine-docs",
        owner: "vine-co",
        files: &[],
    },
];

/// The sessions that changed files under projects of the corpus: the session, then the names of
/// the projects it wrote in. `Corpus::session_notes` makes up each one's title and day.
const SESSIONS: &[(&str, &[&str])] = &[
    ("session-1", &["birch-api", "cedar-app"]),
    ("session-2", &["birch-api", "cedar-app"]),
    ("session-3", &["pearl-cms", "ruby-mail"]),
    ("session-4", &["tulip-ui"]),
    ("session-5", &["tulip-ui"]),
];

/// The corpus: its projects, its claude.ai Project, and what must and must not be proved.
pub struct Corpus {
    /// Every project, each with its committed files.
    pub projects: Vec<ProjectFiles>,
    /// The claude.ai Projects, as step 04's index holds them.
    pub claude_ai: Vec<ClaudeAiProjectText>,
    /// The related pairs: the kind, then the names of `a` and `b` as the proof names them.
    pub related: Vec<(ProofKind, &'static str, &'static str)>,
    /// The unrelated pairs, by name, each with the temptation that makes it look related.
    pub unrelated: Vec<(&'static str, &'static str, &'static str)>,
}

impl Corpus {
    /// The project named `name`.
    ///
    /// # Panics
    ///
    /// When the corpus has no such project: a test names a project it does not hold.
    #[must_use]
    pub fn project(&self, name: &str) -> &ProjectFiles {
        self.projects
            .iter()
            .find(|project| project.name == name)
            .unwrap_or_else(|| panic!("the corpus has no project {name}"))
    }

    /// The sessions that wrote a file under the project named `name`.
    #[must_use]
    pub fn written_in(&self, name: &str) -> BTreeSet<String> {
        SESSIONS
            .iter()
            .filter(|(_, wrote)| wrote.contains(&name))
            .map(|(session, _)| (*session).to_string())
            .collect()
    }

    /// What is known of each session of the corpus: a title and a day, session 1 the newest.
    ///
    /// # Panics
    ///
    /// Never in practice: the days are valid dates.
    #[must_use]
    pub fn session_notes(&self) -> BTreeMap<String, SessionNote> {
        SESSIONS
            .iter()
            .zip(1u32..)
            .map(|((session, _), day)| {
                let last_at = Utc
                    .with_ymd_and_hms(2026, 10, 25 - day, 10, 0, 0)
                    .single()
                    .expect("a time");
                (
                    (*session).to_string(),
                    SessionNote {
                        title: Some(format!("Work of {session}")),
                        last_at,
                    },
                )
            })
            .collect()
    }

    /// The key of the project (or claude.ai Project) named `name`.
    ///
    /// # Panics
    ///
    /// When the corpus has no such project: a test names a project it does not hold.
    #[must_use]
    pub fn key_of(&self, name: &str) -> String {
        if let Some(project) = self.projects.iter().find(|project| project.name == name) {
            return project.key.clone();
        }
        let Some(project) = self.claude_ai.iter().find(|project| project.name == name) else {
            panic!("the corpus has no project {name}");
        };
        project.uuid.clone()
    }
}

/// The remote key of a corpus project.
fn remote_of(entry: &Entry) -> String {
    format!("github.com/{}/{}", entry.owner, entry.name)
}

/// The tempting corpus.
#[must_use]
pub fn tempting_corpus() -> Corpus {
    let projects = ENTRIES
        .iter()
        .map(|entry| ProjectFiles {
            key: remote_of(entry),
            name: entry.name.to_string(),
            remote_key: Some(remote_of(entry)),
            root: Some(format!("/work/{}", entry.name)),
            files: entry
                .files
                .iter()
                .map(|(path, text)| ((*path).to_string(), (*text).to_string()))
                .collect(),
        })
        .collect();
    Corpus {
        projects,
        claude_ai: vec![ClaudeAiProjectText {
            uuid: "claude-ai-launch-notes".to_string(),
            name: "Launch notes".to_string(),
            description: Some(
                "Release notes and plans for github.com/orbit-co/orbit-cli, the command line."
                    .to_string(),
            ),
            file_names: vec!["checklist.md".to_string()],
        }],
        related: vec![
            (ProofKind::PathDependency, "tidepool-web", "tidepool-api"),
            (ProofKind::Submodule, "harbor-site", "harbor-theme"),
            (ProofKind::DeclaredPackage, "ledger-app", "ledger-money"),
            (ProofKind::Deploys, "shop-stack", "shop-checkout"),
            (ProofKind::Deploys, "cluster-config", "relay-worker"),
            (ProofKind::Workflow, "release-bot", "docs-hub"),
            (ProofKind::Domain, "pixel-api", "pixel-garden"),
            (ProofKind::ClaudeAiNames, "Launch notes", "orbit-cli"),
            (ProofKind::SharedWork, "birch-api", "cedar-app"),
        ],
        unrelated: vec![
            ("alpha-site", "zeta-cli", "the same owner"),
            ("mint-notes", "quartz-tools", "the same days"),
            ("price-watch", "watch-tower", "similar names"),
            ("river-blog", "comet-lab", "a session that read both"),
            ("pearl-cms", "ruby-mail", "one session wrote both"),
            ("tulip-ui", "vine-docs", "sessions read both, wrote one"),
            ("dune-app", "fern-app", "both depend on react"),
            (
                "opal-shop",
                "pine-admin",
                "both depend on @acme/ui, which neither declares",
            ),
            ("lark-crm", "moss-books", "both use Stripe through Claude"),
            (
                "gold-forms",
                "iris-pages",
                "a compose build that resolves outside both",
            ),
            (
                "reed-docs",
                "sand-app",
                "a CNAME of one and other subdomains in the other",
            ),
            (
                "kelp-ui",
                "oak-sdk",
                "a file: dependency to a folder neither is",
            ),
        ],
    }
}
