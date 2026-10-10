# Settings as a directory

Status: design approved in chat by the founder, 2026-10-10 (layout, sections, links and testing); the written design and its mockup await the founder's review.
Branch: `fix/settings-fewer-words` (pull request #35), after the fewer-words change it builds on.
Spec: `docs/SPEC.md` section 8's web UI paragraph (line 934), section 4's Change project ("This computer", line 131), section 10's configuring requirement (line 959), and F15 (team rules in Settings); `docs/design/web-ui.md`'s Settings row.

## Why

The founder, 2026-10-10: "the structure of settings must be in a directory style setting. For example there will be general, team settings, allowed actions, etc. The current settings page is messy and everything is in a single page. Divide them into endpoints to make it easier to navigate."

Settings today is one page at `/settings`: theme, how to open your app, the Advanced switch, the team's five rules, saved teams, this computer, the AI account, language and a testing link, one under the other. Catervas is for non-technical users (ADR 0016), and a long page of unrelated settings makes each one hard to find.

## What it is

Settings becomes a list of sections, each at its own address. Nothing a setting does changes; only where it lives.

| Address | Section | Holds |
|---|---|---|
| `/settings/general` | General | Theme, language, the Advanced switch |
| `/settings/allowed-actions` | Allowed actions | What agents may do |
| `/settings/spending` | Spending | The daily limit |
| `/settings/work` | How work runs | Finishing work, planning in sprints, checking plans; the UI file patterns when Advanced is on |
| `/settings/your-app` | Your app | How to open your app |
| `/settings/saved-teams` | Saved teams | Rename and delete saved teams |
| `/settings/computer` | This computer | Connected, project folder, address, Change project, Disconnect, Events for testing |
| `/settings/account` | AI account | The Claude connection, Connect again, Disconnect |

The list's order is the table's. The Advanced switch stays in General because it changes every page, not one section.

## Layout

- Desktop (1024 px and wider, the shell's `useWide`): the page is titled "Settings"; a side list of the sections sits on the left and the chosen section fills the rest. The current section's row is marked (`aria-current="page"`, the rail's active style). `/settings` opens General (a replace, so Back does not return to the bare address).
- Phone (below 1024 px): `/settings` is the list alone, one row per section with a one-line summary under its name. A section fills the screen with a "← Settings" link above its heading, which goes back to the list.
- Settings stays under Team on a phone (`web-ui.md`), and the rail's Settings link stays active on every section.
- Each section's heading carries an ⓘ where it needs explaining, its notes in bullets (`docs/brand/brand.md`, Voice). Phone summaries are a few words each: "Theme, language, advanced", "What agents may do", "The daily limit", "Sprints and plan checks", "How to open your app", "Your saved teams", "Folder and connection", "Your Claude account".

## Edge cases

- "Your app" is listed only when a UI/UX Designer who is not retired is on the team. Without one, `/settings/your-app` says "Add a UI/UX Designer to your team first", with a link to Team.
- An unknown section (`/settings/anything`) shows the app's not-found page.
- `/settings#preview` (Today's old link, perhaps in an open tab) goes to `/settings/your-app`.
- Each part still saves on its own, with its effect shown first, as today. A draft not saved is dropped on leaving a section, as leaving Settings drops it today.

## Links that move

| From | To |
|---|---|
| Today's "Open Settings" for the preview (`Today.tsx:130`) | `/settings/your-app` |
| Today's "Connect again" when the AI key is refused (`Today.tsx:907`) | `/settings/account` |
| A chat's "connect again" link (`OneToOne.tsx:239`) | `/settings/account` |
| The Team page's "Saved as … rename or delete it in Settings" (`Team.tsx:190`) | `/settings/saved-teams` |
| The "Use a saved team" dialog's note (`UseTemplate.tsx:236`) | `/settings/saved-teams` |
| Setup's finish screen (`SetupFinish.tsx:145`) | `/settings`; `finishCarriedAfter` loses ", under Your team’s rules" |

The rail and the phone's link under Team stay `/settings`.

## Building blocks

- `apps/web/src/pages/settings/SettingsLayout.tsx`: the title, the side list (a `nav` labelled "Settings sections", one `NavLink` per section) and an `Outlet`; on a phone, the list at `/settings` and the back link on a section. The sections' order, addresses, names and summaries are one constant in that file.
- One file per section in `apps/web/src/pages/settings/`: `General.tsx`, `AllowedActions.tsx`, `Spending.tsx`, `Work.tsx`, `YourApp.tsx`, `SavedTeamsPage.tsx`, `Computer.tsx`, `Account.tsx`. Each is assembled from today's pieces: `TeamRules.tsx`'s parts (`May`, `Spend`, `Finish`, `Planning`, `Plans`, `UiPaths`) and `HowToOpen` become exports; `Settings.tsx`'s `Account` moves; `SavedTeams` and `ChangeProject` are used as they are.
- The team-rules parts' shared state (each part's round, the team, the defaults and the base check) moves into a `useRuleParts()` hook used by the Allowed actions, Spending and How work runs pages.
- The theme stays one state in `App.tsx`, passed to General.
- `App.tsx` nests the section routes under `/settings`. `Settings.tsx` keeps `useAdvanced` (or it moves to its own module) and loses the page; `TeamRules`'s wrapper component goes once no page uses it.

## Testing

Test first, per task. The Settings tests in `apps/web/src/pages/team.test.tsx` move to their section's address and keep their assertions. New tests: the side list's sections, order and `aria-current`; desktop opening General; the phone list and its summaries, and the back link; "Your app" absent without a Designer and its address's note; an unknown section; `/settings#preview`; each moved link's target. The e2e journeys that open `/settings` (`templates.spec.ts`, `board.spec.ts`, `sprints.spec.ts`) go to their section. Every page keeps its axe check, and the phone width keeps the e2e `narrow()` checks.

## Docs

`docs/SPEC.md` (a revision in the header, and section 8's web UI paragraph) and `web-ui.md`'s Settings row describe the directory. A mockup of the desktop and phone layout, approved by the founder before code.

## Out of scope

Rewording the team-rules questions (setup's own components); any setting's behaviour; the agent pages under Team; search within Settings.
