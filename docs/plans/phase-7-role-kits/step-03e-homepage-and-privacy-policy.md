# Phase 7, step 03e: A homepage and a privacy policy

Status: deferred by the founder, 2026-10-02: Slack is a later step ("Connecting slack is a later step keep it simple for now"). Reviewed and folded; lands after the launch as phase 12 step 02d (project plan revisions 30 and 31), where it is reviewed for readiness again, moved and renumbered. Where it says phase 11 for Slack's listing, read phase 12 step 02d; the website (phase 11 step 01) is live by then.
Branch: `phase/12-desktop` when taken up (written on `phase/7-role-kits`)
Spec: `docs/SPEC.md` 8.6
Depends on: step 03c (committed and deployed before Task 4: `infra`, the hosted zone input, GitHub's OpenID Connect provider, the deploy role and the `infra` workflow). Independent of step 03d; either may run first.
Readiness: fresh-session Opus reviewer, 2026-10-02: not ready, 3 Blocking, all folded with the founder's answers; no second round (ADR 0032)
Controller: pending (the founder's first action: the company's legal name and its country)
Contact: pending (the founder's first action: `support@<domain>`, or the address the founder picks on `<domain>`)
Mockups approved by: pending (Task 1's gate)

## Goal

Slack's Marketplace listing (a launch dependency, ADR 0035's amendment), the GitHub App's homepage, and later Google's verification each need a public homepage and a privacy policy on Farik's own domain. Until now both waited for the website (phase 11 step 01). When this step is done, `https://<domain>/` is a one-page homepage and `https://<domain>/privacy` the privacy policy, static, served by CloudFront from a private bucket as ADR 0017 sets out, so the founder can start those reviews months before the launch. Out of scope: the website itself (`apps/site`, the docs, the downloads), which phase 11 step 01 builds and which replaces these two pages.

## Decisions

- **Two hand-written pages, no build.** `infra/site/index.html`, `infra/site/privacy.html` and `infra/site/site.css` (the brand's colours as plain CSS values, the wordmark as text, no image, no script). Rejected: starting `apps/site` now, which is phase 11's design and build work. Fonts are self-hosted: `infra/site/fonts/` holds the WOFF2 files of Space Grotesk, JetBrains Mono and Silkscreen, each under the SIL Open Font License 1.1, with their `OFL.txt`. They are subset to Latin (the Fontsource packages' `latin` files, copied in, not a dependency) and declared by `@font-face` in `site.css`. No font comes from another origin. Rejected: Google Fonts, which would give visitors' IP addresses to Google; and system fonts only, which lose the wordmark.
- **The controller and the contact** are the founder's facts, never an agent's: the data controller is a company, and the header's `Controller:` and `Contact:` lines hold its legal name, its country and the contact address once the founder gives them. The contact is one mailbox on Farik's domain (for example `support@<domain>`), which the founder reads and answers within two business days; it is the policy's contact and Slack's support contact. **The executor stops** at Task 1 if either line still says pending, and no page ships placeholder text.
- **The mailbox's DNS records stay out of the stacks.** Recommended host: Amazon WorkMail (AWS, as ADR 0017 sets out; any provider works). The founder adds its MX, SPF, DKIM and DMARC records in the Route 53 hosted zone by hand, or with WorkMail's own "update in Route 53" step. Why not CDK: the values are the provider's and change with it, the hosted zone is imported (`fromHostedZoneAttributes`), not owned by a stack, so hand-added records do not drift against one, and code would add four inputs and their tests for records set once. Revisit if a second mail record set is ever needed.
- **The homepage** is the `Site` board's hero and its "Open source, and free to run yourself" section (`docs/design/mockups/Site.dc.html`), plus three short sections the reviews need:
  - **Farik in Slack**: the Scrum Master posts the team's updates and reads replies in the channel the user picks, with the user token scopes `channels:read`, `channels:history`, `chat:write`, `users:read`, and why each is needed.
  - **Connecting Slack**: in Farik, on an agent's Add connector page, choose Slack and sign in. Slack's consent page names Farik, and Farik shows "Connected" when it is done. This covers Slack's "path to installing" and "confirms the installation" items.
  - **Help** (anchor `#help`): the contact address, and "we answer within two business days".

  A sentence near the top says: "Farik's agents use an AI model, which can produce inaccurate answers, summaries or other output; check what they write before relying on it." The page links the GitHub repository and `/privacy`.
- **The privacy policy** says, in plain words, in this order:
  1. Who publishes it: the data controller's name and country, and the contact address, from the header.
  2. Farik runs on the user's computer. Its makers receive nothing from it: no telemetry, no analytics, no crash reports.
  3. Connector keys and sign-ins stay in the user's own keychain.
  4. What Farik's agents read from a connected service (for Slack: channel names, messages in the channels the user picks, and user names) is sent only to the AI model provider the user configured, under the user's own account and that provider's terms. Neither Farik nor its makers use it to train any AI model.
  5. The sign-in relay at `signin.<domain>` sees a service's tokens in transit during a sign-in or a refresh, and keeps none. It logs one line per request (time, route, service, outcome, the service's status, duration) and holds no IP address, code or token. Lines are deleted after 30 days.
  6. The relay's firewall counts requests per IP address for 5 minutes to block abuse, and records none.
  7. The site and the relay run on Amazon Web Services, which processes visitors' IP addresses to deliver them. The site sets no cookie, runs no analytics and keeps no access log.
  8. To access, export or delete data: everything Farik holds is on the user's computer, where the user can delete it. Disconnecting Farik in Slack's settings revokes its access. For anything else, write to the contact address, which answers within 30 days.
  9. Changes to the policy are dated on the page.
  10. The effective date.

  The founder approves the wording at Task 1's gate; it is a policy the founder publishes, not legal advice.
- **The stack, `SiteStack` (`FarikSite`, `us-east-1`)**, in `infra/lib/site-stack.ts`, from the same environment inputs as `RelayStack` (`FARIK_DOMAIN`, `FARIK_HOSTED_ZONE_ID`, `CDK_DEFAULT_ACCOUNT`):
  - a private S3 bucket: all public access blocked, versioned, `enforceSSL`, `RemovalPolicy.RETAIN`, no server access logging;
  - a CloudFront distribution with the bucket as an origin through origin access control (`S3BucketOrigin.withOriginAccessControl`), the aliases `<domain>` and `www.<domain>`, an ACM certificate for both, validated through the hosted zone, `minimumProtocolVersion` `TLSv1.2_2021`, viewer protocol redirect to HTTPS, `defaultRootObject` `index.html`, price class 100, no access logging of any kind (no standard logs, no log delivery, no real-time logs);
  - a response headers policy: HSTS `max-age=31536000`, `Content-Security-Policy: default-src 'none'; style-src 'self'; img-src 'self'; font-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, `X-Frame-Options: DENY`;
  - a CloudFront Function (`cloudfront-js-2.0`, viewer request) in `infra/site/edge.js`: a request whose `host` header begins with `www.` is a 301 to `https://` + the host without `www.` + the same URI, with the query string dropped; `/privacy` is served from `/privacy.html`; any other request is passed on. The source holds no domain, so the stack loads it with `FunctionCode.fromFile` unchanged;
  - A and AAAA alias records for `<domain>` and `www.<domain>`;
  - a `BucketDeployment` from `Source.asset(<infra/site>, { exclude: ['edge.js'] })`, so the bucket holds `index.html`, `privacy.html`, `site.css` and `fonts/`, invalidating `/*` on the distribution.
- **Deployment** is step 03c's `infra` workflow, which deploys every stack (`cdk deploy --all`) through the same OpenID Connect role; nothing new is created for it. The first deploy of `FarikSite` is the founder's, from the founder's machine, as for the relay.
- **Left as they are:** unknown paths, `/privacy/` and `/favicon.ico` get S3's 403 through CloudFront, which is harmless; a custom 404 page is phase 11's. HSTS has no `includeSubDomains` (`signin.` sets its own); phase 11 adds `includeSubDomains; preload` once `downloads.` exists. Google's verification, deferred until after the launch, will need the policy to add which Google data is accessed, used, shared, protected and kept, and Google's Limited Use and AI-training statement, when its route is planned; the apex URL is the one registered, so the `www` redirect does not matter to it.

## File map

```
docs/design/mockups/{Homepage,Privacy}.dc.html, canvas.json             creates: two new boards; Site.dc.html unchanged (Task 1)
infra/site/{index.html,privacy.html,site.css,edge.js}                   creates: the pages and the edge function (Task 2)
infra/site/fonts/*.woff2, infra/site/fonts/OFL.txt                      creates: the self-hosted fonts (Task 2)
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

Founder's action before Task 1 (the executor stops until it is done):
- [ ] **The controller and the contact**: the company's legal name and its country, and the contact address on `<domain>`, written into this plan's `Controller:` and `Contact:` lines.

### Task 1: The two pages, mocked up

On the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf), desktop and phone width, the canvas's tokens and the self-hosted fonts, muted and light, copied to `docs/design/mockups/`: a new `Homepage` board, copied from the `Site` board's hero and open-source section, with the Decisions' three sections and the AI-accuracy sentence; and a new `Privacy` board with the policy's ten items in full, using the header's controller and contact. `Site.dc.html` is not edited.

Gate: the founder approves both boards and the policy's wording. The approval's date goes into this plan's `Mockups approved by:` line. Task 2 waits for it.

- [ ] `docs(design): mock up the homepage and the privacy policy`

### Task 2: The pages

Files: `infra/site/*`, `infra/test/site-pages.test.ts`. The tests read the files from disk; the function's tests evaluate `edge.js` with `new Function(source + '; return handler;')()`, since a CloudFront Function cannot `export`. If Biome's `noUnusedVariables` flags `edge.js`'s top-level `handler`, which CloudFront calls but nothing in the module does, suppress it with one `biome-ignore` comment that gives that reason; do not exclude the file from Biome.

- `pages_are_static_and_link_the_policy`: neither page has a `<script`, a `style=` attribute, an inline `<style>`, or an external URL other than the GitHub repository and `mailto:`; neither holds `<domain>`, `pending`, `TODO` or `example.`; `index.html` links `/privacy`, and both link `site.css`.
- `the_homepage_says_what_slack_needs`: `index.html` holds the approved "Farik in Slack", "Connecting Slack" and "Help" sentences, the AI-accuracy sentence, the four scope names, the contact address, an element with id `help`, and a link to `/privacy`.
- `the_policy_says_what_farik_keeps`: `privacy.html` holds each of the ten items' approved sentences, verbatim from the approved `Privacy` board, among them the controller's name and country and the contact address from the header.
- `fonts_are_self_hosted`: `site.css` declares the three families by `@font-face` with `url(fonts/…woff2)`, each file exists, and `fonts/OFL.txt` exists.
- `edge_redirects_www`: a request with host `www.example.test` and URI `/privacy` gives a 301 with `location` `https://example.test/privacy`.
- `edge_serves_clean_urls`: `/privacy` on `example.test` is passed on with URI `/privacy.html`; `/` and `/site.css` are passed on unchanged.

- [ ] `feat(infra): a homepage and a privacy policy`

### Task 3: The site's stack

Files: `site-stack.ts`, `bin/farik.ts`, `site-stack.test.ts`. Tests (the CDK's `Template` assertions, step 03c's made-up inputs):

- `serves_the_site_only_through_cloudfront`: the bucket blocks all public access, is versioned and retained, has no `LoggingConfiguration`, and its policy denies non-TLS requests and allows `s3:GetObject` only to the CloudFront service principal with `AWS:SourceArn` the distribution; one origin access control exists.
- `answers_over_https_with_its_headers`: the distribution has the aliases `example.test` and `www.example.test`, `MinimumProtocolVersion: TLSv1.2_2021`, `ViewerProtocolPolicy: redirect-to-https`, `DefaultRootObject: index.html`, no `Logging`, the viewer-request function, and a response headers policy with the Decisions' HSTS, CSP (with `font-src 'self'`), `nosniff`, referrer and frame values; no `AWS::Logs::DeliverySource`, `AWS::Logs::Delivery` or `AWS::CloudFront::RealtimeLogConfig` exists in the template.
- `names_both_hosts`: A and AAAA alias records for `example.test` and `www.example.test`, and a DNS-validated certificate covering both.
- `publishes_only_the_pages`: the `Custom::CDKBucketDeployment` has `DistributionPaths: ['/*']` and the distribution's id.

- [ ] `feat(infra): serve the homepage from CloudFront`

### Task 4: The pages, live

Gate: step 03c's live check has passed, and the founder has run the first deploy: `pnpm --filter @farik/infra exec cdk deploy FarikSite`, from the founder's machine with the inputs set.

Files: `docs/SPEC.md` (8.6: the homepage and privacy policy, and what they promise); `docs/plans/project-plan.md` (row 03e, corrected if execution changed it).

- [ ] `docs(spec): record the homepage and the privacy policy`

Founder's actions (no agent holds AWS credentials):
- [ ] **The mailbox** for the contact address: choose mail hosting (Amazon WorkMail recommended), add its MX, SPF, DKIM and DMARC records in the Route 53 hosted zone by hand (the Decisions say why not in a stack), and confirm a test message from an outside address arrives. Done before the live check.
- [ ] **The first deploy** of `FarikSite`, as the Gate says.
- [ ] **The GitHub App's homepage** (step 03b) set to `https://<domain>/`.
- [ ] **Slack's app** (step 03c): set the privacy policy URL to `https://<domain>/privacy`, the landing page to `https://<domain>/`, and the support URL to `https://<domain>/#help`. That makes it ready for the Marketplace submission in phase 11, whose Security & Compliance section names the model provider, its retention and that Slack data trains no model, as the policy says.

The pull request lists the three fonts' licences.

## Verification

```
cargo xtask check
# expected: xtask check: ok
FARIK_DOMAIN=example.test FARIK_HOSTED_ZONE_ID=Z000000000000 FARIK_ALERT_EMAIL=a@example.test \
  FARIK_MONTHLY_BUDGET_USD=20 CDK_DEFAULT_ACCOUNT=123456789012 \
  pnpm --filter @farik/infra exec cdk synth FarikSite --quiet
# expected: exit 0
```

The founder's live check, recorded in the pull request:
- `curl -sI https://<domain>/` answers 200 with the HSTS and CSP headers, `x-content-type-options: nosniff`, `referrer-policy: no-referrer` and `x-frame-options: DENY`;
- `curl -sI https://<domain>/site.css` answers 200 with `content-type: text/css` (`nosniff` drops a stylesheet served with any other type);
- `curl -sI https://<domain>/edge.js` answers 403;
- `curl -sI https://www.<domain>/privacy` answers 301 to `https://<domain>/privacy`, and `curl -sI https://<domain>/privacy` answers 200;
- a message to the contact address arrives;
- both pages read correctly on a phone, in the brand's fonts.
