//! The rule that sends a data pipeline request to the owner (`docs/SPEC.md` 6.10, ADR 0039):
//! pure, so the governor's answer does not depend on a prompt.

/// What a data source costs to use, as the Procurement Specialist says on asking. Unless the
/// source's own page says it is free, the agent says `unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineCost {
    /// The source's own page says it costs nothing.
    Free,
    /// Using it costs money.
    Paid,
    /// The agent could not tell.
    Unknown,
}

/// Whether a request to set up a data source is the owner's to approve, never the Product
/// Manager's: one that costs money (`Paid`), one whose cost is not known (`Unknown`), and one that
/// sends the project's data out (`sends_project_data`) are, whatever else is true of them. The
/// Product Manager may still decline any request, or pass it to the owner. Whether the request
/// needs an account is the Product Manager's judgement, and decides nothing here.
#[must_use]
pub fn pipeline_needs_owner(cost: PipelineCost, sends_project_data: bool) -> bool {
    sends_project_data || !matches!(cost, PipelineCost::Free)
}

#[cfg(test)]
mod tests {
    use super::{PipelineCost, pipeline_needs_owner};

    #[test]
    fn the_owner_decides_what_costs_money_or_sends_data() {
        assert!(!pipeline_needs_owner(PipelineCost::Free, false));
        assert!(pipeline_needs_owner(PipelineCost::Paid, false));
        assert!(pipeline_needs_owner(PipelineCost::Unknown, false));
        assert!(pipeline_needs_owner(PipelineCost::Free, true));
    }
}
