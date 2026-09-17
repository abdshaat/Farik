# Phase 2, step 05: Team, rules, and the criterion library

Status: ready
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; a session may not push to another branch without permission, so phase 2 reuses it as phase 1 did; steps do not get their own)
Spec: `docs/SPEC.md` section 3 (team, agent, role), 5.6 (permission tiers and protected paths), 5.12 (team rules), 5.13 (the criterion library), 5.14 (the integration policy), 5.16 (who accepts a contract); F1, F15, F16; decision D18 in `docs/plans/project-plan.md`
Depends on: phase 0 (merged in #4), phase 1 (merged in #5), steps 01 to 04 of this phase (last commit `fc70cd6`)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, nothing ambiguous, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: a review session that did not write this plan, on 2026-09-17, against `38f0d27` and the one-word correction it made itself. It confirmed the three rules by rebuilding the whole step from this plan alone in a fresh copy outside the working tree, with a target directory of its own: every predicted red exact including the count of errors, every green exact at 231, 242, 245, 251 and 258, `cargo xtask check --integration` ending in `xtask check: ok` at all five, and the changed-file set exactly the File map's fifteen. The first round refused on four findings — a schema that forbade the work-in-progress limit of zero that 5.2 calls a pause, an agent whose tiers could only be widened while 5.6 calls them overridable, a reason for a decision that was one of four things the generator does rather than the only one, and five surviving mutants. The second refused on one: `Agent::tiers` calls `default_tiers` and Task 2's import did not name it, so that task could not reach its own green. The reviewer applied the one-word fix, reached 242, and confirmed the rest; it is applied below.

## Goal

A team is a file a person writes and the governor reads. `.farik/team.yaml` says who the agents are, what they may spend, what the team's policy is, and what rules every contract and every tool call is held to; `.farik/team/criteria.yaml` holds the named exit criteria the project verifies its work with. This step adds both schemas, generates their types into `farik-core`, and writes the three functions that turn an untrusted value into something the harness can decide with: `validate_team`, `validate_criteria`, and `expand_criteria`.

It adds no I/O. `farik-core` performs none, ever (hard rule 5), so reading and writing those files is step 06's, and this step is the shape and the rules they are held to.

## Decisions

- **Three of the team's rules are `validate_team`'s rather than the schema's.** D18 says "the schema enforces two to seven agents with at least one active Product Manager and one active Software Developer". The count it does. The other two want `contains` on the agents array, and every way of writing that fails differently, all four measured on `typify` 0.8.0 with this schema:
  - one `contains` on the array: `cargo xtask generate` exits 1 with `invalid schema for FarikTeam_agents: unhandled array validation`, and nothing is generated;
  - two, as two branches of an `allOf` on the array, which is the natural way to say "one of each": generation *succeeds* and writes `pub enum FarikTeamAgents {}`, an uninhabited type, so `FarikTeam` cannot be built at all and nothing announces it;
  - the same two moved to a root-level `allOf`: the generator panics — `typify-impl-0.8.0/src/type_entry.rs:290: called Option::unwrap() on a None value`;
  - `dependentSchemas` at the root: generation succeeds, `typify` ignores the keyword, the types come out byte-identical — the generator rewrites only the schema copy — and the copy keeps the keyword, so `jsonschema` does enforce the rule.

  That last one works, and is still the wrong call. Its refusal is `None of [{"display_name":"ada",…},{…}] are valid under the given schema` — the whole agents array dumped, with the words "Product Manager" nowhere in it — and because `validate_team` returns on schema errors before it reaches its own rules, putting the rule in the schema would *replace* the readable refusal rather than back it up. So the schema says two to seven, and `validate_team` says the rest: ids are unique, an active Product Manager is there, an active Software Developer is there. This is the same shape as step 01's decision that the schema does not pair `kind` with `body` and the reader does. Task 6 records it.
- **Every rule the schema cannot say is reported, not just the first.** A team file with a repeated id and no developer is two problems, and a person fixing one at a time is a person running the command twice.
- **The criterion library carries a copy of the contract schema's `verification`, and a test holds the two copies to being identical.** A criterion in the library is an exit criterion without an id, so the two schemas have to agree about what a verification is. A `$ref` across files would need both `typify` and `jsonschema` to resolve an external reference, which is a dependency on a resolver that nothing else here needs; the copy needs only a test, and that test reads both embedded schemas and compares the sub-tree. If they ever drift, the mapping between the two generated types starts lying, and the test fails before it can.
- **A reference to a criterion is the id it will carry and the name it has, in that order.** `("C1", "cargo-check")` reads as the criterion it produces. The library does not know a contract's requirements, so an expanded criterion arrives with an empty `satisfies`: which requirements it provides evidence for is a fact about one contract.
- **`CriteriaError` gains a second variant, `Refused { name, detail }`**, beyond the `UnknownCriterion` the project plan records. The id in a reference is the caller's text, and a contract's ids are `C1`, `C2`, and so on; expanding `("C0", ...)` has to answer something, and a panic is not an answer. It also stands ready for the day the two schemas disagree. Task 6 records it.
- **The protected paths `farik-core` ships are kept whatever the team writes, and the team's are added to them.** 5.12 says rules only narrow what a tier allows; a team that could delete `.env` from the list would be widening one. A path the team repeats is still one path.
- **There is no way to write "no cap" for `max_task_budget_usd`.** A missing field and an explicit `null` are one value to serde, so a schema that offered both would be promising something the types cannot keep. Left out, it is the five dollars `farik-core` ships; a team that wants more writes a larger number.
- **A role's tiers are the user's to widen *and* to narrow.** `docs/SPEC.md` 5.6 says a role's capability tiers are "overridable per agent by the user", and an override that could only widen would leave no way to say that this Developer does not run commands. So an agent has `revokes` beside `grants`, and `Agent::tiers()` answers what it actually holds: the role's defaults, plus what was granted, minus what was taken away. Taking away wins over granting, because a tier in both lists is a person's mistake and the narrower reading of a mistake is the safer one. Task 6 records the field and the method; the spec already says this, so it does not change.
- **A work-in-progress limit of zero is a number a person may write.** 5.2 says a limit of zero refuses every assignment, which is how a team pauses an agent without retiring it, and `farik-core`'s assignment gate already answers "takes no work: its limit is zero". A schema with `minimum: 1` would have made that answer unreachable, which is what the first draft of this plan had.
- **The team's roles are the contract's minus `human`, and its permission tiers are a second spelling of the governor's.** A contract may name the human as a reviewer; no agent is a human. Two `From` implementations are this crate's one mapping layer, at its edge, as `docs/standards/code.md` asks.
- **Budgets hold `daily_usd` and the session limits, and no sprint budget.** D18 puts the sprint's budget on the sprint, which is phase 3's; a team file that carried one would be two sources of truth for the same number.
- **The fixtures are `pub mod fixtures`, not `#[cfg(test)]`.** `docs/standards/code.md` says a crate's fixtures are public so that another crate's tests can use them, and step 06 writes a team file in a test on its way to reading one back.
- **Four helpers in `contract.rs` become `pub(crate)`**: `pointer`, `with_integers_normalised`, `repeated_ids` and `named`. Three validators in one crate that disagreed about how to report a JSON pointer, or about whether `1.0` is an integer, would be three ways for the same file to be refused differently.
- **`rules` is required although every property inside it is optional**, so a hand-written file carries one `rules: {}`. Kept required: `farik init` writes the file in step 06, and an empty `rules:` key is a line that shows a reader where rules go. The alternative — optional, defaulting inside `Team::rules` — hides the whole feature from anyone reading an example.
- **`status` is required on every agent, with no default.** Making it optional would make the generated field an `Option<AgentStatus>` and put a `None` case into `active_agents`, `has_active` and everything after them, to save one word per agent in a file a command writes.
- The generated `ModelEffort` is this step's; `farik-runtime`'s own `Effort` arrives in phase 3 step 01 and maps to it there. Nothing here depends on that crate.

## Design

`docs/schemas/team.schema.json` and `docs/schemas/criteria.schema.json` are the sources of truth. `cargo xtask generate` turns each into Rust under `crates/core/src/generated/`, next to a verbatim copy of its schema, and `cargo xtask check` fails when either is stale — the same path the task contract and the price table already take.

`crates/core/src/team.rs` holds the validator, `Team`'s three methods, and the two conversions. `crates/core/src/criteria.rs` holds its validator, `CriteriaError`, and `expand_criteria`. Each has a `fixtures.rs` beside it.

Out of scope: reading or writing any file (step 06); the `farik doctor` that reports what a team file got wrong (step 07); the role definitions themselves, which are phase 3 step 07's `farik-roles`; `mcp_servers` and `skills` on an agent, which D18 lists as later.

## Architecture notes

- Modified: `farik-core` gains `team` and `criteria`, siblings of `contract` and `pricing`. They depend on `contract` for the shared validator helpers and on `governor` for `TeamRules` and `PermissionTier`.
- Consumed: `jsonschema` and `serde_json`, both already dependencies. No new dependency, and `Cargo.lock` does not change.
- `farik-core` still does no I/O; `cargo xtask core-io` still passes. `include_str!` of an embedded schema is a compile-time read, which that check already allows for the contract and the price table.

## Global constraints

- Every value is checked against its schema before it is deserialised, and refused with one error per violation at its own JSON pointer.
- No `unwrap` outside tests. The two `expect`s on the embedded schemas are the ones `contract.rs` and `pricing.rs` already make, with the same reason: the file is a copy the generator wrote and the check holds fresh.
- Every refusal reads as English about the file a person wrote, not as a type error.
- No test is skipped, ignored, or quarantined to get green.

## File map

```
docs/schemas/team.schema.json                  creates: the team file's shape
docs/schemas/criteria.schema.json              creates: the criterion library's shape
crates/core/src/team.rs                        creates: validate_team, Team's methods, the two conversions
crates/core/src/team/fixtures.rs               creates: a_team_wire, an_agent_wire, a_full_team_wire
crates/core/src/criteria.rs                    creates: validate_criteria, CriteriaError, expand_criteria
crates/core/src/criteria/fixtures.rs           creates: a_criteria_library_wire, an_empty_criteria_library_wire
crates/core/src/generated/team.rs              creates: written by cargo xtask generate
crates/core/src/generated/team.schema.json     creates: written by cargo xtask generate
crates/core/src/generated/criteria.rs          creates: written by cargo xtask generate
crates/core/src/generated/criteria.schema.json creates: written by cargo xtask generate
crates/core/src/generated/mod.rs               modifies: declares the two new modules
crates/core/src/lib.rs                         modifies: declares team and criteria
crates/core/src/contract.rs                    modifies: four helpers become pub(crate)
xtask/src/generate.rs                          modifies: two more entries in GENERATED_SCHEMAS
docs/plans/project-plan.md                     modifies: records what this step's interface became
docs/plans/phase-2-protocol-store-cli/step-05-team-and-criteria.md modifies: this plan, ticked as it goes
```

`Cargo.toml` and `Cargo.lock` do not change: this step adds no dependency.

## Tasks

Blocks are separated by exactly one blank line. `rustfmt.toml` allows no more, and `cargo xtask check` runs the format check before it runs a test, so two blank lines at a seam fail a task before anything is tried.

### Task 1: The team file, read from the wire

Files: created `docs/schemas/team.schema.json`, `crates/core/src/team.rs`, `crates/core/src/team/fixtures.rs`; modified `crates/core/src/lib.rs`, `crates/core/src/generated/mod.rs`, `crates/core/src/contract.rs`, `xtask/src/generate.rs`, `docs/plans/phase-2-protocol-store-cli/step-05-team-and-criteria.md`

Consumes: nothing from this plan
Produces: `farik_core::team::{Team, validate_team}` and the generated team types

- [x] Write the failing tests. Create `crates/core/src/team.rs` with the module doc and the fixtures declaration:

  ```rust
  //! The team (`docs/SPEC.md` sections 3, 5.12, 5.14 and 5.16): `docs/schemas/team.schema.json` as
  //! Rust types, the validator that turns an untrusted JSON value into one, and the three rules the
  //! schema cannot say.

  /// Wire fixtures for tests, in this crate and in others.
  pub mod fixtures;
  ```

- [x] Create `crates/core/src/team/fixtures.rs`:

  ```rust
  use serde_json::{Value, json};

  /// A schema-valid wire team: the two agents a team cannot work without, and no optional field.
  #[must_use]
  pub fn a_team_wire() -> Value {
      json!({
          "name": "Farik",
          "agents": [an_agent_wire("ada", "product_manager"), an_agent_wire("linus", "software_developer")],
          "budgets": { "daily_usd": 20 },
          "policy": {
              "human_accepts_contracts": "high_risk",
              "wip_limit_per_agent": 2,
              "blocked_limit_hours": 24,
              "max_iterations": 3,
              "integration": "manual"
          },
          "rules": {}
      })
  }

  /// One active agent, with every required field and no optional one.
  #[must_use]
  pub fn an_agent_wire(id: &str, role: &str) -> Value {
      json!({
          "id": id,
          "display_name": id,
          "role": role,
          "status": "active"
      })
  }

  /// A wire team with every optional field present, so that a reader of a test can see the whole
  /// shape in one place and a change to the schema has one fixture to update.
  #[must_use]
  pub fn a_full_team_wire() -> Value {
      json!({
          "name": "Farik",
          "agents": [
              {
                  "id": "ada",
                  "display_name": "Ada",
                  "role": "product_manager",
                  "persona": "Asks the question nobody asked.",
                  "avatar": "ada.png",
                  "status": "active",
                  "model": { "id": "claude-opus-5", "effort": "high" },
                  "grants": ["execute"],
                  "revokes": ["network"],
                  "preauthorized_external_tools": ["mcp__linear__create_issue"]
              },
              {
                  "id": "linus",
                  "display_name": "Linus",
                  "role": "software_developer",
                  "status": "active",
                  "model": { "id": "claude-opus-5" }
              }
          ],
          "budgets": {
              "daily_usd": 20.5,
              "session": {
                  "max_input_tokens": 200_000,
                  "max_output_tokens": 64000,
                  "max_wall_clock_seconds": 1800,
                  "max_tool_calls": 200
              }
          },
          "policy": {
              "human_accepts_contracts": "all",
              "wip_limit_per_agent": 2,
              "blocked_limit_hours": 24,
              "max_iterations": 3,
              "integration": "local_merge",
              "integration_branch": "trunk"
          },
          "rules": {
              "protected_paths": ["infra/**"],
              "allowed_paths_ceiling": ["src/**"],
              "required_criteria": ["test", "review"],
              "require_new_tests": true,
              "max_task_budget_usd": 12.5,
              "forbidden_commands": ["^rm -rf /"]
          }
      })
  }
  ```

- [x] Append the tests module to `crates/core/src/team.rs`:

  ```rust
  #[cfg(test)]
  mod tests {
      use serde_json::{Value, json};

      use super::fixtures::{a_full_team_wire, a_team_wire, an_agent_wire};
      use super::{HumanAcceptsContracts, Integration, Team, validate_team};

      fn team(wire: &Value) -> Team {
          validate_team(wire).expect("the fixture is a team")
      }

      /// Dollars, compared the way `pricing`'s tests compare them.
      fn close(actual: f64, expected: f64) -> bool {
          (actual - expected).abs() < 1e-9
      }

      /// Every refusal as a pointer and its message, which is what a caller shows a person.
      fn refusals(wire: &Value) -> Vec<(String, String)> {
          validate_team(wire)
              .expect_err("this wire team is refused")
              .into_iter()
              .map(|error| (error.path, error.message))
              .collect()
      }

      fn paths(wire: &Value) -> Vec<String> {
          refusals(wire).into_iter().map(|(path, _)| path).collect()
      }

      #[test]
      fn reads_a_team_with_only_what_it_must_have() {
          let team = team(&a_team_wire());
          assert_eq!(team.name.as_str(), "Farik");
          assert_eq!(team.agents.len(), 2);
          assert!(close(team.budgets.daily_usd, 20.0));
          assert!(team.budgets.session.is_none(), "the role's limits stand");
          assert_eq!(
              team.policy.human_accepts_contracts,
              HumanAcceptsContracts::HighRisk
          );
          assert_eq!(team.policy.integration, Integration::Manual);
          assert!(
              team.policy.integration_branch.is_none(),
              "the repository's default branch stands (5.14)"
          );
      }

      #[test]
      fn reads_a_team_with_every_field_it_may_have() {
          let team = team(&a_full_team_wire());
          let ada = &team.agents[0];
          assert_eq!(
              ada.persona.as_deref().map(String::as_str),
              Some("Asks the question nobody asked.")
          );
          assert_eq!(
              ada.model.as_ref().expect("a model").id.as_str(),
              "claude-opus-5"
          );
          assert_eq!(
              ada.preauthorized_external_tools
                  .iter()
                  .map(|tool| tool.to_string())
                  .collect::<Vec<_>>(),
              ["mcp__linear__create_issue"]
          );
          let session = team.budgets.session.as_ref().expect("session limits");
          assert_eq!(
              session.max_input_tokens.map(std::num::NonZeroU64::get),
              Some(200_000)
          );
          assert_eq!(
              team.policy
                  .integration_branch
                  .as_ref()
                  .map(|branch| branch.to_string()),
              Some("trunk".to_string())
          );
      }

      #[test]
      fn refuses_a_team_that_is_too_small_or_too_large() {
          let mut one = a_team_wire();
          one["agents"] = json!([an_agent_wire("ada", "product_manager")]);
          assert_eq!(paths(&one), ["/agents"], "a team is two agents at least");

          let mut eight = a_team_wire();
          eight["agents"] = Value::Array(
              (0..8)
                  .map(|n| an_agent_wire(&format!("agent-{n}"), "software_developer"))
                  .collect(),
          );
          assert_eq!(paths(&eight), ["/agents"], "and seven at most");
      }

      #[test]
      fn refuses_a_field_no_schema_knows() {
          let mut wire = a_team_wire();
          wire["mascot"] = json!("a penguin");
          assert_eq!(paths(&wire), ["/"]);
      }

      #[test]
      fn refuses_an_agent_id_that_is_not_a_slug() {
          for id in ["Ada", "ada lovelace", "ada_lovelace", "-ada", ""] {
              let mut wire = a_team_wire();
              wire["agents"][0]["id"] = json!(id);
              assert_eq!(paths(&wire), ["/agents/0/id"], "{id:?}");
          }
      }

      #[test]
      fn refuses_a_role_no_agent_can_hold() {
          // The contract schema's roles include `human`, because a contract may name the human as a
          // reviewer. No agent is a human, so the team schema's roles are the other five.
          let mut wire = a_team_wire();
          wire["agents"][0]["role"] = json!("human");
          assert_eq!(paths(&wire), ["/agents/0/role"]);
      }
  }
  ```

- [x] Declare the module in `crates/core/src/lib.rs`. The list is alphabetical, so this goes between `pricing` and `text` — insert before

  ```rust
  /// Small shared pieces of English used in refusal messages.
  mod text;
  ```

  the two lines:

  ```rust
  /// The team, its rules, and its validator.
  pub mod team;
  ```

- [x] Run them and confirm they fail because nothing of the team exists:

  ```
  cargo test -p farik-core --lib
  # expected: FAIL to compile,
  # error[E0432]: unresolved imports `super::HumanAcceptsContracts`, `super::Integration`,
  #   `super::Team`, `super::validate_team`
  # error: could not compile `farik-core` (lib test) due to 1 previous error
  ```

- [x] Write the schema. Create `docs/schemas/team.schema.json`:

  ```json
  {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "$id": "https://farik.dev/schemas/team.schema.json",
    "title": "FarikTeam",
    "description": "A team and everything the human configures about it, stored as .farik/team.yaml and read by the governor on every decision. See docs/SPEC.md sections 3, 5.12, 5.14 and 5.16, and decision D18 in docs/plans/project-plan.md.",
    "type": "object",
    "additionalProperties": false,
    "required": [
      "name",
      "agents",
      "budgets",
      "policy",
      "rules"
    ],
    "properties": {
      "name": {
        "type": "string",
        "minLength": 1,
        "maxLength": 100,
        "description": "What the human calls this team."
      },
      "agents": {
        "type": "array",
        "minItems": 2,
        "maxItems": 7,
        "items": {
          "$ref": "#/$defs/agent"
        },
        "description": "Two to seven agents. Three more rules are validate_team's rather than this schema's, because typify cannot generate a usable type from an array that carries a `contains`: ids are unique, and the team has an active Product Manager and an active Software Developer."
      },
      "budgets": {
        "$ref": "#/$defs/budgets"
      },
      "policy": {
        "$ref": "#/$defs/policy"
      },
      "rules": {
        "$ref": "#/$defs/rules"
      }
    },
    "$defs": {
      "role": {
        "type": "string",
        "enum": [
          "product_manager",
          "scrum_master",
          "architect",
          "software_developer",
          "marketing_specialist"
        ],
        "description": "The role an agent is instantiated from. `human` is a role a contract may name as a reviewer, never a role an agent holds."
      },
      "agent": {
        "type": "object",
        "additionalProperties": false,
        "required": [
          "id",
          "display_name",
          "role",
          "status"
        ],
        "properties": {
          "id": {
            "type": "string",
            "pattern": "^[a-z0-9]+(-[a-z0-9]+)*$",
            "maxLength": 64,
            "description": "A kebab-case slug of the display name, unique within the team. Events and contracts refer to an agent by it, so it never changes."
          },
          "display_name": {
            "type": "string",
            "minLength": 1,
            "maxLength": 100
          },
          "role": {
            "$ref": "#/$defs/role"
          },
          "persona": {
            "type": "string",
            "maxLength": 2000,
            "description": "A few lines of character added to the role's system prompt. It never grants anything."
          },
          "avatar": {
            "type": "string",
            "maxLength": 200,
            "description": "The name of a shipped avatar or a path under .farik/team/avatars/."
          },
          "status": {
            "type": "string",
            "enum": [
              "active",
              "paused",
              "retired"
            ],
            "description": "A paused agent keeps its work and takes none; a retired one is kept only so that its past events still name someone."
          },
          "model": {
            "$ref": "#/$defs/model"
          },
          "preauthorized_external_tools": {
            "type": "array",
            "maxItems": 100,
            "items": {
              "type": "string",
              "minLength": 1
            },
            "description": "External-effect tools this agent may use without asking the human each time (spec 5.6)."
          },
          "grants": {
            "type": "array",
            "maxItems": 7,
            "uniqueItems": true,
            "items": {
              "$ref": "#/$defs/permissionTier"
            },
            "description": "Permission tiers this agent holds on top of its role's defaults (spec 5.6)."
          },
          "revokes": {
            "type": "array",
            "maxItems": 7,
            "uniqueItems": true,
            "items": {
              "$ref": "#/$defs/permissionTier"
            },
            "description": "Permission tiers this agent does not hold, whatever its role's defaults say (spec 5.6: a role's tiers are the user's to override, which means taking one away as well as adding one). Taking away wins over granting."
          }
        }
      },
      "permissionTier": {
        "type": "string",
        "enum": [
          "read",
          "write_workspace",
          "execute",
          "network",
          "git_local",
          "git_remote",
          "external_effect"
        ]
      },
      "model": {
        "type": "object",
        "additionalProperties": false,
        "required": [
          "id"
        ],
        "properties": {
          "id": {
            "type": "string",
            "minLength": 1,
            "maxLength": 100,
            "description": "The provider's model id, exactly as the runtime reports it in usage, so that the price table can be asked about it."
          },
          "effort": {
            "type": "string",
            "enum": [
              "low",
              "medium",
              "high"
            ],
            "description": "How hard the model thinks. The runtime's own Effort arrives in phase 3."
          }
        }
      },
      "budgets": {
        "type": "object",
        "additionalProperties": false,
        "required": [
          "daily_usd"
        ],
        "properties": {
          "daily_usd": {
            "type": "number",
            "exclusiveMinimum": 0,
            "description": "What the team may spend in a day before it pauses (spec 5.5). A sprint's own budget belongs to the sprint, not here."
          },
          "session": {
            "$ref": "#/$defs/sessionLimits"
          }
        }
      },
      "sessionLimits": {
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "max_input_tokens": {
            "type": "integer",
            "minimum": 1
          },
          "max_output_tokens": {
            "type": "integer",
            "minimum": 1
          },
          "max_wall_clock_seconds": {
            "type": "integer",
            "minimum": 1
          },
          "max_tool_calls": {
            "type": "integer",
            "minimum": 1
          }
        },
        "description": "Overrides for one session's limits. What is left out keeps the role's default (farik-core's default_session_limits)."
      },
      "policy": {
        "type": "object",
        "additionalProperties": false,
        "required": [
          "human_accepts_contracts",
          "wip_limit_per_agent",
          "blocked_limit_hours",
          "max_iterations",
          "integration"
        ],
        "properties": {
          "human_accepts_contracts": {
            "type": "string",
            "enum": [
              "high_risk",
              "all"
            ],
            "description": "Which contracts wait for the human's acceptance (spec 5.16). `high_risk` is the default the first run writes."
          },
          "wip_limit_per_agent": {
            "type": "integer",
            "minimum": 0,
            "maximum": 100,
            "description": "How many tasks one agent may hold that are neither accepted nor cancelled (spec 5.2). Zero refuses every assignment, which is how a team pauses an agent without retiring it."
          },
          "blocked_limit_hours": {
            "type": "integer",
            "minimum": 1,
            "maximum": 720,
            "description": "How long a task may stay blocked before it escalates (spec 5.7)."
          },
          "max_iterations": {
            "type": "integer",
            "minimum": 1,
            "maximum": 100,
            "description": "How many times a task may be rejected before it escalates (spec 5.7)."
          },
          "integration": {
            "type": "string",
            "enum": [
              "manual",
              "local_merge",
              "pull_request"
            ],
            "description": "What happens to a task's branch after it is accepted (spec 5.14)."
          },
          "integration_branch": {
            "type": "string",
            "minLength": 1,
            "maxLength": 200,
            "description": "The branch accepted work integrates into. Left out, it is the repository's default branch (spec 5.14)."
          }
        }
      },
      "rules": {
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "protected_paths": {
            "type": "array",
            "maxItems": 100,
            "items": {
              "type": "string",
              "minLength": 1
            },
            "description": "Globs no tool may read or write, whatever its tier (spec 5.6). Left out, the five farik-core ships."
          },
          "allowed_paths_ceiling": {
            "type": "array",
            "maxItems": 100,
            "items": {
              "type": "string",
              "minLength": 1
            },
            "description": "Globs a contract's allowed_paths must fall within. Empty means no ceiling."
          },
          "required_criteria": {
            "type": "array",
            "maxItems": 5,
            "uniqueItems": true,
            "items": {
              "type": "string",
              "enum": [
                "command",
                "test",
                "artifact",
                "review",
                "human"
              ]
            },
            "description": "Verification methods every contract must have at least one criterion of."
          },
          "require_new_tests": {
            "type": "boolean",
            "description": "Whether every `test` criterion must set new_tests_required."
          },
          "max_task_budget_usd": {
            "type": "number",
            "exclusiveMinimum": 0,
            "description": "The most one task's budget may be. Left out, the five dollars farik-core ships; to lift the cap in practice, write a number large enough. There is no way to say \"no cap\", because a team that can turn a rule off is a rule that only narrows in name (spec 5.12)."
          },
          "forbidden_commands": {
            "type": "array",
            "maxItems": 100,
            "items": {
              "type": "string",
              "minLength": 1
            },
            "description": "ECMAScript regular expressions a command must not match (spec 5.12)."
          }
        }
      }
    }
  }
  ```

- [x] Generate its types. In `xtask/src/generate.rs`, change the array's length to 5 and add the entry after the task contract's:

  ```rust
      GeneratedSchema {
          schema: "docs/schemas/team.schema.json",
          types: "crates/core/src/generated/team.rs",
          schema_copy: "crates/core/src/generated/team.schema.json",
      },
  ```

  then run the generator, which writes both generated files:

  ```
  cargo xtask generate
  # expected: generated crates/core/src/generated/team.rs
  #           generated crates/core/src/generated/team.schema.json
  ```

- [x] Declare the generated module in `crates/core/src/generated/mod.rs`, after `task_contract`:

  ```rust
  pub mod team;
  ```

- [x] Make the four validator helpers the crate's rather than the module's. In `crates/core/src/contract.rs`, put `pub(crate) ` in front of each of these four signatures, changing nothing else about them:

  ```rust
  fn repeated_ids<'a>(ids: impl Iterator<Item = &'a str>) -> Vec<String> {
  fn named(ids: &[String]) -> String {
  fn with_integers_normalised(value: &Value) -> Value {
  fn pointer(path: &str) -> String {
  ```

- [x] Write the minimal implementation. In `crates/core/src/team.rs`, replace the module doc with the doc and its imports:

  ```rust
  //! The team (`docs/SPEC.md` sections 3, 5.12, 5.14 and 5.16): `docs/schemas/team.schema.json` as
  //! Rust types, the validator that turns an untrusted JSON value into one, and the three rules the
  //! schema cannot say.

  use std::sync::LazyLock;

  use jsonschema::Validator;
  use serde_json::Value;

  use crate::contract::{pointer, with_integers_normalised};

  pub use crate::contract::{Role, ValidationError};
  pub use crate::generated::team::{
      Agent, AgentStatus, Budgets as TeamBudgets, FarikTeam as Team, Model as AgentModel,
      ModelEffort as Effort, PermissionTier as PermissionTierWire, Policy as TeamPolicy,
      PolicyHumanAcceptsContracts as HumanAcceptsContracts, PolicyIntegration as Integration,
      Role as RoleWire, Rules as RulesWire, SessionLimits as SessionLimitsWire,
  };

  /// Wire fixtures for tests, in this crate and in others.
  pub mod fixtures;
  ```

  then insert, between that and the tests module:

  ```rust
  const SCHEMA_JSON: &str = include_str!("generated/team.schema.json");

  static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
      let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
          "the embedded team schema is valid JSON: it is a copy of docs/schemas/ written by \
           cargo xtask generate and checked for freshness by cargo xtask check",
      );
      jsonschema::options()
          .should_validate_formats(true)
          .build(&schema)
          .expect(
              "the embedded team schema compiles: it is JSON Schema 2020-12 with no external \
               references, and the generator already parsed it",
          )
  });
  ```

  and after that:

  ```rust
  /// Checks a value against `docs/schemas/team.schema.json` and, when it conforms, returns the typed
  /// team.
  ///
  /// # Errors
  ///
  /// Every schema violation, each at its own JSON pointer; or, when the schema passes and the typed
  /// team cannot be built, one error at the root.
  pub fn validate_team(input: &Value) -> Result<Team, Vec<ValidationError>> {
      let errors: Vec<ValidationError> = VALIDATOR
          .iter_errors(input)
          .map(|error| ValidationError {
              path: pointer(&error.instance_path().to_string()),
              message: error.to_string(),
          })
          .collect();
      if !errors.is_empty() {
          return Err(errors);
      }
      serde_json::from_value::<Team>(with_integers_normalised(input)).map_err(|error| {
          vec![ValidationError {
              path: "/".to_string(),
              message: format!("the schema passed but the typed team could not be built: {error}"),
          }]
      })
  }
  ```

- [x] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 231 passed (farik-core)
  #   test result: ok. 37 passed (farik-protocol)
  #   test result: ok. 44 passed (farik-store)
  #   test result: ok. 15 passed (crates/store/tests/git.rs)
  #   test result: ok. 29 passed (xtask)
  ```

- [x] Commit: `feat(core): read a team file and refuse one that is not`

### Task 2: Who is active, and what a team cannot do without

Files: modified `crates/core/src/team.rs`, `docs/plans/phase-2-protocol-store-cli/step-05-team-and-criteria.md`

Consumes: `Team`, `validate_team` from Task 1
Produces: `Team::{active_agents, has_active}`, `Agent::tiers`, `From<RoleWire> for Role`, `From<PermissionTierWire> for PermissionTier`, and the three rules the schema cannot say

They arrive together because the rules are written in terms of who is active: a team needs an active Product Manager, and "active" is a question about the agents' status that nothing could ask yet. `Agent::tiers` comes with them because it is the other half of the same mapping — the permission tiers a team file spells one way and the governor another.

- [x] Write the failing tests. Replace the whole tests module of `crates/core/src/team.rs` with:

  ```rust
  #[cfg(test)]
  mod tests {
      use serde_json::{Value, json};

      use super::fixtures::{a_full_team_wire, a_team_wire, an_agent_wire};
      use super::{
          AgentStatus, HumanAcceptsContracts, Integration, PermissionTier, PermissionTierWire, Role,
          RoleWire, Team, validate_team,
      };

      fn team(wire: &Value) -> Team {
          validate_team(wire).expect("the fixture is a team")
      }

      /// Dollars, compared the way `pricing`'s tests compare them.
      fn close(actual: f64, expected: f64) -> bool {
          (actual - expected).abs() < 1e-9
      }

      /// Every refusal as a pointer and its message, which is what a caller shows a person.
      fn refusals(wire: &Value) -> Vec<(String, String)> {
          validate_team(wire)
              .expect_err("this wire team is refused")
              .into_iter()
              .map(|error| (error.path, error.message))
              .collect()
      }

      fn paths(wire: &Value) -> Vec<String> {
          refusals(wire).into_iter().map(|(path, _)| path).collect()
      }

      #[test]
      fn reads_a_team_with_only_what_it_must_have() {
          let team = team(&a_team_wire());
          assert_eq!(team.name.as_str(), "Farik");
          assert_eq!(team.agents.len(), 2);
          assert!(close(team.budgets.daily_usd, 20.0));
          assert!(team.budgets.session.is_none(), "the role's limits stand");
          assert_eq!(
              team.policy.human_accepts_contracts,
              HumanAcceptsContracts::HighRisk
          );
          assert_eq!(team.policy.integration, Integration::Manual);
          assert!(
              team.policy.integration_branch.is_none(),
              "the repository's default branch stands (5.14)"
          );
      }

      #[test]
      fn reads_a_team_with_every_field_it_may_have() {
          let team = team(&a_full_team_wire());
          let ada = &team.agents[0];
          assert_eq!(
              ada.persona.as_deref().map(String::as_str),
              Some("Asks the question nobody asked.")
          );
          assert_eq!(
              ada.model.as_ref().expect("a model").id.as_str(),
              "claude-opus-5"
          );
          assert_eq!(
              ada.preauthorized_external_tools
                  .iter()
                  .map(|tool| tool.to_string())
                  .collect::<Vec<_>>(),
              ["mcp__linear__create_issue"]
          );
          let session = team.budgets.session.as_ref().expect("session limits");
          assert_eq!(
              session.max_input_tokens.map(std::num::NonZeroU64::get),
              Some(200_000)
          );
          assert_eq!(
              team.policy
                  .integration_branch
                  .as_ref()
                  .map(|branch| branch.to_string()),
              Some("trunk".to_string())
          );
      }

      #[test]
      fn refuses_a_team_that_is_too_small_or_too_large() {
          let mut one = a_team_wire();
          one["agents"] = json!([an_agent_wire("ada", "product_manager")]);
          assert_eq!(paths(&one), ["/agents"], "a team is two agents at least");

          let mut eight = a_team_wire();
          eight["agents"] = Value::Array(
              (0..8)
                  .map(|n| an_agent_wire(&format!("agent-{n}"), "software_developer"))
                  .collect(),
          );
          assert_eq!(paths(&eight), ["/agents"], "and seven at most");
      }

      #[test]
      fn refuses_a_field_no_schema_knows() {
          let mut wire = a_team_wire();
          wire["mascot"] = json!("a penguin");
          assert_eq!(paths(&wire), ["/"]);
      }

      #[test]
      fn refuses_an_agent_id_that_is_not_a_slug() {
          for id in ["Ada", "ada lovelace", "ada_lovelace", "-ada", ""] {
              let mut wire = a_team_wire();
              wire["agents"][0]["id"] = json!(id);
              assert_eq!(paths(&wire), ["/agents/0/id"], "{id:?}");
          }
      }

      #[test]
      fn refuses_a_role_no_agent_can_hold() {
          // The contract schema's roles include `human`, because a contract may name the human as a
          // reviewer. No agent is a human, so the team schema's roles are the other five.
          let mut wire = a_team_wire();
          wire["agents"][0]["role"] = json!("human");
          assert_eq!(paths(&wire), ["/agents/0/role"]);
      }

      #[test]
      fn names_every_agent_id_that_appears_twice() {
          let mut wire = a_team_wire();
          wire["agents"] = json!([
              an_agent_wire("ada", "product_manager"),
              an_agent_wire("ada", "software_developer"),
              an_agent_wire("linus", "architect"),
              an_agent_wire("linus", "scrum_master"),
          ]);
          let refusals = refusals(&wire);
          assert_eq!(refusals.len(), 1, "{refusals:?}");
          assert_eq!(refusals[0].0, "/agents");
          assert_eq!(
              refusals[0].1,
              "an agent id names one agent, and the ids ada, linus name more than one"
          );
      }

      #[test]
      fn refuses_a_team_with_nobody_to_write_a_contract_or_do_the_work() {
          let mut wire = a_team_wire();
          wire["agents"] = json!([
              an_agent_wire("ada", "architect"),
              an_agent_wire("linus", "scrum_master"),
          ]);
          let messages: Vec<String> = refusals(&wire)
              .into_iter()
              .map(|(_, message)| message)
              .collect();
          assert_eq!(
              messages,
              [
                  "a team needs an active product_manager to write a contract; this one has none \
                   (docs/SPEC.md section 3)",
                  "a team needs an active software_developer to do the work; this one has none \
                   (docs/SPEC.md section 3)",
              ],
              "both, not the first of them"
          );
      }

      #[test]
      fn reports_a_repeated_id_and_a_missing_role_together() {
          // Two problems in one file are two refusals: a person fixing one at a time is a person
          // running the command twice.
          let mut wire = a_team_wire();
          wire["agents"] = json!([
              an_agent_wire("ada", "product_manager"),
              an_agent_wire("ada", "architect"),
          ]);
          let messages: Vec<String> = refusals(&wire)
              .into_iter()
              .map(|(_, message)| message)
              .collect();
          assert_eq!(messages.len(), 2, "{messages:?}");
          assert!(messages[0].contains("the id ada names"), "{}", messages[0]);
          assert!(
              messages[1].contains("software_developer"),
              "{}",
              messages[1]
          );
      }

      #[test]
      fn a_paused_product_manager_is_not_an_active_one() {
          // A paused agent keeps the work it holds and is given nothing new, so a team whose only
          // Product Manager is paused cannot start anything.
          let mut wire = a_team_wire();
          wire["agents"][0]["status"] = json!("paused");
          let messages: Vec<String> = refusals(&wire)
              .into_iter()
              .map(|(_, message)| message)
              .collect();
          assert_eq!(messages.len(), 1, "{messages:?}");
          assert!(messages[0].contains("product_manager"), "{}", messages[0]);
      }

      #[test]
      fn counts_only_the_agents_that_take_work() {
          let mut wire = a_team_wire();
          wire["agents"] = json!([
              an_agent_wire("ada", "product_manager"),
              an_agent_wire("linus", "software_developer"),
              an_agent_wire("grace", "architect"),
              an_agent_wire("alan", "scrum_master"),
          ]);
          wire["agents"][2]["status"] = json!("paused");
          wire["agents"][3]["status"] = json!("retired");
          let team = team(&wire);
          assert_eq!(
              team.active_agents()
                  .map(|agent| agent.id.to_string())
                  .collect::<Vec<_>>(),
              ["ada", "linus"]
          );
          assert!(team.has_active(Role::ProductManager));
          assert!(team.has_active(Role::SoftwareDeveloper));
          assert!(!team.has_active(Role::Architect), "paused");
          assert!(!team.has_active(Role::ScrumMaster), "retired");
          assert!(!team.has_active(Role::Human), "the human is not an agent");
          assert_eq!(team.agents[2].status, AgentStatus::Paused);
      }

      #[test]
      fn a_work_in_progress_limit_of_zero_is_how_an_agent_is_paused() {
          // 5.2: a limit of zero refuses every assignment, which is how a team pauses an agent
          // without retiring it, and the governor already answers "takes no work: its limit is zero".
          // A schema that would not let a person write the number would make that answer unreachable.
          let mut wire = a_team_wire();
          wire["policy"]["wip_limit_per_agent"] = json!(0);
          assert_eq!(team(&wire).policy.wip_limit_per_agent, 0);
      }

      #[test]
      fn refuses_a_number_outside_what_a_rule_allows() {
          // Every bound here carries a spec number: 5.2's work-in-progress limit, 5.7's blocked age
          // and iteration count, 5.5's daily budget, 5.12's task cap and its list of methods. A bound
          // nothing tests is a bound the next person deletes to make something else compile.
          for (pointer, value) in [
              ("/policy/wip_limit_per_agent", json!(-1)),
              ("/policy/wip_limit_per_agent", json!(101)),
              ("/policy/blocked_limit_hours", json!(0)),
              ("/policy/blocked_limit_hours", json!(721)),
              ("/policy/max_iterations", json!(0)),
              ("/policy/max_iterations", json!(101)),
              ("/budgets/daily_usd", json!(0)),
              ("/rules/max_task_budget_usd", json!(0)),
              ("/rules/required_criteria", json!(["vibes"])),
          ] {
              let mut wire = a_full_team_wire();
              *wire
                  .pointer_mut(pointer)
                  .expect("the full fixture has every field") = value.clone();
              let paths = paths(&wire);
              assert!(
                  !paths.is_empty() && paths.iter().all(|path| path.starts_with(pointer)),
                  "{pointer} = {value}: {paths:?}"
              );
          }
      }

      #[test]
      fn a_role_s_tiers_are_the_user_s_to_widen_and_to_narrow() {
          // 5.6: a role's tiers are overridable per agent. Ada is a Product Manager, whose defaults
          // are read and network; she is granted execute and denied network.
          let team = team(&a_full_team_wire());
          assert_eq!(
              team.agents[0].tiers(),
              [PermissionTier::Read, PermissionTier::Execute]
          );
          assert_eq!(
              team.agents[1].tiers(),
              [
                  PermissionTier::Read,
                  PermissionTier::WriteWorkspace,
                  PermissionTier::Execute,
                  PermissionTier::GitLocal,
              ],
              "and an agent that overrides nothing holds what its role holds"
          );
      }

      #[test]
      fn taking_a_tier_away_wins_over_granting_it() {
          // Both lists naming one tier is a person's mistake, and the narrower reading of a mistake
          // is the safer one.
          let mut wire = a_team_wire();
          wire["agents"][1]["grants"] = json!(["git_remote", "read"]);
          wire["agents"][1]["revokes"] = json!(["git_remote", "execute"]);
          let team = team(&wire);
          assert_eq!(
              team.agents[1].tiers(),
              [
                  PermissionTier::Read,
                  PermissionTier::WriteWorkspace,
                  PermissionTier::GitLocal,
              ],
              "execute taken away, git_remote granted and taken away, read granted twice over"
          );
      }

      #[test]
      fn names_every_role_an_agent_can_hold() {
          assert_eq!(
              [
                  RoleWire::ProductManager,
                  RoleWire::ScrumMaster,
                  RoleWire::Architect,
                  RoleWire::SoftwareDeveloper,
                  RoleWire::MarketingSpecialist,
              ]
              .map(Role::from),
              [
                  Role::ProductManager,
                  Role::ScrumMaster,
                  Role::Architect,
                  Role::SoftwareDeveloper,
                  Role::MarketingSpecialist,
              ]
          );
      }

      #[test]
      fn names_every_permission_tier_an_agent_can_be_granted() {
          assert_eq!(
              [
                  PermissionTierWire::Read,
                  PermissionTierWire::WriteWorkspace,
                  PermissionTierWire::Execute,
                  PermissionTierWire::Network,
                  PermissionTierWire::GitLocal,
                  PermissionTierWire::GitRemote,
                  PermissionTierWire::ExternalEffect,
              ]
              .map(PermissionTier::from),
              [
                  PermissionTier::Read,
                  PermissionTier::WriteWorkspace,
                  PermissionTier::Execute,
                  PermissionTier::Network,
                  PermissionTier::GitLocal,
                  PermissionTier::GitRemote,
                  PermissionTier::ExternalEffect,
              ]
          );
      }
  }
  ```

- [x] Run them and confirm they fail because nothing knows who is active:

  ```
  cargo test -p farik-core --lib
  # expected: FAIL to compile. These three codes and no others:
  # error[E0432]: unresolved import `super::PermissionTier`
  # error[E0599]: no method named `active_agents` found for struct `FarikTeam` in the
  #   current scope
  # error[E0599]: no method named `has_active` found for struct `FarikTeam` in the current
  #   scope  (five times, once per call site)
  # error[E0599]: no method named `tiers` found for struct `Agent` in the current scope
  #   (three times)
  # error[E0631]: type mismatch in function arguments  (the `.map(Role::from)` that has no
  #   From to call yet)
  # error: could not compile `farik-core` (lib test) due to 11 previous errors
  ```

- [x] Write the minimal implementation. In `crates/core/src/team.rs`, add to the imports, after the `crate::contract` line — both names on one line, which is what rustfmt leaves alone:

  ```rust
  use crate::governor::permissions::{PermissionTier, default_tiers};
  ```

  and change the `crate::contract` line itself to:

  ```rust
  use crate::contract::{named, pointer, repeated_ids, with_integers_normalised};
  ```

- [x] Insert, between the validator's statics and `validate_team`:

  ```rust
  /// The two roles a team cannot work without, and what each of them is for.
  ///
  /// `docs/SPEC.md` section 3 and F1: a team with nobody to write a contract has no way to start, and
  /// a team with nobody to do the work has no way to finish. Every other role is the human's choice.
  const REQUIRED_ROLES: [(RoleWire, &str); 2] = [
      (RoleWire::ProductManager, "write a contract"),
      (RoleWire::SoftwareDeveloper, "do the work"),
  ];
  ```

- [x] Replace `validate_team`, whole, with the version that checks the three rules the schema cannot:

  ```rust
  /// Checks a value against `docs/schemas/team.schema.json` and, when it conforms, returns the typed
  /// team.
  ///
  /// Three rules are this function's rather than the schema's, because `typify` cannot generate a
  /// usable type from an array that carries a `contains` — it writes an empty enum for the array and
  /// the whole team becomes unbuildable. So the schema says two to seven agents and this says the
  /// rest: ids are unique, and an active Product Manager and an active Software Developer are there.
  /// They are checked after the schema passes, on the typed value, and every one of them is reported
  /// rather than only the first.
  ///
  /// # Errors
  ///
  /// Every schema violation, each at its own JSON pointer; or, when the schema passes, one error per
  /// repeated agent id and one per missing required role; or, when the schema passes and the typed
  /// team cannot be built, one error at the root.
  pub fn validate_team(input: &Value) -> Result<Team, Vec<ValidationError>> {
      let errors: Vec<ValidationError> = VALIDATOR
          .iter_errors(input)
          .map(|error| ValidationError {
              path: pointer(&error.instance_path().to_string()),
              message: error.to_string(),
          })
          .collect();
      if !errors.is_empty() {
          return Err(errors);
      }
      let team =
          serde_json::from_value::<Team>(with_integers_normalised(input)).map_err(|error| {
              vec![ValidationError {
                  path: "/".to_string(),
                  message: format!(
                      "the schema passed but the typed team could not be built: {error}"
                  ),
              }]
          })?;
      let mut errors = Vec::new();
      let repeated = repeated_ids(team.agents.iter().map(|agent| agent.id.as_str()));
      if !repeated.is_empty() {
          errors.push(ValidationError {
              path: "/agents".to_string(),
              message: format!(
                  "an agent id names one agent, and {} more than one",
                  named(&repeated)
              ),
          });
      }
      for (role, what) in REQUIRED_ROLES {
          if !team.has_active(Role::from(role)) {
              errors.push(ValidationError {
                  path: "/agents".to_string(),
                  message: format!(
                      "a team needs an active {role} to {what}; this one has none (docs/SPEC.md \
                       section 3)"
                  ),
              });
          }
      }
      if errors.is_empty() {
          Ok(team)
      } else {
          Err(errors)
      }
  }
  ```

- [x] Insert, after `validate_team`:

  ```rust
  impl Team {
      /// The agents that take work. A paused agent keeps what it holds and is given nothing new; a
      /// retired one is kept only so that its past events still name someone (D18).
      pub fn active_agents(&self) -> impl Iterator<Item = &Agent> {
          self.agents
              .iter()
              .filter(|agent| agent.status == AgentStatus::Active)
      }

      /// Whether an active agent holds this role.
      ///
      /// `Role::Human` is never one of them: the human is not an agent, which is why no agent may
      /// carry that role in the first place.
      #[must_use]
      pub fn has_active(&self, role: Role) -> bool {
          self.active_agents()
              .any(|agent| Role::from(agent.role) == role)
      }
  }
  ```

- [x] And after that, what an agent may do:

  ```rust
  impl Agent {
      /// Every permission tier this agent holds: its role's defaults, widened by what it was granted
      /// and narrowed by what was taken away.
      ///
      /// `docs/SPEC.md` section 5.6 says a role's tiers are the user's to override, and an override
      /// that could only widen would leave no way to say that this Developer does not run commands.
      /// Taking away wins over granting, because a tier in both lists is a person's mistake and the
      /// narrower reading of a mistake is the safer one. The order is the role's own first, then what
      /// a grant added, so that a list read back reads as the role plus the exceptions.
      #[must_use]
      pub fn tiers(&self) -> Vec<PermissionTier> {
          let mut tiers = default_tiers(Role::from(self.role)).to_vec();
          for granted in self
              .grants
              .iter()
              .flatten()
              .copied()
              .map(PermissionTier::from)
          {
              if !tiers.contains(&granted) {
                  tiers.push(granted);
              }
          }
          let revoked: Vec<PermissionTier> = self
              .revokes
              .iter()
              .flatten()
              .copied()
              .map(PermissionTier::from)
              .collect();
          tiers.retain(|tier| !revoked.contains(tier));
          tiers
      }
  }
  ```

- [x] And after that:

  ```rust
  /// The team schema's roles are the contract schema's minus `human`: a contract may name the human
  /// as a reviewer, and no agent is one. This is the mapping `docs/standards/code.md` allows one of
  /// per crate, kept at the crate's edge.
  impl From<RoleWire> for Role {
      fn from(wire: RoleWire) -> Self {
          match wire {
              RoleWire::ProductManager => Self::ProductManager,
              RoleWire::ScrumMaster => Self::ScrumMaster,
              RoleWire::Architect => Self::Architect,
              RoleWire::SoftwareDeveloper => Self::SoftwareDeveloper,
              RoleWire::MarketingSpecialist => Self::MarketingSpecialist,
          }
      }
  }

  /// The same mapping for the permission tiers an agent is granted (`docs/SPEC.md` section 5.6).
  impl From<PermissionTierWire> for PermissionTier {
      fn from(wire: PermissionTierWire) -> Self {
          match wire {
              PermissionTierWire::Read => Self::Read,
              PermissionTierWire::WriteWorkspace => Self::WriteWorkspace,
              PermissionTierWire::Execute => Self::Execute,
              PermissionTierWire::Network => Self::Network,
              PermissionTierWire::GitLocal => Self::GitLocal,
              PermissionTierWire::GitRemote => Self::GitRemote,
              PermissionTierWire::ExternalEffect => Self::ExternalEffect,
          }
      }
  }
  ```

- [x] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 242 passed (farik-core)
  #   test result: ok. 15 passed (crates/store/tests/git.rs)
  #   test result: ok. 29 passed (xtask)
  ```

- [x] Commit: `feat(core): say who is active and what a team needs`

### Task 3: The rules the governor applies

Files: modified `crates/core/src/team.rs`, `docs/plans/phase-2-protocol-store-cli/step-05-team-and-criteria.md`

Consumes: `Team` from Tasks 1 and 2
Produces: `Team::rules`

- [ ] Write the failing tests. Replace the whole tests module of `crates/core/src/team.rs` with:

  ```rust
  #[cfg(test)]
  mod tests {
      use serde_json::{Value, json};

      use super::fixtures::{a_full_team_wire, a_team_wire, an_agent_wire};
      use super::{
          AgentStatus, HumanAcceptsContracts, Integration, PermissionTier, PermissionTierWire, Role,
          RoleWire, Team, validate_team,
      };
      use crate::governor::team_rules::{DEFAULT_MAX_TASK_BUDGET_USD, DEFAULT_PROTECTED_PATHS};

      fn team(wire: &Value) -> Team {
          validate_team(wire).expect("the fixture is a team")
      }

      /// Dollars, compared the way `pricing`'s tests compare them.
      fn close(actual: f64, expected: f64) -> bool {
          (actual - expected).abs() < 1e-9
      }

      /// Every refusal as a pointer and its message, which is what a caller shows a person.
      fn refusals(wire: &Value) -> Vec<(String, String)> {
          validate_team(wire)
              .expect_err("this wire team is refused")
              .into_iter()
              .map(|error| (error.path, error.message))
              .collect()
      }

      fn paths(wire: &Value) -> Vec<String> {
          refusals(wire).into_iter().map(|(path, _)| path).collect()
      }

      #[test]
      fn reads_a_team_with_only_what_it_must_have() {
          let team = team(&a_team_wire());
          assert_eq!(team.name.as_str(), "Farik");
          assert_eq!(team.agents.len(), 2);
          assert!(close(team.budgets.daily_usd, 20.0));
          assert!(team.budgets.session.is_none(), "the role's limits stand");
          assert_eq!(
              team.policy.human_accepts_contracts,
              HumanAcceptsContracts::HighRisk
          );
          assert_eq!(team.policy.integration, Integration::Manual);
          assert!(
              team.policy.integration_branch.is_none(),
              "the repository's default branch stands (5.14)"
          );
      }

      #[test]
      fn reads_a_team_with_every_field_it_may_have() {
          let team = team(&a_full_team_wire());
          let ada = &team.agents[0];
          assert_eq!(
              ada.persona.as_deref().map(String::as_str),
              Some("Asks the question nobody asked.")
          );
          assert_eq!(
              ada.model.as_ref().expect("a model").id.as_str(),
              "claude-opus-5"
          );
          assert_eq!(
              ada.preauthorized_external_tools
                  .iter()
                  .map(|tool| tool.to_string())
                  .collect::<Vec<_>>(),
              ["mcp__linear__create_issue"]
          );
          let session = team.budgets.session.as_ref().expect("session limits");
          assert_eq!(
              session.max_input_tokens.map(std::num::NonZeroU64::get),
              Some(200_000)
          );
          assert_eq!(
              team.policy
                  .integration_branch
                  .as_ref()
                  .map(|branch| branch.to_string()),
              Some("trunk".to_string())
          );
      }

      #[test]
      fn refuses_a_team_that_is_too_small_or_too_large() {
          let mut one = a_team_wire();
          one["agents"] = json!([an_agent_wire("ada", "product_manager")]);
          assert_eq!(paths(&one), ["/agents"], "a team is two agents at least");

          let mut eight = a_team_wire();
          eight["agents"] = Value::Array(
              (0..8)
                  .map(|n| an_agent_wire(&format!("agent-{n}"), "software_developer"))
                  .collect(),
          );
          assert_eq!(paths(&eight), ["/agents"], "and seven at most");
      }

      #[test]
      fn refuses_a_field_no_schema_knows() {
          let mut wire = a_team_wire();
          wire["mascot"] = json!("a penguin");
          assert_eq!(paths(&wire), ["/"]);
      }

      #[test]
      fn refuses_an_agent_id_that_is_not_a_slug() {
          for id in ["Ada", "ada lovelace", "ada_lovelace", "-ada", ""] {
              let mut wire = a_team_wire();
              wire["agents"][0]["id"] = json!(id);
              assert_eq!(paths(&wire), ["/agents/0/id"], "{id:?}");
          }
      }

      #[test]
      fn refuses_a_role_no_agent_can_hold() {
          // The contract schema's roles include `human`, because a contract may name the human as a
          // reviewer. No agent is a human, so the team schema's roles are the other five.
          let mut wire = a_team_wire();
          wire["agents"][0]["role"] = json!("human");
          assert_eq!(paths(&wire), ["/agents/0/role"]);
      }

      #[test]
      fn names_every_agent_id_that_appears_twice() {
          let mut wire = a_team_wire();
          wire["agents"] = json!([
              an_agent_wire("ada", "product_manager"),
              an_agent_wire("ada", "software_developer"),
              an_agent_wire("linus", "architect"),
              an_agent_wire("linus", "scrum_master"),
          ]);
          let refusals = refusals(&wire);
          assert_eq!(refusals.len(), 1, "{refusals:?}");
          assert_eq!(refusals[0].0, "/agents");
          assert_eq!(
              refusals[0].1,
              "an agent id names one agent, and the ids ada, linus name more than one"
          );
      }

      #[test]
      fn refuses_a_team_with_nobody_to_write_a_contract_or_do_the_work() {
          let mut wire = a_team_wire();
          wire["agents"] = json!([
              an_agent_wire("ada", "architect"),
              an_agent_wire("linus", "scrum_master"),
          ]);
          let messages: Vec<String> = refusals(&wire)
              .into_iter()
              .map(|(_, message)| message)
              .collect();
          assert_eq!(
              messages,
              [
                  "a team needs an active product_manager to write a contract; this one has none \
                   (docs/SPEC.md section 3)",
                  "a team needs an active software_developer to do the work; this one has none \
                   (docs/SPEC.md section 3)",
              ],
              "both, not the first of them"
          );
      }

      #[test]
      fn reports_a_repeated_id_and_a_missing_role_together() {
          // Two problems in one file are two refusals: a person fixing one at a time is a person
          // running the command twice.
          let mut wire = a_team_wire();
          wire["agents"] = json!([
              an_agent_wire("ada", "product_manager"),
              an_agent_wire("ada", "architect"),
          ]);
          let messages: Vec<String> = refusals(&wire)
              .into_iter()
              .map(|(_, message)| message)
              .collect();
          assert_eq!(messages.len(), 2, "{messages:?}");
          assert!(messages[0].contains("the id ada names"), "{}", messages[0]);
          assert!(
              messages[1].contains("software_developer"),
              "{}",
              messages[1]
          );
      }

      #[test]
      fn a_paused_product_manager_is_not_an_active_one() {
          // A paused agent keeps the work it holds and is given nothing new, so a team whose only
          // Product Manager is paused cannot start anything.
          let mut wire = a_team_wire();
          wire["agents"][0]["status"] = json!("paused");
          let messages: Vec<String> = refusals(&wire)
              .into_iter()
              .map(|(_, message)| message)
              .collect();
          assert_eq!(messages.len(), 1, "{messages:?}");
          assert!(messages[0].contains("product_manager"), "{}", messages[0]);
      }

      #[test]
      fn fills_in_every_rule_the_team_left_out() {
          let rules = team(&a_team_wire()).rules();
          assert_eq!(
              rules.protected_paths,
              DEFAULT_PROTECTED_PATHS.map(str::to_string),
              "the five farik-core ships"
          );
          assert!(rules.allowed_paths_ceiling.is_empty(), "no ceiling");
          assert!(rules.required_criteria.is_empty());
          assert!(!rules.require_new_tests);
          assert!(
              rules
                  .max_task_budget_usd
                  .is_some_and(|cap| close(cap, DEFAULT_MAX_TASK_BUDGET_USD))
          );
          assert!(rules.forbidden_commands.is_empty());
      }

      #[test]
      fn reads_the_rules_a_team_wrote() {
          let rules = team(&a_full_team_wire()).rules();
          assert_eq!(rules.allowed_paths_ceiling, ["src/**"]);
          assert_eq!(rules.required_criteria, ["test", "review"]);
          assert!(rules.require_new_tests);
          assert!(
              rules
                  .max_task_budget_usd
                  .is_some_and(|cap| close(cap, 12.5))
          );
          assert_eq!(rules.forbidden_commands, ["^rm -rf /"]);
      }

      #[test]
      fn keeps_the_protected_paths_it_ships_and_adds_the_team_s() {
          // 5.12: rules only narrow what a tier allows. A team that could drop `.env` from the list
          // would be widening one, so the shipped paths are kept whatever the team writes.
          let mut wire = a_team_wire();
          wire["rules"]["protected_paths"] = json!(["infra/**", ".env"]);
          let rules = team(&wire).rules();
          for shipped in DEFAULT_PROTECTED_PATHS {
              assert!(
                  rules.protected_paths.contains(&shipped.to_string()),
                  "{shipped} is kept"
              );
          }
          assert!(rules.protected_paths.contains(&"infra/**".to_string()));
          assert_eq!(
              rules
                  .protected_paths
                  .iter()
                  .filter(|path| *path == ".env")
                  .count(),
              1,
              "and a path the team repeated is still one path"
          );
      }

      #[test]
      fn counts_only_the_agents_that_take_work() {
          let mut wire = a_team_wire();
          wire["agents"] = json!([
              an_agent_wire("ada", "product_manager"),
              an_agent_wire("linus", "software_developer"),
              an_agent_wire("grace", "architect"),
              an_agent_wire("alan", "scrum_master"),
          ]);
          wire["agents"][2]["status"] = json!("paused");
          wire["agents"][3]["status"] = json!("retired");
          let team = team(&wire);
          assert_eq!(
              team.active_agents()
                  .map(|agent| agent.id.to_string())
                  .collect::<Vec<_>>(),
              ["ada", "linus"]
          );
          assert!(team.has_active(Role::ProductManager));
          assert!(team.has_active(Role::SoftwareDeveloper));
          assert!(!team.has_active(Role::Architect), "paused");
          assert!(!team.has_active(Role::ScrumMaster), "retired");
          assert!(!team.has_active(Role::Human), "the human is not an agent");
          assert_eq!(team.agents[2].status, AgentStatus::Paused);
      }

      #[test]
      fn a_work_in_progress_limit_of_zero_is_how_an_agent_is_paused() {
          // 5.2: a limit of zero refuses every assignment, which is how a team pauses an agent
          // without retiring it, and the governor already answers "takes no work: its limit is zero".
          // A schema that would not let a person write the number would make that answer unreachable.
          let mut wire = a_team_wire();
          wire["policy"]["wip_limit_per_agent"] = json!(0);
          assert_eq!(team(&wire).policy.wip_limit_per_agent, 0);
      }

      #[test]
      fn refuses_a_number_outside_what_a_rule_allows() {
          // Every bound here carries a spec number: 5.2's work-in-progress limit, 5.7's blocked age
          // and iteration count, 5.5's daily budget, 5.12's task cap and its list of methods. A bound
          // nothing tests is a bound the next person deletes to make something else compile.
          for (pointer, value) in [
              ("/policy/wip_limit_per_agent", json!(-1)),
              ("/policy/wip_limit_per_agent", json!(101)),
              ("/policy/blocked_limit_hours", json!(0)),
              ("/policy/blocked_limit_hours", json!(721)),
              ("/policy/max_iterations", json!(0)),
              ("/policy/max_iterations", json!(101)),
              ("/budgets/daily_usd", json!(0)),
              ("/rules/max_task_budget_usd", json!(0)),
              ("/rules/required_criteria", json!(["vibes"])),
          ] {
              let mut wire = a_full_team_wire();
              *wire
                  .pointer_mut(pointer)
                  .expect("the full fixture has every field") = value.clone();
              let paths = paths(&wire);
              assert!(
                  !paths.is_empty() && paths.iter().all(|path| path.starts_with(pointer)),
                  "{pointer} = {value}: {paths:?}"
              );
          }
      }

      #[test]
      fn a_role_s_tiers_are_the_user_s_to_widen_and_to_narrow() {
          // 5.6: a role's tiers are overridable per agent. Ada is a Product Manager, whose defaults
          // are read and network; she is granted execute and denied network.
          let team = team(&a_full_team_wire());
          assert_eq!(
              team.agents[0].tiers(),
              [PermissionTier::Read, PermissionTier::Execute]
          );
          assert_eq!(
              team.agents[1].tiers(),
              [
                  PermissionTier::Read,
                  PermissionTier::WriteWorkspace,
                  PermissionTier::Execute,
                  PermissionTier::GitLocal,
              ],
              "and an agent that overrides nothing holds what its role holds"
          );
      }

      #[test]
      fn taking_a_tier_away_wins_over_granting_it() {
          // Both lists naming one tier is a person's mistake, and the narrower reading of a mistake
          // is the safer one.
          let mut wire = a_team_wire();
          wire["agents"][1]["grants"] = json!(["git_remote", "read"]);
          wire["agents"][1]["revokes"] = json!(["git_remote", "execute"]);
          let team = team(&wire);
          assert_eq!(
              team.agents[1].tiers(),
              [
                  PermissionTier::Read,
                  PermissionTier::WriteWorkspace,
                  PermissionTier::GitLocal,
              ],
              "execute taken away, git_remote granted and taken away, read granted twice over"
          );
      }

      #[test]
      fn names_every_role_an_agent_can_hold() {
          assert_eq!(
              [
                  RoleWire::ProductManager,
                  RoleWire::ScrumMaster,
                  RoleWire::Architect,
                  RoleWire::SoftwareDeveloper,
                  RoleWire::MarketingSpecialist,
              ]
              .map(Role::from),
              [
                  Role::ProductManager,
                  Role::ScrumMaster,
                  Role::Architect,
                  Role::SoftwareDeveloper,
                  Role::MarketingSpecialist,
              ]
          );
      }

      #[test]
      fn names_every_permission_tier_an_agent_can_be_granted() {
          assert_eq!(
              [
                  PermissionTierWire::Read,
                  PermissionTierWire::WriteWorkspace,
                  PermissionTierWire::Execute,
                  PermissionTierWire::Network,
                  PermissionTierWire::GitLocal,
                  PermissionTierWire::GitRemote,
                  PermissionTierWire::ExternalEffect,
              ]
              .map(PermissionTier::from),
              [
                  PermissionTier::Read,
                  PermissionTier::WriteWorkspace,
                  PermissionTier::Execute,
                  PermissionTier::Network,
                  PermissionTier::GitLocal,
                  PermissionTier::GitRemote,
                  PermissionTier::ExternalEffect,
              ]
          );
      }
  }
  ```

- [ ] Run them and confirm they fail because nothing answers what the rules are:

  ```
  cargo test -p farik-core --lib
  # expected: FAIL to compile, one error per call site and nothing else:
  # error[E0599]: no method named `rules` found for struct `FarikTeam` in the current
  #   scope  (three times)
  # error: could not compile `farik-core` (lib test) due to 3 previous errors
  ```

- [ ] Write the minimal implementation. In `crates/core/src/team.rs`, add to the imports, after the `crate::governor::permissions` line:

  ```rust
  use crate::governor::team_rules::TeamRules;
  ```

- [ ] Insert into `impl Team`, before `active_agents`:

  ```rust
      /// The rules the governor applies, with what the team left out filled in from what
      /// `farik-core` ships (`docs/SPEC.md` section 5.12).
      ///
      /// The shipped protected paths are kept whatever the team writes, and the team's are added to
      /// them: 5.12 says rules only narrow what a tier allows, and a team that could delete `.env`
      /// from the list would be widening one.
      #[must_use]
      pub fn rules(&self) -> TeamRules {
          let shipped = TeamRules::default();
          let mut protected_paths = shipped.protected_paths;
          for path in &self.rules.protected_paths {
              let path = path.to_string();
              if !protected_paths.contains(&path) {
                  protected_paths.push(path);
              }
          }
          TeamRules {
              protected_paths,
              allowed_paths_ceiling: self
                  .rules
                  .allowed_paths_ceiling
                  .iter()
                  .map(|glob| glob.to_string())
                  .collect(),
              required_criteria: self
                  .rules
                  .required_criteria
                  .as_deref()
                  .unwrap_or_default()
                  .iter()
                  .map(std::string::ToString::to_string)
                  .collect(),
              require_new_tests: self
                  .rules
                  .require_new_tests
                  .unwrap_or(shipped.require_new_tests),
              max_task_budget_usd: self
                  .rules
                  .max_task_budget_usd
                  .or(shipped.max_task_budget_usd),
              forbidden_commands: self
                  .rules
                  .forbidden_commands
                  .iter()
                  .map(|pattern| pattern.to_string())
                  .collect(),
          }
      }
  ```

- [ ] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 245 passed (farik-core)
  #   test result: ok. 15 passed (crates/store/tests/git.rs)
  #   test result: ok. 29 passed (xtask)
  ```

- [ ] Commit: `feat(core): fill in the rules a team left out`

### Task 4: The criterion library

Files: created `docs/schemas/criteria.schema.json`, `crates/core/src/criteria.rs`, `crates/core/src/criteria/fixtures.rs`; modified `crates/core/src/lib.rs`, `crates/core/src/generated/mod.rs`, `xtask/src/generate.rs`, `docs/plans/phase-2-protocol-store-cli/step-05-team-and-criteria.md`

Consumes: the four `pub(crate)` helpers Task 1 made
Produces: `farik_core::criteria::{CriteriaLibrary, CriterionTemplate, validate_criteria}`

- [ ] Write the failing tests. Create `crates/core/src/criteria.rs` with the module doc and the fixtures declaration:

  ```rust
  //! The criterion library (`docs/SPEC.md` section 5.13): `docs/schemas/criteria.schema.json` as
  //! Rust types, its validator, and the expansion that turns a reference by name into a contract's
  //! own exit criterion.

  /// Wire fixtures for tests, in this crate and in others.
  pub mod fixtures;
  ```

- [ ] Create `crates/core/src/criteria/fixtures.rs`:

  ```rust
  use serde_json::{Value, json};

  /// A schema-valid wire criterion library holding one criterion of every verification method, in
  /// the order the schema lists them.
  #[must_use]
  pub fn a_criteria_library_wire() -> Value {
      json!({
          "criteria": [
              {
                  "name": "cargo-check",
                  "text": "The repository's own check command passes.",
                  "source": "project_scan",
                  "verification": {
                      "method": "command",
                      "command": "cargo xtask check",
                      "expect": {
                          "exit_code": 2,
                          "stdout_contains": "xtask check: ok",
                          "stdout_not_contains": "warning"
                      }
                  }
              },
              {
                  "name": "unit-tests",
                  "text": "The unit tests pass, and this change adds one.",
                  "source": "project_scan",
                  "verification": {
                      "method": "test",
                      "command": "cargo test --workspace",
                      "new_tests_required": true
                  }
              },
              {
                  "name": "decision-recorded",
                  "text": "An architecture decision record says why.",
                  "verification": {
                      "method": "artifact",
                      "path": "docs/decisions",
                      "must_contain": ["Status: accepted"]
                  }
              },
              {
                  "name": "reviewed-for-clarity",
                  "text": "A reviewer read it and found it clear.",
                  "verification": {
                      "method": "review",
                      "rubric": ["Does every public item say what it is for?"]
                  }
              },
              {
                  "name": "human-accepted",
                  "text": "The human accepted the result.",
                  "verification": {
                      "method": "human",
                      "question": "Does this do what you asked for?"
                  }
              },
              {
                  "name": "tests-pass",
                  "text": "The tests pass, and this change need not add one.",
                  "verification": {
                      "method": "test",
                      "command": "cargo test --workspace"
                  }
              }
          ]
      })
  }

  /// A library with nothing in it: what a project whose scan found no check command starts with.
  #[must_use]
  pub fn an_empty_criteria_library_wire() -> Value {
      json!({ "criteria": [] })
  }
  ```

- [ ] Append the tests module to `crates/core/src/criteria.rs`:

  ```rust
  #[cfg(test)]
  mod tests {
      use serde_json::{Value, json};

      use super::fixtures::{a_criteria_library_wire, an_empty_criteria_library_wire};
      use super::{CriteriaLibrary, CriterionSource, validate_criteria};

      fn library(wire: &Value) -> CriteriaLibrary {
          validate_criteria(wire).expect("the fixture is a library")
      }

      fn refusals(wire: &Value) -> Vec<(String, String)> {
          validate_criteria(wire)
              .expect_err("this wire library is refused")
              .into_iter()
              .map(|error| (error.path, error.message))
              .collect()
      }

      fn paths(wire: &Value) -> Vec<String> {
          refusals(wire).into_iter().map(|(path, _)| path).collect()
      }

      #[test]
      fn reads_a_library_with_a_criterion_of_every_method() {
          let library = library(&a_criteria_library_wire());
          assert_eq!(library.criteria.len(), 6);
          assert_eq!(library.criteria[0].name.as_str(), "cargo-check");
          assert_eq!(
              library.criteria[0].source,
              Some(CriterionSource::ProjectScan)
          );
          assert_eq!(
              library.criteria[2].source, None,
              "left out, and a refresh of the scan will not touch it"
          );
      }

      #[test]
      fn reads_a_library_with_nothing_in_it() {
          // What a project whose scan found no check command starts with. A file that must hold at
          // least one criterion would mean `farik init` could not write one.
          assert!(
              library(&an_empty_criteria_library_wire())
                  .criteria
                  .is_empty()
          );
      }

      #[test]
      fn refuses_a_name_that_is_not_a_slug() {
          for name in ["Cargo Check", "cargo_check", "-check", ""] {
              let mut wire = a_criteria_library_wire();
              wire["criteria"][0]["name"] = json!(name);
              assert_eq!(paths(&wire), ["/criteria/0/name"], "{name:?}");
          }
      }

      #[test]
      fn refuses_a_criterion_too_short_to_say_anything() {
          let mut wire = a_criteria_library_wire();
          wire["criteria"][0]["text"] = json!("passes");
          assert_eq!(paths(&wire), ["/criteria/0/text"]);
      }

      #[test]
      fn refuses_a_verification_method_that_is_not_one() {
          let mut wire = a_criteria_library_wire();
          wire["criteria"][0]["verification"] = json!({ "method": "vibes" });
          assert_eq!(paths(&wire), ["/criteria/0/verification"]);
      }

      #[test]
      fn names_every_name_used_twice() {
          let mut wire = a_criteria_library_wire();
          wire["criteria"][1]["name"] = json!("cargo-check");
          wire["criteria"][3]["name"] = json!("decision-recorded");
          let refusals = refusals(&wire);
          assert_eq!(refusals.len(), 1, "{refusals:?}");
          assert_eq!(refusals[0].0, "/criteria");
          assert_eq!(
              refusals[0].1,
              "a name names one criterion, and the names cargo-check, decision-recorded used more \
               than once"
          );
      }
  }
  ```

- [ ] Declare the module in `crates/core/src/lib.rs`, after `contract` and before `generated`:

  ```rust
  /// The criterion library and its validator.
  pub mod criteria;
  ```

- [ ] Run them and confirm they fail because nothing of the library exists:

  ```
  cargo test -p farik-core --lib
  # expected: FAIL to compile,
  # error[E0432]: unresolved imports `super::CriteriaLibrary`, `super::CriterionSource`,
  #   `super::validate_criteria`
  # error: could not compile `farik-core` (lib test) due to 1 previous error
  ```

- [ ] Write the schema. Create `docs/schemas/criteria.schema.json`. Its `verification` is a copy of the task contract schema's, and task 5 adds the test that holds the two copies to being identical:

  ```json
  {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "$id": "https://farik.dev/schemas/criteria.schema.json",
    "title": "FarikCriteriaLibrary",
    "description": "The named, reusable exit criteria a project verifies its work with, stored as .farik/team/criteria.yaml: the project's own check and test commands found by the project scan, and any the human adds. Referenced by name when a contract is written and expanded into it, so that contracts across a project verify the same way. See docs/SPEC.md section 5.13.",
    "type": "object",
    "additionalProperties": false,
    "required": [
      "criteria"
    ],
    "properties": {
      "criteria": {
        "type": "array",
        "maxItems": 200,
        "items": {
          "$ref": "#/$defs/criterionTemplate"
        },
        "description": "Names are unique, which JSON Schema cannot say and validate_criteria therefore does. A library may be empty: a project whose scan found nothing still has a file."
      }
    },
    "$defs": {
      "criterionTemplate": {
        "type": "object",
        "additionalProperties": false,
        "required": [
          "name",
          "text",
          "verification"
        ],
        "properties": {
          "name": {
            "type": "string",
            "pattern": "^[a-z0-9]+(-[a-z0-9]+)*$",
            "maxLength": 64,
            "description": "What a contract refers to this criterion by. Kebab-case, unique in the library."
          },
          "text": {
            "type": "string",
            "minLength": 10,
            "description": "What the criterion says, in the words it will carry into every contract that uses it."
          },
          "source": {
            "type": "string",
            "enum": [
              "project_scan",
              "human"
            ],
            "description": "Where it came from. Left out, the human's: a refresh of the project scan replaces what the scan found and never touches what a person wrote."
          },
          "verification": {
            "type": "object",
            "required": [
              "method"
            ],
            "oneOf": [
              {
                "additionalProperties": false,
                "required": [
                  "method",
                  "command",
                  "expect"
                ],
                "properties": {
                  "method": {
                    "const": "command"
                  },
                  "command": {
                    "type": "string",
                    "description": "Run inside the sandbox from the project root."
                  },
                  "expect": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                      "exit_code": {
                        "type": "integer",
                        "default": 0
                      },
                      "stdout_contains": {
                        "type": "string"
                      },
                      "stdout_not_contains": {
                        "type": "string"
                      }
                    }
                  }
                }
              },
              {
                "additionalProperties": false,
                "required": [
                  "method",
                  "command"
                ],
                "properties": {
                  "method": {
                    "const": "test"
                  },
                  "command": {
                    "type": "string",
                    "description": "A test command. Passes on exit code 0."
                  },
                  "new_tests_required": {
                    "type": "boolean",
                    "default": false,
                    "description": "If true, the reviewer checks that the diff adds at least one test and that it fails on the base branch."
                  }
                }
              },
              {
                "additionalProperties": false,
                "required": [
                  "method",
                  "path"
                ],
                "properties": {
                  "method": {
                    "const": "artifact"
                  },
                  "path": {
                    "type": "string",
                    "description": "A file that must exist after the task, relative to the project root."
                  },
                  "must_contain": {
                    "type": "array",
                    "maxItems": 100,
                    "items": {
                      "type": "string"
                    }
                  }
                }
              },
              {
                "additionalProperties": false,
                "required": [
                  "method",
                  "rubric"
                ],
                "properties": {
                  "method": {
                    "const": "review"
                  },
                  "rubric": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 100,
                    "items": {
                      "type": "string"
                    },
                    "description": "Yes/no questions the reviewer answers with a cited reason each. Used where no command can decide."
                  }
                }
              },
              {
                "additionalProperties": false,
                "required": [
                  "method",
                  "question"
                ],
                "properties": {
                  "method": {
                    "const": "human"
                  },
                  "question": {
                    "type": "string",
                    "description": "What the human is asked to confirm. Satisfied only by a human.accepted event."
                  }
                }
              }
            ]
          }
        }
      }
    }
  }
  ```

- [ ] Generate its types. In `xtask/src/generate.rs`, change the array's length to 6 and add the entry after the team's:

  ```rust
      GeneratedSchema {
          schema: "docs/schemas/criteria.schema.json",
          types: "crates/core/src/generated/criteria.rs",
          schema_copy: "crates/core/src/generated/criteria.schema.json",
      },
  ```

  then run the generator:

  ```
  cargo xtask generate
  # expected: generated crates/core/src/generated/criteria.rs
  #           generated crates/core/src/generated/criteria.schema.json
  ```

- [ ] Declare the generated module in `crates/core/src/generated/mod.rs`. The list is alphabetical, so this goes before `prices`:

  ```rust
  pub mod criteria;
  ```

- [ ] Write the minimal implementation. In `crates/core/src/criteria.rs`, replace the module doc with the doc and its imports:

  ```rust
  //! The criterion library (`docs/SPEC.md` section 5.13): `docs/schemas/criteria.schema.json` as
  //! Rust types, its validator, and the expansion that turns a reference by name into a contract's
  //! own exit criterion.

  use std::sync::LazyLock;

  use jsonschema::Validator;
  use serde_json::Value;

  use crate::contract::{pointer, repeated_ids, with_integers_normalised};
  use crate::text::listed;

  pub use crate::contract::ValidationError;
  pub use crate::generated::criteria::{
      CriterionTemplate, CriterionTemplateSource as CriterionSource,
      FarikCriteriaLibrary as CriteriaLibrary,
  };

  /// Wire fixtures for tests, in this crate and in others.
  pub mod fixtures;
  ```

  then insert, between that and the tests module:

  ```rust
  const SCHEMA_JSON: &str = include_str!("generated/criteria.schema.json");

  static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
      let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
          "the embedded criteria schema is valid JSON: it is a copy of docs/schemas/ written by \
           cargo xtask generate and checked for freshness by cargo xtask check",
      );
      jsonschema::options()
          .should_validate_formats(true)
          .build(&schema)
          .expect(
              "the embedded criteria schema compiles: it is JSON Schema 2020-12 with no external \
               references, and the generator already parsed it",
          )
  });
  ```

  and after that:

  ```rust
  /// Checks a value against `docs/schemas/criteria.schema.json` and, when it conforms, returns the
  /// typed library.
  ///
  /// One rule is this function's rather than the schema's: a name names one criterion. JSON Schema
  /// cannot say that of an array's items, and a library with one name twice would expand to whichever
  /// of the two came first, which is not something a person should have to know.
  ///
  /// # Errors
  ///
  /// Every schema violation, each at its own JSON pointer; or one error naming every repeated name;
  /// or, when the schema passes and the typed library cannot be built, one error at the root.
  pub fn validate_criteria(input: &Value) -> Result<CriteriaLibrary, Vec<ValidationError>> {
      let errors: Vec<ValidationError> = VALIDATOR
          .iter_errors(input)
          .map(|error| ValidationError {
              path: pointer(&error.instance_path().to_string()),
              message: error.to_string(),
          })
          .collect();
      if !errors.is_empty() {
          return Err(errors);
      }
      let library = serde_json::from_value::<CriteriaLibrary>(with_integers_normalised(input))
          .map_err(|error| {
              vec![ValidationError {
                  path: "/".to_string(),
                  message: format!(
                      "the schema passed but the typed criterion library could not be built: \
                           {error}"
                  ),
              }]
          })?;
      let repeated = repeated_ids(
          library
              .criteria
              .iter()
              .map(|criterion| criterion.name.as_str()),
      );
      if repeated.is_empty() {
          Ok(library)
      } else {
          Err(vec![ValidationError {
              path: "/criteria".to_string(),
              message: format!(
                  "a name names one criterion, and {} used more than once",
                  listed("the name", "the names", &repeated)
              ),
          }])
      }
  }
  ```

- [ ] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 251 passed (farik-core)
  #   test result: ok. 15 passed (crates/store/tests/git.rs)
  #   test result: ok. 29 passed (xtask)
  ```

- [ ] Commit: `feat(core): read a criterion library and refuse a repeated name`

### Task 5: Expanding a reference into a criterion

Files: modified `crates/core/src/criteria.rs`, `docs/plans/phase-2-protocol-store-cli/step-05-team-and-criteria.md`

Consumes: everything above
Produces: `farik_core::criteria::{CriteriaError, expand_criteria}`

- [ ] Write the failing tests. Replace the whole tests module of `crates/core/src/criteria.rs` with:

  ```rust
  #[cfg(test)]
  mod tests {
      use serde_json::{Value, json};

      use super::fixtures::{a_criteria_library_wire, an_empty_criteria_library_wire};
      use super::{
          CriteriaError, CriteriaLibrary, CriterionSource, SCHEMA_JSON, expand_criteria,
          validate_criteria,
      };
      use crate::contract::{Verification, fixtures::a_contract_wire, validate_contract};

      fn library(wire: &Value) -> CriteriaLibrary {
          validate_criteria(wire).expect("the fixture is a library")
      }

      fn refusals(wire: &Value) -> Vec<(String, String)> {
          validate_criteria(wire)
              .expect_err("this wire library is refused")
              .into_iter()
              .map(|error| (error.path, error.message))
              .collect()
      }

      fn paths(wire: &Value) -> Vec<String> {
          refusals(wire).into_iter().map(|(path, _)| path).collect()
      }

      /// Every criterion of the fixture, referenced in order as C1 to C5.
      fn every_reference() -> Vec<(String, String)> {
          [
              "cargo-check",
              "unit-tests",
              "decision-recorded",
              "reviewed-for-clarity",
              "human-accepted",
              "tests-pass",
          ]
          .iter()
          .enumerate()
          .map(|(index, name)| (format!("C{}", index + 1), (*name).to_string()))
          .collect()
      }

      #[test]
      fn reads_a_library_with_a_criterion_of_every_method() {
          let library = library(&a_criteria_library_wire());
          assert_eq!(library.criteria.len(), 6);
          assert_eq!(library.criteria[0].name.as_str(), "cargo-check");
          assert_eq!(
              library.criteria[0].source,
              Some(CriterionSource::ProjectScan)
          );
          assert_eq!(
              library.criteria[2].source, None,
              "left out, and a refresh of the scan will not touch it"
          );
      }

      #[test]
      fn reads_a_library_with_nothing_in_it() {
          // What a project whose scan found no check command starts with. A file that must hold at
          // least one criterion would mean `farik init` could not write one.
          assert!(
              library(&an_empty_criteria_library_wire())
                  .criteria
                  .is_empty()
          );
      }

      #[test]
      fn refuses_a_name_that_is_not_a_slug() {
          for name in ["Cargo Check", "cargo_check", "-check", ""] {
              let mut wire = a_criteria_library_wire();
              wire["criteria"][0]["name"] = json!(name);
              assert_eq!(paths(&wire), ["/criteria/0/name"], "{name:?}");
          }
      }

      #[test]
      fn refuses_a_criterion_too_short_to_say_anything() {
          let mut wire = a_criteria_library_wire();
          wire["criteria"][0]["text"] = json!("passes");
          assert_eq!(paths(&wire), ["/criteria/0/text"]);
      }

      #[test]
      fn refuses_a_verification_method_that_is_not_one() {
          let mut wire = a_criteria_library_wire();
          wire["criteria"][0]["verification"] = json!({ "method": "vibes" });
          assert_eq!(paths(&wire), ["/criteria/0/verification"]);
      }

      #[test]
      fn names_every_name_used_twice() {
          let mut wire = a_criteria_library_wire();
          wire["criteria"][1]["name"] = json!("cargo-check");
          wire["criteria"][3]["name"] = json!("decision-recorded");
          let refusals = refusals(&wire);
          assert_eq!(refusals.len(), 1, "{refusals:?}");
          assert_eq!(refusals[0].0, "/criteria");
          assert_eq!(
              refusals[0].1,
              "a name names one criterion, and the names cargo-check, decision-recorded used more \
               than once"
          );
      }

      #[test]
      fn expands_a_reference_into_the_contract_s_own_criterion() {
          let library = library(&a_criteria_library_wire());
          let expanded = expand_criteria(&every_reference(), &library).expect("every name is known");
          assert_eq!(
              expanded
                  .iter()
                  .map(|criterion| criterion.id.to_string())
                  .collect::<Vec<_>>(),
              ["C1", "C2", "C3", "C4", "C5", "C6"],
              "in the order they were asked for, with the ids the caller named"
          );
          assert_eq!(
              expanded[0].text.as_str(),
              "The repository's own check command passes.",
              "the library's words, carried in"
          );
          assert_eq!(
              expanded
                  .iter()
                  .map(|criterion| Verification::from(&criterion.verification))
                  .collect::<Vec<_>>(),
              [
                  Verification::Command {
                      command: "cargo xtask check".to_string(),
                      exit_code: 2,
                      stdout_contains: Some("xtask check: ok".to_string()),
                      stdout_not_contains: Some("warning".to_string()),
                  },
                  Verification::Test {
                      command: "cargo test --workspace".to_string(),
                      new_tests_required: true,
                  },
                  Verification::Artifact {
                      path: "docs/decisions".to_string(),
                      must_contain: vec!["Status: accepted".to_string()],
                  },
                  Verification::Review {
                      rubric: vec!["Does every public item say what it is for?".to_string()],
                  },
                  Verification::Human {
                      question: "Does this do what you asked for?".to_string(),
                  },
                  Verification::Test {
                      command: "cargo test --workspace".to_string(),
                      new_tests_required: false,
                  },
              ],
              "every method, and both sides of every field the mapping carries"
          );
      }

      #[test]
      fn leaves_what_a_criterion_satisfies_to_the_contract() {
          // Which requirements a criterion provides evidence for is a fact about one contract, and
          // the library has never heard of that contract's requirements.
          let library = library(&a_criteria_library_wire());
          let expanded = expand_criteria(&[("C1".to_string(), "cargo-check".to_string())], &library)
              .expect("the name is known");
          assert!(expanded[0].satisfies.is_empty());
      }

      #[test]
      fn an_expanded_criterion_is_one_a_contract_takes() {
          // The point of the library: what comes out of it goes into a contract without further
          // work, and the contract's own validator is what says so.
          let library = library(&a_criteria_library_wire());
          let expanded = expand_criteria(&every_reference(), &library).expect("every name is known");
          let mut wire = a_contract_wire();
          wire["exit_criteria"] = Value::Array(
              expanded
                  .iter()
                  .map(|criterion| serde_json::to_value(criterion).expect("a criterion serialises"))
                  .collect(),
          );
          let contract = validate_contract(&wire).expect("the contract holds");
          assert_eq!(contract.exit_criteria.len(), 6);
      }

      #[test]
      fn refuses_a_name_the_library_does_not_hold() {
          let library = library(&a_criteria_library_wire());
          assert_eq!(
              expand_criteria(
                  &[
                      ("C1".to_string(), "cargo-check".to_string()),
                      ("C2".to_string(), "pnpm-check".to_string()),
                  ],
                  &library,
              ),
              Err(CriteriaError::UnknownCriterion {
                  name: "pnpm-check".to_string(),
              })
          );
      }

      #[test]
      fn refuses_an_id_a_contract_would_not_take() {
          let library = library(&a_criteria_library_wire());
          let refused = expand_criteria(&[("C0".to_string(), "cargo-check".to_string())], &library);
          let Err(CriteriaError::Refused { name, detail }) = refused else {
              panic!("a contract's ids start at C1: {refused:?}");
          };
          assert_eq!(name, "cargo-check");
          assert!(
              detail.starts_with("C0 is not a criterion id a contract takes"),
              "{detail}"
          );
      }

      #[test]
      fn says_what_it_refused_and_why_in_plain_words() {
          let said: Vec<String> = [
              CriteriaError::UnknownCriterion {
                  name: "pnpm-check".to_string(),
              },
              CriteriaError::Refused {
                  name: "cargo-check".to_string(),
                  detail: "C0 is not a criterion id a contract takes".to_string(),
              },
          ]
          .iter()
          .map(std::string::ToString::to_string)
          .collect();
          assert_eq!(
              said,
              [
                  "the criterion library has no pnpm-check",
                  "the criterion cargo-check cannot go in a contract: C0 is not a criterion id a \
                   contract takes",
              ]
          );
      }

      #[test]
      fn the_two_schemas_say_the_same_thing_about_a_verification() {
          // `criteria.schema.json` carries a copy of the contract schema's `verification`, and
          // `verification_of` is the mapping between the two Rust types that copy produces. If the
          // two ever drift, that mapping quietly starts lying, so the copy is checked here.
          let contract: Value =
              serde_json::from_str(include_str!("generated/task_contract.schema.json"))
                  .expect("the embedded contract schema is valid JSON");
          let library: Value =
              serde_json::from_str(SCHEMA_JSON).expect("the embedded criteria schema is valid JSON");
          let copy = &library["$defs"]["criterionTemplate"]["properties"]["verification"];
          assert!(
              copy.is_object(),
              "the criteria schema still keeps its verification where this test looks; two pointers \
               that both went stale would compare null to null and hold nothing"
          );
          assert_eq!(
              copy,
              &contract["$defs"]["exitCriterion"]["properties"]["verification"],
          );
      }
  }
  ```

- [ ] Run them and confirm they fail because nothing expands a reference:

  ```
  cargo test -p farik-core --lib
  # expected: FAIL to compile,
  # error[E0432]: unresolved imports `super::CriteriaError`, `super::expand_criteria`
  # error: could not compile `farik-core` (lib test) due to 1 previous error
  ```

- [ ] Write the minimal implementation. In `crates/core/src/criteria.rs`, replace the imports, whole, with:

  ```rust
  //! The criterion library (`docs/SPEC.md` section 5.13): `docs/schemas/criteria.schema.json` as
  //! Rust types, its validator, and the expansion that turns a reference by name into a contract's
  //! own exit criterion.

  use std::fmt;
  use std::sync::LazyLock;

  use jsonschema::Validator;
  use serde_json::Value;

  use crate::contract::{pointer, repeated_ids, with_integers_normalised};
  use crate::text::listed;

  pub use crate::contract::{ExitCriterion, ValidationError};
  pub use crate::generated::criteria::{
      CriterionTemplate, CriterionTemplateSource as CriterionSource,
      CriterionTemplateVerification as TemplateVerification, FarikCriteriaLibrary as CriteriaLibrary,
  };
  use crate::generated::task_contract::{
      ExitCriterionId, ExitCriterionText, ExitCriterionVerification,
      ExitCriterionVerificationVariant0Expect,
  };

  /// Wire fixtures for tests, in this crate and in others.
  pub mod fixtures;
  ```

- [ ] Insert, between the validator's statics and `validate_criteria`:

  ```rust
  /// Why a reference could not be expanded into a criterion.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum CriteriaError {
      /// The library has no criterion of this name.
      UnknownCriterion {
          /// The name that was asked for.
          name: String,
      },
      /// The library's criterion is not one a contract will take. Today only an id reaches this: the
      /// caller names the id the criterion will carry, and a contract's ids are `C1`, `C2`, and so
      /// on. It also stands ready for the day the two schemas disagree about what a criterion is.
      Refused {
          /// The name of the criterion being expanded.
          name: String,
          /// What the contract's own shape refused, in its words.
          detail: String,
      },
  }

  impl fmt::Display for CriteriaError {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
          match self {
              Self::UnknownCriterion { name } => {
                  write!(formatter, "the criterion library has no {name}")
              }
              Self::Refused { name, detail } => {
                  write!(
                      formatter,
                      "the criterion {name} cannot go in a contract: {detail}"
                  )
              }
          }
      }
  }

  impl std::error::Error for CriteriaError {}
  ```

- [ ] And append, after `validate_criteria`:

  ```rust
  /// Expands references into a contract's exit criteria, in the order given.
  ///
  /// Each reference is the id the criterion will carry in the contract and the name it has in the
  /// library, in that order — `("C1", "cargo-check")` reads as the criterion it produces. A criterion
  /// arrives with no `satisfies`: which requirements it provides evidence for is a fact about one
  /// contract, not about the library.
  ///
  /// # Errors
  ///
  /// `UnknownCriterion` for the first name the library does not hold, or `Refused` when the id is not
  /// one a contract accepts.
  pub fn expand_criteria(
      refs: &[(String, String)],
      library: &CriteriaLibrary,
  ) -> Result<Vec<ExitCriterion>, CriteriaError> {
      refs.iter()
          .map(|(id, name)| {
              let template = library
                  .criteria
                  .iter()
                  .find(|criterion| criterion.name.as_str() == name)
                  .ok_or_else(|| CriteriaError::UnknownCriterion { name: name.clone() })?;
              Ok(ExitCriterion {
                  id: ExitCriterionId::try_from(id.as_str()).map_err(|error| {
                      CriteriaError::Refused {
                          name: name.clone(),
                          detail: format!("{id} is not a criterion id a contract takes: {error}"),
                      }
                  })?,
                  satisfies: Vec::new(),
                  text: ExitCriterionText::try_from(template.text.as_str()).map_err(|error| {
                      CriteriaError::Refused {
                          name: name.clone(),
                          detail: format!("its text is not one a contract takes: {error}"),
                      }
                  })?,
                  verification: verification_of(&template.verification),
              })
          })
          .collect()
  }

  /// A template's verification as a contract's.
  ///
  /// The two are the same shape because they are the same JSON: `criteria.schema.json` carries a copy
  /// of the contract schema's `verification`, and a test holds the two copies to being the same
  /// value. This is the mapping between the two Rust types that copy produces.
  fn verification_of(template: &TemplateVerification) -> ExitCriterionVerification {
      match template {
          TemplateVerification::Variant0 {
              command,
              expect,
              method,
          } => ExitCriterionVerification::Variant0 {
              command: command.clone(),
              expect: ExitCriterionVerificationVariant0Expect {
                  exit_code: expect.exit_code,
                  stdout_contains: expect.stdout_contains.clone(),
                  stdout_not_contains: expect.stdout_not_contains.clone(),
              },
              method: method.clone(),
          },
          TemplateVerification::Variant1 {
              command,
              method,
              new_tests_required,
          } => ExitCriterionVerification::Variant1 {
              command: command.clone(),
              method: method.clone(),
              new_tests_required: *new_tests_required,
          },
          TemplateVerification::Variant2 {
              method,
              must_contain,
              path,
          } => ExitCriterionVerification::Variant2 {
              method: method.clone(),
              must_contain: must_contain.clone(),
              path: path.clone(),
          },
          TemplateVerification::Variant3 { method, rubric } => ExitCriterionVerification::Variant3 {
              method: method.clone(),
              rubric: rubric.clone(),
          },
          TemplateVerification::Variant4 { method, question } => {
              ExitCriterionVerification::Variant4 {
                  method: method.clone(),
                  question: question.clone(),
              }
          }
      }
  }
  ```

- [ ] Run the check and confirm green:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 258 passed (farik-core)
  #   test result: ok. 15 passed (crates/store/tests/git.rs)
  #   test result: ok. 29 passed (xtask)
  ```

- [ ] Commit: `feat(core): expand a named criterion into a contract's own`

### Task 6: The plans say what the step became

Files: modified `docs/plans/project-plan.md`, `docs/plans/phase-2-protocol-store-cli/step-05-team-and-criteria.md`

Consumes: everything above
Produces: a project plan that describes the two validators as they are

This task changes documentation and has no test cycle. The `> ` marker on each block below is this plan's and is not part of the text to write.

- [ ] In `docs/plans/project-plan.md`, replace the phase 2 line beginning `- Step 05: \`generated::team::*\`` — everything up to and including the sentence that ends `only where the step boundary falls.` — with:

  > - Step 05 (`farik-core::team` and `farik-core::criteria`): `generated::team::*`; `Team`, `Agent`, `TeamPolicy`, `TeamBudgets` (aliases of the generated `FarikTeam`, `Agent`, `Policy`, `Budgets`); `fn validate_team(input: &Value) -> Result<Team, Vec<ValidationError>>`; `impl Team { fn rules(&self) -> TeamRules; fn active_agents(&self) -> impl Iterator<Item = &Agent>; fn has_active(&self, role: Role) -> bool }`; `impl Agent { fn tiers(&self) -> Vec<PermissionTier> }` — the role's defaults, widened by the agent's `grants` and narrowed by its `revokes`, with taking away winning over granting (added 2026-09-17 by the step 05 plan: 5.6 says a role's tiers are overridable per agent, and `grants` alone could only widen, so there was no way to say that this Developer does not run commands); `impl From<team::Role> for contract::Role` and `impl From<team::PermissionTier> for governor::PermissionTier`, the crate's one mapping layer at its edge. **Three rules are `validate_team`'s rather than the schema's** (2026-09-17, by the step 05 plan): D18 asked the schema to enforce an active Product Manager and an active Software Developer, and every way of writing that `contains` in `typify` 0.8.0 fails: one on the array is `unhandled array validation` and generates nothing, two as branches of an `allOf` generate an uninhabited `pub enum FarikTeamAgents {}` that makes the team unbuildable, and the same two at the root panic the generator. `dependentSchemas` does work, and was refused because its refusal dumps the whole agents array without naming a role, and `validate_team` returns on schema errors before its own rules run, so it would replace the readable message rather than back it up. So the schema says two to seven agents and the validator says unique ids, an active Product Manager and an active Software Developer, reporting every one that fails rather than the first. `Team::rules` keeps the protected paths `farik-core` ships whatever the team writes and adds the team's to them, because 5.12 says a rule only narrows. `generated::criteria::*`; `CriterionTemplate`, `CriteriaLibrary`; `fn validate_criteria` (a name names one criterion, which the schema cannot say either); `enum CriteriaError { UnknownCriterion { name }, Refused { name, detail } }` (the second added 2026-09-17 by the step 05 plan: the id in a reference is the caller's text and a contract's ids are `C1`, `C2`, so expanding `("C0", …)` has to answer something); `fn expand_criteria(refs: &[(String, String)], library: &CriteriaLibrary) -> Result<Vec<ExitCriterion>, CriteriaError>`, where each reference is the id the criterion will carry and the name it has in the library, in that order, and an expanded criterion arrives with an empty `satisfies` because the library has not heard of a contract's requirements. `criteria.schema.json` carries a copy of the contract schema's `verification` and a test holds the two copies identical, because a `$ref` across files would need an external-reference resolver in both `typify` and `jsonschema`. **Split from the old step 05 on 2026-09-17**, which held the file adapters as well: they are `farik-store`'s, because `farik-core` does no I/O (hard rule 5), and one plan for both halves would have been twice the size of any step so far. Nothing about what is built changed, only where the step boundary falls.

- [ ] In `docs/plans/project-plan.md`, on the D18 line, after `The schema enforces two to seven agents with at least one active Product Manager and one active Software Developer.`, add:

  > (Corrected 2026-09-17 by the step 05 plan: the schema enforces the count, and `validate_team` enforces the two roles and the unique ids, because no arrangement of `contains` in `typify` 0.8.0 both generates usable types and refuses in words a person can read.)

- [ ] In `docs/plans/project-plan.md`, on the D18 line, after the sentence just added, add:

  > An agent also has `revokes`, added 2026-09-17 by the step 05 plan, because spec 5.6 makes a role's tiers overridable per agent and `grants` alone could only widen them.

- [ ] Nothing in `docs/SPEC.md` changes. 5.6 already says a role's tiers are overridable per agent, which `revokes` is what makes true; 5.2 already says a work-in-progress limit of zero pauses an agent, which the schema now lets a person write. This step adds no rule and no event kind.

- [ ] Set this plan's `Status:` to `done` and confirm every checkbox above is ticked, each in the commit of the task it belongs to.

- [ ] Commit: `docs(docs): record what step 05 changed about the team`

## Verification

- [ ] The whole check, from the workspace root:

  ```
  cargo xtask check --integration
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 258 passed (farik-core: budget, contract, criteria, governor, pricing, team)
  #   test result: ok. 37 passed (farik-protocol)
  #   test result: ok. 44 passed (farik-store)
  #   test result: ok. 8 passed (crates/store/tests/event_log_file.rs)
  #   test result: ok. 15 passed (crates/store/tests/git.rs)
  #   test result: ok. 29 passed (xtask)
  ```

- [ ] The generated files are fresh, which the check already asks but which this step doubles the number of:

  ```
  cargo xtask generate --check
  # expected: silent
  ```

- [ ] `farik-core` still performs no I/O:

  ```
  cargo xtask core-io
  # expected: silent
  ```

- [ ] Every commit subject is accepted:

  ```
  for subject in \\
    "feat(core): read a team file and refuse one that is not" \\
    "feat(core): say who is active and what a team needs" \\
    "feat(core): fill in the rules a team left out" \\
    "feat(core): read a criterion library and refuse a repeated name" \\
    "feat(core): expand a named criterion into a contract's own" \\
    "docs(docs): record what step 05 changed about the team"; do
    printf '%s\\n' "$subject" > /tmp/subject && cargo xtask commit-msg /tmp/subject
  done
  # expected: silent, six times
  ```

## Open questions

none
