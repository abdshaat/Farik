# 0002. A public repository, a hosted service, and their operations apart

Date: 2026-10-24
Status: proposed

## Context

The founder wants the repository public and a fully hosted option. Catervas set the pattern: Apache 2.0 for everything that governs agents, a commercial `ee/` directory for what only the hosted service needs (Catervas SPEC section 9), and the hosted service's infrastructure planned in a private operations repository (Catervas ADR 0047), so that the public repository never carries a credential or an account id of the company's (Catervas ADR 0043). The alternatives were AGPL, which some companies will not run, and a source-available license, which weakens the claim that the governor can be audited.

## Decision

Praeco's repository is public, Apache 2.0 for everything outside `ee/`. `ee/` holds the hosted service's billing and nothing else, under a commercial license added by the step that creates it (phase 10). The hosted service runs the same image as a self-hosted installation; its infrastructure (AWS, Catervas ADR 0017's choice), its secrets and its app registrations (Slack, Google, GitHub, Anthropic) live in the private `praeco-ops` repository, which this repository never references by account id or secret. Nothing that makes the agent safer or more controllable is ever paid for.

## Consequences

A company can run all of Praeco on its own server with its own keys and app registrations, and audit every rule it is held to. The hosted service's code paths are the self-hosted ones plus `ee/`, so a bug found by one is fixed for both.

What cuts against it: self-hosting means registering a Slack app, a Google app and a GitHub app, which the self-hosting guide (phase 10) has to make bearable; and a competitor can host Praeco too. The second is the price of the first claim, and is accepted.
