# 0009. Generate schema types at compile time

Date: 2026-09-21
Status: accepted

## Context

Phase 0 step 02 wrote the Rust types for every schema in `docs/schemas/` with `cargo xtask generate`: `typify` turned each schema into a module under `crates/<name>/src/generated/`, `prettyplease` and `rustfmt` formatted it, a byte-for-byte copy of the schema went next to it so the crate could `include_str!` it, and `cargo xtask check` failed when either file was stale. That is 3,333 lines of generated Rust, 1,213 lines of schema copies, an xtask module, a check step, and three build dependencies (`schemars`, `syn`, `prettyplease`), all to reproduce what `typify` already does as a macro.

`typify::import_types!` reads a schema at compile time and emits the same types, with the same settings (`struct_builder = false`, `derives = [PartialEq]`). The copies existed only because a published crate cannot read outside its directory; no crate here is published (the workspace is `0.0.0`), so `include_str!` can read `docs/schemas/` directly.

## Decision

Each `generated` module is a `typify::import_types!` call on the schema in `docs/schemas/`. The committed generated files, the schema copies, `cargo xtask generate` and its freshness check are deleted.

## Consequences

One file per schema instead of three, and nothing to regenerate or keep fresh: the build is the generator. Every schema is also embedded with `include_str!` in the crate that uses it, so cargo rebuilds that crate, and re-runs the macro, when the schema changes.

The generated code is no longer in the tree for a reviewer to read in a diff; `cargo expand` shows it. A schema change now shows up only as a schema diff, and a `typify` upgrade can change the types without a visible generated-file diff, so the pinned version in the root `Cargo.toml` matters more than it did. Publishing a crate later means either vendoring its schemas into the crate or going back to committed output.
