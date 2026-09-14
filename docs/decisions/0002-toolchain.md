# 0002. TypeScript monorepo toolchain

Date: 2026-09-14
Status: accepted

## Context

`docs/SPEC.md` section 8 commits to a TypeScript monorepo. The remaining choices are the tools around it, and they are best made before the first line of code so that no pull request ever argues about them.

Lint and format: ESLint plus Prettier is the common pair and requires two configurations that occasionally disagree. Biome does both in one tool, is fast enough to run on every save, and its rule set covers what this project needs. Its ecosystem of plugins is smaller; that is acceptable because the project does not intend to write custom lint rules.

Tests: Vitest runs TypeScript natively, shares configuration with the Vite-based front end, and is fast enough for the exhaustive governor test suite. Jest would need a transform layer for no gain.

Commit hooks: Husky is the default choice in many templates; lefthook is a single binary with a YAML config and no Node dependency in the hook path. Either works. Hooks are a convenience anyway; CI enforces.

Versioning: Changesets fits a workspace with several published packages and makes the changelog a by-product of pull requests.

Package manager: pnpm for workspace support and disk efficiency.

## Decision

pnpm workspaces, TypeScript strict, Biome for lint and format, Vitest for tests, lefthook for hooks, Changesets for versioning, GitHub Actions for CI, with one `pnpm check` command as the definition of mergeable. Details in `docs/standards/code.md`.

## Consequences

Contributors need pnpm and a current Node LTS installed; the scaffold's README will say which versions.

Biome's formatting is opinionated and not identical to Prettier's. Contributors coming from Prettier projects will see diffs they did not expect on first save. That is a one-time cost.

Switching any of these later is a mechanical change while the codebase is small and an expensive one after. The scaffold plan should verify each tool works end to end before adding the second package.
