# 0001. Adopt the superpowers workflow for all changes

Date: 2026-09-14
Status: accepted

## Context

Farik's product is a governance harness for agent teams. Its own development will be done largely by AI agents under human direction. If the repository's process is looser than the one the product enforces, two things go wrong: the code gets worse in exactly the ways the product is meant to prevent, and the project loses the standing to tell users that discipline is worth its cost.

Three options were considered. A conventional contributing guide with tests-required and review-required rules, enforced by CI, is familiar but says nothing about how an agent should work between receiving a task and opening a pull request, which is where most agent failures happen. A custom workflow written for Farik would fit perfectly and cost weeks to write and tune. The superpowers plugin for Claude Code (obra/superpowers) already encodes a complete sequence, brainstorm, plan, TDD execution, verification with evidence, staged review, branch finishing, as skills that trigger automatically in Claude Code sessions, and its rules are strict in the same places Farik's harness is strict: nothing starts without a plan, nothing is accepted on the author's word, code before a test is deleted.

## Decision

Adopt the superpowers workflow as Farik's contribution workflow, documented in `docs/standards/workflow.md` with Farik-specific paths (`docs/plans/`, `docs/decisions/`) and additions (spec references in plans, `docs/SPEC.md` updated with behavior, the no-self-acceptance rule). Contributors using Claude Code install the plugin; everyone else follows the document by hand.

## Consequences

Every change, including the initial scaffold, gets a written plan before code. Per-change overhead goes up, especially for small changes, and the first weeks will feel slow.

The workflow depends on a third-party plugin's conventions. If the plugin changes its skills materially, `workflow.md` is the contract and the plugin is the implementation; the document wins, and this ADR is revisited.

The repository becomes a worked example of the discipline the product sells. Its plans, ADRs, and pull requests are usable as reference material for Farik's own Product Manager and Scrum Master roles.
