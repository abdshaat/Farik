# 0052. Task ids are CTV-<n>

Date: 2026-10-10
Status: accepted (the founder, 2026-10-10, in conversation; asked whether task ids and branch names keep `FRK-<n>`, he chose "CTV-<n>, separate PR now"). Amends ADR 0050.

## Context

ADR 0050 renamed Farik to Catervas by replacing the word `farik`, so it kept the task id prefix `FRK-<n>`, which is an abbreviation, not the word. The prefix then named a product that no longer exists, in every contract, branch, event and test. The options were to keep `FRK-<n>` for good, or to change the prefix now, before phase 8, while no release exists and no one but the founder has a project made with it.

## Decision

Task ids are `CTV-<n>`, and their branches `feature/CTV-<n>`, `fix/CTV-<n>` and `docs/CTV-<n>`. The id pattern in the schemas is `^CTV-[0-9]{1,6}$`, and no other prefix is read. Everything outside the record changes at once: schemas, code, tests, recorded transcripts, mockups and the spec (revision 0.82).

## Consequences

- A project made before this change keeps `FRK-` contracts, branches and events that Catervas no longer reads. There is no migration, as in ADR 0050: the founder starts a new project.
- The ADRs and the step plans of phases 0 to 7 keep `FRK-<n>`, as history, and so does every record that was written with it.
- The step plans of phase 8 and after read `CTV-<n>`.
