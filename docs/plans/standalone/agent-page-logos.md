# Standalone: logos and info buttons on the agent page

Status: ready
Branch: `feat/agent-page-logos` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` section 6.7 (role kits' setup copy), F9 (the agent page)
Depends on: phase 7 (merged in #22)
Readiness confirmed by: a fresh Opus 5.5 session, 2026-10-09 (one round, against `docs/standards/workflow.md` stage 2)

## Goal

The founder asked on 2026-10-09, before phase 8: on the agent page (`/team/<agent>`), every connected service shows its logo beside its name; every description on the page hides behind a small info button that opens on hover, focus or tap; and what stays visible is a few plain words. Done means: Playwright, every kit service and every user-added connector carry a logo (bundled, never fetched); each kit row reads logo, name, two to five words, ⓘ; the other sections of the page (talks, effort, permissions, custom connector, save, skills, place, and the Procurement Specialist's mailbox, orders and sites) keep their headings, labels, statuses and buttons visible and move their notes behind ⓘ; the kit copy in every `kit.yaml` is rewritten short. Out of scope: other pages, the connect dialogs' layout (`KitConnect`, `ConnectorAdd` read the same `about` and `why`, so they get the short copy and nothing else), the Team page's cards.

## Decisions

- Logos are files committed under `apps/web/src/assets/services/<kit name>.svg` or `.png` (a PNG only where the company publishes no SVG mark), loaded with Vite's `import.meta.glob("../assets/services/*.{svg,png}", { eager: true, query: "?url", import: "default" })`. Rejected: fetching each service's favicon at run time (sends the user's IP to a dozen companies, breaks offline); `packages/brand` (Farik's own brand, not third parties'; the approved design said brand, moved here because the web app is the only user); the `simple-icons` npm package (covers 10 of 21 marks, adds a dependency).
- Sources: Simple Icons 16.34.0 (CC0) where it has the mark; otherwise the company's own SVG mark, else its own square PNG icon (apple-touch-icon). Files without a brand (Exchange rates, Safety recalls) and user-added connectors get a drawn monochrome symbol: `fx.svg`, `recalls.svg`, `plug.svg`. The source table is below; Task 3 downloads exactly those URLs.
- The file name is the kit connector's `name` (`google-ads`, `aws-pricing`), so the map is the directory itself. An unknown name, a user-added connector included, falls back to `plug.svg`. The built-in Playwright switch uses `playwright.svg`.
- A logo is `<img alt="" width="20" height="20">`: decorative, because the service's name is beside it. An SVG in `<img>` runs no script.
- The info button is a new `@farik/ui` component, `InfoTip`. Its text stays in the DOM (hidden with CSS, not `hidden`), tied to the button by `aria-describedby`, shown on `:hover` and `:focus-within`, toggled by click (touch), closed by Escape. Rejected: the native `title` attribute (no touch, no keyboard, unstyled); `<details>` (pushes layout, not a tip).
- `Switch` and `Choice` gain an optional `info?: ReactNode`, rendered as an `InfoTip` after the label (Switch) or the legend (Choice). An `InfoTip` never goes inside a `Choice` option card, because a button inside a radio's label is a nested control; the effort options lose their descriptions and one `InfoTip` on the legend explains all three.
- Visible: headings, labels, the service's short line, statuses that ask for action (Connect again, Sign in again, the Playwright-off warning, kitAtLaunch, kitGone), counts, buttons. Behind ⓘ: every string named `*Note`, `*Lead`, `why`, and the custom connector's how-it-starts and tool list. Field hints under a text box (`agentTalksHint`, `sitesAddHint`) stay visible, shortened, because they say what to type there.
- An `InfoTip` on a heading is the heading's next sibling, never inside `<h2>`/`<h3>`; the lead spans now inside headings (`connectorsLead`, `kitLead`, `connectorsYoursLead`, `skillsLead`) are removed from them, so each heading's accessible name is its title alone (orders.test.tsx:68, sites.test.tsx:52 and :64, team.test.tsx:989 find headings by name).
- `InfoTip` ids are unique on the page: `kit-${service.name}-info`, `gone-${name}-info`, `custom-${server.name}-info`, `tier-${tier}-info` (Switch's own `${id}-info`), and `<string key>-info` for every other one (for example `agentPauseNote-info`).
- Copy style (founder: "sacrifice grammar for less text"): fragments, no "So the <role>", read-only said as "Read-only.". `about` (visible) at most 6 words; `why` (behind ⓘ) at most 20 words. Pinned by a test, not by a loader rule, because a kit file from elsewhere is not the founder's copy.
- Rewording copy does not ask anyone to connect again: `SetupCopy` is outside the hashed entry (`KitConnector::Server`, `crates/roles/src/kit.rs:38`; `matches_kit` compares `entry` only).
- Plans for work outside a phase live at `docs/plans/standalone/<name>.md`; this step adds that row to `docs/standards/code.md`'s Documents table, the route `code.md` gives for a missing row.

## Logo sources

| File | Source | Terms |
|---|---|---|
| playwright.svg | https://raw.githubusercontent.com/microsoft/playwright/HEAD/packages/web/src/assets/playwright-logo.svg | Apache-2.0 |
| context7.svg | https://raw.githubusercontent.com/upstash/context7/HEAD/public/context7-icon.svg | Upstash's own asset |
| grep.svg | Simple Icons `vercel.svg`, `fill="#000000"` on `<svg>` (Grep is run by Vercel) | CC0 |
| osv.svg | https://raw.githubusercontent.com/google/osv.dev/HEAD/docs/images/osv_logo_light-full.svg, cropped to its first four paths (the magnifier mark), `viewBox="-40 0 560 548"` | Google's own asset |
| github.svg, stripe.svg, buffer.svg, google-ads.svg, brex.svg, ebay.svg, linear.svg, notion.svg | Simple Icons 16.34.0 (`googleads.svg` for google-ads), each with `fill="#<brand hex from simple-icons.json>"` on `<svg>` | CC0 |
| digits.svg | https://digits.com/favicon/favicon.svg?v=3 | Digits' own asset |
| kick.svg | https://kick.co/safari-pinned-tab.svg | Kick's own asset |
| serpapi.svg | https://serpapi.com/favicon.svg | SerpApi's own asset |
| exa.svg | https://exa.ai/images/logo/exa-logo-blue.svg, first path only, `viewBox="-11 0 129 129"` | Exa's own asset |
| amplitude.png | https://amplitude.com/nextjs-public/favicon/apple-touch-icon.png (180px) | Amplitude's own asset |
| higgsfield.png | https://higgsfield.ai/apple-icon.png (180px) | Higgsfield's own asset |
| recraft.png | https://framerusercontent.com/images/abL15N3UiE3oDA6S807KUBarXrQ.png (recraft.ai's apple-touch-icon, 180px) | Recraft's own asset |
| aws-pricing.png | https://a0.awsstatic.com/libra-css/images/site/touch-icon-ipad-144-smile.png (144px) | Amazon's own asset |
| kit.png | https://kit.com/apple-touch-icon.png (180px; Simple Icons' `kit` is a wordmark) | Kit's own asset |
| fx.svg, recalls.svg, plug.svg | drawn for Farik: 24×24, `fill="none" stroke="#5f6b7a" stroke-width="1.75"`, round caps; two opposing curved arrows; a shield with an exclamation mark; an electrical plug | ours |

Every SVG must parse as XML and hold no `<script`, `foreignObject`, `on*=` attribute or `http` `href`; every file under 20 KB. The planner vetted all 24 files and left them in `/home/ashaat/.claude/jobs/48939cb5/tmp/logos/` (ignore its `kit.svg`, a wordmark); the executor copies them from there, re-runs these checks, and downloads from the URLs above only a file missing there.

## Copy

Kit services, `about` (visible) | `why` (behind ⓘ). Titles unchanged.

| Kit file, name | about | why |
|---|---|---|
| architect, context7 | Current coding docs | Checks how a library works today, in your version, before deciding. Read-only. |
| software_developer, context7 | Current coding docs | Writes code for the library version you use, not an old one. Read-only. |
| architect, grep | Search public code | Sees how other projects solve it before choosing. Read-only. |
| architect, osv | Known security flaws | Checks your libraries for known flaws and the version that fixes them. Read-only. |
| architect, github | Your code and issues | Reads your repositories and pull requests, private ones too. Read-only. |
| product_manager, github | Your code and issues | Turns your issues into requests. Files an issue or comment only when you say yes. |
| finance_specialist, stripe | Your payments | Books revenue, fees and payouts from Stripe's own numbers. Read-only. |
| finance_specialist, digits | Your books | Reads the books you already keep there. Read-only. |
| finance_specialist, kick | Your books, from your bank | Reads the books you already keep there. Read-only. |
| marketing_specialist, higgsfield | AI images and videos | Makes pictures and clips. Uses your credits; asks before going past your limit. |
| marketing_specialist, recraft | AI logos and graphics | Makes on-brand graphics and icons. Uses your credits; asks before going past your limit. |
| marketing_specialist, buffer | Schedules social posts | Reads your channels and past posts. Sends posts from your approved plan; asks about others. |
| marketing_specialist, kit | Email newsletters | Drafts emails and landing pages. You send every email. |
| marketing_specialist, google-ads | Google search ads | Finds what customers search for. Runs ads from your approved plan, within budget. |
| procurement_specialist, fx | Daily central-bank rates | Compares prices in one currency, rate and date shown. Read-only. |
| procurement_specialist, exa | Web search | Finds makers, sellers and price pages. Read-only. |
| procurement_specialist, serpapi | Prices across big shops | Google Shopping, Amazon, eBay, Walmart in one search. Each uses one of your SerpApi searches. |
| procurement_specialist, brex | Your company cards and bills | Sees what you pay vendors, and repeat charges nobody listed. Read-only. |
| procurement_specialist, aws-pricing | AWS price list | Prices an AWS option exactly before anyone buys. Read-only. |
| procurement_specialist, recalls | Product and car recalls | Never suggests a recalled product; checks a used car's VIN. Read-only. |
| procurement_specialist, ebay | eBay asking prices | Real prices and seller ratings. Read-only; never bids or buys. |
| product_manager, amplitude | How people use your product | Checks real feature use before deciding, and again after release. Read-only. |
| product_manager, linear | Your backlog | Turns your Linear issues into requests, comments included. Read-only. |
| product_manager, notion | Your docs and notes | Starts from what you already wrote. Read-only. |

`apps/web/src/strings/en.ts`, new value. "→ ⓘ" means the string is now rendered inside an `InfoTip` placed where named.

| Key | New value | Placed |
|---|---|---|
| agentTalksHint | One line. Changes tone, not work. | visible hint |
| effortLowNote | Cheap, simple jobs | dropped from the card |
| effortMediumNote | Most work | dropped from the card |
| effortHighNote | Thinks longest, costs most | dropped from the card |
| effortInfo (new) | Quick: {low}. Balanced: {medium}. Careful: {high}. | → ⓘ on the effort legend, filled from the three notes |
| agentMayAdvanced | Advanced shows each permission. | → ⓘ on the "may do" heading |
| tierReadNote | Every agent can. | → ⓘ on its switch |
| tierWriteNote | Only files the plan allows. | → ⓘ on its switch |
| tierExecuteNote | In a safe box on your computer. | → ⓘ on its switch |
| tierGitLocalNote | Never your main branch. | → ⓘ on its switch |
| tierNetworkNote | Looks things up, reads web pages. | → ⓘ on its switch |
| tierGitRemoteNote | Off until you give it. | → ⓘ on its switch |
| tierExternalNote | Mail, posts, deploys. You approve each. | → ⓘ on its switch |
| connectorCustomNote | Any tool, by command or web address. Not checked by Farik: you label its tools. Runs with your rights; add only ones you trust. Give files by full path, starting with /. | → ⓘ on "A custom connector" |
| connectorCustomKeychain | If asked about the keychain, choose "Always". | → same ⓘ, second paragraph |
| agentNextWork | Applies from {name}'s next task. | → ⓘ after Cancel |
| agentPauseNote | Stops taking work; finishes nothing halfway. | → ⓘ after its button |
| agentReplaceNote | New {role} starts from what {name} learned. | → ⓘ after its button |
| agentRetireNote | Leaves the team. What it learned stays. | → ⓘ after its button |
| connectorsLead | Outside tools {name} can use. | → ⓘ on the Connectors heading |
| connectorPlaywrightShort (new) | Sees your app like a customer | Playwright switch's `description` |
| connectorPlaywrightNote | Built in. Runs in Docker's sandbox; reaches only your app. | → ⓘ on the Playwright switch |
| connectorsNote | On for UI/UX Designers, optional for others. | → same ⓘ, second paragraph |
| connectorOff | Off: {name} can't see your app, so gets no work. | visible warning |
| kitLead | Checked by Farik; tools already labelled. | → ⓘ on the kit heading |
| kitAtLaunch | Comes with Farik's web launch. | visible |
| kitGoneNote | {name} doesn't use it. Removing deletes what it kept. | → ⓘ after kitGone |
| connectorsYoursLead | Not checked by Farik; you labelled their tools. | → ⓘ on "Added by you" |
| connectorStdio | Runs on this computer | visible, beside the name |
| connectorHttp | Web address | visible, beside the name |
| connectorTools / connectorOneTool | unchanged | → ⓘ on the row's name |
| connectorKeychain | Keys in your keychain. | visible |
| connectorFile | Keys in a private file here. | visible |
| connectorAgainNote | Its settings changed since you connected it here. {name} skips it until you connect again. | → ⓘ after connectorAgain |
| connectorUnreadableNote | Keychain or key file couldn't be opened, so {name} works without {server}. If asked, choose "Always". | → ⓘ after connectorUnreadable |
| skillsLead | How your team likes things done. | → ⓘ on the Skills heading |
| mailboxNone | No mailbox yet. | visible |
| mailboxNoneNote (new) | {name} drafts seller messages; you send them once a mailbox is connected. | → ⓘ after mailboxNone |
| ordersLead | {name} suggests and tracks orders. Farik never orders or pays: you place each, then mark it placed and received. | → ⓘ on the Orders heading |
| sitesLead | Searches the whole web; opens pages only on these sites. Asks you on Today for others. | → ⓘ on the Sites heading |
| sitesFarikNote | Long-running shops Farik checked. Turn one off to stop it. | → ⓘ on "Farik's approved sites" |
| sitesAddHint | shop.com, or any page on it. | visible hint |

## File map

```
packages/ui/src/InfoTip.tsx                     creates: the info button and its tip
packages/ui/src/InfoTip.module.css              creates: its look; tip shown on hover, focus-within, open
packages/ui/src/InfoTip.test.tsx                tests:   InfoTip
packages/ui/src/index.ts                        modifies: exports InfoTip
packages/ui/src/strings.ts                      modifies: uiStrings.infoLabel
packages/ui/src/Switch.tsx, Choice.tsx          modifies: optional `info`
packages/ui/src/Switch.test.tsx, Choice.test.tsx tests:  the `info` prop
apps/web/src/assets/services/*.svg              creates: 21 marks (16 SVG, 5 PNG) and 3 symbols (table above)
apps/web/src/pages/service-logo.ts              creates: serviceLogo(name)
apps/web/src/pages/service-logo.test.ts         tests:   every shipped kit connector has a logo
apps/web/src/pages/ServiceLogo.tsx              creates: the 20px <img>
apps/web/src/pages/AgentEdit.tsx                modifies: logos and InfoTips (Tasks 4, 5)
apps/web/src/pages/Mailbox.tsx, Orders.tsx, Sites.tsx  modifies: InfoTips (Task 5)
apps/web/src/pages/connectors.test.tsx          tests:   connector section (Task 4)
apps/web/src/pages/team.test.tsx                tests:   the rest of the page (Task 5)
apps/web/src/strings/en.ts                      modifies: the copy table (Tasks 4, 5, each its own keys)
crates/roles/roles/*/kit.yaml                   modifies: about and why (Task 6)
crates/roles/src/kit.rs                         tests:   shipped kit copy is short (Task 6, `mod tests`)
docs/SPEC.md, docs/design/role-kits.md, docs/standards/code.md  modifies: Task 7
```

## Interfaces

Consumes: `KitService` (`apps/web/src/pages/Team.tsx`, on main), `McpServer` (`apps/web/src/pages/setup/TeamSetup.tsx`, on main); `Switch`, `Choice`, `uiStrings` (`@farik/ui`, on main); `load_kit(role: Role) -> Result<Kit, KitError>` and `Role` (`crates/roles`, on main).

Produces:

```ts
// @farik/ui
export function InfoTip(props: { id: string; label?: string; children: ReactNode }): JSX.Element;
// label defaults to uiStrings.infoLabel ("More about this"); the button's accessible name.
Switch: props & { info?: ReactNode }   // InfoTip id `${id}-info`
Choice: props & { info?: ReactNode }   // InfoTip id `${name}-info`, beside the legend
// apps/web
export function serviceLogo(name: string): string;  // a URL; plug.svg's for an unknown name
export function ServiceLogo(props: { name: string }): JSX.Element;
```

## Tasks

### Task 1: InfoTip

Files: created `packages/ui/src/InfoTip.tsx`, `InfoTip.module.css`, `InfoTip.test.tsx`; modified `index.ts`, `strings.ts`. `InfoTip.module.css` uses tokens only and its first rule sets a `var(--farik-type-*-family)` font-family (`tokens-only.test.ts`).
Produces: `InfoTip`
Consumes: nothing

Tests:

- `it('names its button and describes it with the tip')` — the button's accessible name is `label`; its `aria-describedby` is `id`, and the element with that id holds the children's text and has `role="tooltip"`.
- `it('defaults its name to More about this')` — without `label` the button's name is `uiStrings.infoLabel`.
- `it('opens on click and closes on a second click or Escape')` — `aria-expanded` goes `false` → `true` on click, `false` on a second click; after reopening, Escape sets it `false`.
- `it('has no axe violations open or closed')` — `expectNoAxeViolations` on both states.

- [x] `feat(ui): add an info button that shows a tip on hover, focus or tap`

### Task 2: `info` on Switch and Choice

Files: modified `Switch.tsx`, `Choice.tsx`, `Switch.test.tsx`, `Choice.test.tsx`
Consumes: `InfoTip` from Task 1

Tests:

- `Switch` `it('shows an info button beside its label when given info')` — a button named "More about this" exists, its tip holds the `info` text, and the switch's own accessible name is still exactly `label`.
- `Switch` `it('shows no info button without info')`.
- `Choice` `it('shows one info button by the legend, outside every option')` — the button is not inside any `label` element, and each radio's name is its option label alone.

- [ ] `feat(ui): let a switch or a choice carry an info button`

### Task 3: service logos

Files: created `apps/web/src/assets/services/*.{svg,png}` (exactly the 24 files of the source table, one file per name), `service-logo.ts`, `service-logo.test.ts`, `ServiceLogo.tsx`
Produces: `serviceLogo`, `ServiceLogo`

Tests:

- `it('has a logo for every connector a shipped kit offers')` — reads every `crates/roles/roles/*/kit.yaml` (node `fs`, path from `import.meta.dirname`), takes each line matching `/^  - name: ([a-z0-9-]+)$/gm` with `matchAll`, and asserts `serviceLogo(name)` is not plug's URL for each; the list is non-empty.
- `it('falls back to the plug for a name it does not know')` — `serviceLogo("airtable")` equals `serviceLogo("plug")`.
- `it('has a logo for the built-in Playwright')` — `serviceLogo("playwright")` is not plug's URL.

- [ ] `feat(web): bundle a logo for every service an agent can connect`

### Task 4: the Connectors section

Files: modified `AgentEdit.tsx` (Connectors section, `KitRow`, `CustomRow`), `en.ts` (keys from `connectorsLead` to `connectorUnreadableNote` in the copy table), `connectors.test.tsx`
Consumes: Tasks 1 to 3

Tests:

- `it('shows each kit service with its logo, title and short line, its reason behind info')` — in the Product Manager's kit row for Notion: an `img` whose `getAttribute("src")` equals `serviceLogo("notion")`, the text of `about`, a button "More about this" whose tip holds `why`; no element of the row outside its `role="tooltip"` element contains `why`'s text.
- `it('shows the plug for a connector you added')` — the "Added by you" row's `img` `getAttribute("src")` equals `serviceLogo("plug")`; its tool list sits inside a tooltip.
- `it('shows Playwright with its logo, a short line and the rest behind info')` — the switch row has an `img` whose `getAttribute("src")` equals `serviceLogo("playwright")`, the text `connectorPlaywrightShort`, and a tooltip holding `connectorPlaywrightNote`.

- [ ] `feat(web): show service logos and hide connector details behind info`

### Task 5: the rest of the agent page

Files: modified `AgentEdit.tsx` (all sections but Connectors), `Mailbox.tsx`, `Orders.tsx`, `Sites.tsx`, `en.ts` (the remaining keys of the copy table), `team.test.tsx`
Consumes: Tasks 1 and 2

Tests:

- `it('puts every note on the agent page behind an info button')` — for the default Developer: each of `agentMayAdvanced`, `agentNextWork`, `agentPauseNote`, `agentReplaceNote`, `agentRetireNote`, `skillsLead` and (with Advanced on) each `tier*Note` and `connectorCustomNote` is found inside a `role="tooltip"` element and nowhere else on the page.
- `it('explains the effort levels once, by the legend')` — the effort radios' names are exactly Quick, Balanced, Careful; one tooltip in the group holds all three notes.
- `it('puts the Procurement Specialist's notes behind info')` — `ordersLead`, `sitesLead`, `sitesFarikNote`, `mailboxNoneNote` are each inside a tooltip.

- [ ] `feat(web): hide the agent page's notes behind info buttons`

### Task 6: short kit copy

Files: modified every `crates/roles/roles/*/kit.yaml` with connectors (copy table), tested in `crates/roles/src/kit.rs` `mod tests`

Tests:

- `shipped_kit_copy_is_short` — for every `Role` with a shipped `kit.yaml`, `load_kit(role)`'s every `KitConnector::Server`, `copy.about` has at most 6 words and `copy.why` at most 20 (split on whitespace).

The existing tests in `kit.rs` that pin the old `why` text (kit.rs:1611, 1675, 1742, 1799, 1866, 1945, 2020, 3540, 3853, 3932, 3982, 4064) are updated to the new copy in the same commit; what each asserts besides the words stays.

- [ ] `feat(roles): say each kit service in a few plain words`

### Task 7: docs

Files: modified `docs/SPEC.md` 6.7 (a paragraph "Service logos and info buttons (added in 0.79; standalone plan agent-page-logos)": bundled logos by kit name, plug fallback, never fetched; `about` is the row's visible line, at most 6 words, `why` sits behind ⓘ, at most 20; every note on the agent page behind ⓘ; and a "Revision 0.79 (2026-10-09)" sentence in the header at `docs/SPEC.md:3`, as every revision has), `docs/design/role-kits.md` line 80 ("a one-line reason each" → a logo and a few words each, the reason behind ⓘ), `docs/standards/code.md` (Documents row: "Plan outside a phase | `docs/plans/standalone/<name-kebab>.md`, from the step template | `docs/plans/standalone/agent-page-logos.md`"), this plan's checkboxes.

- [ ] `docs: record service logos and info buttons on the agent page`

## Verification

```
cargo xtask check
# expected: xtask check: ok
```

Then the founder opens `/team/<agent>` for a Product Manager, a UI/UX Designer and a Procurement Specialist in `farik serve` and sees the logos and the info buttons.
