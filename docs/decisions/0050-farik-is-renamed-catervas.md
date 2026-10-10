# 0050. Farik is renamed Catervas, in its code as well as its words

Date: 2026-10-09
Status: accepted (the founder, 2026-10-09, in conversation: "I am currently looking to do rebranding across farik repository. I included the new banner as well as the new brand-kit. I also included new characters that I need modified instead of the current ones. [...] The new name for project is Catervas"; asked how deep the rename goes, "Full rename, code too"; asked which new characters the Architect and the Marketing Specialist wear, "Arch=Extra-2, MKT=Extra-3"; asked about the Product Manager, whose new file looks like the Finance Specialist's, "PM takes Extra-1"; asked about the new wordmark's navy background, "Make it transparent"). Amends ADR 0047 and 0048, whose `farik-ops` is `Catervas-ops`.

## Context

The founder renamed the product from Farik to Catervas on 2026-10-09 and supplied, in `docs/brand/`, a brand kit with the new name, a README banner, a wordmark in cream on navy, and ten redrawn characters. The kit is the old one with the name changed: the palette, type, logo mark and voice hold. The new characters are drawn from the waist up and wear glasses; there is no new Architect or Marketing Specialist, and the new Product Manager and Financial analyst look alike.

The name was everywhere: in prose, and in identifiers that are wire and file formats (spec 8): the crates (`farik-core` and the rest), the binary `farik`, the agents' tools (`farik_*`), the npm scope `@farik`, the CSS tokens (`--farik-*`), the workspace folder `.farik/`, the keychain service `farik`, the environment variables `FARIK_*`, and the event type `FarikEvent`.

The options were:
- **The visible name only.** Prose, the web UI's words and the art; identifiers kept. A small change, but the code would name a product that no longer exists, for good.
- **The name everywhere, code included.** One large mechanical change, while no one but the founder runs Catervas: there is no release (the web launch is phase 13), so no user's workspace or keychain has to be migrated. This was the founder's choice.
- **Everything, the history too.** Also rewriting the ADRs and step plans, which would make them misquote what was decided under the old name.

## Decision

**Catervas replaces Farik everywhere but the record.** Every tracked text file outside `docs/decisions/` and `docs/plans/` has `farik`, `Farik` and `FARIK` replaced by `catervas`, `Catervas` and `CATERVAS`, and the two files with the name in their path are renamed. What keeps the old name:
- the ADRs and the step plans, which are history; a link into them keeps their file names;
- the GitHub repository, `abdshaat/Farik`, and every link to it, until the founder renames it on GitHub (GitHub then redirects the old address).

**No migration.** A machine that ran Farik keeps its old state under the old names: the founder moves a project's `.farik/` to `.catervas/`, signs in again (the AI account's credential and the connectors' keys are kept under the keychain service `catervas`), and renames any `FARIK_*` environment variable.

**The art.**
- The characters are kept under the avatar key they serve, so no agent's stored `avatar` changes: the Product Manager wears Extra-1, the Architect Extra-2, the Marketing Specialist Extra-3, the UI/UX Designer (`extra-1`) the UI-UX engineer, `extra-3` (the DevOps Engineer's in phase 11) Dev-ops, the Finance Specialist and `extra-4` the Financial analyst, the Procurement Specialist (`extra-5`) Extra-4, and `extra-2` the founder's "Product-manager PM". `docs/brand/brand.md` has the table.
- The 256 px avatars are derived again from the new characters.
- The wordmark is the founder's file with the navy keyed out, in Soft Sand on a transparent background, as the old one was; the founder's file is kept beside it.
- The README's banner is the founder's; its team card is rebuilt from the new characters.

## Consequences

- Every name in code, command and wire format changes at once: `catervas serve`, `catervas_ask_human`, `@catervas/ui`, `.catervas/`. Spec revision 0.80 records it.
- A pull request open against the old names (#30) needs rebasing onto this one.
- The step plans not yet executed still say `farik`; each is read against the code at its readiness review, which takes the new names.
- The wordmark is wider than the old one, so the web UI shows it wider at the same height (220 px on the first-run screen, 264 px on the sign-in page).
