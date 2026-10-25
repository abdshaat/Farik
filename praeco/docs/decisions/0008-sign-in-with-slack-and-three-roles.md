# 0008. Sign in with Slack, and three roles for people

Date: 2026-10-24
Status: proposed

## Context

Catervas has one human and a one-time code (Catervas ADR 0021). Praeco has several people per company, and approvals must come from the right ones. Options for identity: email and password; magic links; Google sign-in; Sign in with Slack (OpenID Connect). Every Praeco user is in the company's Slack, and the Slack user is who presses Approve.

## Decision

People sign in to the web app with Slack's OpenID Connect; the Slack user id and team id are the member's identity, so the person who approves in Slack and in the web app is one member. The first person to install Praeco into a Slack workspace is its owner. Roles: `owner` (settings, budget, connections, members, roles), `approver` (approves, sends back, and is the only feedback that teaches, SPEC 5.7) and `member` (asks and reads). A workspace always has an owner: removing or demoting the last one is refused. Sessions are cookies valid 30 days, `HttpOnly`, `SameSite=Lax` (Slack's sign-in returns by a top-level redirect, which `Strict` would drop), `Secure` on the hosted service, with an `Origin` check on every state-changing request.

## Consequences

No passwords to store, and no gap between the Slack person and the web app person.

What cuts against it: a company without Slack cannot sign in at all until another chat is added; and the agency persona, one person across several clients' Slacks, signs in once per Slack, which SPEC open question 3 leaves to phase 8.
