# Standalone UX pass, plan 7: fewer words on the Team page and setup's team step

Status: ready
Branch: `feat/ux-pass` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 4.1 (setup's team step), 4.4 (changing the team); recorded by plan 8 (revision 0.123)
Depends on: plan 1 of this folder (committed on this branch; it changed `Team.tsx`'s card line, which this plan leaves alone)
Readiness confirmed by: the planner itself, 2026-10-17 (ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The Team page (screen 19) and setup's "Who should be on your team?" (screen 06) lose their paragraphs, by the founder's rule that explanations are short bullets with the detail behind an ⓘ. Out of scope: the Designer's line and preview card in setup (`jobDesigner`, `previewSetupLead`, the preview fields), which `fix/open-issues` (ADR 0069) rewords; the start choices' own labels and notes; every role's job line other than the Designer's (each is one sentence already); the agent cards and their buttons; "Add someone".

## Decisions

The words (each `\n` a bullet where the key is drawn with `Bullets`):

| Key | Where | Before | After |
|---|---|---|---|
| `teamPageLead` | Team page, visible under the title | "{count} agents work on {name}. A team needs a Product Manager; everyone else is up to you. You can have up to seven." | "{count} agents work on {name}." |
| `teamPageRules` (new) | ⓘ beside the Team page's title | | "A team needs a Product Manager.\nEveryone else is up to you.\nSeven people at most." |
| `teamChangesBody` | visible, as `Bullets`, under "Pause, retire or replace" | one paragraph | "Pause: the agent takes no new work and leaves nothing half done.\nRetire: it leaves the team; what it learned stays in your project for whoever replaces it.\nA change starts with the agent’s next piece of work." |
| `teamCostDesigner` | visible, as `Bullets`, under "What a team like this costs" | one paragraph | "{designer} uses the same model as {developer}, so a day costs more.\nBefore changing a screen, {designer} writes a plan, which costs a little too." |
| `teamLead` | setup, the step's lead (`Wizard`'s `lead`, a `<p>`) | "We suggest six, one for each job. You can rename them now or later. To add a second developer, choose “Add someone”." | "Six people, one for each job. Rename anyone now or later." |
| `startsLegend` | setup, the start choices' legend | "Start from the team Catervas suggests, from a ready-made team, from a team you saved in another project, or from nothing. Whichever you pick, you can change anyone below." | "Where should your team come from?" |
| `setupMoreRolesNote` | setup, under "More roles" | "Roles Catervas does not suggest. Tick one to add it to your team; a team has seven people at most." | "Tick one to add it. Seven people at most." |

- The Team page's title sits in a `div.titled` (`pages.module.css`) with `<InfoTip id="team-rules-info"><Bullets text={t("teamPageRules")} /></InfoTip>`.
- "Set a limit in Costs." after the Designer's cost bullets stays as it is (`teamCostLimitBefore`, the link, `teamCostLimitAfter`), now its own `<p>` under the bullets; the small team's line (`firstDay` + `teamCostNote`) is unchanged.
- The "second developer" hint leaves setup's lead: "Add someone" is the visible link right under the list (screen 06), and the Team page adds people too. Rejected: keeping it as an ⓘ (one more button on a dense step for a rare choice).
- `Wizard`'s `lead` stays a string; no bullets in setup's lead, which is now one line.

## File map

```
apps/web/src/strings/en.ts               modifies: teamPageLead, teamChangesBody, teamCostDesigner, teamLead, startsLegend, setupMoreRolesNote; teamPageRules added
apps/web/src/pages/Team.tsx              modifies: the title's ⓘ; Bullets for the changes and the Designer's cost
apps/web/src/pages/team.test.tsx         tests:   lists_the_team_with_the_first_day_line, shows_the_designer_card_in_its_colour, counts_no_agents_in_the_designer_cost_line_of_a_small_team updated
apps/web/src/pages/setup/team.test.tsx   tests:   the lead's and legend's assertions use the new words
```

## Interfaces

Consumes: `InfoTip` (`@catervas/ui`), `Bullets` (`components/Bullets.tsx`), `onlyInTips` (`apps/web/src/test/tips.ts`), `styles.titled`.

Produces: nothing exported.

## Tasks

### Task 1: the Team page in bullets

Files: modified `apps/web/src/strings/en.ts`, `apps/web/src/pages/Team.tsx`, `apps/web/src/pages/team.test.tsx`
Consumes: nothing from earlier plans

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `lists_the_team_with_the_first_day_line` (updated in place) — asserts the lead reads "<n> agents work on <name>." with the test's count and name; each line of `en.teamPageRules` is only in a tip (`onlyInTips`); under the heading "Pause, retire or replace" a list holds three items, the lines of `en.teamChangesBody`; no axe violations. Fails first because the lead is the long sentence and the rules are visible.
- `shows_the_designer_card_in_its_colour` (updated, `team.test.tsx:278-282`) — its `getByText("Iris uses the same model as Theo, so Iris adds …")` becomes two list items inside the region "What a team like this costs": "Iris uses the same model as Theo, so a day costs more." and "Before changing a screen, Iris writes a plan, which costs a little too."; the "Costs" link assertion is unchanged.
- `counts_no_agents_in_the_designer_cost_line_of_a_small_team` (updated, line 233) — its `getByText("so Iris adds to what a day costs", { exact: false })` becomes `getByText("so a day costs more", { exact: false })`; the "no sixth" assertion is unchanged.

- [ ] `feat(web): the Team page explains itself in bullets and an ⓘ`

### Task 2: setup's team step in fewer words

Files: modified `apps/web/src/strings/en.ts`, `apps/web/src/pages/setup/team.test.tsx`
Consumes: nothing from Task 1

Tests:

- `setup/team.test.tsx` line 322: `expect(en.teamLead).toBe("Six people, one for each job. Rename anyone now or later.")` replaces the `toMatch(/^We suggest six…/)`; line 321's `findByText(en.teamLead)` is unchanged and passes with the new words; line 473 finds the new `setupMoreRolesNote`; lines 1979 and 2043 find the group by the new `startsLegend` ("Where should your team come from?") and the order test at 2036–2050 still holds. The changed `toBe` fails first against the old words.
- No Playwright journey matches the old lead, legend or note (checked 2026-10-17 with grep over `apps/web/e2e`); `setup-team.spec.ts` and `templates.spec.ts` are re-run.

- [ ] `feat(web): setup's team step says less`

## Verification

```
pnpm --filter @catervas/web test -- team
# expected: pass (team.test.tsx and setup/team.test.tsx)
grep -rn "We suggest six\|Start from the team Catervas suggests\|Pausing stops an agent" apps/web/src apps/web/e2e
# expected: no output
pnpm --filter @catervas/web e2e -- setup-team.spec.ts templates.spec.ts
# expected: all pass
pnpm check
# expected: exit 0
cargo xtask check
# expected: xtask check: ok
```
