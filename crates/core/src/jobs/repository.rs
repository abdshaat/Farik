//! The schedules committed in a repository: GitHub Actions, `vercel.json`, Kubernetes `CronJob`,
//! `render.yaml` and Terraform. Only a job's name and schedules are read, never what it runs.

use std::path::Path;

use serde_json::Value;

use super::JobKind;

/// A job found in a file: its name and its schedules as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileJob {
    /// Where it was found.
    pub kind: JobKind,
    /// Its name.
    pub name: String,
    /// Its schedules, as written.
    pub schedules: Vec<String>,
}

/// What a repository's file may hold, by its path: a workflow is a direct child of
/// `.github/workflows/`, `vercel.json` and `render.yaml` are found in any folder, `*.tf` is
/// Terraform, and any other YAML file may hold a Kubernetes `CronJob`.
#[must_use]
pub fn schedule_file_kind(path: &str) -> Option<JobKind> {
    let path = Path::new(path);
    let name = path.file_name()?.to_str()?;
    let extension = path.extension().and_then(|extension| extension.to_str());
    let in_workflows = path.parent() == Some(Path::new(".github/workflows"));
    match (name, extension) {
        ("vercel.json", _) => Some(JobKind::Vercel),
        ("render.yaml", _) => Some(JobKind::Render),
        (_, Some("tf")) => Some(JobKind::Terraform),
        (_, Some("yml" | "yaml")) if in_workflows => Some(JobKind::GithubActions),
        (_, Some("yml" | "yaml")) => Some(JobKind::Kubernetes),
        _ => None,
    }
}

fn string_at<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .try_fold(value, |value, key| value.get(*key))?
        .as_str()
}

/// The jobs in the parsed documents of a file of any kind but Terraform.
#[must_use]
pub fn jobs_in_documents(kind: JobKind, path: &str, documents: &[Value]) -> Vec<FileJob> {
    let job = |name: &str, schedules: Vec<String>| FileJob {
        kind,
        name: name.to_string(),
        schedules,
    };
    let entries = |document: &Value, key: &str| -> Vec<Value> {
        document
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    match kind {
        JobKind::GithubActions => documents
            .iter()
            .filter_map(|document| {
                let crons: Vec<String> = document
                    .get("on")
                    .map(|on| entries(on, "schedule"))
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|entry| string_at(entry, &["cron"]).map(str::to_string))
                    .collect();
                let stem = Path::new(path).file_stem()?.to_str()?;
                let name = string_at(document, &["name"]).unwrap_or(stem);
                (!crons.is_empty()).then(|| job(name, crons))
            })
            .collect(),
        JobKind::Vercel => documents
            .iter()
            .flat_map(|document| entries(document, "crons"))
            .filter_map(|cron| {
                let schedule = string_at(&cron, &["schedule"])?.to_string();
                let path = string_at(&cron, &["path"])?;
                let name = path.split(['?', '#']).next().unwrap_or(path);
                Some(job(name, vec![schedule]))
            })
            .collect(),
        JobKind::Render => documents
            .iter()
            .flat_map(|document| entries(document, "services"))
            .filter(|service| string_at(service, &["type"]) == Some("cron"))
            .filter_map(|service| {
                let schedule = string_at(&service, &["schedule"])?.to_string();
                Some(job(string_at(&service, &["name"])?, vec![schedule]))
            })
            .collect(),
        JobKind::Kubernetes => documents
            .iter()
            .filter(|document| string_at(document, &["kind"]) == Some("CronJob"))
            .filter_map(|document| {
                let schedule = string_at(document, &["spec", "schedule"])?.to_string();
                Some(job(
                    string_at(document, &["metadata", "name"])?,
                    vec![schedule],
                ))
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The text between a line's quotes, in order.
fn quoted(line: &str) -> Vec<&str> {
    line.split('"').skip(1).step_by(2).collect()
}

/// The literal `schedule_expression` values of a Terraform file, by a line scan, not a parser. A
/// `resource "<type>" "<name>"` or `module "<name>"` opens a block and names its job
/// `<type>.<name>` or `module.<name>`; one before any block is named `schedule_expression`. A
/// value with `${`, a heredoc, a variable or a commented line is not read.
#[must_use]
pub fn jobs_in_terraform(text: &str) -> Vec<FileJob> {
    let mut found: Vec<FileJob> = Vec::new();
    let mut block: Option<(String, i64)> = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        if let Some((name, depth)) = block.as_mut() {
            if let Some(value) = schedule_value(line) {
                match found.iter_mut().find(|job| job.name == *name) {
                    Some(job) => job.schedules.push(value),
                    None => found.push(terraform_job(name, value)),
                }
            }
            *depth += brace_change(line);
            if *depth <= 0 {
                block = None;
            }
            continue;
        }
        let words = quoted(line);
        let opened = match (line.split_whitespace().next(), words.as_slice()) {
            (Some("resource"), [kind, name, ..]) => Some(format!("{kind}.{name}")),
            (Some("module"), [name, ..]) => Some(format!("module.{name}")),
            _ => None,
        };
        if let Some(name) = opened {
            let depth = brace_change(line);
            if depth > 0 {
                block = Some((name, depth));
            }
        } else if let Some(value) = schedule_value(line) {
            found.push(terraform_job("schedule_expression", value));
        }
    }
    found
}

fn terraform_job(name: &str, schedule: String) -> FileJob {
    FileJob {
        kind: JobKind::Terraform,
        name: name.to_string(),
        schedules: vec![schedule],
    }
}

fn brace_change(line: &str) -> i64 {
    let count = |brace| i64::try_from(line.matches(brace).count()).unwrap_or(0);
    count('{') - count('}')
}

fn schedule_value(line: &str) -> Option<String> {
    let rest = line.strip_prefix("schedule_expression")?.trim_start();
    let value = rest.strip_prefix('=')?.trim_start().strip_prefix('"')?;
    let value = value.split('"').next()?;
    (!value.contains("${")).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::fixtures::TERRAFORM_MAIN_TF;
    use serde_json::json;

    #[test]
    fn classifies_schedule_files() {
        let kind = schedule_file_kind;
        assert_eq!(
            kind(".github/workflows/nightly.yml"),
            Some(JobKind::GithubActions)
        );
        assert_eq!(
            kind(".github/workflows/nightly.yaml"),
            Some(JobKind::GithubActions)
        );
        assert_eq!(
            kind(".github/workflows/sub/x.yml"),
            Some(JobKind::Kubernetes)
        );
        assert_eq!(kind("k8s/report.yaml"), Some(JobKind::Kubernetes));
        assert_eq!(kind("apps/web/vercel.json"), Some(JobKind::Vercel));
        assert_eq!(kind("render.yaml"), Some(JobKind::Render));
        assert_eq!(kind("infra/main.tf"), Some(JobKind::Terraform));
        assert_eq!(kind("README.md"), None);
        assert_eq!(kind("vercel.json.bak"), None);
    }

    fn jobs(kind: JobKind, path: &str, document: &Value) -> Vec<(String, Vec<String>)> {
        jobs_in_documents(kind, path, std::slice::from_ref(document))
            .into_iter()
            .map(|job| {
                assert_eq!(job.kind, kind);
                (job.name, job.schedules)
            })
            .collect()
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn finds_a_workflows_schedules() {
        let path = ".github/workflows/nightly.yml";
        let workflow = json!({
            "name": "nightly-sync",
            "on": {"schedule": [{"cron": "0 3 * * *"}, {"cron": "30 4 * * 1"}], "workflow_dispatch": null},
        });
        assert_eq!(
            jobs(JobKind::GithubActions, path, &workflow),
            vec![(
                "nightly-sync".to_string(),
                strings(&["0 3 * * *", "30 4 * * 1"])
            )]
        );
        let unnamed = json!({"on": {"schedule": [{"cron": "0 3 * * *"}, {"run": "x"}]}});
        assert_eq!(
            jobs(JobKind::GithubActions, path, &unnamed),
            vec![("nightly".to_string(), strings(&["0 3 * * *"]))]
        );
        for on in [
            json!("push"),
            json!(["push"]),
            json!({"push": null}),
            json!({"schedule": []}),
            json!({"schedule": [{"run": "x"}]}),
        ] {
            let document = json!({"name": "ci", "on": on});
            assert_eq!(jobs(JobKind::GithubActions, path, &document), vec![]);
        }
    }

    #[test]
    fn finds_vercel_render_and_kubernetes_jobs() {
        let vercel = json!({"crons": [
            {"path": "/api/cron/digest", "schedule": "0 5 * * *"},
            {"path": "/api/cron/cleanup", "schedule": "0 0 * * 0"},
            {"path": "/api/cron/x?key=SECRET-VERCEL", "schedule": "0 * * * *"},
            {"path": "/a#b", "schedule": "* * * * *"},
            {"path": "/no-schedule"},
        ]});
        let found = jobs(JobKind::Vercel, "vercel.json", &vercel);
        let names: Vec<&str> = found.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            ["/api/cron/digest", "/api/cron/cleanup", "/api/cron/x", "/a"]
        );
        assert_eq!(found[2].1, strings(&["0 * * * *"]));
        assert_eq!(jobs(JobKind::Vercel, "vercel.json", &json!({})), vec![]);
        let render = json!({"services": [
            {"type": "web", "name": "shop"},
            {"type": "cron", "name": "daily-report", "schedule": "0 6 * * *"},
        ]});
        assert_eq!(
            jobs(JobKind::Render, "render.yaml", &render),
            vec![("daily-report".to_string(), strings(&["0 6 * * *"]))]
        );
        let documents = [
            json!({"kind": "Deployment", "metadata": {"name": "web"}}),
            json!({"kind": "CronJob", "metadata": {"name": "report"}, "spec": {"schedule": "15 2 * * *"}}),
            json!({"kind": "CronJob", "metadata": {"name": "idle"}, "spec": {}}),
        ];
        let found = jobs_in_documents(JobKind::Kubernetes, "k8s/report.yaml", &documents);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "report");
        assert_eq!(found[0].schedules, strings(&["15 2 * * *"]));
    }

    #[test]
    fn finds_terraform_schedule_expressions() {
        let found = jobs_in_terraform(TERRAFORM_MAIN_TF);
        let pairs: Vec<(&str, &[String])> = found
            .iter()
            .map(|job| (job.name.as_str(), job.schedules.as_slice()))
            .collect();
        assert_eq!(
            pairs,
            [
                (
                    "aws_cloudwatch_event_rule.nightly",
                    strings(&["cron(0 3 * * ? *)"]).as_slice()
                ),
                (
                    "aws_scheduler_schedule.hourly",
                    strings(&["rate(1 hour)"]).as_slice()
                ),
            ]
        );
        assert!(found.iter().all(|job| job.kind == JobKind::Terraform));
        let bare = jobs_in_terraform("schedule_expression = \"rate(2 hours)\"\n");
        assert_eq!(bare.len(), 1);
        assert_eq!(bare[0].name, "schedule_expression");
        let module =
            jobs_in_terraform("module \"cleanup\" {\n  schedule_expression = \"rate(1 day)\"\n}\n");
        assert_eq!(module[0].name, "module.cleanup");
    }
}
