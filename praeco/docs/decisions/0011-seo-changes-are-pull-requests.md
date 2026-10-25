# 0011. SEO changes are pull requests and drafts, never publishing

Date: 2026-10-24
Status: proposed

## Context

The founder wants the agent to "optimize SEO for the product". Its fixes touch the company's live website. Options: write to a CMS (WordPress, Webflow) directly; open pull requests to the site's repository; only report. Praeco's first persona keeps the site in a repository and reviews pull requests as part of their day; Catervas's own rule is that a reviewer is never the author.

## Decision

- SEO fixes are pull requests to one site repository the owner connects through Praeco's GitHub App, on branches `praeco/<job number>`, touching only paths matching the owner's `seo.allowed_paths` globs (default none: the owner sets them in setup). The governor refuses `open_pull_request` for any other repository or path. Praeco never merges, never pushes to another branch, and never edits a branch a person has pushed to.
- Every pull request is an outward act and asks (SPEC 5.6), carrying the brief and the measured reason.
- Sites not kept in a repository get drafts (title, description, article, page) in the web app and Notion; a CMS connector is after `v0.1.0`.
- Read access to the product's own repository (for its state, SPEC 6.1) is a separate, read-only installation of the same app.

## Consequences

Every change to the live site goes through the company's own review, history and rollback, and a bad fix costs a closed pull request, which teaches (SPEC 5.7).

What cuts against it: a site on Webflow or WordPress gets drafts only until a CMS connector exists, which narrows the first persona to sites in a repository.
