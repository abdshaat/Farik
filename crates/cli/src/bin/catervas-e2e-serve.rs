//! `farik serve` on recorded sessions, for the browser suites: `farik-e2e-serve --port <p>
//! [--transcripts <name>,...] [--pace <ms>] [--no-keychain] [--sandbox-image <image>]
//! [--preview]`, run in a project directory, or anywhere else for the first-run wizard.
//! `--no-keychain` keeps the credential in the state folder's file alone. `--sandbox-image` runs
//! Docker's sandbox and the preview in `<image>`. `--preview` serves as a project's preview, which
//! a code-free browser at `http://localhost:<p>` is let into (step 12, D1): it implies
//! `--no-keychain`, a state folder of its own, and a new project of its own with step 08's team
//! (Mira, Ada and Theo), so that such a browser never shares a daemon with a real credential store. `--pace` holds each recorded session that long before it plays, as a live
//! one takes time, so that a page sees every state a task passes through. Once
//! the named sessions are played, each new session waits until it is aborted, which the second
//! Ctrl-C does. Built only with the `e2e` feature, so
//! the shipped `farik` has no path to replay.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use farik::ids::SystemClock;
use farik::{CliIo, Engine, Interrupts, run_cli};
use farik_core::pricing::Usage;
use farik_runtime::recorded::fixtures::{
    UsageThenWaitAdapter, accept_frk_1, ask_with_choices_frk_1, chat_answers_with_a_request,
    decide_design_plan_approves_frk_1, design_review_passes_frk_2, explore_checks_and_plans_frk_1,
    explore_plans_frk_1, implement_after_send_back_frk_1, implement_by_iris_page_frk_1,
    implement_css_frk_2, implement_finishes_frk_1, judge_frk_1_by_architect,
    plan_assigns_frk_1_to_iris, plan_assigns_frk_1_to_theo, plan_assigns_frk_2_to_theo,
    plan_breaks_down_frk_2, planning_ceremony_frk_1, planning_ceremony_frk_1_frk_2,
    refine_writes_css_task_for_theo_frk_2, refine_writes_epic_frk_1, refine_writes_high_risk_frk_1,
    refine_writes_page_task_for_iris_frk_1, refine_writes_task_for_iris_frk_1,
    refine_writes_task_for_theo_frk_1, reply_to_a_mention, retro, review,
    review_answers_the_rubric_frk_1, review_answers_the_rubric_frk_2, review_writes_note,
    tool_runner, triage_frk_1_large, triage_frk_1_small_by_pm, triage_frk_2_small_by_pm,
};
use farik_runtime::{
    RecordedAdapter, RuntimeAdapter, RuntimeError, SessionHandle, SessionSpec, Transcript,
};
use farik_store::git::fixtures::TempRepo;

/// The transcript a suite names.
fn transcript(name: &str) -> Option<Transcript> {
    match name {
        "triage_frk_1_large" => Some(triage_frk_1_large()),
        "triage_frk_1_small_by_pm" => Some(triage_frk_1_small_by_pm()),
        "ask_with_choices_frk_1" => Some(ask_with_choices_frk_1()),
        "refine_writes_task_for_theo_frk_1" => Some(refine_writes_task_for_theo_frk_1()),
        "refine_writes_task_for_iris_frk_1" => Some(refine_writes_task_for_iris_frk_1()),
        "plan_assigns_frk_1_to_iris" => Some(plan_assigns_frk_1_to_iris()),
        "explore_plans_frk_1" => Some(explore_plans_frk_1()),
        "refine_writes_epic_frk_1" => Some(refine_writes_epic_frk_1()),
        "refine_writes_high_risk_frk_1" => Some(refine_writes_high_risk_frk_1()),
        "judge_frk_1_by_architect" => Some(judge_frk_1_by_architect()),
        "plan_assigns_frk_1_to_theo" => Some(plan_assigns_frk_1_to_theo()),
        "implement_finishes_frk_1" => Some(implement_finishes_frk_1()),
        "review_writes_note" => Some(review_writes_note()),
        "implement_after_send_back_frk_1" => Some(implement_after_send_back_frk_1()),
        "planning_ceremony_frk_1" => Some(planning_ceremony_frk_1()),
        "plan_breaks_down_frk_2" => Some(plan_breaks_down_frk_2()),
        "planning_ceremony_frk_1_frk_2" => Some(planning_ceremony_frk_1_frk_2()),
        "accept_frk_1" => Some(accept_frk_1()),
        "review" => Some(review()),
        "retro" => Some(retro()),
        "reply_to_a_mention" => Some(reply_to_a_mention()),
        "refine_writes_page_task_for_iris_frk_1" => Some(refine_writes_page_task_for_iris_frk_1()),
        "explore_checks_and_plans_frk_1" => Some(explore_checks_and_plans_frk_1()),
        "decide_design_plan_approves_frk_1" => Some(decide_design_plan_approves_frk_1()),
        "implement_by_iris_page_frk_1" => Some(implement_by_iris_page_frk_1()),
        "review_answers_the_rubric_frk_1" => Some(review_answers_the_rubric_frk_1()),
        "triage_frk_2_small_by_pm" => Some(triage_frk_2_small_by_pm()),
        "refine_writes_css_task_for_theo_frk_2" => Some(refine_writes_css_task_for_theo_frk_2()),
        "plan_assigns_frk_2_to_theo" => Some(plan_assigns_frk_2_to_theo()),
        "implement_css_frk_2" => Some(implement_css_frk_2()),
        "design_review_passes_frk_2" => Some(design_review_passes_frk_2()),
        "review_answers_the_rubric_frk_2" => Some(review_answers_the_rubric_frk_2()),
        "chat_answers_with_a_request" => Some(chat_answers_with_a_request()),
        _ => None,
    }
}

fn main() -> std::process::ExitCode {
    let mut arguments = std::env::args().skip(1);
    let (mut port, mut names, mut keychain, mut pace) = (None, String::new(), true, 0);
    let (mut preview, mut sandbox_image) = (false, None);
    while let Some(flag) = arguments.next() {
        if flag == "--no-keychain" {
            keychain = false;
            continue;
        }
        if flag == "--preview" {
            preview = true;
            continue;
        }
        match (flag.as_str(), arguments.next()) {
            ("--port", Some(value)) => port = Some(value),
            ("--sandbox-image", Some(value)) => sandbox_image = Some(value),
            ("--transcripts", Some(value)) => names = value,
            ("--pace", Some(value)) => match value.parse() {
                Ok(ms) => pace = ms,
                Err(_) => return usage(),
            },
            _ => return usage(),
        }
    }
    let Some(port) = port else {
        return usage();
    };
    let mut transcripts = Vec::new();
    for name in names.split(',').filter(|name| !name.is_empty()) {
        let Some(found) = transcript(name) else {
            eprintln!("unknown transcript: {name}");
            return std::process::ExitCode::from(2);
        };
        transcripts.push(found);
    }

    let mut env: std::collections::BTreeMap<String, String> = std::env::vars_os()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        })
        .collect();
    let mut cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    // Removed when `main` returns, however the serve ends.
    let mut own = None;
    if preview {
        keychain = false;
        let project = TempRepo::new("preview");
        let state = project.path.with_extension("state");
        if let Err(error) = std::fs::create_dir_all(&state) {
            eprintln!("the preview's state folder cannot be made: {error}");
            return std::process::ExitCode::from(1);
        }
        env.insert("XDG_CONFIG_HOME".to_string(), state.display().to_string());
        if let Err(error) = recorded_team(&project, &env) {
            eprintln!("the preview's project cannot be made: {error}");
            return std::process::ExitCode::from(1);
        }
        cwd.clone_from(&project.path);
        own = Some(Removed(project, state));
    }

    let mut io = CliIo::new(
        cwd,
        Box::new(std::io::stdout()),
        Box::new(std::io::stderr()),
        Arc::new(SystemClock),
    );
    io.env = env;
    io.admit_local_preview = preview;
    io.sandbox_image = sandbox_image;
    io.engine = Engine::Given(Arc::new(move |daemon| {
        let adapter: Arc<dyn RuntimeAdapter> = Arc::new(ThenWaits {
            recorded: RecordedAdapter::with_tools(transcripts.clone(), tool_runner(daemon)),
            waits: UsageThenWaitAdapter::waiting(Usage::default()),
            pace: Duration::from_millis(pace),
        });
        adapter
    }));
    io.interrupts = Interrupts::CtrlC;
    io.credential_stores = farik::system_credential_stores(&io.env, keychain);
    let arguments = ["farik", "serve", "--no-open", "--port", &port].map(String::from);
    let code = run_cli(&arguments, &mut io);
    drop(io);
    drop(own);
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}

/// The preview's own project and state folder, both removed when it is dropped.
struct Removed(#[allow(dead_code)] TempRepo, PathBuf);

impl Drop for Removed {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.1);
    }
}

/// Makes `project` a Farik project, as `startServe` in the browser suites does for step 08's
/// journeys: `farik init`, sandboxing off, and Mira (Product Manager), Ada (Architect) and Theo
/// (Developer) as the team, which does not plan its work in sprints.
fn recorded_team(
    project: &TempRepo,
    env: &std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    let mut io = CliIo::new(
        project.path.clone(),
        Box::new(std::io::sink()),
        Box::new(std::io::stderr()),
        Arc::new(SystemClock),
    );
    io.env.clone_from(env);
    if run_cli(&["farik".to_string(), "init".to_string()], &mut io) != 0 {
        return Err("farik init failed".to_string());
    }
    project.write(".farik/local/settings.json", r#"{"sandbox":"none"}"#);
    let path = project.path.join(".farik/team.yaml");
    let yaml = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let (Some(start), Some(end)) = (yaml.find("agents:\n"), yaml.find("\nbudgets:")) else {
        return Err("team.yaml has no agents or budgets".to_string());
    };
    let agent = |id: &str, name: &str, role: &str| {
        format!(
            "- display_name: {name}\n  id: {id}\n  model:\n    effort: high\n    id: claude-opus-5-5\n  persona: {name}.\n  role: {role}\n  status: active\n"
        )
    };
    let agents = [
        agent("mira", "Mira", "product_manager"),
        agent("ada", "Ada", "architect"),
        agent("theo", "Theo", "software_developer"),
    ]
    .concat();
    let team = format!("{}agents:\n{agents}{}", &yaml[..start], &yaml[end + 1..])
        .replace("plan_in_sprints: true", "plan_in_sprints: false");
    std::fs::write(&path, team).map_err(|error| error.to_string())
}

/// The named sessions in order, then sessions that wait until they are aborted, as a live one
/// still at work would, so that the server keeps answering once a journey's transcripts are played.
struct ThenWaits {
    recorded: RecordedAdapter,
    waits: UsageThenWaitAdapter,
    pace: Duration,
}

impl RuntimeAdapter for ThenWaits {
    fn start_session(&self, spec: SessionSpec) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        if self.recorded.transcripts_left() == 0 {
            self.waits.start_session(spec)
        } else {
            // The daemon's runtime has other workers, so its queries go on being answered.
            std::thread::sleep(self.pace);
            self.recorded.start_session(spec)
        }
    }

    fn resume(
        &self,
        session_id: &str,
        prompt: &str,
    ) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        self.recorded.resume(session_id, prompt)
    }
}

fn usage() -> std::process::ExitCode {
    eprintln!(
        "usage: farik-e2e-serve --port <p> [--transcripts <name>,...] [--pace <ms>] [--no-keychain] \
         [--sandbox-image <image>] [--preview]"
    );
    std::process::ExitCode::from(2)
}
