# Phase 7, step 03e: A homepage and a privacy policy

Status: draft
Branch: `phase/7-role-kits`
Spec: `docs/SPEC.md` 8.6
Depends on: step 03c (committed and deployed before Task 4: `infra`, the hosted zone input, GitHub's OpenID Connect provider, the deploy role and the `infra` workflow). Independent of step 03d; either may run first.
Readiness: added on 2026-10-02 by the planner folding the readiness reviews of steps 03b and 03c, on the founder's decision of that day; not reviewed separately (ADR 0032)
Mockups approved by: pending (Task 1's gate)

## Goal

Slack's Marketplace listing (a launch dependency, ADR 0035's amendment), the GitHub App's homepage, and later Google's verification each need a public homepage and a privacy policy on Farik's own domain. Until now both waited for the website (phase 11 step 01). When this step is done, `https://<domain>/` is a one-page homepage and `https://<domain>/privacy` the privacy policy, static, served by CloudFront from a private bucket as ADR 0017 sets out, so the founder can start those reviews months before the launch. Out of scope: the website itself (`apps/site`, the docs, the downloads), which phase 11 step 01 builds and which replaces these two pages.

## Decisions

- **Two hand-written pages, no build.** `infra/site/index.html`, `infra/site/privacy.html` and `infra/site/site.css` (the brand's colours and fonts as plain CSS values, the wordmark as text, no image, no script). Rejected: starting `apps/site` now, which is phase 11's design and build work.
- **The homepage** is the `Site` board's hero and its "Open source, and free to run yourself" section (`docs/design/mockups/Site.dc.html`), with a link to the GitHub repository and to `/privacy`, and the contact address the founder gives.
- **The privacy policy** says, in plain words: Farik runs on the user's computer and sends Farik's makers nothing; connector keys and sign-ins stay in the user's own keychain; the sign-in relay at `signin.<domain>` sees a service's tokens in transit during a sign-in or a refresh, keeps none, and logs one line per request (time, route, service, outcome, the service's status, duration) for 30 days; this site sets no cookie and runs no analytics; CloudFront's own access logs are off; the contact address; the date. The founder approves the wording at Task 1's gate; it is a policy the founder publishes, not legal advice.
- **The stack, `SiteStack` (`FarikSite`, `us-east-1`)**, in `infra/lib/site-stack.ts`, from the same environment inputs as `RelayStack` (`FARIK_DOMAIN`, `FARIK_HOSTED_ZONE_ID`, `CDK_DEFAULT_ACCOUNT`):
  - a private S3 bucket: all public access blocked, versioned, `enforceSSL`, `RemovalPolicy.RETAIN`;
  - a CloudFront distribution with the bucket as an origin through origin access control (`S3BucketOrigin.withOriginAccessControl`), the aliases `<domain>` and `www.<domain>`, an ACM certificate for both, validated through the hosted zone, `minimumProtocolVersion` `TLSv1.2_2021`, viewer protocol redirect to HTTPS, `defaultRootObject` `index.html`, price class 100, no access logging;
  - a response headers policy: HSTS `max-age=31536000`, `Content-Security-Policy: default-src 'none'; style-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, `X-Frame-Options: DENY`;
  - a CloudFront Function (`cloudfront-js-2.0`, viewer request) in `infra/site/edge.js`: a request to `www.<domain>` is a 301 to the same path on `<domain>`; `/privacy` is served from `/privacy.html`; any other path is passed on;
  - A and AAAA alias records for `<domain>` and `www.<domain>`;
  - a `BucketDeployment` of `infra/site/*.html` and `site.css`, invalidating `/*` on the distribution.
- **Deployment** is step 03c's `infra` workflow, which deploys every stack (`cdk deploy --all`) through the same OpenID Connect role; nothing new is created for it. The first deploy of `FarikSite` is the founder's, from the founder's machine, as for the relay.

## File map

```
docs/design/mockups/{Site,Privacy}.dc.html, canvas.json                 Task 1
infra/site/{index.html,privacy.html,site.css,edge.js}                   creates: the pages and the edge function (Task 2)
infra/test/site-pages.test.ts                                           creates: the pages' and the function's tests (Task 2)
infra/lib/site-stack.ts, infra/bin/farik.ts                             creates, modifies: the stack (Task 3)
infra/test/site-stack.test.ts                                           creates: the stack's assertions (Task 3)
docs/SPEC.md, docs/plans/project-plan.md                                Task 4
```

## Interfaces

Consumes: `infra`'s package, `bin/farik.ts` and its inputs, and the `infra` workflow (step 03c).

Produces:

```ts
// infra/lib
export class SiteStack extends Stack { constructor(scope: Construct, id: string, props: SiteStackProps) }
export interface SiteStackProps extends StackProps { domain: string; hostedZoneId: string }
```

## Tasks

### Task 1: The two pages, mocked up

On the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf), desktop and phone width, the canvas's tokens, muted and light, copied to `docs/design/mockups/`: the homepage, cut from the `Site` board as the Decisions say, and a `Privacy` board with the policy's full text.

Gate: the founder approves both boards and the policy's wording, and gives the contact address in conversation; the approval and its date go into this plan's header. Task 2 waits for it.

- [ ] `docs(design): mock up the homepage and the privacy policy`

### Task 2: The pages

Files: `infra/site/*`, `infra/test/site-pages.test.ts`. The tests read the files from disk; the function's tests evaluate `edge.js` with `new Function(source + '; return handler;')()`, since a CloudFront Function cannot `export`.

- `pages_hold_no_script_and_link_the_policy`: neither page has a `<script`, a `style=` attribute, an inline `<style>`, or an external URL other than the GitHub repository and `mailto:`; `index.html` links `/privacy`, and both link `site.css`.
- `the_policy_says_what_farik_keeps`: `privacy.html` holds the approved sentences on the computer, the keychain, the relay's tokens in transit and its six-field log, no cookie and no analytics, the contact address, and its date.
- `edge_redirects_www`: a request with host `www.example.test` and URI `/privacy` gives a 301 with `location` `https://example.test/privacy`.
- `edge_serves_clean_urls`: `/privacy` on `example.test` is passed on with URI `/privacy.html`; `/` and `/site.css` are passed on unchanged.

- [ ] `feat(infra): a homepage and a privacy policy`

### Task 3: The site's stack

Files: `site-stack.ts`, `bin/farik.ts`, `site-stack.test.ts`. Tests (the CDK's `Template` assertions, step 03c's made-up inputs):

- `serves_the_site_only_through_cloudfront`: the bucket blocks all public access, is versioned and retained, and its policy denies non-TLS requests and allows `s3:GetObject` only to the CloudFront service principal with `AWS:SourceArn` the distribution; one origin access control exists.
- `answers_over_https_with_its_headers`: the distribution has the aliases `example.test` and `www.example.test`, `MinimumProtocolVersion: TLSv1.2_2021`, `ViewerProtocolPolicy: redirect-to-https`, `DefaultRootObject: index.html`, no `Logging`, the viewer-request function, and a response headers policy with the Decisions' HSTS, CSP, `nosniff`, referrer and frame values.
- `names_both_hosts`: A and AAAA alias records for `example.test` and `www.example.test`, and a DNS-validated certificate covering both.

- [ ] `feat(infra): serve the homepage from CloudFront`

### Task 4: The pages, live

Gate: step 03c's live check has passed, and the founder has run the first deploy: `pnpm --filter @farik/infra exec cdk deploy FarikSite`, from the founder's machine with the inputs set.

Files: `docs/SPEC.md` (8.6: the homepage and privacy policy, and what they promise); `docs/plans/project-plan.md` (row 03e, corrected if execution changed it).

- [ ] `docs(spec): record the homepage and the privacy policy`

Founder's actions (no agent holds AWS credentials):
- [ ] **The first deploy** of `FarikSite`, as the Gate says.
- [ ] **The GitHub App's homepage** (step 03b) set to `https://<domain>/`.
- [ ] **Slack's app** (step 03c): its privacy policy URL set to `https://<domain>/privacy`, ready for the Marketplace submission in phase 11.

## Verification

```
cargo xtask check
# expected: xtask check: ok
FARIK_DOMAIN=example.test FARIK_HOSTED_ZONE_ID=Z000000000000 FARIK_ALERT_EMAIL=a@example.test \
  FARIK_MONTHLY_BUDGET_USD=20 CDK_DEFAULT_ACCOUNT=123456789012 \
  pnpm --filter @farik/infra exec cdk synth FarikSite --quiet
# expected: exit 0
```

The founder's live check, recorded in the pull request: `curl -sI https://<domain>/` answers 200 with the HSTS and CSP headers; `curl -sI https://www.<domain>/privacy` answers 301 to `https://<domain>/privacy`; `curl -sI https://<domain>/privacy` answers 200; both pages read correctly on a phone.
