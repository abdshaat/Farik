# 0006. Error enums write their own Display while thiserror is not a dependency

Date: 2026-09-17
Status: accepted

## Context

`docs/standards/code.md` said that a function which may fail "returns `Result<T, E>` with a named error enum (`thiserror`) per crate". Phase 2 step 02 added `farik-store`, the first crate whose error enum needs a `Display`: `StoreError` is what `farik doctor`, the command line, and eventually the user read when the log refuses something, so its four reasons have to turn into sentences somewhere.

`thiserror` is not in `[workspace.dependencies]`, and the workspace pins every version by hand with `=` (`docs/standards/code.md`, ADR 0005). Adding it for one crate's four messages means a new dependency, its `proc-macro2`/`quote`/`syn` tree in the lock file, and a version to keep pinned, in exchange for replacing about twenty lines of `match` with about four attributes. The two error enums that already existed, `farik_protocol::event::EventError` and `farik_core::pricing::PricingError`, carry no `Display` at all, so nothing in the workspace uses the crate yet and nothing is made inconsistent by not adding it.

The realistic options were three. Add `thiserror` now, and take the dependency for one caller. Hand-write `Display` and `std::error::Error` for `StoreError`, and decide again when a second or third crate needs one. Or give `StoreError` no `Display`, like the two enums before it, and make every caller format it — which only moves the twenty lines somewhere worse, into each caller, where they can disagree.

## Decision

An error enum writes its own `Display` and `std::error::Error` by hand. `thiserror` is added when the saving is across several crates rather than one, and this record is superseded when it is.

## Consequences

`farik-store` has no new dependency, and the messages a user sees are written where they are read, as ordinary Rust, rather than in attributes. A `From` impl per source error stays explicit, which is where the store decides that a `rusqlite::Error` is a `Sqlite` refusal and an `io::Error` is an `Io` one.

Against the decision: every later crate pays the same twenty lines, and a hand-written `match` can go stale when a variant is added — nothing makes it an error to forget one, whereas the derive would. The mitigation is a test per crate that renders every variant, which `farik-store`'s error tests do.

`docs/standards/code.md`'s error row points here instead of naming a crate the workspace does not have.
