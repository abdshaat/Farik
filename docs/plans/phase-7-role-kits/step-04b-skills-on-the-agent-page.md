# Phase 7, step 04b: Skills on the agent page

Status: draft
Branch: `phase/7-role-kits`
Spec: `docs/SPEC.md` 6.7; F9
Depends on: step 04 of this phase (its commands `skill_save`, `skill_remove`, `skill_confirm` and its RPCs `skills.list`, `skill.get`; it lands first), phase 6 (merged in #19)
Readiness confirmed by: fresh-session Opus reviewer, 2026-10-02, as part of step 04's plan (Tasks 1 and 8 there, split out on folding it to keep step 04 under ADR 0008's length): ready with findings, 4 Blocking, all folded; no second round (ADR 0032)
Mockups approved by: the founder, 2026-10-02

## Goal

Step 04 lets a user add, review, confirm and remove a skill from the command line, and gives the daemon the commands and reads to do it. When this step is done, a non-technical user does all of it on the agent page in the browser: they see which skills an agent has and where each came from, add or edit one after reading it whole, review a skill that arrived in the project, and remove one. Out of scope: anything step 04 does not already expose; a folder of several files is still added with `farik skill add`.

## Decisions

Every rule of a skill (levels, states, limits, refusals, confirming) is step 04's and is not restated here.

- **The agent page's new Skills section** lists three groups: "Comes with <role>" (read-only, "Read"), "For the whole team" and "Just for <name>", each row with its description, "Edit" and "Remove". A `review` row says "Changed in the project. Review before <name> uses it", with "Review". A `replaced` team row carries the muted note "<name>'s own <skill> replaces this one for <name>"; a `replaced` role row "Your team's <skill> replaces this one". A `missing` row says "<skill> is in the team file but its folder is gone", with "Remove".
- **"Add a skill" opens `SkillEdit`:**
  - who it is for ("Just <name>" or "Everyone on the team");
  - a name;
  - "When should <name> use it?" for the description;
  - the instructions, or "Upload a SKILL.md".

  Other files of a skill being edited are listed by name and size and kept as they are. Save shows the whole text with "Farik will follow these instructions. Read them before adding." and "Add skill". The dialog writes `SKILL.md` from its three fields with the frontmatter step 04 prescribes (`name`, then `description` as a JSON string). When the skill being edited has fields Farik ignores, Edit opens on "Upload a SKILL.md" with the whole file in the textarea rather than the three fields, so nothing the person did not see is dropped. Changing an existing skill's name saves a new skill and leaves the old one, and the dialog says so under the name: "This adds a new skill. Remove <old> yourself if you no longer want it." Upload replaces the three fields with the file's text.
- **A shipped skill's name.** When the name is one of the role's shipped skills, the reading step adds "This replaces <role>'s own <skill> for <whom>." and the button reads "Replace and add skill", which sends `replace_shipped: true`. Without that click nothing replaces a shipped skill (step 04's explicit confirmation).
- **`SkillReview`** shows a skill from the project: its whole text in an `untrusted` frame, its files, "Use this skill" (`skill_confirm` with `skill.get`'s `sha256`, and `replace_shipped: true` when the name is shipped, with the same sentence above the button), and "Remove". When `skill.get` answers a refusal, it shows "This skill can't be used: <reason>" with "Remove" alone.
- **The key mapper.** `files` joins `NAMED` in `packages/protocol-client/src/mapping.ts`, so a path key such as `SKILL.md` or `references/api_notes.md` crosses unchanged both ways.

For the founder: **O1, the mockups.** The founder approves Task 1's boards, or says to approve them automatically. Task 2 does not start until then.

## File map

```
docs/design/mockups/{AgentEdit,SkillEdit,SkillReview}.dc.html, canvas.json   Task 1
packages/protocol-client/src/mapping.ts, mapping.test.ts           modifies: files in NAMED (Task 2)
apps/web/src/pages/AgentEdit.tsx                                   modifies: the Skills section (Task 2)
apps/web/src/dialogs/{SkillEdit,SkillReview}.tsx                   creates (Task 2)
apps/web/src/skills.test.tsx, strings/en.ts                        creates/modifies (Task 2)
docs/SPEC.md, docs/plans/project-plan.md, docs/design/role-kits.md   modifies (Task 3)
```

## Interfaces

Consumes: the commands `skill_save`, `skill_remove`, `skill_confirm` and the RPCs `skills.list`, `skill.get`, with their refusal codes (step 04); the RPC `command`, `mapping.ts`'s `NAMED`, the `untrusted` frame of `ToolApproval` (main).

Produces: the `SkillEdit` and `SkillReview` dialogs and the agent page's Skills section. No wire change.

## Tasks

### Task 1: The skill screens, mocked up

An Opus session (ADR 0032) draws these on the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf, page "Team and settings"), each at desktop and phone width, in the canvas's tokens, muted and light, one colour per job. They are copied into `docs/design/mockups/`.

- **`AgentEdit`, the Skills section**, below Connectors, for Theo, a Software Developer:
  - "Comes with Software Developer": `implementing-a-contract`, its description, "Read".
  - "For the whole team": `release-notes`, "Write release notes in our voice", with "Edit" and "Remove"; and `api-style`, with the muted note "Theo's own api-style replaces this one for Theo".
  - "Just for Theo": `api-style`, with "Edit" and "Remove".
  - A row to review: `deploy-checklist`, "Changed in the project. Review before Theo uses it", with "Review".
  - A missing row: "`old-skill` is in the team file but its folder is gone", with "Remove".
  - The button "Add a skill".
  - Remove's confirmation: "Remove api-style? Theo stops using it, and its folder is deleted from the project."
- **`SkillEdit`**, a dialog in these states:
  - **Adding.** Who it is for, two choices: "Just Theo" and "Everyone on the team". Then "Name", with the hint "lower-case words joined by hyphens". Then "When should Theo use it?", with "1024 characters at most". Then "Instructions", a tall monospace textarea, with the link "Upload a SKILL.md instead". Buttons: "Next" and "Cancel".
  - **Editing a skill with more files.** As above, filled in, with "Also in this skill: references/checklist.md, 2 KB; templates/note.md, 1 KB. Farik keeps them as they are." The name changed shows "This adds a new skill. Remove api-style yourself if you no longer want it."
  - **Reading before adding.** "Farik will follow these instructions. Read them before adding." Then the whole SKILL.md in a scrolling frame. Then, when there are ignored fields, the muted line "Farik ignores: allowed-tools, hooks". Then "Add skill" and "Back". A variant for a shipped name: "This replaces Software Developer's own implementing-a-contract for Theo." and "Replace and add skill".
  - **Refusals, under their field:** "This skill runs commands when it loads, which Farik doesn't allow.", "This skill pulls in files when it loads, which Farik doesn't allow. Name a file without the @." and "Instructions are limited to 32 KB."
- **`SkillReview`**, a dialog: the title "Review deploy-checklist", and "It came with the project, by a clone, a pull or an edit, and Theo won't use it until you've read it." Below that, the whole text in the `untrusted` frame the `ToolApproval` dialog uses, then the file list with sizes, then "Use this skill" and "Remove". A refused variant: "This skill can't be used: it runs commands when it loads." with "Remove" alone.

Gate (O1): the founder approves the boards, or says to approve them automatically, and the approval is written into this plan's header with its date.

- [ ] `docs(design): mock up skills per agent`

### Task 2: The screens

Files: as the file map's Task 2 lines. Built from Task 1's approved boards.

- `mapping_keeps_skill_file_paths`: `toSnake` and `toCamel` of `{ files: { "SKILL.md": "a", "references/api_notes.md": "b" } }` keep both keys.
- `agent_edit_lists_skills_by_level`: the three group headings, both replaced notes, and the review and missing rows.
- `skill_edit_shows_the_whole_text_before_adding`: "Add skill" appears only on the reading step, and sends `skill_save` with every file, the untouched ones included.
- `skill_edit_opens_a_skill_with_ignored_fields_as_its_file`: Edit of a skill whose `ignored_fields` is not empty shows the whole `SKILL.md` in the textarea and not the three fields.
- `skill_edit_says_a_rename_adds_a_new_skill`: a changed name shows the sentence and sends `skill_save` for the new name alone.
- `skill_edit_replaces_a_shipped_skill_only_on_its_button`: a shipped name shows the replace sentence, and "Replace and add skill" sends `replace_shipped: true`.
- `skill_edit_says_refusals_at_their_field`: `skill_runs_commands`, `skill_attaches_files` and `skill_too_large`.
- `skill_review_confirms_with_the_hash_it_showed`: "Use this skill" sends `skill_confirm` with `skill.get`'s `sha256`.
- `skill_review_shows_a_refused_folder_with_remove_alone`: `skill.get` answering `skill_runs_commands` shows the reason and "Remove" without "Use this skill".
- `skill_review_renders_markup_as_text`.

- [ ] `feat(web): skills on the agent page`

### Task 3: Spec and plan

`docs/SPEC.md` 6.7: the agent page's Skills section and its two dialogs. `docs/plans/project-plan.md`: phase 7's row 04b, corrected if execution changed it. `docs/design/role-kits.md`: its steps table.

- [ ] `docs(spec): record skills on the agent page`

## Verification

```
cargo xtask check
# expected: xtask check: ok (with pnpm check)
```

Then, in the web app against a project with a team skill, an agent skill, a hand-edited skill and a missing one: the four rows show as the boards do; adding a skill through the dialog writes its folder and pin; "Use this skill" on the hand-edited one turns its row to in use.
