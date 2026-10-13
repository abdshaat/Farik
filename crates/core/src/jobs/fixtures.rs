//! Recorded and hand-written inputs of the jobs readers, for tests in this crate and in others.
//! Secrets are spelled `SECRET-<where>` and prompts `NEVER-KEPT-TEXT`.

/// `.claude/scheduled_tasks.json`: a recurring, a one-time and a permanent task, and a task with
/// nothing but an id.
pub const SCHEDULED_TASKS: &str = include_str!("fixtures/scheduled_tasks.json");

/// A background job's `state.json` (an undocumented file, read fail-soft).
pub const BACKGROUND_JOB: &str = include_str!("fixtures/background_job_state.json");
