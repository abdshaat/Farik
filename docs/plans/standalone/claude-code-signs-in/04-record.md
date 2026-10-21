# Claude Code keeps the AI account, plan 4: the end-to-end run, the reset script and the spec

Status: ready (after plan 3)
Branch: `docs/claude-code-signs-in`
Spec: `docs/SPEC.md` 4.1, 5.5, 8.2, 8.6, Settings' AI account; ADR 0084
Depends on: plans 1 to 3 committed on this branch
Readiness confirmed by: the planner (Opus 5.5), 2026-10-21 (ADR 0065)

## Goal

The browser journey of the first run signs in through a fake Claude Code. The reset script no longer touches a key. The spec says what Catervas does now.

## File map

```
apps/web/e2e/fixtures/fake-bin/claude                modifies: answers auth status, auth login, auth logout and the check
apps/web/e2e/setup-project.spec.ts           modifies: signs in instead of pasting a key
apps/web/e2e/fixtures/serve.ts               modifies: the comment on project: false; --no-keychain kept
scripts/catervas-reset.sh                    modifies: no keychain or credential.json; --keep-key kept, now a no-op with a note
docs/SPEC.md                                 modifies: the sections below, revision 0.141
docs/decisions/0022-...md                    modifies: Status "superseded by ADR 0084"
docs/decisions/0021-...md                    modifies: a line under its Decision: "amended by ADR 0084: the first run signs in through Claude Code"
```

## Tasks

### Task 13: The first run's journey signs in through a fake Claude Code

The fake `claude` behaves as follows:

- `auth status --json` answers `loggedIn: false` until `$HOME/.fake-claude-signed-in` exists, then `loggedIn: true`, `authMethod: "oauth_token"`.
- `auth login` prints ADR 0084's two lines, with the address `https://claude.com/cai/oauth/authorize?fake=1`. It reads one line; when that is `e2e-code` it touches the file and exits 0, else it exits 1.
- `-p` with the check's arguments prints the ADR's good answer.

The test (`setup-project.spec.ts:46-56`) does the following and asserts the results:

1. It presses "Sign in with Claude". The page tries to open the address; the test stubs `window.open` and asserts the URL.
2. It fills "Your sign-in code" with `e2e-code` and presses "Finish signing in".
3. It sees "It works.", goes on to choose the project, and asserts that no `credential.json` exists in the state folder.

- [x] `test(web): the first run signs in through claude code`

### Task 14: The reset script, the ADRs and the spec

- `scripts/catervas-reset.sh`: lines 8, 62 and 134 stop deleting or keeping a keychain entry or `credential.json`. `--keep-key` prints "Catervas keeps no key: your sign-in is Claude Code's" and goes on.
- ADR 0022's Status reads `superseded by ADR 0084 (2026-10-21)`. ADR 0021 gains its line.
- `docs/SPEC.md`, revision 0.141 (2026-10-21), "records that Claude Code keeps the AI account (ADR 0084)". It rewrites:
  - :141, 4.1's first run: the sign-in through Claude Code, the check, the four states;
  - :442, 5.5: a refused key pauses the team; signing in again or a working check resumes it;
  - :1217, 8.2: `catervas run` names the account's kind and never a key;
  - :1325 and :1335, 8.6: Catervas keeps and reads no key; a key in the user's environment passes through unread; the check's command, cost and outcomes; the one-time removal;
  - :1347, host mode: `/proc/<pid>/environ` holds no key of Catervas's;
  - :1361, Settings' AI account section;
  - :1363, e2e's `--no-keychain`.

  F9's methods list `account.sign_in`, `account.sign_in_code`, `account.check` and `account.sign_out`, in place of `account.connect` and `account.disconnect`.

- [x] `docs(spec): claude code keeps the ai account, revision 0.141`

## Verification

```
python3 reaper.py cargo xtask check --integration
# expected: xtask check: ok, the e2e journey included
```
