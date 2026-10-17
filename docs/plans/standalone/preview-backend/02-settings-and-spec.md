# Standalone (preview backend), plan 2: Settings says whether the file is there, and the spec

Status: ready
Branch: `feat/preview-backend` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 4.1, 5.6, 8.3, 8.6, revision 0.120
Depends on: plan 1 of this folder (Tasks 1 to 3, committed on this branch)
Readiness confirmed by: none (ADR 0065); the author held it to the three rules

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

Settings, under "How to open your app", says in one line whether the owner's environment file is
there, with plain words on where to put it and what it is for; the start command's hint no longer
says the internet is off. `docs/SPEC.md` records plan 1 and this in revision 0.120. Out of scope: an
upload of the file, the setup wizard's card.

## Decisions

- **Where the line goes.** In `Preview` (`apps/web/src/pages/TeamRules.tsx`), after
  `<PreviewFields>` and before `{foot}`: a `div` of `styles.titled` holding
  `<p id="preview-env">` and an `InfoTip` with id `previewEnv-info` whose child is
  `<Bullets text={t("previewEnvMore")} />`, the pattern `previewMore` uses. The setup card gets
  nothing: the file is for an owner whose app needs a backend, who meets it in Settings.
- **Where it learns.** `HowToOpen`'s query type becomes `{ team: Team; previewEnvPresent?: boolean }`
  (the client's mapping layer turns `preview_env_present` into `previewEnvPresent`, as it does
  `hidden_suggestions`), and it passes `envPresent={data.previewEnvPresent === true}` to `Preview`.
  The page never asks for the file's contents, and no request carries them (plan 1 Task 3).
- **The words** (`apps/web/src/strings/en.ts`), short bullets with the detail behind the ⓘ, plain
  words, one curly apostrophe style as the file uses:
  - `previewEnvPresent`: "Your app’s settings file is in place. {designer} starts your app with it."
  - `previewEnvAbsent`: "No settings file for your app. If it needs keys or addresses, put them in .catervas/local/preview.env in your project folder."
  - `previewEnvMore`: "One NAME=value per line, without quotes.\nOnly your app gets it, while it runs. No agent can read it, and it is never committed.\nUse keys for testing, not your live ones.\nFor a service on this computer, write host.docker.internal instead of localhost."
  - `previewStartHint` becomes: "The command that starts your app, such as “npm start”.\nYour app can reach the internet and this computer while it runs."
- **The spec** is one revision, 0.120, dated 2026-10-17, naming ADR 0067 and the standalone plans
  `preview-backend` 1 and 2, its line placed after 0.119's (ADR 0066's) in the revisions list at
  the top. Each changed paragraph carries "(changed in 0.120; ADR 0067)" or "(added in 0.120;
  ADR 0067)". If 0.119's line is not there when this task runs, the branch was not rebased onto
  `feat/internal-task-ids`: stop and rebase (README, step 1).
- No ADR beyond 0067.

## File map

```
apps/web/src/strings/en.ts                modifies: previewEnvPresent, previewEnvAbsent, previewEnvMore, previewStartHint (Task 4)
apps/web/src/pages/TeamRules.tsx          modifies: HowToOpen's query type, Preview's `envPresent` and its line (Task 4)
apps/web/src/pages/team.test.tsx          tests:    says_whether_the_apps_settings_file_is_there; `settings` takes an extra answer (Task 4)
docs/SPEC.md                              modifies: revision 0.120; 4.1, 5.6, 8.3, 8.6 (Task 5)
```

## Interfaces

Consumes: `team.get`'s `preview_env_present` (plan 1 Task 3, `docs/schemas/rpc.schema.json`);
`HowToOpen`, `Preview`, `PreviewFields`, `InfoTip`, `Bullets`, `t` (`apps/web/src`, on this
branch); `settings`, `answerQuery`, `WITH_IRIS`, `EFFECTIVE`, `en` (`team.test.tsx`).

Produces:

```ts
// apps/web/src/pages/TeamRules.tsx
function Preview(props: { saved: Team; designer: string; envPresent: boolean; done: () => void }): JSX.Element;
// apps/web/src/pages/team.test.tsx
async function settings(team?: object, path?: string, answer?: object): Promise<{ container; s; main; part }>;
// `answer` is spread into the team.get answer it serves, after `team`, `agents` and `max_agents`.
```

## Tasks

### Task 4: Settings says whether the app's settings file is there

Files: modified `apps/web/src/strings/en.ts`, `apps/web/src/pages/TeamRules.tsx`; tested by
`apps/web/src/pages/team.test.tsx`
Produces: the strings and `Preview`'s `envPresent` above
Consumes: plan 1 Task 3's `preview_env_present`

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `says_whether_the_apps_settings_file_is_there` — `settings(WITH_IRIS, "/settings/your-app",
  { preview_env_present: true })`: the region named `en.previewTitle` holds
  `t("previewEnvPresent", { designer: "Iris" })` and not `en.previewEnvAbsent`, and the ⓘ
  `previewEnv-info` holds each line of `en.previewEnvMore` as a list item; then, rendered again
  with `{ preview_env_present: false }`, the region holds `en.previewEnvAbsent` and not the present
  line; and with no `preview_env_present` in the answer it holds the absent line too. Fails first
  because `en.previewEnvPresent` does not exist (the test does not type-check).

`saves_the_preview_in_settings` and any test that reads `previewStartHint` keep passing with the
new hint, which they read from `en`.

- [ ] `feat(web): settings says whether the app's settings file is there`

### Task 5: the spec records the preview's backend

Files: modified `docs/SPEC.md`
Produces: revision 0.120
Consumes: plan 1 and Task 4, as built

This task touches a document only, so it has no test; its check is the full check and a reading
against plan 1's Decisions and this plan's.

- **The revision line**: "Revision 0.120 (2026-10-17) records that the preview reaches the app's
  backend (ADR 0067, standalone plans `preview-backend` 1 and 2): in 8.3 the preview's `prepare`
  and `start` get the owner's `.catervas/local/preview.env` with `--env-file` when it is there, and
  `start` runs on the bridge with the owner's computer at `host.docker.internal`; the Designer's
  browser and the page check bypass the dead proxy for `host.docker.internal` too, and its allowed
  origins add that host on any port; in 8.6 no agent reads the file, it is never committed, and a
  preview's failure text shows its values as `[hidden]`; in 4.1 Settings says whether the file is
  there; in 5.6 the browser is confined to the preview and the owner's computer."
- **4.1**, after the sentence that ends "the change shown before it is saved (added in 0.33).":
  "Under its fields a line says whether the owner's environment file for the app is there, from
  `team.get`'s `preview_env_present`: "Your app’s settings file is in place. <designer> starts your
  app with it." or "No settings file for your app. If it needs keys or addresses, put them in
  .catervas/local/preview.env in your project folder.", with an ⓘ saying one `NAME=value` per
  line without quotes, that only the app gets it while it runs, that no agent can read it and it
  is never committed, to use keys for testing, and to write `host.docker.internal` for a service on
  this computer. Settings never shows, takes or uploads its contents; the owner puts the file there
  (added in 0.120; ADR 0067)." The start hint's words change with it.
- **5.6**, the connectors paragraph: "a browser confined to the project's preview (8.3)" becomes "a
  browser confined to the project's preview and the owner's computer (8.3; changed in 0.120;
  ADR 0067)".
- **8.3**, the preview paragraph:
  - `prepare`'s sentence gains "and the owner's computer named `host.docker.internal`
    (`--add-host host.docker.internal:host-gateway`; changed in 0.120; ADR 0067)" after "(`bridge`)".
  - "`start` runs in `catervas-preview-<project>-<task>`, with the worktree mounted and
    `--network none`." becomes "... with the worktree mounted, on the network (`bridge`) with the
    same `--add-host`, so the app reaches the internet and the services on the owner's computer at
    `host.docker.internal` (changed in 0.120; ADR 0067; on Linux that is the Docker bridge's
    address, so such a service must listen on every address, not `127.0.0.1` alone)."
  - "Each preview has its own namespace with the network off, so two previews on one port never
    collide, and a server on the host is out of the browser's reach." becomes "Each preview has
    its own network namespace, so two previews on one port never collide (changed in 0.120)."
  - A new sentence after the `start` one: "The owner's environment file (added in 0.120;
    ADR 0067): when `.catervas/local/preview.env` is a file, `prepare` and `start` get it with
    `--env-file <its absolute path>`, read by Docker itself in Docker's env-file format (one
    `NAME=value` per line, `#` comments, no quotes, a line without `=` passing Catervas's own
    variable of that name), and no other container, session or process gets it. It is read each
    time a preview opens; editing it does not prepare again."
- **8.3**, the browser paragraph: "so it sees only the preview's loopback" becomes "so it shares the
  preview's network and its `/etc/hosts`, `host.docker.internal` included (Docker refuses
  `--add-host` beside `--network container:`)"; "bypassed for `localhost` alone" becomes "bypassed
  for `localhost` and `host.docker.internal` (`localhost,host.docker.internal`)"; the allowed
  origins become `http://localhost:<port>;http://host.docker.internal:*;https://host.docker.internal:*`;
  "These are four barriers. A test in Docker proves each of the first three on its own" becomes
  "These are three barriers, the preview's network being on the bridge (changed in 0.120;
  ADR 0067). A test in Docker proves the proxy and the origins each on its own, and that with every
  flag on the browser reaches a page on the owner's computer and nothing else"; the rest stays. The
  page check (`catervas_check_page`) takes the same bypass.
- **8.6**, a new paragraph after the connectors' keys: "The preview's environment file (added in
  0.120; ADR 0067). `.catervas/local/preview.env` holds the owner's keys for the app, and no agent
  reads it: `.catervas/local/**` is protected for every team, the hook refuses it from a task's
  worktree (`path_outside_workspace`) and from the project folder, as a session's working
  directory or the `project` read root (`path_protected`), and the sandbox mounts neither the
  project folder nor the file. `.catervas/local/.gitignore` keeps it out of every commit. A
  preview's failure text, the last lines of `prepare`'s output or `start`'s, or Docker's refusal,
  has every value of the file of at least 4 characters replaced with `[hidden]`, longest first,
  before it is `preview.stopped`'s reason, the escalation's detail, a session's end or a log line;
  `team.get` answers only `preview_env_present`. The app's own code runs with the values, as on
  the owner's computer, so a change that shows or sends one is caught, if at all, by the code
  review; a screenshot of a page that shows one shows it. Keys for testing belong in it, not live
  ones."

- [ ] `docs(spec): the preview reaches the app's backend, revision 0.120`

## Verification

```
cargo xtask check
# expected: xtask check: ok (pnpm check among it)
```

Then mark both plans and the README `Status: done`, ticking each task's box in its own commit.
