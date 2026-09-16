//! Farik's harness: schemas, the task state machine, the governor, and the cost model.
//! This crate performs no I/O.

/// The task contract and its validator.
pub mod contract;
/// Types generated from `docs/schemas/`.
pub mod generated;
/// The governor: every rule of `docs/SPEC.md` section 5 as pure functions.
pub mod governor;

/// The crate's package name, as published.
pub const CORE_CRATE_NAME: &str = "farik-core";

#[cfg(test)]
mod tests {
    use super::CORE_CRATE_NAME;

    #[test]
    fn exposes_its_crate_name() {
        assert_eq!(CORE_CRATE_NAME, "farik-core");
    }
}
