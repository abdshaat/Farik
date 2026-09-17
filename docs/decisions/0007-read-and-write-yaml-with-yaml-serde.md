# 0007. Read and write YAML with `yaml_serde`

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
literally versioned `0.9.34+deprecated`. Three things were compared, on 2026-09-17:

| Crate | Last release | Recent downloads | Repository | Licence |
|---|---|---|---|---|
| `serde_yaml` | 2024-03-25, deprecated | 90M | `dtolnay/serde-yaml`, archived | MIT OR Apache-2.0 |
| `serde_yaml_ng` | 2024-05-26 | 6.9M | `acatton/serde-yaml-ng` | MIT |
| `yaml_serde` | 2026-08-18 | 1.8M | `yaml/yaml-serde` | MIT OR Apache-2.0 |

Both forks were tried on the job itself: reading a team file into `serde_json::Value`, writing it
back, and reading it again. They produce identical values, and both round-trip.

## Decision

Use `yaml_serde`, pinned exactly as every other dependency is.

It is the only one of the three with a release this year, it lives under the YAML organisation
rather than one person's account, and it is dual-licensed MIT OR Apache-2.0, which matches this
repository's own Apache-2.0. The download counts run the other way, and that is the trade taken
deliberately: for a format whose parser decides what a hand-edited file means, a maintained parser
is worth more than a popular one.

Only two functions are used, `from_str` and `to_string`, both against `serde_json::Value`. Nothing
in Farik derives serde for a YAML shape, so replacing this crate would be a change to one module.

## Consequences

- `farik-store` gains one dependency. `farik-core` gains none: it does no I/O and never sees a file.
- A duplicate key in a YAML file is not an error in either crate — the last value wins, silently.
  Farik cannot see the difference, because the duplicate is gone before the value reaches it. That
  is recorded for `farik doctor`, which can read the text itself and say so.
- YAML's other sharp edges are the parser's to handle, not Farik's: anchors and aliases expand, and
  a document that is not a mapping is refused by the schema rather than by the parser.
