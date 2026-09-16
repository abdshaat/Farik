//! Farik's harness: schemas, the task state machine, the governor, and the cost model.
//! This crate performs no I/O.

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
