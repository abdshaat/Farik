# 0007. Read and write YAML with `serde-saphyr`

Date: 2026-09-17
Status: accepted

## Context

`.farik/team.yaml`, `.farik/team/criteria.yaml` and a task contract are YAML files a person writes
and edits by hand (`docs/SPEC.md` sections 5.12, 5.13 and 8.4). Phase 2 step 06 is the first code
that reads one, and the workspace has no YAML dependency.

Everything in Farik that validates a file does it the same way: parse to a `serde_json::Value`, hand
that to the JSON Schema validator, and only then build the typed value. So what is needed is narrow —
YAML text to `Value`, and `Value` back to YAML text — and not serde derive support for YAML at all.

The obvious crate, `serde_yaml`, was archived by its author in March 2024 and its last release is
literally versioned `0.9.34+deprecated`. Four were compared, on 2026-09-17, against the crates.io
API for the numbers and against this job for the behaviour:

| Crate | Last release | Recent downloads | Repository | Licence |
|---|---|---|---|---|
| `serde_yaml` 0.9.34 | 2024-03-25, deprecated | 90M | `dtolnay/serde-yaml`, archived | MIT OR Apache-2.0 |
| `serde_yaml_ng` 0.10.0 | 2024-05-26 | 7.0M | `acatton/serde-yaml-ng` | MIT |
| `yaml_serde` 0.10.7 | 2026-08-18 | 1.8M | `yaml/yaml-serde` | MIT OR Apache-2.0 |
| `serde-saphyr` 1.3.0 | 2026-09-16 | 5.2M | `bourumir-wyngs/serde-saphyr` | MIT OR Apache-2.0 |

All four read a team file into `serde_json::Value`, write it back, and read it again to the same
value. They part company on what they do with a file that is wrong, which is the whole question for
a file a person edits by hand. Measured, on the same two inputs:

- A mapping with `name` twice. `yaml_serde` and `serde_yaml_ng` both return `{"name":"b"}`: the last
  value wins and nothing is said. `serde-saphyr` refuses, naming the line and column and printing
  the two lines: `duplicate mapping key: name`.
- An alias-expansion bomb, nine aliases deep. `serde-saphyr` stops it at a node budget and points at
  the anchor the nodes came from.

`serde-saphyr` is the youngest of the four at the 1.x line, and 1.3.0 was released the day before
this record. That is the cost side of it, and it is why the version is pinned exactly.

## Decision

Use `serde-saphyr`, pinned exactly as every other dependency is. This supersedes the project plan's
earlier "Made" line, which chose `serde_saphyr` 1.2.0, and the first draft of this record on this
branch, which changed that to `yaml_serde` before the duplicate-key behaviour was measured.

A duplicate key in a file that decides who may do what is a governance failure, not a formatting
nit: two `wip_limit_per_agent` lines in `team.yaml` under either fork give a limit the person did
not knowingly choose, and Farik cannot see the difference, because the duplicate is gone before the
value reaches it. The parser is the only layer that can refuse it, and one of the four does.

Only two functions are used, `from_str_with_options` and `to_string`, both against
`serde_json::Value`, plus the error's own `render_with_formatter`. Nothing in Farik derives serde
for a YAML shape, so replacing this crate would be a change to one module.

`farik-store` gains one direct dependency and, with it, fourteen transitive ones — the parser, its
snippet renderer and their support crates. That is recorded here rather than left to whoever reads
`Cargo.lock`: it is the largest single addition to the workspace so far.

## Consequences

- `farik-store` gains one dependency. `farik-core` gains none: it does no I/O and never sees a file.
- A duplicate key, and a file holding more than one document, are refused with a line, a column, and
  the offending text, by the parser rather than by the schema. `farik doctor` does not need to
  re-read the text to find either, which the earlier draft of this record had planned for.
- The refusal is rendered with the crate's `UserMessageFormatter`, not its default. The default is
  documented as developer-facing and says things like "set `DuplicateKeyPolicy` in Options if
  acceptable" — advice to whoever calls the crate, printed at whoever edited `team.yaml`. The user
  formatter says "duplicate mapping key: name not allowed here" and keeps the line, the column and
  the quoted text. The crate does not know the file's name and calls it `<input>`, so Farik puts the
  name back.
- These files are read with `strict_booleans`, so YAML 1.1's boolean spellings are off: an unquoted
  `no`, `y` or `off` stays the word the person wrote rather than becoming `true` or `false`. An
  agent whose id is `no` is an agent, not a refusal. Only `true` and `false` are booleans, which is
  what YAML 1.2's core schema says and what a person writing a configuration file expects. What
  Farik itself writes is quoted where it would otherwise be ambiguous, so a file it wrote reads the
  same under either setting — measured on `NO`, `y`, `off` and `1.10`.
- The sharp edge that is left is the rest of scalar resolution, which no setting turns off: an
  unquoted `1.10` is the number `1.1`, not the string. Where the schema wants a string this surfaces
  as a refusal a person can act on rather than as a wrong value; where it would want a number, it
  would not. Nothing in `team.schema.json`, `criteria.schema.json` or `contract.schema.json` takes a
  free-form scalar whose type is not fixed, so today there is no such place, and a new property that
  takes one has to be written with this in mind.
- The version is a day old and the crate is young. It is pinned exactly, its two functions are used
  behind `read_yaml` and `write_yaml` in one module, and each behaviour this record chose it for —
  the duplicate key, the second document, the scalar YAML has an opinion about, and the words the
  refusal is in — has a test of its own in `crates/store/tests/project_files.rs`, so a regression in
  it fails our suite rather than reaching a team file.
