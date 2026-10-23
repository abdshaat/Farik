//! The proposal (`docs/SPEC.md` 4.1, 4.4; ADR 0059): the teams to make from the projects the owner
//! chose and the links between them, the owner's edits of each card, creating the teams paused,
//! adding a project to a team that exists, and taking a made team back until it first starts.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use catervas_core::claude_services::catalog::CATALOG;
use catervas_core::finding::Choice;
use catervas_core::pricing::{Usage, compute_cost_usd, prices::PRICE_TABLE};
use catervas_core::proposal::{Card, CardKind, LinkedGroup, ProjectKind, ProposalProject, propose};
use catervas_core::scrub::scrub;
use catervas_protocol::event::EventKind;
use catervas_store::Git;
use catervas_store::files::ProjectFiles;
use catervas_store::linking::{Answer, confirmed_groups, evaluate};
use catervas_store::machine::{MachineIndex, ProposalEdit};
use catervas_store::{EventQuery, open_event_log};
use serde_json::{Value, json};

use super::{
    FindingHost, MadeProject, PlanKind, TeamPlan, build_listing, internal, link_scope, moment,
    open_index, read_of, refusal, unknown_project,
};
use crate::daemon::hub::{Hub, TeamAdded, TeamSlug};
use crate::daemon::web::Failure;
use crate::templates::{Templates, is_builtin};
use crate::tools::ToolDeps;

/// The methods of this module.
pub(super) const METHODS: [&str; 9] = [
    "linking.reject",
    "proposal.get",
    "proposal.edit",
    "proposal.move_out",
    "proposal.revert",
    "proposal.names_estimate",
    "proposal.names",
    "proposal.create",
    "teams.undo",
];

/// How long a made team is given to start being driven, and a removed one to stop.
const DRIVE_WAIT: Duration = Duration::from_secs(60);

/// The longest a name from Claude is, and a purpose.
const NAME_FROM_CLAUDE: usize = 40;
const PURPOSE_FROM_CLAUDE: usize = 120;
/// The most output tokens the price of the names run is worked out for.
const MAX_OUTPUT_TOKENS: u64 = 400;
/// How many lines of a README Claude is shown, and how many session titles.
const README_LINES: usize = 40;
const SESSION_TITLES: usize = 20;
/// The longest a name is, and a purpose.
const NAME_LIMIT: usize = 100;
const PURPOSE_LIMIT: usize = 200;

/// The methods of this module, whose params the schema already passed.
pub(super) fn call(
    hub: &Hub,
    host: &dyn FindingHost,
    method: &str,
    params: &Value,
) -> Result<Value, Failure> {
    match method {
        "proposal.get" => get(hub, host),
        "proposal.edit" => edit(hub, host, params),
        "proposal.move_out" => move_out(hub, host, params),
        "proposal.revert" => {
            open_index(host)?
                .revert_edit(params["id"].as_str().unwrap_or_default())
                .map_err(|e| internal(&e))?;
            Ok(json!({}))
        }
        "proposal.names_estimate" => names_estimate(hub, host),
        "proposal.names" => names(hub, host, params),
        "proposal.create" => create(hub, host, params),
        "linking.reject" => reject(host, params),
        _ => undo(hub, host, params),
    }
}

/// The slugs of the teams Catervas made from the proposal.
pub(in crate::daemon) fn made_slugs(hub: &Hub) -> BTreeSet<String> {
    let Some(host) = hub.finding.host.get() else {
        return BTreeSet::new();
    };
    if !host
        .state_dir()
        .join(catervas_store::machine::MACHINE_DB)
        .exists()
    {
        return BTreeSet::new();
    }
    open_index(host.as_ref())
        .ok()
        .and_then(|index| index.made_teams().ok())
        .map(|teams| teams.into_iter().map(|team| team.slug).collect())
        .unwrap_or_default()
}

/// Whether the team's log holds a `session.started`; a log that cannot be read counts as started.
pub(in crate::daemon) fn has_started(deps: &ToolDeps) -> bool {
    deps.log
        .read(&EventQuery {
            kinds: vec![EventKind::SessionStarted],
            limit: Some(1),
            ..EventQuery::default()
        })
        .map_or(true, |events| !events.is_empty())
}

/// Whether the log of the project at `root` holds a `session.started`.
fn started_at(root: &Path, now: chrono::DateTime<chrono::Utc>) -> bool {
    open_event_log(&root.join(".catervas/local/catervas.db"), now)
        .and_then(|log| {
            log.read(&EventQuery {
                kinds: vec![EventKind::SessionStarted],
                limit: Some(1),
                ..EventQuery::default()
            })
        })
        .is_ok_and(|events| !events.is_empty())
}

/// What the proposal is made of now.
struct Built {
    cards: Vec<Card>,
    edits: BTreeMap<String, ProposalEdit>,
    /// The name of each team a card can add to.
    team_names: BTreeMap<String, String>,
}

/// The `slug` of the driven team a folder belongs to: its project folder, or a member of it.
fn teams_by_folder(hub: &Hub) -> Vec<(String, Vec<PathBuf>)> {
    hub.team_roots()
        .into_iter()
        .map(|(slug, root)| {
            let canonical = |path: PathBuf| path.canonicalize().unwrap_or(path);
            let mut folders = vec![canonical(root.clone())];
            if let Ok(Some(repos)) = ProjectFiles::open(root).read_repos() {
                folders.extend(
                    repos
                        .repos
                        .iter()
                        .map(|repo| canonical(PathBuf::from(repo.path.as_str()))),
                );
            }
            (slug, folders)
        })
        .collect()
}

/// The cards, from the index and the hub.
fn build(hub: &Hub, host: &dyn FindingHost, index: &MachineIndex) -> Result<Built, Failure> {
    let listing = build_listing(hub, host, index).map_err(|e| internal(&e))?;
    let scope = link_scope(hub, host, index)?;
    let evaluation = evaluate(&scope.projects, &scope.claude_ai, &read_of(host));
    let groups = confirmed_groups(index, &scope.projects, &evaluation).map_err(|e| internal(&e))?;
    let teams = teams_by_folder(hub);
    let made: BTreeMap<String, String> = index
        .made_teams()
        .map_err(|e| internal(&e))?
        .into_iter()
        .filter(|team| hub.knows(&team.slug))
        .flat_map(|team| {
            serde_json::from_str::<Value>(&team.made)
                .ok()
                .and_then(|made| made["projects"].as_array().cloned())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|key| key.as_str().map(|key| (key.to_string(), team.slug.clone())))
                .collect::<Vec<_>>()
        })
        .collect();
    let declared = index.declared_services().map_err(|e| internal(&e))?;
    let mut projects = Vec::new();
    for project in &scope.projects {
        let Some(row) = listing.find(&project.key) else {
            continue;
        };
        let team = row
            .already_team
            .clone()
            .or_else(|| made.get(&row.key).cloned())
            .or_else(|| {
                row.folders.iter().find_map(|folder| {
                    let folder = Path::new(folder);
                    let folder = folder
                        .canonicalize()
                        .unwrap_or_else(|_| folder.to_path_buf());
                    teams
                        .iter()
                        .find(|(_, folders)| folders.contains(&folder))
                        .map(|(slug, _)| slug.clone())
                })
            });
        let connector = row.kind == "connector";
        let mut services = row.services.clone();
        if connector && services.is_empty() {
            services = declared
                .iter()
                .filter(|(_, name)| row.name.contains(&format!("({name})")))
                .map(|(_, name)| name.clone())
                .collect();
        }
        projects.push(ProposalProject {
            key: row.key.clone(),
            name: row.name.clone(),
            folder: row.folders.first().cloned(),
            kind: if connector {
                ProjectKind::Connector
            } else {
                ProjectKind::Folder
            },
            services,
            jobs: row.jobs.iter().map(|(name, _)| name.clone()).collect(),
            last_at: row.last_at,
            team,
        });
    }
    let linked: Vec<LinkedGroup> = groups
        .into_iter()
        .map(|group| LinkedGroup {
            projects: group.projects,
            why: group.why,
        })
        .collect();
    let moved = index.moved_out().map_err(|e| internal(&e))?;
    let cards = propose(&projects, &linked, &moved);
    let team_names = hub
        .team_roots()
        .into_iter()
        .map(|(slug, root)| {
            let name = ProjectFiles::open(root)
                .read_team()
                .map_or_else(|_| slug.clone(), |team| team.name.to_string());
            (slug, name)
        })
        .collect();
    Ok(Built {
        cards,
        edits: index.edits().map_err(|e| internal(&e))?,
        team_names,
    })
}

/// A path with home written `~`.
fn from_home(host: &dyn FindingHost, path: &str) -> String {
    Path::new(path)
        .strip_prefix(host.home())
        .map_or_else(|_| path.to_string(), |rest| format!("~/{}", rest.display()))
}

/// What a card shows and creates: its own, else what the owner (or Claude) edited.
struct Shown {
    name: String,
    purpose: Option<String>,
    template: String,
    from_claude: bool,
}

fn shown(card: &Card, edit: Option<&ProposalEdit>) -> Shown {
    Shown {
        name: edit
            .and_then(|edit| edit.name.clone())
            .unwrap_or_else(|| card.name.clone()),
        purpose: edit.and_then(|edit| edit.purpose.clone()),
        template: edit
            .and_then(|edit| edit.template.clone())
            .unwrap_or_else(|| card.template.clone()),
        from_claude: edit.is_some_and(|edit| edit.from_claude),
    }
}

fn card_json(host: &dyn FindingHost, built: &Built, card: &Card) -> Value {
    let kept = shown(card, built.edits.get(&card.id));
    let mut wire = json!({
        "id": card.id,
        "kind": card.kind.as_wire(),
        "projects": card.projects.iter().map(|project| {
            let mut each = json!({
                "key": project.key,
                "name": project.name,
                "services": project.services,
            });
            if let Some(folder) = &project.folder {
                each["folder"] = json!(from_home(host, folder));
            }
            each
        }).collect::<Vec<_>>(),
        "jobs": card.jobs,
        "why": card.why,
        "name": kept.name,
        "template": kept.template,
        "from_claude": kept.from_claude,
    });
    if let Some(purpose) = kept.purpose {
        wire["purpose"] = json!(purpose);
    }
    if let Some(folder) = &card.folder {
        wire["folder"] = json!(folder);
    }
    if let Some(team) = &card.team {
        wire["team"] = json!(team);
        wire["team_name"] = json!(
            built
                .team_names
                .get(team)
                .cloned()
                .unwrap_or_else(|| team.clone())
        );
    }
    wire
}

/// `proposal.get`.
fn get(hub: &Hub, host: &dyn FindingHost) -> Result<Value, Failure> {
    if !host
        .state_dir()
        .join(catervas_store::machine::MACHINE_DB)
        .exists()
    {
        return Ok(json!({ "cards": [], "templates": templates_json(host) }));
    }
    let index = open_index(host)?;
    let built = build(hub, host, &index)?;
    *hub.finding
        .shown
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = built
        .cards
        .iter()
        .map(|card| {
            (
                card.id.clone(),
                card.projects
                    .iter()
                    .map(|project| project.key.clone())
                    .collect(),
            )
        })
        .collect();
    Ok(json!({
        "cards": built.cards.iter().map(|card| card_json(host, &built, card)).collect::<Vec<_>>(),
        "templates": templates_json(host),
    }))
}

/// The slugs of the owner's saved teams.
fn saved_templates(host: &dyn FindingHost) -> Vec<String> {
    Templates::new(host.state_dir().join("templates"))
        .list()
        .map(|listing| {
            listing
                .templates
                .into_iter()
                .map(|(slug, _)| slug)
                .collect()
        })
        .unwrap_or_default()
}

/// The teams a card can start from: the ready-made ones, then the owner's saved ones.
fn templates_json(host: &dyn FindingHost) -> Vec<Value> {
    let mut all: Vec<Value> = crate::templates::builtins()
        .into_iter()
        .map(|(slug, template)| json!({ "slug": slug, "name": template.name.to_string() }))
        .collect();
    if let Ok(listing) = Templates::new(host.state_dir().join("templates")).list() {
        all.extend(
            listing
                .templates
                .into_iter()
                .map(|(slug, template)| json!({ "slug": slug, "name": template.name.to_string() })),
        );
    }
    all
}

/// `proposal.edit`.
fn edit(hub: &Hub, host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    let id = params["id"].as_str().unwrap_or_default();
    let mut index = open_index(host)?;
    let built = build(hub, host, &index)?;
    if built.cards.iter().all(|card| card.id != id) {
        return Err(refusal(
            "unknown_card",
            "that team is not on the list any more",
        ));
    }
    let name = params["name"].as_str().map(str::trim);
    if let Some(name) = name
        && (name.is_empty() || name.chars().count() > NAME_LIMIT)
    {
        return Err(refusal("bad_name", "a team's name is 1 to 100 characters"));
    }
    let purpose = params["purpose"].as_str().map(str::trim);
    if purpose.is_some_and(|purpose| purpose.chars().count() > PURPOSE_LIMIT) {
        return Err(refusal(
            "bad_purpose",
            "a purpose is at most 200 characters",
        ));
    }
    let template = params["template"].as_str();
    if let Some(template) = template
        && !is_builtin(template)
        && !saved_templates(host).iter().any(|saved| saved == template)
    {
        return Err(refusal(
            "template_unknown",
            "that team is not one of the ready-made or saved teams",
        ));
    }
    index
        .put_edit(
            id,
            &ProposalEdit {
                name: name.map(str::to_string),
                template: template.map(str::to_string),
                purpose: purpose
                    .filter(|purpose| !purpose.is_empty())
                    .map(str::to_string),
                from_claude: false,
            },
        )
        .map_err(|e| internal(&e))?;
    Ok(json!({}))
}

/// `proposal.move_out`.
fn move_out(hub: &Hub, host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    let (id, key) = (
        params["id"].as_str().unwrap_or_default(),
        params["key"].as_str().unwrap_or_default(),
    );
    let mut index = open_index(host)?;
    let built = build(hub, host, &index)?;
    let card = built
        .cards
        .iter()
        .find(|card| card.id == id)
        .filter(|card| card.projects.len() > 1 && card.projects.iter().any(|each| each.key == key))
        .ok_or_else(unknown_project)?;
    let others: Vec<String> = card
        .projects
        .iter()
        .filter(|each| each.key != key)
        .map(|each| each.key.clone())
        .collect();
    index
        .move_out(id, key, &others, host.now())
        .map_err(|e| internal(&e))?;
    Ok(json!({}))
}

/// `linking.reject`.
fn reject(host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    let key = params["key"].as_str().unwrap_or_default();
    let mut index = open_index(host)?;
    for other in params["others"].as_array().into_iter().flatten() {
        let other = other.as_str().unwrap_or_default();
        if other != key {
            index
                .put_link(key, other, Answer::No, &[], host.now())
                .map_err(|e| internal(&e))?;
        }
    }
    Ok(json!({}))
}

/// `name` as a folder's or a repository's name: lowercase letters, digits and single `-`, starting
/// with a letter, at most `limit` characters; `fallback` when nothing is left.
fn kebab(name: &str, limit: usize, fallback: &str) -> String {
    let mut slug = String::new();
    for letter in name.chars().flat_map(char::to_lowercase) {
        if letter.is_ascii_alphanumeric() {
            slug.push(letter);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    let slug = if slug.starts_with(|c: char| c.is_ascii_digit()) {
        format!("a{slug}")
    } else {
        slug.to_string()
    };
    let cut: String = slug.chars().take(limit).collect();
    let cut = cut.trim_end_matches('-').to_string();
    if cut.is_empty() {
        fallback.to_string()
    } else {
        cut
    }
}

/// A folder under `~/Catervas` named after `name` that is not there yet.
fn new_folder(host: &dyn FindingHost, name: &str) -> PathBuf {
    let parent = host.home().join("Catervas");
    let base = kebab(name, 40, "team");
    let mut count = 1;
    loop {
        let folder = parent.join(if count == 1 {
            base.clone()
        } else {
            format!("{base}-{count}")
        });
        if std::fs::symlink_metadata(&folder).is_err() {
            return folder;
        }
        count += 1;
    }
}

/// The service's id in the catalog, else `other:<name>`.
fn service_id(name: &str) -> String {
    CATALOG
        .iter()
        .find(|entry| entry.name == name)
        .map_or_else(|| format!("other:{name}"), |entry| entry.id.to_string())
}

/// The plan for a new team, or why the card cannot become one.
fn plan_of(host: &dyn FindingHost, card: &Card, kept: &Shown) -> Result<TeamPlan, String> {
    let services: Vec<(String, String)> = card
        .projects
        .iter()
        .flat_map(|project| project.services.iter())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|name| (service_id(name), name.clone()))
        .collect();
    let connectors = card
        .projects
        .iter()
        .filter(|project| project.kind == ProjectKind::Connector)
        .count();
    let kind = if connectors == card.projects.len() {
        PlanKind::Connector {
            folder: new_folder(host, &kept.name),
        }
    } else if connectors > 0 {
        return Err(
            "mixed_projects: work done through connectors and repositories cannot be one team"
                .to_string(),
        );
    } else {
        let mut folders = Vec::new();
        for project in &card.projects {
            let folder = project.folder.as_deref().ok_or_else(|| {
                format!(
                    "no_folder: {} is not on this computer; copy it here first",
                    project.name
                )
            })?;
            folders.push((project.name.clone(), PathBuf::from(folder)));
        }
        if let [(_, folder)] = folders.as_slice() {
            PlanKind::Folder {
                folder: folder.clone(),
            }
        } else {
            let mut taken: Vec<String> = Vec::new();
            let members = folders
                .into_iter()
                .map(|(name, folder)| {
                    let base = kebab(
                        folder
                            .file_name()
                            .and_then(|each| each.to_str())
                            .unwrap_or(&name),
                        28,
                        "repo",
                    );
                    let mut each = base.clone();
                    let mut count = 2;
                    while taken.contains(&each) {
                        each = format!("{base}-{count}");
                        count += 1;
                    }
                    taken.push(each.clone());
                    (each, folder)
                })
                .collect();
            PlanKind::Project {
                folder: new_folder(host, &kept.name),
                members,
            }
        }
    };
    Ok(TeamPlan {
        name: kept.name.clone(),
        template: kept.template.clone(),
        kind,
        services,
    })
}

/// The slug `base` makes that no team on the hub has.
fn free_slug(hub: &Hub, base: &str) -> TeamSlug {
    let taken: BTreeSet<String> = hub.team_roots().into_iter().map(|(slug, _)| slug).collect();
    let mut count = 1;
    loop {
        let candidate = if count == 1 {
            base.to_string()
        } else {
            format!("{base}-{count}")
        };
        if !taken.contains(&candidate) {
            return TeamSlug::new(&candidate);
        }
        count += 1;
    }
}

/// Waits until `done`, at most `DRIVE_WAIT`.
fn wait_for(mut done: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + DRIVE_WAIT;
    while Instant::now() < until {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    done()
}

/// A team made and driven, as `proposal.create` answers it.
struct MadeTeam {
    slug: String,
    root: PathBuf,
}

/// Makes the team of a `new_team` card, paused, and starts driving it.
fn make_team(
    hub: &Hub,
    host: &dyn FindingHost,
    index: &mut MachineIndex,
    card: &Card,
    kept: &Shown,
) -> Result<MadeTeam, String> {
    let plan = plan_of(host, card, kept)?;
    let MadeProject {
        root,
        team_id,
        created,
    } = host.create_team(&plan)?;
    let slug = free_slug(hub, &kebab(&team_id, 64, "team"));
    let keys: Vec<&String> = card.projects.iter().map(|project| &project.key).collect();
    let made = json!({
        "created": created.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
        "projects": keys,
    });
    index
        .put_made_team(
            slug.as_str(),
            &root.display().to_string(),
            &made.to_string(),
            host.now(),
        )
        .map_err(|e| e.to_string())?;
    for project in &card.projects {
        index
            .put_choice(&project.key, Choice::Team, host.now())
            .map_err(|e| e.to_string())?;
    }
    if !hub.add_team(TeamAdded {
        root: root.clone(),
        slug: slug.clone(),
    }) {
        return Err("not_available: this catervas cannot drive another team".to_string());
    }
    if !wait_for(|| hub.knows(slug.as_str())) {
        return Err(format!(
            "team_did_not_start: {} was made but did not start; remove it from All teams",
            kept.name
        ));
    }
    Ok(MadeTeam {
        slug: slug.as_str().to_string(),
        root,
    })
}

/// Adds the projects of an `add_to_team` card to the team as repositories.
fn add_projects(hub: &Hub, host: &dyn FindingHost, card: &Card) -> Result<Vec<String>, String> {
    let slug = card.team.as_deref().unwrap_or_default();
    let state = hub
        .team(slug)
        .ok_or_else(|| format!("team_unknown: {slug} is not running"))?;
    let deps = state
        .deps()
        .ok_or_else(|| format!("team_unknown: {slug} is not running"))?;
    let mut taken: Vec<String> = match deps.files.read_repos().map_err(|e| e.to_string())? {
        Some(repos) => repos
            .repos
            .iter()
            .map(|repo| repo.name.as_str().to_string())
            .collect(),
        None => {
            return Err(
                "conversion_not_available: this team is one repository, and Catervas cannot yet \
                 make a project folder of it to add another"
                    .to_string(),
            );
        }
    };
    let handle = tokio::runtime::Handle::try_current().map_err(|e| e.to_string())?;
    let mut added = Vec::new();
    for project in &card.projects {
        let folder = project.folder.as_deref().ok_or_else(|| {
            format!(
                "no_folder: {} is not on this computer; copy it here first",
                project.name
            )
        })?;
        let base = kebab(
            Path::new(folder)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&project.name),
            28,
            "repo",
        );
        let mut name = base.clone();
        let mut count = 2;
        while taken.contains(&name) {
            name = format!("{base}-{count}");
            count += 1;
        }
        let answered = handle.block_on(crate::daemon::repos::call(
            &state,
            "repo.add",
            &json!({ "name": name, "path": folder }),
        ));
        if let Err(failure) = answered {
            return Err(failure.message);
        }
        taken.push(name.clone());
        added.push(name);
    }
    let mut index = open_index(host).map_err(|failure| failure.message)?;
    for project in &card.projects {
        index
            .put_choice(&project.key, Choice::Team, host.now())
            .map_err(|e| e.to_string())?;
    }
    Ok(added)
}

/// The first lines of the README at `HEAD` of the repository at `folder`.
fn readme_head(folder: &str) -> Option<String> {
    let git = Git::open(PathBuf::from(folder));
    ["README.md", "readme.md", "README"]
        .iter()
        .find_map(|name| git.file_at("HEAD", name).ok())
        .map(|text| {
            text.lines()
                .take(README_LINES)
                .collect::<Vec<_>>()
                .join("\n")
        })
}

/// What Claude is told about the cards, with every key scrubbed from it.
fn names_prompt(host: &dyn FindingHost, index: &MachineIndex, built: &Built) -> String {
    let mut allowed: Vec<String> = catervas_roles::BUILTIN_TEAMS
        .iter()
        .map(|(slug, _)| (*slug).to_string())
        .collect();
    allowed.extend(saved_templates(host));
    let mut text = format!(
        "Give each team below a short name (at most {NAME_FROM_CLAUDE} characters), a purpose in \
         one sentence (at most {PURPOSE_FROM_CLAUDE} characters) and the template that suits it, \
         one of: {}.\nAnswer with a JSON list and nothing else: \
         [{{\"id\": \"...\", \"name\": \"...\", \"purpose\": \"...\", \"template\": \"...\"}}].\n",
        allowed.join(", ")
    );
    for card in &built.cards {
        let _ = write!(text, "\nTeam {}\n", card.id);
        let folders: Vec<String> = card
            .projects
            .iter()
            .map(|project| {
                project
                    .folder
                    .as_deref()
                    .and_then(|folder| Path::new(folder).file_name())
                    .map_or_else(
                        || project.name.clone(),
                        |name| name.to_string_lossy().into_owned(),
                    )
            })
            .collect();
        let _ = writeln!(text, "Folders: {}", folders.join(", "));
        for project in &card.projects {
            if let Some(readme) = project.folder.as_deref().and_then(readme_head) {
                let _ = writeln!(text, "README of {}:\n{readme}", project.name);
            }
            let titles = project
                .folder
                .as_deref()
                .and_then(|folder| index.session_titles(folder, SESSION_TITLES).ok())
                .unwrap_or_default();
            if !titles.is_empty() {
                let _ = writeln!(
                    text,
                    "Recent sessions in {}: {}",
                    project.name,
                    titles.join("; ")
                );
            }
        }
        if !card.jobs.is_empty() {
            let _ = writeln!(text, "Jobs: {}", card.jobs.join(", "));
        }
    }
    scrub(&text)
}

/// `proposal.names_estimate`.
fn names_estimate(hub: &Hub, host: &dyn FindingHost) -> Result<Value, Failure> {
    let index = open_index(host)?;
    let built = build(hub, host, &index)?;
    let input_tokens = u64::try_from(names_prompt(host, &index, &built).chars().count())
        .unwrap_or(u64::MAX)
        .div_ceil(4);
    let usd = compute_cost_usd(
        &Usage {
            input_tokens,
            output_tokens: MAX_OUTPUT_TOKENS,
            ..Usage::default()
        },
        "claude-haiku-4-5",
        &PRICE_TABLE,
    )
    .map_err(|_| internal(&"the price table has no price for Haiku"))?;
    Ok(json!({ "usd": usd, "input_tokens": input_tokens }))
}

/// The list of objects in what Claude said, which may be wrapped in words or a code fence.
fn names_in(said: &str) -> Option<Vec<Value>> {
    let (from, to) = (said.find('[')?, said.rfind(']')?);
    serde_json::from_str(said.get(from..=to)?).ok()
}

/// `text` cut to `limit` characters.
fn cut(text: &str, limit: usize) -> String {
    text.trim()
        .chars()
        .take(limit)
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// `proposal.names`.
fn names(hub: &Hub, host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    if params["confirmed"].as_bool() != Some(true) {
        return Err(refusal(
            "confirmation_needed",
            "asking Claude costs money; ask again with confirmed set",
        ));
    }
    let mut index = open_index(host)?;
    let built = build(hub, host, &index)?;
    let prompt = names_prompt(host, &index, &built);
    let asked = host
        .ask_names(&prompt)
        .map_err(|why| refusal("names_failed", &why))?;
    let list = names_in(&asked.result).ok_or_else(|| {
        refusal(
            "names_failed",
            "Claude Code's answer was not a list of names",
        )
    })?;
    let saved = saved_templates(host);
    let mut kept = Vec::new();
    for each in &list {
        // Only these four keys are read: Claude never makes or changes a link.
        let Some(card) = each["id"]
            .as_str()
            .and_then(|id| built.cards.iter().find(|card| card.id == id))
        else {
            continue;
        };
        let name = each["name"]
            .as_str()
            .map(|name| cut(name, NAME_FROM_CLAUDE))
            .filter(|name| !name.is_empty());
        let purpose = each["purpose"]
            .as_str()
            .map(|text| cut(text, PURPOSE_FROM_CLAUDE))
            .filter(|text| !text.is_empty());
        let template = each["template"]
            .as_str()
            .filter(|template| is_builtin(template) || saved.iter().any(|slug| slug == template))
            .map(str::to_string);
        if name.is_none() && purpose.is_none() && template.is_none() {
            continue;
        }
        index
            .put_edit(
                &card.id,
                &ProposalEdit {
                    name: name.clone(),
                    template: template.clone(),
                    purpose: purpose.clone(),
                    from_claude: true,
                },
            )
            .map_err(|e| internal(&e))?;
        let mut entry = json!({ "id": card.id, "name": name.unwrap_or_else(|| card.name.clone()) });
        if let Some(purpose) = purpose {
            entry["purpose"] = json!(purpose);
        }
        if let Some(template) = template {
            entry["template"] = json!(template);
        }
        kept.push(entry);
    }
    Ok(json!({ "cost_usd": asked.cost_usd, "cards": kept }))
}

/// `proposal.create`.
fn create(hub: &Hub, host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    if params["confirmed"].as_bool() != Some(true) {
        return Err(refusal(
            "confirmation_needed",
            "making teams writes files in your folders; ask again with confirmed set",
        ));
    }
    let mut index = open_index(host)?;
    let built = build(hub, host, &index)?;
    let shown_before: BTreeMap<String, Vec<String>> = std::mem::take(
        &mut *hub
            .finding
            .shown
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    let only: Option<BTreeSet<&str>> = params["only"]
        .as_array()
        .map(|keys| keys.iter().filter_map(Value::as_str).collect());
    let wanted: Option<BTreeSet<String>> = params["ids"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .or_else(|| (!shown_before.is_empty()).then(|| shown_before.keys().cloned().collect()));
    let (mut made_all, mut added_all, mut failed_all, mut split_all) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    if only.is_none()
        && let Some(wanted) = &wanted
    {
        for id in wanted
            .iter()
            .filter(|id| built.cards.iter().all(|card| &card.id != *id))
        {
            let projects = shown_before.get(id).cloned().unwrap_or_default();
            split_all.push(json!({ "id": id, "projects": projects }));
        }
    }
    for card in &built.cards {
        let chosen = match (&only, &wanted) {
            (Some(only), _) => card
                .projects
                .iter()
                .all(|project| only.contains(project.key.as_str())),
            (None, Some(wanted)) => wanted.contains(&card.id),
            (None, None) => true,
        };
        if !chosen {
            continue;
        }
        let kept = shown(card, built.edits.get(&card.id));
        let failed = |reason: String| json!({ "id": card.id, "name": kept.name, "reason": reason });
        match card.kind {
            CardKind::NewTeam => match make_team(hub, host, &mut index, card, &kept) {
                Ok(made) => made_all.push(json!({
                    "id": card.id,
                    "slug": made.slug,
                    "name": kept.name,
                    "root": made.root.display().to_string(),
                })),
                Err(reason) => failed_all.push(failed(reason)),
            },
            CardKind::AddToTeam => match add_projects(hub, host, card) {
                Ok(names) => added_all.push(json!({
                    "id": card.id,
                    "team": card.team.clone().unwrap_or_default(),
                    "names": names,
                })),
                Err(reason) => failed_all.push(failed(reason)),
            },
        }
    }
    Ok(json!({
        "made": made_all,
        "added": added_all,
        "failed": failed_all,
        "split": split_all,
    }))
}

/// `teams.undo`.
fn undo(hub: &Hub, host: &dyn FindingHost, params: &Value) -> Result<Value, Failure> {
    let slug = params["slug"].as_str().unwrap_or_default();
    let mut index = open_index(host)?;
    let made = index
        .made_teams()
        .map_err(|e| internal(&e))?
        .into_iter()
        .find(|team| team.slug == slug)
        .ok_or_else(|| refusal("team_unknown", &format!("Catervas made no team {slug}")))?;
    let root = PathBuf::from(&made.root);
    let started = match hub.team(slug).and_then(|state| state.deps().cloned()) {
        Some(deps) => has_started(&deps),
        None => started_at(&root, host.now()),
    };
    if started {
        return Err(refusal(
            "team_has_started",
            "this team has started working; remove it from All teams instead",
        ));
    }
    let details: Value = serde_json::from_str(&made.made).unwrap_or(Value::Null);
    let created: Vec<PathBuf> = details["created"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(PathBuf::from)
        .collect();
    let keys: Vec<String> = details["projects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    if hub.knows(slug) {
        if !hub.remove_team(&TeamSlug::new(slug)) {
            return Err(internal(&"this catervas has nothing to stop the team with"));
        }
        if !wait_for(|| !hub.knows(slug)) {
            return Err(internal(&"the team did not stop in time"));
        }
    }
    let left = host
        .remove_made(&root, &created)
        .map_err(|why| internal(&why))?;
    index.forget_made_team(slug).map_err(|e| internal(&e))?;
    for key in &keys {
        index
            .put_choice(key, Choice::Team, host.now())
            .map_err(|e| internal(&e))?;
    }
    index.clear_moved_out_of(&keys).map_err(|e| internal(&e))?;
    let mut answer = json!({ "removed": true });
    if let Some(sentence) = left {
        answer["left"] = json!(sentence);
    }
    let _ = moment;
    Ok(answer)
}
