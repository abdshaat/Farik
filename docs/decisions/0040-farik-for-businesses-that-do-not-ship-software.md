# 0040. Farik for businesses that do not ship software

Date: 2026-10-05
Status: proposed. The founder set the direction on 2026-10-05 ("From product point of view. I am looking to generalize farik for non technical products as well. For example car flipping business, selling items on amazon, car rentals, etc."); the questions G1 to G6 below are the founder's, and the ADR is accepted, and planned into the project plan, when they are answered. Nothing here changes a phase before then, except that the Procurement Specialist (ADR 0039) is built so it already works for such a business.

## Context

Spec section 1 lists "non-software teams" as a non-goal of the first launch: "The role set and the harness are built around shipping software." The founder now wants Farik to run a team for a business whose product is not software: flipping cars, selling goods on Amazon, renting cars.

What in Farik today assumes software:
- **A git repository is the project** (spec 1, goal 2; 5.14). Every task has a worktree, a branch and an integration. A car-flipping business has no repository.
- **The team must have a Software Developer** (D18, spec 1 goal 1). A car-rental business may need none.
- **The role set is a product team's** (spec 6): Product Manager, Scrum Master, Architect, Developer, Designer, Marketing, with Finance and DevOps optional.
- **Exit criteria are mostly code's** (5.13): a command passes, a test passes, a file exists in the diff.

What already does not assume software, and is the seed of the change:
- **The Finance Specialist and the Procurement Specialist work in a private folder, with no branch and nothing to integrate** (spec 6.6, 6.10). Their tasks end at `accepted`, their criteria are `artifact`, `review` and `human`, and their outputs are workbooks and documents. That is how most work in a non-software business would run.
- **The governance is general**: contracts with exit criteria, nobody accepting their own work, budgets, the human's gates, the audit log, approvals for anything that reaches outside.
- **The Procurement Specialist researches any product** (the founder, 2026-10-05, O4: "cars, baby mirrors, or any product under the sun"), and contacts sellers only with the founder's approval of each message.

The realistic options:
- **A business project beside the software project.** A project is either a repository (today) or a business workspace, a private folder with no git, where every task runs as the finance and procurement tasks do. The team builder offers business roles (operations, sales, customer service, procurement, finance, marketing) and does not require a Developer. Software teams are unchanged.
- **Templates only.** Keep the repository and the Developer, and offer business-flavoured team templates. Cheap, but a car-flipping team would carry a repository and a Developer it never uses, and every task would pretend to be code.
- **After the launch.** Launch for software as planned, prove the harness, then add business projects as their own phase. Keeps the launch's scope and the proof-of-concept benchmark (phase 10) honest, which is built around software tasks.

## Decision (proposed)

Farik will run teams for businesses that do not ship software, through a second kind of project, a business workspace, built on the private-folder model the Finance and Procurement Specialists already use. The planner recommends it as its own phase after the web launch, planned from its own brainstorm, with the Procurement Specialist built product-agnostic now so it is the first role that already serves such a business.

## Consequences

Easier:
- Farik's governance, the part that is the product, reaches far more people: a non-technical founder of a small business is exactly spec 2's first persona.
- The private-folder model is already built for two roles; a business workspace generalises it rather than inventing a new one.

Harder:
- Spec 1's non-goal, D18 (a Developer in every team), goal 2 (a repository) and the role set change; each needs its own spec revision.
- New roles (operations, sales, customer service, and others a business names) each need a role, a kit and a check, as every role in phase 7 does.
- The benchmark of phase 10 measures software tasks; a business team needs its own measure or is launched unmeasured.
- Business connectors (marketplaces, listings, bookings, payments) bring more spending and sending tools, each asking the human by spec 6.7's rule.

## Open, for the founder

- **G1. When.** After the web launch as its own phase (recommended), or before it, which moves the launch.
- **G2. The project kind.** A business workspace beside the repository (recommended), or templates only.
- **G3. The first businesses.** Which to design for first, e.g. car flipping, Amazon resale, car rentals; each implies its roles and services.
- **G4. The roles.** Which business roles beyond Procurement, Finance and Marketing (e.g. Operations, Sales, Customer Service, Listings).
- **G5. A team without a Developer.** Whether D18 becomes "at least one Product Manager (or an owner's deputy) and one doer of the business's kind".
- **G6. Spending and sending.** Whether the "always asks" rule for paying, posting and sending stays as it is for business teams, where those acts are daily work.
