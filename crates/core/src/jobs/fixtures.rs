//! Recorded and hand-written inputs of the jobs readers, for tests in this crate and in others.
//! Secrets are spelled `SECRET-<where>` and prompts `NEVER-KEPT-TEXT`.

/// `.claude/scheduled_tasks.json`: a recurring, a one-time and a permanent task, and a task with
/// nothing but an id.
pub const SCHEDULED_TASKS: &str = include_str!("fixtures/scheduled_tasks.json");

/// A background job's `state.json` (an undocumented file, read fail-soft).
pub const BACKGROUND_JOB: &str = include_str!("fixtures/background_job_state.json");

/// A workflow named `nightly-sync` with two schedules and a manual trigger.
pub const GITHUB_WORKFLOW: &str = include_str!("fixtures/github_workflow.yml");

/// A workflow that runs on `push` alone.
pub const GITHUB_WORKFLOW_UNSCHEDULED: &str =
    include_str!("fixtures/github_workflow_unscheduled.yml");

/// A `vercel.json` with three crons, the last with a key in its query.
pub const VERCEL_JSON: &str = include_str!("fixtures/vercel.json");

/// A Deployment, a `CronJob` `report`, and a `CronJob` with no schedule.
pub const KUBERNETES_MANIFEST: &str = include_str!("fixtures/kubernetes_manifest.yaml");

/// A `render.yaml` with a web service and a cron `daily-report`.
pub const RENDER_YAML: &str = include_str!("fixtures/render.yaml");

/// A Terraform file with two literal schedules, a variable one and a commented one.
pub const TERRAFORM_MAIN_TF: &str = include_str!("fixtures/main.tf");

/// A crontab: comments, variables with tokens, four jobs and a line that is not one.
pub const CRONTAB: &str = include_str!("fixtures/crontab.txt");

/// `systemctl list-timers --all --no-legend --no-pager` (systemd 255): `logrotate` and `db-backup`.
pub const SYSTEMCTL_TIMERS_SYSTEM: &str = include_str!("fixtures/systemctl_timers_system.txt");

/// `systemctl --user list-timers …`: `shop-sync`.
pub const SYSTEMCTL_TIMERS_USER: &str = include_str!("fixtures/systemctl_timers_user.txt");

/// `systemctl cat logrotate.timer logrotate.service`: a timer that came with the system.
pub const SYSTEMCTL_CAT_LOGROTATE: &str = include_str!("fixtures/systemctl_cat_logrotate.txt");

/// `systemctl cat db-backup.timer db-backup.service`: a system timer of the owner's, with two
/// relative schedules.
pub const SYSTEMCTL_CAT_DB_BACKUP: &str = include_str!("fixtures/systemctl_cat_db_backup.txt");

/// `systemctl --user cat shop-sync.timer shop-sync.service`, with a key and an environment value.
pub const SYSTEMCTL_CAT_SHOP_SYNC: &str = include_str!("fixtures/systemctl_cat_shop_sync.txt");

/// A launchd agent as `plutil -convert json` prints it (hand-written; no Mac was recorded).
pub const LAUNCHD_SHOP_SYNC: &str = include_str!("fixtures/launchd_shop_sync.json");

/// A launchd agent with no interval.
pub const LAUNCHD_KEEP_ALIVE: &str = include_str!("fixtures/launchd_keep_alive.json");

/// `schtasks.exe /query /fo CSV /v` (Windows 11 header), its header twice.
pub const SCHTASKS_CSV: &str = include_str!("fixtures/schtasks.csv");
