//! What `catervas serve` does for the first-run wizard before there is a project (`docs/SPEC.md`
//! 4.1): the CLI's side of runtime's `SetupHost`, which opens or makes the project, keeps the
//! credential, and says on a watch which project the wizard chose.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use catervas_protocol::clock::Clock;
use catervas_protocol::event::EventBody;
use catervas_runtime::claude::CredentialKind;
use catervas_runtime::computer::on_path;
use catervas_runtime::connectors::ConnectorSecrets;
use catervas_runtime::credential::{
    CredentialError, CredentialStore, Source, credential_of_kind, load_credential, save_credential,
};
use catervas_runtime::daemon::{SETUP_PENDING, SetupError, SetupHost};
use catervas_store::files::{LocalSettings, Sandbox};
use catervas_store::requests::{
    RequestError, file_request, placeholder_budget_usd, request_from_brief,
};
use tokio::sync::watch;

use crate::carry::carry;
use crate::project::{Project, open_project, repository_root};
use crate::start::try_lock;
use crate::state::{make_state_dir, state_dir};
use crate::{HUMAN, init};

/// What `open` and `create` refuse a project another process drives with.
const BUSY: &str = "another catervas is already running this project";
/// What `open` and `create` refuse with before a credential is kept.
const NO_ACCOUNT: &str = "connect your AI account first";
/// What `open` refuses a folder that already has a team with, while a project is being left, unless
/// the user chose to replace it. The page recognises the prefix `has_team`.
pub(crate) const HAS_TEAM: &str = "has_team: that folder already has a Catervas team";
/// What `open` refuses a folder outside home with.
const OUTSIDE_HOME: &str = "that folder is outside your home folder";
/// What `open` refuses home itself with: Catervas's settings folder, `~/.config/catervas`, would be in
/// the project, where a commit could write it (re-review 2 m1).
const HOME_ITSELF: &str = "your home folder itself cannot be a project; choose a folder inside it";

/// The CLI's setup host.
pub(crate) struct CliHost {
    /// The environment `catervas serve` was given.
    pub(crate) env: BTreeMap<String, String>,
    /// Home: `HOME`, else where `catervas serve` was run.
    pub(crate) home: PathBuf,
    /// The time the events it records are stamped with.
    pub(crate) clock: Arc<dyn Clock + Send + Sync>,
    /// Where the credential is kept.
    pub(crate) stores: Vec<Arc<dyn CredentialStore>>,
    /// Where the chosen project is said.
    pub(crate) chosen: watch::Sender<Option<PathBuf>>,
    /// A project already chosen, which has no credential yet: `connect` takes it on.
    pub(crate) waiting: Option<PathBuf>,
    /// The root the team just left from the browser: opening it again stays on it as it is.
    pub(crate) leaving: Option<PathBuf>,
    /// Where the connector keys are kept, for the team taken to another project.
    pub(crate) secrets: Arc<dyn ConnectorSecrets>,
}

impl CliHost {
    /// `path`, relative to home, followed through its links, or the refusal of one outside home.
    fn inside_home(&self, path: &str) -> Result<PathBuf, SetupError> {
        let home = std::fs::canonicalize(&self.home)
            .map_err(|error| failed(format!("your home folder cannot be read: {error}")))?;
        let folder = std::fs::canonicalize(home.join(path))
            .map_err(|_| refused("that folder is not there"))?;
        if folder.starts_with(&home) {
            Ok(folder)
        } else {
            Err(refused(OUTSIDE_HOME))
        }
    }

    /// Refuses unless a credential is kept.
    fn has_account(&self) -> Result<(), SetupError> {
        match load_credential(&self.env, &self.stores) {
            Some(_) => Ok(()),
            None => Err(refused(NO_ACCOUNT)),
        }
    }

    /// Makes `root` a Catervas project when it is not one: `init`, then the old team carried while a
    /// project is being left, else the team paused and the marker. Answers the project, and
    /// whether it was made.
    fn taken_on(&self, root: &Path, no_sandbox: bool) -> Result<(Project, bool), SetupError> {
        let now = self.clock.now();
        let made = !root.join(".catervas/team.yaml").exists();
        if made {
            init::init(root, now).map_err(failed)?;
        }
        let mut project = open_project(root, now).map_err(failed)?;
        if let (true, Some(old)) = (made, &self.leaving) {
            let state = state_dir(&self.env);
            carry(old, &project, &*self.secrets, state.as_deref(), now).map_err(failed)?;
            project = open_project(root, now).map_err(failed)?;
        } else if made {
            let by = serde_json::from_value(serde_json::json!({ "by": HUMAN }))
                .map_err(|error| failed(error.to_string()))?;
            let event = project
                .event(EventBody::TeamPaused(by), now, None)
                .map_err(failed)?;
            project.append(&event).map_err(failed)?;
            std::fs::write(root.join(SETUP_PENDING), "")
                .map_err(|error| failed(format!("{SETUP_PENDING} cannot be written: {error}")))?;
        }
        if no_sandbox {
            project
                .files
                .write_settings(&LocalSettings {
                    sandbox: Sandbox::None,
                })
                .map_err(|error| failed(error.to_string()))?;
        }
        Ok((project, made))
    }

    /// Removes the `.catervas/` of the project at `root`, a canonical repository root inside home, for
    /// a team taken on over it: a `.catervas` that is a link goes as a link, never followed to what it
    /// points at. Then forgets the worktrees it held.
    fn clear(&self, root: &Path) -> Result<(), SetupError> {
        let catervas = root.join(".catervas");
        let gone = if std::fs::symlink_metadata(&catervas)
            .map_err(|error| failed(format!("{} cannot be read: {error}", catervas.display())))?
            .file_type()
            .is_symlink()
        {
            std::fs::remove_file(&catervas)
        } else {
            std::fs::remove_dir_all(&catervas)
        };
        gone.map_err(|error| failed(format!("{} cannot be removed: {error}", catervas.display())))?;
        self.git(root, &["worktree", "prune"])
    }

    /// Says `root` is the chosen project, and answers it.
    fn choose(&self, root: PathBuf) -> PathBuf {
        self.chosen.send_replace(Some(root.clone()));
        root
    }

    /// Runs `git <args>` in `directory`, as catervas, with `git` from the environment's `PATH`.
    fn git(&self, directory: &Path, args: &[&str]) -> Result<(), SetupError> {
        let program = on_path("git", &self.env).ok_or_else(|| refused("git is not installed"))?;
        let output = std::process::Command::new(program)
            .args(catervas_store::git::CATERVAS_IDENTITY)
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(directory)
            .env_clear()
            .envs(&self.env)
            .output()
            .map_err(|error| failed(format!("git could not be run: {error}")))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(failed(format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }
}

impl SetupHost for CliHost {
    fn open(&self, path: &str, no_sandbox: bool, replace: bool) -> Result<PathBuf, SetupError> {
        // Stay on: the old root, as it is; an old root outside home can be stayed on too.
        if let Some(old) = self
            .leaving
            .as_ref()
            .and_then(|old| std::fs::canonicalize(old).ok())
            && std::fs::canonicalize(self.home.join(path)).is_ok_and(|asked| asked == old)
        {
            match try_lock(&old).map_err(failed)? {
                Some(lock) => drop(lock),
                None => return Err(refused(BUSY)),
            }
            self.has_account()?;
            return Ok(self.choose(old));
        }
        let folder = self.inside_home(path)?;
        let root = repository_root(&folder).map_err(|_| {
            refused("that folder is not a git project; choose another, or start a new project")
        })?;
        let root = std::fs::canonicalize(&root).map_err(|error| failed(error.to_string()))?;
        if root != folder {
            return Err(refused(&format!(
                "that folder is inside a git project; choose {} instead",
                root.display()
            )));
        }
        if std::fs::canonicalize(&self.home).is_ok_and(|home| home == root) {
            return Err(refused(HOME_ITSELF));
        }
        // Checked before anything is changed; the lock is given back for the driver to take.
        match try_lock(&root).map_err(failed)? {
            Some(lock) => drop(lock),
            None => return Err(refused(BUSY)),
        }
        self.has_account()?;
        // Only while a project is being left: a folder with a team is the user's to replace.
        if self.leaving.is_some() {
            if root.join(".catervas/team.yaml").exists() {
                if !replace {
                    return Err(refused(HAS_TEAM));
                }
                self.clear(&root)?;
            } else if std::fs::symlink_metadata(root.join(".catervas")).is_ok() {
                // A leftover with no team: none of it may carry context into the new team.
                self.clear(&root)?;
            }
        }
        self.taken_on(&root, no_sandbox)?;
        Ok(self.choose(root))
    }

    fn create(
        &self,
        parent: &str,
        name: &str,
        description: &str,
        no_sandbox: bool,
    ) -> Result<PathBuf, SetupError> {
        let named = (1..=64).contains(&name.len())
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !named {
            return Err(refused(
                "a project's name is lowercase letters, digits, and -, up to 64 characters",
            ));
        }
        let length = description.trim().chars().count();
        if length < 20 {
            return Err(refused(
                "say a little more about the project: at least 20 characters",
            ));
        }
        if length > 2000 {
            return Err(refused(
                "that description is too long: at most 2000 characters",
            ));
        }
        // The request is built once before anything is made, so that one it refuses makes nothing.
        request_from_brief(name, description, 0.0).map_err(|why| refused(&why))?;
        let parent = self.inside_home(parent)?;
        let root = parent.join(name);
        if root.exists() {
            return Err(refused("a folder with that name is already there"));
        }
        self.has_account()?;
        on_path("git", &self.env).ok_or_else(|| refused("git is not installed"))?;

        std::fs::create_dir(&root)
            .map_err(|error| failed(format!("{} cannot be made: {error}", root.display())))?;
        self.git(&root, &["init", "-b", "main"])?;
        std::fs::write(
            root.join("README.md"),
            format!("# {name}\n\n{description}\n"),
        )
        .map_err(|error| failed(format!("README.md cannot be written: {error}")))?;
        self.git(&root, &["add", "README.md"])?;
        self.git(&root, &["commit", "-m", "Start the project"])?;

        let (project, _) = self.taken_on(&root, no_sandbox)?;
        let request = request_from_brief(
            name,
            description,
            placeholder_budget_usd(&project.team.rules()),
        )
        .map_err(|why| refused(&why))?;
        file_request(
            &project.files,
            &project.log,
            request,
            HUMAN,
            None,
            self.clock.now(),
            &crate::contract::event_ids(&project),
            None,
        )
        .map_err(|error| match error {
            RequestError::Refused { reason } => failed(format!("the request {reason}")),
            other => failed(other.to_string()),
        })?;
        Ok(self.choose(root))
    }

    fn connect(&self, kind: CredentialKind, secret: &str) -> Result<(Source, bool), SetupError> {
        let credential = credential_of_kind(kind, secret).map_err(|why| refused(&why))?;
        if let Some(directory) = state_dir(&self.env) {
            make_state_dir(&directory).map_err(failed)?;
        }
        let source = save_credential(&credential, &self.stores).map_err(|error| match error {
            CredentialError::Failed(why) => refused(&why),
            CredentialError::NoKeychain => {
                refused("this computer has no keychain and no folder to keep the key in")
            }
        })?;
        if let Some(root) = &self.waiting {
            self.choose(root.clone());
        }
        Ok((source, self.waiting.is_some()))
    }

    fn home(&self) -> PathBuf {
        self.home.clone()
    }

    fn env(&self) -> BTreeMap<String, String> {
        self.env.clone()
    }

    fn account(&self) -> Option<(CredentialKind, Source)> {
        load_credential(&self.env, &self.stores)
            .map(|(credential, source)| (credential.kind(), source))
    }
}

fn refused(sentence: &str) -> SetupError {
    SetupError::Refused(sentence.to_string())
}

fn failed(why: String) -> SetupError {
    SetupError::Failed(why)
}

#[cfg(all(test, unix))]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use catervas_core::criteria::fixtures::a_criteria_library_wire;
    use catervas_core::team::fixtures::an_agent_wire;
    use catervas_core::team::{AgentId, carried_team, validate_team};
    use catervas_protocol::clock::FixedClock;
    use catervas_protocol::event::{EventBody, EventKind};
    use catervas_runtime::claude::Secret;
    use catervas_runtime::connectors::{
        ConnectorEntry, ConnectorSecrets, MemoryConnectorSecrets, SecretAt, local_project_id,
    };
    use catervas_runtime::daemon::{SetupError, SetupHost};
    use catervas_store::EventQuery;
    use catervas_store::files::{LocalSettings, ProjectFiles, Sandbox};
    use catervas_store::git::fixtures::TempRepo;
    use chrono::Utc;
    use serde_json::{Value, json};

    use super::{BUSY, CliHost, HAS_TEAM, NO_ACCOUNT};
    use crate::project::open_project;
    use crate::start::try_lock;
    use crate::{HUMAN, init};

    /// Home is the temporary folder, where `TempRepo` makes its repositories.
    fn home() -> PathBuf {
        std::env::temp_dir()
    }

    fn scratch(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("catervas-setup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a scratch folder");
        path
    }

    /// What `open` is asked for a repository: its name under home.
    fn named(repository: &TempRepo) -> String {
        repository
            .path
            .file_name()
            .expect("a name")
            .to_string_lossy()
            .to_string()
    }

    struct World {
        state: PathBuf,
        secrets: Arc<MemoryConnectorSecrets>,
        env: BTreeMap<String, String>,
    }

    impl World {
        fn new(name: &str) -> Self {
            let state = scratch(name);
            let env = BTreeMap::from([
                (
                    "PATH".to_string(),
                    std::env::var("PATH").unwrap_or_default(),
                ),
                ("HOME".to_string(), home().display().to_string()),
                ("XDG_CONFIG_HOME".to_string(), state.display().to_string()),
                (
                    "ANTHROPIC_API_KEY".to_string(),
                    "sk-ant-api03-test".to_string(),
                ),
            ]);
            Self {
                state: state.join("catervas"),
                secrets: Arc::new(MemoryConnectorSecrets::default()),
                env,
            }
        }

        /// A host that is leaving `old`, when it is.
        fn host(&self, leaving: Option<&Path>) -> CliHost {
            CliHost {
                env: self.env.clone(),
                home: home(),
                clock: Arc::new(FixedClock::new(Utc::now())),
                stores: Vec::new(),
                chosen: tokio::sync::watch::channel(None).0,
                waiting: None,
                leaving: leaving.map(Path::to_path_buf),
                secrets: self.secrets.clone(),
            }
        }

        fn at(&self, root: &Path, agent: &str, server: &str) -> SecretAt {
            std::fs::create_dir_all(&self.state).expect("a state folder");
            SecretAt {
                project_id: local_project_id(&self.state, root).expect("an id"),
                agent_id: agent.to_string(),
                server: server.to_string(),
            }
        }

        fn keep(&self, root: &Path, agent: &str, server: &str, value: &str) {
            let at = self.at(root, agent, server);
            self.secrets.save(&at, &entry(value)).expect("kept");
        }

        fn kept(&self, root: &Path, agent: &str, server: &str) -> Option<ConnectorEntry> {
            self.secrets
                .load(&self.at(root, agent, server))
                .expect("read")
        }
    }

    fn entry(value: &str) -> ConnectorEntry {
        ConnectorEntry {
            spec_sha256: "a".repeat(64),
            keys: BTreeMap::from([("API_KEY".to_string(), Secret::new(value.to_string()))]),
            oauth: None,
        }
    }

    fn server(name: &str) -> Value {
        json!({
            "name": name, "source": "custom", "transport": "stdio",
            "command": "server", "tools": { "search": "network" }
        })
    }

    fn agent(id: &str, role: &str, status: &str) -> Value {
        let mut wire = an_agent_wire(id, role);
        wire["status"] = json!(status);
        wire
    }

    /// A project made by `init`, its team changed to the wire `change` leaves.
    fn a_project(name: &str, change: impl FnOnce(&mut Value)) -> TempRepo {
        let repository = TempRepo::new(name);
        init::init(&repository.path, Utc::now()).expect("init");
        let files = ProjectFiles::open(repository.path.clone());
        let mut wire = serde_json::to_value(files.read_team().expect("a team")).expect("wire");
        wire["agents"] = json!([
            agent("pm", "product_manager", "active"),
            agent("theo", "software_developer", "active"),
            agent("ada", "software_developer", "paused"),
            agent("iris", "software_developer", "retired"),
        ]);
        change(&mut wire);
        files
            .write_team(&validate_team(&wire).expect("a team"))
            .expect("written");
        repository
    }

    fn files_of(repository: &TempRepo) -> ProjectFiles {
        ProjectFiles::open(repository.path.clone())
    }

    fn write(root: &Path, path: &str, bytes: &[u8]) {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("a folder")).expect("made");
        std::fs::write(file, bytes).expect("written");
    }

    fn read(root: &Path, path: &str) -> Vec<u8> {
        std::fs::read(root.join(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
    }

    fn events(root: &Path, kind: EventKind) -> Vec<EventBody> {
        open_project(root, Utc::now())
            .expect("a project")
            .log
            .read(&EventQuery {
                kinds: vec![kind],
                ..EventQuery::default()
            })
            .expect("events")
            .into_iter()
            .map(|event| event.body)
            .collect()
    }

    fn event_count(root: &Path) -> usize {
        open_project(root, Utc::now())
            .expect("a project")
            .log
            .read(&EventQuery::default())
            .expect("events")
            .len()
    }

    fn root_of(path: &Path) -> PathBuf {
        path.canonicalize().expect("a root")
    }

    fn pin(name: &str) -> Value {
        json!({ "name": name, "sha256": "b".repeat(64) })
    }

    fn carried_of(old: &TempRepo) -> catervas_core::team::Team {
        carried_team(&files_of(old).read_team().expect("old team")).expect("carried")
    }

    const AVATAR: &str = ".catervas/team/avatars/theo.png";

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn leaving_carries_the_team_to_a_fresh_folder() {
        let world = World::new("carries");
        let old = a_project("carry-old", |wire| {
            wire["skills"] = json!([pin("team-skill")]);
            wire["agents"][1]["skills"] = json!([pin("theo-skill")]);
            wire["agents"][1]["avatar"] = json!(AVATAR);
        });
        write(&old.path, AVATAR, b"\x89PNG theo");
        write(
            &old.path,
            ".catervas/skills/team-skill/SKILL.md",
            b"team skill",
        );
        write(
            &old.path,
            ".catervas/agents/theo/skills/theo-skill/SKILL.md",
            b"theo skill",
        );
        let mut library = a_criteria_library_wire();
        library["criteria"][0]["name"] = json!("only-in-the-old-project");
        files_of(&old)
            .write_criteria(&catervas_core::criteria::validate_criteria(&library).expect("library"))
            .expect("written");
        files_of(&old)
            .write_settings(&LocalSettings {
                sandbox: Sandbox::None,
            })
            .expect("settings");
        let theo = AgentId::try_from("theo").expect("an id");
        files_of(&old)
            .write_memory(&theo, "theo knows the old project\n")
            .expect("memory");
        let fresh = TempRepo::new("carry-new");

        let host = world.host(Some(&root_of(&old.path)));
        let chosen = host.open(&named(&fresh), false, false).expect("taken on");

        let new = open_project(&chosen, Utc::now()).expect("a project");
        assert_eq!(new.team, carried_of(&old));
        let ids: Vec<&str> = new.team.agents.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["pm", "theo", "ada"]);
        assert_eq!(read(&chosen, AVATAR), read(&old.path, AVATAR));
        for skill in [
            ".catervas/skills/team-skill/SKILL.md",
            ".catervas/agents/theo/skills/theo-skill/SKILL.md",
        ] {
            assert_eq!(read(&chosen, skill), read(&old.path, skill), "{skill}");
        }
        assert_eq!(
            files_of(&fresh).read_settings().expect("settings").sandbox,
            Sandbox::None
        );
        let criteria = new.files.read_criteria().expect("criteria");
        assert!(
            !serde_json::to_string(&criteria)
                .expect("json")
                .contains("only-in-the-old-project")
        );
        assert_eq!(new.files.read_memory(&theo).expect("memory"), "");
        assert!(!chosen.join(".catervas/local/setup-pending").exists());
        assert!(events(&chosen, EventKind::TeamPaused).is_empty());
        let EventBody::TeamUpdated(last) = events(&chosen, EventKind::TeamUpdated)
            .pop()
            .expect("a team.updated")
        else {
            panic!("a team.updated");
        };
        assert_eq!(last.agent_ids, ["pm", "theo", "ada"]);
        assert_eq!(last.updated_by, HUMAN);
        assert_eq!(
            events(&chosen, EventKind::CriteriaUpdated).len(),
            1,
            "only init's"
        );
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn leaving_leaves_links_in_carried_folders_behind() {
        let world = World::new("links");
        let old = a_project("links-old", |wire| {
            wire["skills"] = json!([pin("team-skill")]);
        });
        let outside = TempRepo::new("links-outside");
        write(&outside.path, "secret.txt", b"outside secret");
        let secret = outside.path.join("secret.txt");
        write(&old.path, ".catervas/team/avatars/real.png", b"real");
        write(&old.path, ".catervas/skills/team-skill/SKILL.md", b"skill");
        let links = [
            ".catervas/team/avatars/link.png",
            ".catervas/skills/team-skill/link.md",
        ];
        for link in links {
            std::os::unix::fs::symlink(&secret, old.path.join(link)).expect("a link");
        }
        let fresh = TempRepo::new("links-new");

        let host = world.host(Some(&root_of(&old.path)));
        let chosen = host.open(&named(&fresh), false, false).expect("taken on");

        assert_eq!(read(&chosen, ".catervas/team/avatars/real.png"), b"real");
        assert_eq!(
            read(&chosen, ".catervas/skills/team-skill/SKILL.md"),
            b"skill"
        );
        for link in links {
            assert!(
                std::fs::symlink_metadata(chosen.join(link)).is_err(),
                "{link} was carried"
            );
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn leaving_clears_a_leftover_catervas_folder_with_no_team() {
        let world = World::new("leftover");
        let old = a_project("leftover-old", |_| {});
        let target = TempRepo::new("leftover-new");
        let theo = AgentId::try_from("theo").expect("an id");
        write(
            &target.path,
            ".catervas/agents/theo/memory.md",
            b"stale context\n",
        );
        assert!(!target.path.join(".catervas/team.yaml").exists());

        let host = world.host(Some(&root_of(&old.path)));
        let chosen = host.open(&named(&target), false, false).expect("taken on");

        let new = open_project(&chosen, Utc::now()).expect("a project");
        assert_eq!(new.files.read_memory(&theo).expect("memory"), "");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn not_leaving_keeps_a_leftover_catervas_folder_with_no_team() {
        let world = World::new("leftover-kept");
        let target = TempRepo::new("leftover-kept-new");
        write(&target.path, ".catervas/agents/theo/memory.md", b"kept\n");

        let host = world.host(None);
        host.open(&named(&target), false, false).expect("taken on");

        assert_eq!(
            read(&target.path, ".catervas/agents/theo/memory.md"),
            b"kept\n"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn leaving_copies_the_kept_agents_keys_and_says_so() {
        let world = World::new("keys");
        let old = a_project("keys-old", |wire| {
            wire["agents"][1]["mcp_servers"] = json!([server("github")]);
            wire["agents"][3]["mcp_servers"] = json!([server("notion")]);
            wire["agents"][2] = agent("proc", "procurement_specialist", "active");
        });
        let root = root_of(&old.path);
        world.keep(&root, "theo", "github", "ghp-theo");
        world.keep(&root, "iris", "notion", "iris-notion");
        world.keep(&root, "", "procurement", "mail-password");
        write(
            &old.path,
            ".catervas/local/procurement/mail/mailbox.json",
            br#"{"address":"buy@shop.test"}"#,
        );
        write(
            &old.path,
            ".catervas/local/procurement/mail/ledger.json",
            br#"{"last_uid":42}"#,
        );
        let fresh = TempRepo::new("keys-new");

        let chosen = world
            .host(Some(&root))
            .open(&named(&fresh), false, false)
            .expect("taken on");

        assert!(world.kept(&chosen, "theo", "github").is_some());
        assert!(world.kept(&chosen, "iris", "notion").is_none());
        assert!(world.kept(&chosen, "", "procurement").is_some());
        assert!(world.kept(&root, "theo", "github").is_some(), "old stays");
        let note: Value =
            serde_json::from_slice(&read(&chosen, ".catervas/local/keys-copied.json"))
                .expect("json");
        assert_eq!(note["from"], json!(root.display().to_string()));
        assert_eq!(note["keys"].as_array().expect("keys").len(), 2, "{note}");
        for file in ["mailbox.json", "ledger.json"] {
            let path = format!(".catervas/local/procurement/mail/{file}");
            assert_eq!(read(&chosen, &path), read(&root, &path), "{file}");
        }
        let new = open_project(&chosen, Utc::now()).expect("a project");
        let mail = catervas_store::seller_mail::seller_mail(&new.log).expect("mail");
        assert_eq!(mail.address.as_deref(), Some("buy@shop.test"));
        let connected = new
            .log
            .read(&EventQuery {
                kinds: vec![EventKind::MailboxConnected],
                ..EventQuery::default()
            })
            .expect("events");
        assert_eq!(connected.len(), 1);
        assert!(connected[0].envelope.ids.agent_id.is_none());
        assert!(connected[0].envelope.ids.session_id.is_none());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn leaving_without_a_procurement_specialist_moves_no_mailbox() {
        let world = World::new("no-mailbox");
        let old = a_project("no-mailbox-old", |wire| {
            wire["agents"][3] = agent("proc", "procurement_specialist", "retired");
        });
        let root = root_of(&old.path);
        world.keep(&root, "", "procurement", "mail-password");
        write(
            &old.path,
            ".catervas/local/procurement/mail/mailbox.json",
            br#"{"address":"buy@shop.test"}"#,
        );
        let fresh = TempRepo::new("no-mailbox-new");

        let chosen = world
            .host(Some(&root))
            .open(&named(&fresh), false, false)
            .expect("taken on");

        assert!(world.kept(&chosen, "", "procurement").is_none());
        assert!(!chosen.join(".catervas/local/procurement/mail").exists());
        assert!(events(&chosen, EventKind::MailboxConnected).is_empty());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn leaving_writes_no_keys_file_when_no_key_was_copied() {
        let world = World::new("no-keys");
        let old = a_project("no-keys-old", |_| {});
        let fresh = TempRepo::new("no-keys-new");

        let chosen = world
            .host(Some(&root_of(&old.path)))
            .open(&named(&fresh), false, false)
            .expect("taken on");

        assert!(!chosen.join(".catervas/local/keys-copied.json").exists());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn leaving_refuses_a_folder_with_a_team_unless_replaced() {
        let world = World::new("has-team");
        let old = a_project("has-team-old", |_| {});
        let target = a_project("has-team-new", |wire| {
            wire["agents"][1] = agent("someone-else", "software_developer", "active");
        });
        let theirs = AgentId::try_from("someone-else").expect("an id");
        files_of(&target)
            .write_memory(&theirs, "their note\n")
            .expect("memory");
        let before = read(&target.path, ".catervas/team.yaml");
        let host = world.host(Some(&root_of(&old.path)));

        let refused = host.open(&named(&target), false, false);

        assert_eq!(refused, Err(SetupError::Refused(HAS_TEAM.to_string())));
        assert_eq!(read(&target.path, ".catervas/team.yaml"), before);
        let chosen = host.open(&named(&target), false, true).expect("replaced");
        let new = open_project(&chosen, Utc::now()).expect("a project");
        assert_eq!(new.team, carried_of(&old));
        assert_eq!(new.files.read_memory(&theirs).expect("memory"), "");
        let status = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&target.path)
            .output()
            .expect("git");
        for line in String::from_utf8_lossy(&status.stdout).lines() {
            assert!(line.ends_with(".catervas/"), "outside .catervas/: {line}");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn leaving_reopens_the_old_folder_as_it_is() {
        let world = World::new("reopen");
        let old = a_project("reopen-old", |_| {});
        let root = root_of(&old.path);
        let (team, count) = (read(&root, ".catervas/team.yaml"), event_count(&root));

        let chosen = world
            .host(Some(&root))
            .open(&root.display().to_string(), false, false)
            .expect("stays");

        assert_eq!(chosen, root);
        assert_eq!(read(&root, ".catervas/team.yaml"), team);
        assert_eq!(event_count(&root), count);
        assert!(!root.join(".catervas/local/keys-copied.json").exists());
    }

    #[test]
    fn staying_on_still_refuses_a_project_another_process_runs() {
        let world = World::new("stay-busy");
        let old = scratch("stay-busy-old");
        let _held = try_lock(&old).expect("lock").expect("held");

        let refused = world
            .host(Some(&old))
            .open(&old.display().to_string(), false, false);

        assert_eq!(refused, Err(SetupError::Refused(BUSY.to_string())));
    }

    #[test]
    fn staying_on_still_needs_an_account() {
        let mut world = World::new("stay-no-account");
        world.env.remove("ANTHROPIC_API_KEY");
        let old = scratch("stay-no-account-old");

        let refused = world
            .host(Some(&old))
            .open(&old.display().to_string(), false, false);

        assert_eq!(refused, Err(SetupError::Refused(NO_ACCOUNT.to_string())));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn leaving_creates_a_project_with_the_carried_team() {
        let world = World::new("create");
        let old = a_project("create-old", |_| {});
        let parent = format!("catervas-setup-create-parent-{}", std::process::id());
        std::fs::create_dir_all(home().join(&parent)).expect("a parent");

        let chosen = world
            .host(Some(&root_of(&old.path)))
            .create(
                &parent,
                "bakery",
                "A bakery's web shop, with orders taken online.",
                false,
            )
            .expect("created");

        let new = open_project(&chosen, Utc::now()).expect("a project");
        assert_eq!(new.team, carried_of(&old));
        assert_eq!(new.files.list_contracts().expect("contracts").len(), 1);
        assert!(!chosen.join(".catervas/local/setup-pending").exists());
        assert!(events(&chosen, EventKind::TeamPaused).is_empty());
        std::fs::remove_dir_all(home().join(&parent)).expect("cleaned");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn replacing_removes_a_linked_catervas_folder_not_what_it_points_at() {
        let world = World::new("linked");
        let old = a_project("linked-old", |_| {});
        let target = TempRepo::new("linked-new");
        let outside = scratch("linked-outside");
        std::fs::write(outside.join("team.yaml"), "name: elsewhere\n").expect("written");
        std::fs::write(outside.join("keep"), "keep me").expect("written");
        std::os::unix::fs::symlink(&outside, target.path.join(".catervas")).expect("a link");

        let chosen = world
            .host(Some(&root_of(&old.path)))
            .open(&named(&target), false, true)
            .expect("replaced");

        assert_eq!(
            std::fs::read(outside.join("keep")).expect("kept"),
            b"keep me"
        );
        assert!(outside.join("team.yaml").exists());
        let meta = std::fs::symlink_metadata(chosen.join(".catervas")).expect("a folder");
        assert!(meta.is_dir() && !meta.file_type().is_symlink());
        let new = open_project(&chosen, Utc::now()).expect("a project");
        assert_eq!(new.team, carried_of(&old));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn opens_a_folder_with_a_team_as_before_when_not_leaving() {
        let world = World::new("not-leaving");
        let target = a_project("not-leaving-new", |_| {});
        let before = read(&target.path, ".catervas/team.yaml");

        let chosen = world
            .host(None)
            .open(&named(&target), false, false)
            .expect("opened");

        assert_eq!(chosen, root_of(&target.path));
        assert_eq!(read(&target.path, ".catervas/team.yaml"), before);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn leaving_with_a_mailbox_that_cannot_be_recorded_fails_and_leaves_no_mailbox() {
        let world = World::new("bad-mailbox");
        let old = a_project("bad-mailbox-old", |wire| {
            wire["agents"][2] = agent("proc", "procurement_specialist", "active");
        });
        let root = root_of(&old.path);
        world.keep(&root, "", "procurement", "mail-password");
        write(
            &old.path,
            ".catervas/local/procurement/mail/mailbox.json",
            br#"{"address":"not an address"}"#,
        );
        let fresh = TempRepo::new("bad-mailbox-new");

        let failed = world
            .host(Some(&root))
            .open(&named(&fresh), false, false)
            .expect_err("the mailbox cannot be carried");

        let SetupError::Failed(why) = failed else {
            panic!("a failure, not {failed:?}");
        };
        assert!(why.starts_with("the mailbox could not be carried"), "{why}");
        assert!(!fresh.path.join(".catervas/local/procurement/mail").exists());
    }
}
