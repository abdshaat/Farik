# 0017. AWS for everything Farik hosts

Date: 2026-09-26
Status: accepted

## Context

On 2026-09-26 the founder decided that the project uses the AWS stack. That covers getting the domain and deploying Farik on the web. Until now no document named a cloud: the product is local-first (spec 8.1), and hosted execution is phase 10's premium tier (spec 9).

Farik will host two things:
- **The public website, from now on.** It lives on the project's domain: what Farik is, how to get it, the docs, and links to the downloads. It is a static site in the brand (`docs/brand/brand.md`) with no server code and no user data.
- **The hosted tier, in phase 10.** The daemon, the governor and agent execution run in the cloud for users who do not want to run Farik themselves. The same web app talks to it over the phase 6 RPC protocol.

The working web app (`apps/web`) is not hosted on the domain. The local daemon serves it on 127.0.0.1 (spec 8.1). The browser gets the daemon's token through a one-time link, and the daemon refuses any other origin (spec 8.6). Serving the app from a public domain to a daemon on `localhost` would break that origin check. It would also depend on browsers letting a public page reach a private address, which they increasingly block. So "Farik on the web" means the website now and the hosted tier later. It does not mean the local app served from the cloud.

The options were these:
- **AWS**, the founder's choice. It covers the domain registrar, DNS, certificates, the content delivery network, storage and, later, containers, identity and secrets, all in one account under one bill.
- **A static host such as GitHub Pages, Netlify or Cloudflare Pages.** This is simpler for the site alone. But the hosted tier would then need a second provider, and the founder chose one stack.

## Decision

Everything Farik hosts runs on AWS, in the `us-east-1` region. (CloudFront only accepts certificates from that region, and one region keeps the account simple.) The infrastructure is code, written with the AWS CDK in TypeScript as the workspace package `infra` (`@farik/infra`). GitHub Actions deploys through an OpenID Connect role, so no long-lived AWS key exists anywhere.

The services, by purpose:

| Purpose | Service | When |
|---|---|---|
| Register the domain | Amazon Route 53 Domains | Now |
| DNS for the domain | Amazon Route 53 hosted zone | Now |
| HTTPS certificate | AWS Certificate Manager (in `us-east-1`, validated by DNS) | Now |
| Store the built website | Amazon S3 (a private bucket, versioned) | Now |
| Serve the website over HTTPS worldwide | Amazon CloudFront (origin access control to S3, a response-headers policy for HSTS and CSP, and a CloudFront Function for the `www` redirect and clean URLs) | Now |
| Deploy from CI without stored keys | AWS IAM (the GitHub OpenID Connect identity provider, and a deploy role scoped to the bucket and the distribution) | Now |
| People's access to the account | AWS IAM Identity Center (with MFA; the root user is locked away) | Now |
| Infrastructure as code | AWS CDK, deploying through AWS CloudFormation | Now |
| Cost guard | AWS Budgets (a monthly budget with email alerts) | Now |
| Monitoring and audit | Amazon CloudWatch (alarms on CloudFront errors) and AWS CloudTrail (account activity) | Now |
| Release downloads mirror | S3 and CloudFront at `downloads.` on the domain, beside the GitHub release (phase 8) | Phase 8 |
| Hosted tier | Candidates, decided when phase 10 is planned: Amazon ECS (on AWS Fargate, or on EC2 if the sandbox needs Docker on the host, spec 8.3), an Application Load Balancer for the WebSocket, Amazon Cognito for sign-in, AWS Secrets Manager and AWS KMS for model keys and MCP credentials (spec 8.6's vault), Amazon RDS or Amazon EFS for projects and the event log, Amazon ECR for images, and AWS Organizations to separate production from staging | Phase 10 |

The step-by-step deployment plan is kept out of git on purpose (the founder's request). It lives at `deploy/plan.md`, and `.gitignore` excludes `deploy/`. It holds the account-specific details: the domain, the account, the budget, and the order of the steps.

## Consequences

- One provider covers the site now and the hosted tier later. The domain, certificates and DNS are already where the hosted tier will need them.
- The CDK in TypeScript uses the toolchain the front end already has (ADR 0002): pnpm, TypeScript, Biome and Vitest. So `infra` is checked by `pnpm check` like any other package, and its stacks are unit-tested with the CDK's assertions.
- The website costs little while it is static: a hosted zone is about $0.50 a month, the domain is a yearly fee that depends on its ending, and S3 and CloudFront stay near zero at launch traffic. The budget alarm catches mistakes.
- The downside is that AWS is heavier to set up than a static host. The founder has to create the account, secure it and pay for the domain. The CDK adds a dependency set to the workspace. And a single region means a regional outage takes down deploys, though CloudFront keeps serving the site.
- The local-first product is unchanged. Nothing in section 5 depends on AWS, and a user never needs an AWS account to run Farik.
