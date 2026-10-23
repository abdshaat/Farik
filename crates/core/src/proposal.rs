//! The teams to make from the projects the owner chose and the links between them
//! (`docs/SPEC.md` 4.4; ADR 0059). Pure: the groups and the choices come in, the cards go out.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};

use crate::claude_services::catalog::{CATALOG, ServiceKind};
use crate::team::sha256_hex;

/// What kind of thing a project is, as far as making a team of it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    /// A folder, with a repository, on this computer.
    Folder,
    /// Work done through Claude's connectors, with no folder of its own.
    Connector,
}

/// A project the cards are made from: one the owner chose to make a team of, or one that is
/// already a team's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalProject {
    /// Its key.
    pub key: String,
    /// The name shown.
    pub name: String,
    /// Its folder, when it has one.
    pub folder: Option<String>,
    /// What kind of project it is.
    pub kind: ProjectKind,
    /// The names of the services it uses through Claude.
    pub services: Vec<String>,
    /// The names of its jobs.
    pub jobs: Vec<String>,
    /// When it was last worked on.
    pub last_at: Option<DateTime<Utc>>,
    /// The slug of the driven team it already belongs to, when it does.
    pub team: Option<String>,
}

/// Projects the owner said belong together, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedGroup {
    /// The keys.
    pub projects: Vec<String>,
    /// One sentence for each reason.
    pub why: Vec<String>,
}

/// Whether a card makes a team or adds to one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardKind {
    /// A team to make.
    NewTeam,
    /// Projects to add to a team that exists, as repositories.
    AddToTeam,
}

impl CardKind {
    /// The kind as it is written on the wire.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::NewTeam => "new_team",
            Self::AddToTeam => "add_to_team",
        }
    }
}

/// One team to be made, or one team to add projects to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// A hash of the sorted keys of the group it came from, stable across calls.
    pub id: String,
    /// Whether it makes a team or adds to one.
    pub kind: CardKind,
    /// Its projects.
    pub projects: Vec<ProposalProject>,
    /// The names of their jobs.
    pub jobs: Vec<String>,
    /// Why the projects are together.
    pub why: Vec<String>,
    /// The team's name.
    pub name: String,
    /// The built-in team suggested for it.
    pub template: String,
    /// The folder a connector project's team is made in, written from home.
    pub folder: Option<String>,
    /// The team an `add_to_team` card adds to.
    pub team: Option<String>,
}

/// The id of a card: the first twelve characters of the sha256 of the sorted keys.
#[must_use]
pub fn card_id(keys: &[String]) -> String {
    let mut sorted: Vec<&str> = keys.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted.dedup();
    sha256_hex(&sorted.join("\n")).chars().take(12).collect()
}

/// The project `key` names.
fn lookup<'a>(projects: &'a [ProposalProject], key: &str) -> Option<&'a ProposalProject> {
    projects.iter().find(|project| project.key == key)
}

/// A card before the projects are looked up.
struct Draft {
    id: String,
    kind: CardKind,
    keys: Vec<String>,
    why: Vec<String>,
    team: Option<String>,
}

/// Whether `service` is one the catalog sorts as bookkeeping.
fn is_bookkeeping(service: &str) -> bool {
    CATALOG
        .iter()
        .any(|entry| entry.kind == ServiceKind::Bookkeeping && entry.name == service)
}

/// `name` as a folder's name: lowercase letters and digits, the rest a single `-`.
fn slug_of(name: &str) -> String {
    let mut slug = String::new();
    for letter in name.chars().flat_map(char::to_lowercase) {
        if letter.is_ascii_alphanumeric() {
            slug.push(letter);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() {
        "team".to_string()
    } else {
        slug
    }
}

/// The last part of a folder's path.
fn folder_name(folder: &str) -> Option<&str> {
    folder
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
}

/// The cards the projects and their groups make, newest work first: a team to make for each group
/// with a project the owner chose and no driven team, and for each such project in no group; an
/// "add to" card for the projects of a group that has a driven team's. `moved_out` holds the
/// `(card id, project key)` pairs the owner moved out of a card: the project stays in no card but
/// its own, which keeps the first card's id.
#[must_use]
pub fn propose(
    projects: &[ProposalProject],
    groups: &[LinkedGroup],
    moved_out: &[(String, String)],
) -> Vec<Card> {
    let find = |key: &str| lookup(projects, key);
    let mut drafts: Vec<Draft> = Vec::new();
    let mut grouped: BTreeSet<&str> = BTreeSet::new();
    for group in groups {
        let mut keys: Vec<String> = group
            .projects
            .iter()
            .filter(|key| find(key).is_some())
            .cloned()
            .collect();
        keys.sort_unstable();
        keys.dedup();
        grouped.extend(group.projects.iter().map(String::as_str));
        let driven = keys.iter().find_map(|key| find(key)?.team.clone());
        let id = card_id(&keys);
        let (kind, wanted): (CardKind, Vec<String>) = match &driven {
            Some(_) => (
                CardKind::AddToTeam,
                keys.iter()
                    .filter(|key| find(key).is_some_and(|project| project.team.is_none()))
                    .cloned()
                    .collect(),
            ),
            None => (CardKind::NewTeam, keys),
        };
        if !wanted.is_empty() {
            drafts.push(Draft {
                id,
                kind,
                keys: wanted,
                why: group.why.clone(),
                team: driven,
            });
        }
    }
    for project in projects {
        if project.team.is_none() && !grouped.contains(project.key.as_str()) {
            drafts.push(Draft {
                id: card_id(std::slice::from_ref(&project.key)),
                kind: CardKind::NewTeam,
                keys: vec![project.key.clone()],
                why: Vec::new(),
                team: None,
            });
        }
    }
    let mut apart: Vec<Draft> = Vec::new();
    for draft in &mut drafts {
        for (card, key) in moved_out {
            if *card == draft.id && draft.keys.len() > 1 && draft.keys.contains(key) {
                draft.keys.retain(|each| each != key);
                apart.push(Draft {
                    id: card_id(std::slice::from_ref(key)),
                    kind: CardKind::NewTeam,
                    keys: vec![key.clone()],
                    why: Vec::new(),
                    team: None,
                });
            }
        }
    }
    drafts.extend(apart);
    let mut cards: Vec<Card> = drafts
        .into_iter()
        .map(|draft| card_of(draft, projects))
        .collect();
    cards.sort_by(|a, b| {
        let newest = |card: &Card| {
            card.projects
                .iter()
                .filter_map(|project| project.last_at)
                .max()
        };
        newest(b)
            .cmp(&newest(a))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
    cards
}

/// The card a draft is, with its projects looked up.
fn card_of(draft: Draft, projects: &[ProposalProject]) -> Card {
    let members: Vec<ProposalProject> = draft
        .keys
        .iter()
        .filter_map(|key| lookup(projects, key).cloned())
        .collect();
    let mut jobs: Vec<String> = Vec::new();
    for job in members.iter().flat_map(|project| &project.jobs) {
        if !jobs.contains(job) {
            jobs.push(job.clone());
        }
    }
    let bookkeeping = members.iter().any(|project| {
        project.kind == ProjectKind::Connector
            && project
                .services
                .iter()
                .any(|service| is_bookkeeping(service))
    });
    let first = members.first();
    let name = if bookkeeping {
        "Bookkeeping".to_string()
    } else {
        first
            .map(|project| {
                project
                    .folder
                    .as_deref()
                    .and_then(folder_name)
                    .map_or_else(|| project.name.clone(), ToString::to_string)
            })
            .unwrap_or_default()
    };
    let folder = first
        .filter(|project| project.kind == ProjectKind::Connector)
        .map(|_| format!("~/Catervas/{}", slug_of(&name)));
    Card {
        id: draft.id,
        kind: draft.kind,
        projects: members,
        jobs,
        why: draft.why,
        name,
        template: if bookkeeping {
            "financial-analysis"
        } else {
            "software-product"
        }
        .to_string(),
        folder,
        team: draft.team,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{CardKind, LinkedGroup, ProjectKind, ProposalProject, propose};

    fn project(key: &str, days: u32) -> ProposalProject {
        ProposalProject {
            key: key.to_string(),
            name: key.to_string(),
            folder: Some(format!("/home/o/{key}")),
            kind: ProjectKind::Folder,
            services: Vec::new(),
            jobs: vec![format!("{key} job")],
            last_at: Utc.with_ymd_and_hms(2026, 10, days, 9, 0, 0).single(),
            team: None,
        }
    }

    fn group(keys: &[&str]) -> LinkedGroup {
        LinkedGroup {
            projects: keys.iter().map(ToString::to_string).collect(),
            why: vec!["a uses b".to_string()],
        }
    }

    #[test]
    fn makes_a_card_per_group_and_lone_team() {
        let mut shop = project("shop", 3);
        shop.team = Some("shop".to_string());
        let api = project("api", 4);
        let mut books = project("books", 5);
        books.kind = ProjectKind::Connector;
        books.folder = None;
        books.name = "Zoho Books".to_string();
        books.services = vec!["Zoho Books".to_string()];
        let all = vec![
            project("a", 1),
            project("b", 2),
            project("c", 6),
            project("d", 7),
            shop,
            api,
            books,
        ];
        let groups = vec![group(&["a", "b"]), group(&["d", "shop"])];
        let cards = propose(&all, &groups, &[]);
        let kinds: Vec<_> = cards
            .iter()
            .map(|card| (card.kind, card.projects.len()))
            .collect();
        // Newest work first: books (5), c (6)... api is lone and Team too.
        assert_eq!(
            cards.iter().filter(|c| c.kind == CardKind::NewTeam).count(),
            4,
            "{kinds:?}"
        );
        let adds: Vec<_> = cards
            .iter()
            .filter(|c| c.kind == CardKind::AddToTeam)
            .collect();
        assert_eq!(adds.len(), 1);
        assert_eq!(adds[0].team.as_deref(), Some("shop"));
        assert_eq!(adds[0].projects[0].key, "d");
        let pair = cards
            .iter()
            .find(|c| c.projects.len() == 2)
            .expect("the pair");
        assert_eq!(pair.why, vec!["a uses b".to_string()]);
        assert_eq!(pair.name, "a");
        assert_eq!(pair.template, "software-product");
        assert_eq!(pair.jobs, vec!["a job".to_string(), "b job".to_string()]);

        // The ids do not depend on the order the projects come in.
        let mut reversed = all.clone();
        reversed.reverse();
        let again = propose(&reversed, &[group(&["b", "a"]), group(&["shop", "d"])], &[]);
        let mut ids: Vec<_> = cards.iter().map(|c| c.id.clone()).collect();
        let mut ids_again: Vec<_> = again.iter().map(|c| c.id.clone()).collect();
        ids.sort();
        ids_again.sort();
        assert_eq!(ids, ids_again);
        assert!(ids.iter().all(|id| id.len() == 12));

        // A bookkeeping connector project: Financial analysis, in its own folder.
        let books_card = cards
            .iter()
            .find(|c| c.projects[0].key == "books")
            .expect("books");
        assert_eq!(books_card.name, "Bookkeeping");
        assert_eq!(books_card.template, "financial-analysis");
        assert_eq!(books_card.folder.as_deref(), Some("~/Catervas/bookkeeping"));
        // Anything else is software.
        let api_card = cards
            .iter()
            .find(|c| c.projects[0].key == "api")
            .expect("api");
        assert_eq!(
            (api_card.template.as_str(), api_card.folder.clone()),
            ("software-product", None)
        );

        // Moving b out of a card leaves it its own card, and the card keeps its id.
        let moved = propose(&all, &groups, &[(pair.id.clone(), "b".to_string())]);
        let left = moved.iter().find(|c| c.id == pair.id).expect("the card");
        assert_eq!(left.projects.len(), 1);
        assert!(
            moved
                .iter()
                .any(|c| c.projects[0].key == "b" && c.id != pair.id)
        );
    }
}
