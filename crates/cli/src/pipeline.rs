//! `farik pipeline list`: the data sources the Procurement Specialist asked for that the Product
//! Manager passed to the owner, or did not decide (`docs/SPEC.md` 6.10, ADR 0039). Approving and
//! declining are commands, sent as `farik order approve` sends its own.

use farik_core::contract::Role;
use farik_runtime::procurement::{add_pipeline_fields, pipeline_text};
use farik_store::waiting::{Waiting, waiting};
use serde_json::{Value, json};

use crate::Report;
use crate::project::Project;

/// Every request that waits for the owner, oldest first, with what the owner is asked, and the
/// same rows as `waiting.list` gives them with `--json`, alone, as one array.
///
/// # Errors
///
/// A sentence saying what the store refused.
pub fn list(project: &Project) -> Result<Report, String> {
    let team = project
        .files
        .read_team()
        .map_err(|error| error.to_string())?;
    let projections = project.projections()?;
    let kit = farik_roles::load_kit(Role::ProcurementSpecialist).ok();
    let mut lines = Vec::new();
    let mut rows = Vec::new();
    let listed =
        waiting(&projections, &project.log, &project.files, &team).map_err(|e| e.to_string())?;
    // A row of the kind carries its ask, and no other does.
    for (item, ask) in listed
        .iter()
        .filter_map(|item| item.pipeline.as_ref().map(|ask| (item, ask)))
    {
        lines.push(line(item));
        let text = pipeline_text(&project.log, kit.as_ref(), ask.pipeline)
            .map_err(|error| error.to_string())?;
        let mut row = json!({
            "task_id": item.task_id,
            "kind": item.kind.as_str(),
            "agent_id": item.agent_id,
            "title": item.title,
            "line": item.line,
        });
        add_pipeline_fields(&mut row, ask, text.as_deref());
        rows.push(row);
    }
    if lines.is_empty() {
        lines.push("no data pipeline request waits for you".to_string());
    }
    Ok(Report {
        lines,
        json: Value::Array(rows),
        json_lines: None,
    })
}

/// One request on one line: its number, name, the site of its page, what it costs, then what
/// the owner would be agreeing to, and the Product Manager's reason when it gave one. Every word
/// but the number is the agent's or the manager's.
fn line(item: &Waiting) -> String {
    let Some(ask) = &item.pipeline else {
        return String::new();
    };
    let cost = match ask.cost {
        farik_core::pipeline::PipelineCost::Free => "free",
        farik_core::pipeline::PipelineCost::Paid => "costs money",
        farik_core::pipeline::PipelineCost::Unknown => "cost not known",
    };
    let mut parts = vec![
        ask.pipeline.to_string(),
        ask.name.clone(),
        ask.host.clone(),
        cost.to_string(),
        if ask.needs_account {
            "needs an account"
        } else {
            "needs no account"
        }
        .to_string(),
        if ask.sends_project_data {
            "sends your data"
        } else {
            "sends no data"
        }
        .to_string(),
    ];
    if let Some(reason) = &ask.reason {
        parts.push(format!("the Product Manager asks you: {reason}"));
    }
    parts.join("  ")
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use farik_core::contract::TaskId;
    use farik_core::pipeline::PipelineCost;
    use farik_store::waiting::{PipelineAsk, Waiting, WaitingKind};

    use super::line;

    fn a_request(cost: PipelineCost, account: bool, data: bool, reason: Option<&str>) -> Waiting {
        Waiting {
            task_id: "FRK-1".parse::<TaskId>().expect("a task id"),
            kind: WaitingKind::DataPipeline,
            agent_id: Some("ivo".to_string()),
            title: "Price boxes".to_string(),
            line: "Ivo asks for a data source: Firecrawl".to_string(),
            question_id: None,
            reason: None,
            approval: None,
            plan: None,
            post: None,
            site: None,
            order: None,
            pipeline: Some(PipelineAsk {
                pipeline: 4,
                name: "Firecrawl".to_string(),
                what: "Pages as text.".to_string(),
                url: "https://www.firecrawl.dev/pricing".to_string(),
                host: "firecrawl.dev".to_string(),
                why: "Two sellers need a browser.".to_string(),
                cost,
                needs_account: account,
                sends_project_data: data,
                reason: reason.map(ToString::to_string),
                at: Utc::now(),
            }),
        }
    }

    #[test]
    fn a_line_says_what_the_owner_would_agree_to() {
        assert_eq!(
            line(&a_request(
                PipelineCost::Paid,
                true,
                false,
                Some("Your call.")
            )),
            "4  Firecrawl  firecrawl.dev  costs money  needs an account  sends no data  \
             the Product Manager asks you: Your call."
        );
        assert_eq!(
            line(&a_request(PipelineCost::Free, false, true, None)),
            "4  Firecrawl  firecrawl.dev  free  needs no account  sends your data"
        );
        assert!(
            line(&a_request(PipelineCost::Unknown, false, false, None)).contains("cost not known")
        );
    }
}
