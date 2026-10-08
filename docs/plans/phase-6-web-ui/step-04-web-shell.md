# Phase 6, step 04: Web shell

Status: done (landed and landing-reviewed 2026-09-29)
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 8.1 (the daemon serves the app), 8.6 (the web UI), and 10 (360 px, WCAG 2.2 AA, strings externalized, themes)
Depends on: steps 01 to 03 of this phase (`@farik/brand`, the daemon's browser routes and `@farik/protocol-client`, and `@farik/ui`)
Readiness confirmed by: fresh-session reviewer, 2026-09-29 (one round: not ready on one founder decision, the Pause mockup's approval, which the founder gave with changes the same day; the changes are recorded below when received, and Task 4 does not start until they are. The other fourteen findings are folded in)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

`farik serve` opens the web app in the user's browser. The app is built into the binary, so nothing else is installed. The first thing the app does is trade the link's code for a session and connect.

From then on the page has:
- the rail on desktop, and a bottom bar on a phone;
- the team's working or paused state, with Pause and Resume;
- a Settings page with the theme (light, dark, or match the computer), the Advanced switch, what the page is connected to, and "Disconnect this browser";
- a raw event list, for testing.

When Farik stops answering, the page says so plainly and reconnects by itself. A browser with no session sees how to get a start link.

A Playwright suite drives the real binary and browser through this, under `cargo xtask check --integration`.

Out of scope, and where each goes:
- the first-run wizard (steps 05 and 06);
- Today, the request box, and the gates (step 08, on step 07's runtime);
- the board and costs (step 09);
- the channel (step 10).

The rail shows only the places that exist, and each later step adds its own.

## Decisions

- **The app.** `apps/web` (`@farik/web`) is React 19.3.0 and Vite 8.3.1, built from `@farik/brand`, `@farik/ui`, and `@farik/protocol-client` (all `workspace:*`).
  - Routing uses `react-router` `=8.4.0` in declarative mode (`BrowserRouter`, `Routes`, `Route`, `NavLink`). Version 8 is current, and its declarative mode is the one the phase decision names for version 7, so version 7 is not pinned.
  - `pnpm-workspace.yaml` gains `apps/*`. `biome.json` includes `apps/**`, as step 01's ledger required.
- **Routes:**
  - `/` redirects to `/events` until Today exists (step 08 changes it).
  - `/connect` is the connect page.
  - `/settings` is Settings.
  - `/events` is the event list.
  - Any other path shows a plain "No page here" with a link to `/`.
- **Serving the app** (spec 8.1):
  - **Embedding.** The built `apps/web/dist` is embedded into `farik-runtime` with `rust-embed` `=8.12.0` and the `mime-guess` feature. The `#[allow_missing = true]` attribute keeps `cargo build` working before the web app has been built. An empty embed serves a one-line page: "The web app was not built into this farik. Run pnpm --filter @farik/web build, then build farik again." Phase 12's packaging builds the web app first.
  - **Routes.** The browser router gains `GET /` and a `GET` fallback. Each serves the file at the path when the embed has it. Otherwise it serves `index.html`, so that the app's own routes load on refresh. `/connect` becomes `get(app).post(connect)`.
  - **Host check only.** These GETs check `Host` and not `Origin`, because a navigation sends no `Origin`. They need no session: the app itself holds no secret. Without `WebState` (a `farik run` daemon) they answer 404, like the other browser routes.
  - **What is embedded.** Debug builds of rust-embed read `apps/web/dist` from disk at run time; release builds embed it. `crates/runtime/build.rs` watches `apps/web/dist` when it exists and `apps/web` while it does not, in both profiles, so any build picks up a new web build (changed by the landing review: a release-only rule left a fresh checkout's debug binary serving "not built"). The handler is generic, `app_from::<E: RustEmbed>`, served with the real embed (`WebApp`, folder `../../apps/web/dist`); tests use `#[cfg(test)]` embeds of `crates/runtime/src/daemon/app-fixture/` (an `index.html` and `assets/app-3f2a.js`) and of an empty folder, so no test depends on what `dist` holds when `cargo test` runs.
  - **Security headers** on every page and asset:
    - `Content-Security-Policy: default-src 'self'; connect-src 'self' ws://127.0.0.1:<port>; img-src 'self' data:; font-src 'self' data:; style-src 'self'; frame-ancestors 'none'`. `font-src … data:` because Vite inlines small font subsets as `data:` URLs; no `'unsafe-inline'`, because React's `style` props go through the CSSOM, which CSP does not govern, the production build ships CSS files, and the Vite dev server does not send these headers.
    - `X-Content-Type-Options: nosniff`.
    - `Referrer-Policy: no-referrer`.
  - **Caching.** `index.html` gets `Cache-Control: no-store`. Files under `assets/` (Vite's `assetsDir`, whose names are hashed) get `max-age=31536000, immutable`; any other file gets `no-cache`.
  - `frame-ancestors 'none'` stops another page framing the Pause button (clickjacking).
- **Two more browser routes.** Both have `/connect`'s Origin and Host checks and its 404 without `WebState`.
  - `GET /session` answers 204 when a `farik_session` cookie verifies, and 401 otherwise. The page asks it before opening `/rpc`, because a refused WebSocket upgrade gives a browser no status code. A same-origin `GET` fetch sends no `Origin` (Fetch spec), so `/session` checks `Host` and refuses an `Origin` only when one is present and foreign; `POST /disconnect` keeps the full check (a POST carries `Origin`).
  - `POST /disconnect` revokes every `farik_session` cookie the request carries and answers 204 with `Set-Cookie: farik_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0`.
  - `BrowserSessions::revoke(&self, secret: &str) -> Result<(), DaemonError>` is added. It was deferred from step 02.
  - Depends on step 03's fix moving `expectNoAxeViolations` to `@farik/ui/test`, so the app bundles no test code; this step's tests import it from there.
  - The phase plan's query `browser.disconnect` becomes this route, because the socket does not hold the session's secret.
- **The event stream's failure** (deferred from step 02). When `push` fails to read the log three polls running (a store error or a join panic), the socket is closed with code 1011 and the reason "farik could not read its event log". The page then shows "Farik stopped answering" and reconnects, so the failure is visible and not a silent stall. The test makes the reads fail by dropping the event log's table through a second SQLite connection to the same file.
- **Opening the browser.** `farik serve` opens the link unless `--no-open` is given. The opener is `CliIo::open_url: Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>`. `CliIo::new` sets a no-op opener, so no test ever starts a real browser (which would spend the one-time code); `main.rs` sets the system opener:
  - `xdg-open <url>` on Linux, `open <url>` on macOS, and `cmd /c start "" <url>` on Windows;
  - it is spawned and not waited on;
  - a failure prints "could not open a browser: <why>; open the link above yourself" on stderr, and serving goes on.

  Rejected: the `webbrowser` crate, since three commands need no dependency.
- **The end-to-end binary** is `farik-e2e-serve`: a `[[bin]]` of the `farik` crate with `required-features = ["e2e"]`. `crates/cli` gains `[features] e2e = []`.
  - It takes `farik-e2e-serve --port <p> [--transcripts <name>,…]`, run in a project directory.
  - It runs `serve` with `--no-open` and `Engine::Given`, replaying the named transcripts from `farik_runtime::recorded::fixtures` through a `match` over the names the suites use. The only name this step needs is none at all: an empty list. An unknown name exits 2 naming it.
  - The shipped `farik` has no path to replay (step 02's decision).
- **Connection** (`src/app/connection.tsx`) exports `ConnectionProvider` (one per page, around the router; it owns the single socket) and `useConnection(): { status: 'checking' | 'no_session' | 'connecting' | 'open' | 'lost'; client: DaemonClient | null }`, which reads the provider's context.
  1. On load, if `location.hash` holds a 64-hex code, the page is `/connect`: it posts the code to `/connect`, removes the hash with `history.replaceState`, and on 204 navigates to `/`. A 401 shows the link-used message.
  2. It asks `GET /session`. A 401 gives `no_session`, and the connect page shows "Open Farik from its start link" (the `Connect` mockup).
  3. With a session, it opens `ws://<location.host>/rpc` with `connect`.
  4. On close it becomes `lost`, shows the mockup's "Farik stopped answering … Trying again in 5 seconds", and repeats from step 2 every 5 seconds.
- **State** (`src/app/store.ts`). There is no state library (the phase decision).
  - `useEvents(): Event[]` (the client's camelCase `Event`) subscribes from 0 on the first connection and from the last seq it holds after a reconnect, so the list survives a reconnect without gaps or repeats; it keeps the last 500. A `ponytail:` note: the list view needs no more. Like the connection, it lives in the provider, so every reader shares one subscription.
  - `useQuery<T>(name, params): { data: T | undefined; error: RpcError | undefined }` queries on mount, and again after any event, no more than once every 250 ms.
- **Theme** (`src/app/theme.ts`) exports `useTheme(): [ThemeChoice, (c: ThemeChoice) => void]`, where `ThemeChoice = 'light' | 'dark' | 'system'`, kept in `localStorage` key `farik.theme` (read and written inside try/catch, default `light`). It sets `data-theme` on `<html>`. With `system` it follows `matchMedia('(prefers-color-scheme: dark)')`, including changes to it.
- **Advanced** is a switch in Settings, kept in `localStorage` key `farik.advanced` (default off). It changes nothing yet; steps 06 to 08 read it. Its explanation is the Settings mockup's text.
- **Layout** follows `web-ui.md`, and the Pause control follows the mockup as the founder approved it on 2026-09-29, with the changes recorded in the next bullet.
- **The founder's changes to the Pause mockup:** none. On 2026-09-29 the founder confirmed it is built exactly as shown.
  - At 1024 px and wider there is a dark rail (`band`) with the logo, the places, and at its foot the connection state, the team state, and Pause or Resume.
  - Below 1024 px there is a top bar with the project's name and Pause or Resume, and a bottom bar with the places.
  - While paused, a banner reads "The team is paused. Nothing new starts until you resume. You can still answer, approve and accept." This follows the mockup published to the founder on 2026-09-29 (Farik pause control). If the founder asks for changes, they are made in place.
  - The places this step has are Events and Settings.
- **Pause and Resume** send `team_pause` and `team_resume` over the socket. Their state comes from `serve.status.paused`, refreshed by the `team.paused` and `team.resumed` events. A refusal (`already_paused` or `not_paused`, which a race can cause) is shown as the reply's sentence. The button is `@farik/ui`'s `Button` with `busy` set while the command runs.
- **Settings** has four sections:
  - Theme: a `Choice` of Light, Dark, and Match my computer, with the Settings mockup's notes.
  - Advanced settings: a `Switch`.
  - "Farik on this computer": Connected, the project folder, the address, and a "Disconnect this browser" `Button`. It posts `/disconnect` and then shows the connect page.
  - Language: English, with the mockup's line.
- **Events** is a `Table` of the last 100 events, newest first, with the columns seq, time, kind, and task. It is a testing aid, and its route name says so in the rail ("Events").
- **Strings.** Every visible string is in `src/strings/en.ts` as `export const en = { … }`, read through `t(key)` (`src/strings/t.ts`, typed by the object's keys). There is no library (the phase decision).
- **Development.** `apps/web/vite.config.ts` proxies `/rpc` (WebSocket), `/connect`, `/session` and `/disconnect` to `http://127.0.0.1:${FARIK_PORT}`. It rewrites `Origin` and `Host` to the daemon's own (step 02's instruction), so `pnpm --filter @farik/web dev` works against a running `farik serve --port <FARIK_PORT>`.
- **Tests.**
  - Unit and component tests run in Vitest and jsdom, co-located, with `expectNoAxeViolations` (from `@farik/ui/test`) on each screen. `vitest.config.ts`: `environment: "jsdom"`, `setupFiles: ["src/test/setup.ts"]`, `include: ["src/**/*.test.{ts,tsx}"]` (so `e2e/*.spec.ts` stays Playwright's). `setup.ts` registers `afterEach(cleanup)` and defines `window.matchMedia` as a stub the tests control.
  - The Playwright suite uses `@playwright/test` `=1.63.0` (Chromium; its browser matches the cached `chromium-1243`) in `apps/web/e2e/`.
    - `fixtures/serve.ts` exports `startServe({ transcripts }): Promise<{ url: string; port: number; project: string; stop(): Promise<void> }>`. It makes a temporary git repository, runs `farik init` in it with sandboxing off (the same `settings.json` the CLI tests' `no_sandbox` writes), sets `XDG_CONFIG_HOME` to a temporary folder (so the developer's own Farik state is never touched), finds a free port through Node's `net`, spawns `target/debug/farik-e2e-serve --port <p>`, and reads the link from its stdout. `stop()` sends SIGINT and waits for the exit.
    - `connect.spec.ts` is this step's journey.
    - Screenshots at 360 × 780 and 1280 × 800 go to `apps/web/e2e/screenshots/` (gitignored) for the landing review.
- **Wiring the check.** `cargo xtask check --integration` first runs `pnpm -r --if-present generate` and `pnpm --filter @farik/web build`, before its first cargo command, so that no farik it compiles embeds a missing `apps/web/dist` (`xtask::check::web_app_first(tests: Tests) -> Vec<Vec<&'static str>>` lists them, empty without `--integration`). It runs these after `pnpm check`:
  1. `cargo clippy -p farik --features e2e --bin farik-e2e-serve -- -D warnings`;
  2. `cargo test -p farik --features e2e --test serving -- --include-ignored` (runs the e2e binary's own test, which needs git and is `#[ignore]`d);
  3. `cargo build -p farik --features e2e --bin farik-e2e-serve`;
  4. `pnpm --filter @farik/web e2e` (`playwright test`).

  `xtask::check::integration_steps(tests: Tests) -> Vec<(&'static str, Vec<&'static str>)>` lists them, as (program, args), and is empty without `--integration`. CI adds `pnpm install --frozen-lockfile` and `pnpm --filter @farik/web exec playwright install --with-deps chromium` before the check. Clippy's `--all-targets` does not build the feature-gated binary, so `integration_steps`' cargo build is what compiles it.

## File map

```
Cargo.toml, crates/runtime/Cargo.toml, Cargo.lock        modifies: rust-embed (T1)
crates/runtime/src/daemon.rs, daemon/web.rs, daemon/app.rs  modifies/creates: app serving, /session, /disconnect, revoke, 1011 close (T1)
crates/runtime/build.rs, src/daemon/app-fixture/{index.html,assets/app-3f2a.js}  creates: rerun on a new web build; the test embed (T1)
crates/cli/Cargo.toml, src/bin/farik-e2e-serve.rs         modifies/creates: e2e feature and binary (T2)
crates/cli/src/{lib.rs,serve.rs,main.rs}, tests/serving.rs  modifies: --no-open, open_url (no-op in CliIo::new, system opener in main.rs) (T2)
pnpm-workspace.yaml, biome.json, .gitignore               modifies: apps/*, apps/**, e2e screenshots (T3)
apps/web/{package.json,tsconfig.json,vite.config.ts,vitest.config.ts,index.html}  creates (T3)
apps/web/src/{main.tsx,app/App.tsx,app/connection.tsx,app/store.ts,app/theme.ts,strings/en.ts,strings/t.ts,test/setup.ts} (+ tests)  creates (T3)
apps/web/src/shell/{Shell.tsx,PauseControl.tsx}, pages/{Connect,Settings,Events,NotFound}.tsx (+ css, tests)  creates (T4)
apps/web/e2e/{playwright.config.ts,fixtures/serve.ts,connect.spec.ts}                   creates (T5)
xtask/src/{check.rs,main.rs}, .github/workflows/check.yml                                modifies (T5)
docs/SPEC.md (8.1, 8.6), docs/plans/project-plan.md (step 04 line; the phase decision naming react-router 7 → 8.4.0)  modifies (T6)
pnpm-lock.yaml                                                                           modifies (T3, T5)
```

## Interfaces

Consumes:
- `WebState`, `BrowserSessions`, `ConnectCodes`, and `session_cookies` (`daemon::web`); the browser router (`daemon.rs`); `serve::serve`; `CliIo`; `Engine::Given`; `RecordedAdapter`; `fixtures::tool_runner` (step 02);
- `connect`, `DaemonClient`, and `RpcError` (`@farik/protocol-client`);
- the `@farik/ui` components and `uiStrings`;
- `@farik/brand`'s `tokens.css`, `fonts.css`, and assets.

Produces:

```rust
impl BrowserSessions { pub fn revoke(&self, secret: &str) -> Result<(), DaemonError>; }
pub(crate) async fn app_from<E: rust_embed::RustEmbed>(/* state, uri, headers */) -> Response;   // daemon::app: files + index.html fallback + headers
CliIo::open_url: Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>   // no-op in CliIo::new; the system opener in main.rs
Commands::Serve { port: Option<u16>, no_open: bool }
pub fn xtask::check::integration_steps(tests: Tests) -> Vec<(&'static str, Vec<&'static str>)>;
```

```ts
export function ConnectionProvider(props: { children: ReactNode }): JSX.Element;
export function useConnection(): { status: 'checking' | 'no_session' | 'connecting' | 'open' | 'lost'; client: DaemonClient | null };
export function useEvents(): Event[];
export function useQuery<T>(name: QueryName, params: object): { data: T | undefined; error: RpcError | undefined };
export type ThemeChoice = 'light' | 'dark' | 'system';  export function useTheme(): [ThemeChoice, (c: ThemeChoice) => void];
export function t(key: keyof typeof en): string;
export function startServe(o: { transcripts: string[] }): Promise<{ url: string; port: number; project: string; stop(): Promise<void> }>;
```

## Tasks

### Task 1: The daemon serves the app

Tests (Rust, route tests like step 02's, `#[ignore]` where they need `TestDaemon`):
- `serves_the_app_and_its_routes` asserts, with the fixture embed, that `GET /` answers 200 `text/html` with `Cache-Control: no-store`, `GET /settings` answers the same `index.html`, and `GET /assets/app-3f2a.js` answers `text/javascript` with `immutable`. Every Task 1 test that fetches a page uses the fixture or the empty embed.
- `says_when_the_app_was_not_built` asserts that with an empty embed, `GET /` answers 200 with the one-line sentence.
- `sends_the_security_headers` asserts the exact `Content-Security-Policy` (with the port), `X-Content-Type-Options: nosniff`, and `Referrer-Policy: no-referrer` on `/` and on an asset.
- `serves_the_app_only_to_its_own_host` asserts 403 for `Host: evil.example` on `GET /`, and 200 with no `Origin`.
- `tells_the_page_whether_it_has_a_session` asserts that `GET /session` answers 204 with a valid cookie and no `Origin` (as a browser's same-origin fetch sends it), 401 without a cookie, and 403 with a foreign `Origin` or a foreign `Host`.
- `disconnect_revokes_the_session` asserts that `POST /disconnect` answers 204 with the clearing `Set-Cookie`, and that `verify` of that secret is then false.
- `closes_the_socket_when_the_log_cannot_be_read` asserts that after the event log's table is dropped through a second SQLite connection, the subscribed socket closes with code 1011 and the reason text (30 s failure bound).

- [x] `feat(runtime): serve the web app from the binary, with its session routes`

### Task 2: Opening the browser, and the end-to-end binary

Tests:
- `opens_the_link_in_a_browser` asserts that `farik serve`, with a recording `open_url`, calls it once with the printed link.
- `does_not_open_with_no_open` asserts that with `--no-open` it is never called.
- `keeps_serving_when_no_browser_opens` asserts that an `open_url` returning `Err("no display")` prints the stderr sentence and serving continues (`farik stop` then exits 0).
- `the_e2e_binary_serves_with_recorded_sessions` (in `crates/cli/tests/serving.rs`, `#[cfg(feature = "e2e")]`, run by integration step 2) asserts that `farik-e2e-serve --port <p>` prints the link and answers `GET /session` with 401.

- [x] `feat(cli): open the browser from farik serve, and add the e2e server`

### Task 3: The app's frame and its connection

Tests (Vitest, jsdom):
- `trades_the_code_in_the_link_for_a_session` asserts that with `location.hash` `#<64 hex>`, `fetch` is called with `POST /connect` and `{ code }`, the hash is removed, and a 204 leads to `/`.
- `says_the_link_was_used` asserts that a 401 from `/connect` shows `en.linkUsed`.
- `asks_for_a_start_link_without_a_session` asserts status `no_session` when `/session` answers 401.
- `reconnects_after_the_connection_is_lost` asserts that with fake timers, a closed socket gives `lost`, and after 5 s `/session` is asked again, a new socket opens, and it subscribes from the last seq held (not 0).
- `keeps_the_last_five_hundred_events` asserts that after 600 events, `useEvents` holds 500, the newest last.
- `queries_again_after_an_event` asserts that `useQuery('serve.status')` queries once on mount, and that after a burst of 10 events and 250 ms of fake time it has queried exactly twice in all.
- `remembers_the_theme_and_follows_the_computer` asserts that `setTheme('dark')` sets `data-theme="dark"` and `localStorage['farik.theme']`, and that `system` follows a `matchMedia` change.
- `works_without_local_storage` asserts that with `localStorage` throwing, the theme defaults to light and no error escapes.

- [x] `feat(web): add the web app's frame, connection, events and theme`

### Task 4: The shell, Pause, Settings, and the pages

Tests (Vitest, jsdom; each screen also passes `expectNoAxeViolations`):
- `shows_the_rail_on_a_wide_screen_and_a_bar_on_a_phone` asserts that at a `matchMedia('(min-width: 1024px)')` of true, a `navigation` holds links named Events and Settings; at false, a bottom `navigation` does.
- `pauses_and_resumes_the_team` asserts that with `serve.status.paused` false, the button named "Pause the team" sends `team_pause`, shows busy while pending, and after a `team.paused` event reads "Resume the team" and the banner appears.
- `shows_a_refusal_in_words` asserts that a `not_paused` reply shows its sentence.
- `settings_changes_the_theme_and_disconnects` asserts that choosing Dark sets `data-theme`, that the Advanced switch toggles `farik.advanced`, and that "Disconnect this browser" posts `/disconnect` and then shows the connect page's start-link text.
- `shows_the_connect_page_states` asserts the `no_session` text (with `farik serve` in a copyable code element) and the `lost` text with its 5-second line.
- `lists_the_newest_events_first` asserts that the Events table's first row is the highest seq.
- `says_there_is_no_page_here` asserts that an unknown route shows `en.noPage` and a link to `/`.

- [x] `feat(web): add the shell with pause, settings, connect and events`

### Task 5: The Playwright journey and its wiring

Tests:
- `connect.spec.ts: the start link opens the app and pause works end to end`:
  1. `startServe({ transcripts: [] })`, then open the link;
  2. the page reaches `/events` and shows Connected;
  3. "Pause the team" makes the banner appear, and the log then holds `team.paused` (read through `farik log --json` in the project);
  4. "Resume the team" removes the banner;
  5. a reload stays connected with no code;
  6. screenshots are taken at both sizes.
- `connect.spec.ts: a used link and a lost connection are said plainly`:
  1. the same link opened in a second page shows the link-used text;
  2. `stop()` makes the first page show "Farik stopped answering".
- `integration_steps_build_the_app_and_run_the_journey` (xtask unit test) asserts `integration_steps(Tests::All)` is exactly the four steps in order, and `integration_steps(Tests::WithoutTheOnesThatNeedAProgram)` is empty.

- [x] `test(web): drive the shell through the real server and browser`

### Task 6: Spec and plan

`docs/SPEC.md` 8.1: the app is embedded, `farik serve` opens it (`--no-open`), and an unbuilt app says so. 8.6: the page headers (CSP, `frame-ancestors 'none'`, `nosniff`, `no-referrer`), `GET /session`, `POST /disconnect`, and GETs checked on `Host` alone. `docs/plans/project-plan.md`: the phase 6 step 04 interface line becomes this plan's, with `/disconnect` in place of the `browser.disconnect` query.

The phase decision naming `react-router` 7 changes to 8.4.0, with the reason (8 is current; its declarative mode is the one decided).

- [x] `docs(spec): say how the web app is served and how a browser disconnects`

## Verification

```
cargo xtask check --integration
# expected: every cargo "test result:" line 0 failed; pnpm: @farik/web "Tests  15 passed (15)" (T3 8, T4 7);
#   playwright "2 passed"; last line: xtask check: ok
```
