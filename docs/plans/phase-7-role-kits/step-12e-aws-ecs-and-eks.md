# Phase 7, step 12e: AWS ECS and EKS

Status: draft. Its readiness review runs once step 12d has landed.
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 6.7, 6.9, 8.2, 8.6; F9
Depends on: step 12d (ADR 0046, `platforms/k8s_deployment.rs`); step 12c (`Production::image_for`); step 12 (`call_tool`, `ADAPTERS`); step 10d (the first `uvx` package in a kit, `uv`'s first run); phase 6 (merged in #19)
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from row 12 (see step 12's header). It closes row 12.

## Goal

A team whose service runs on Amazon ECS, or on Amazon EKS, connects it to the DevOps Engineer with an AWS key made for that one service, and the region. The agent reads through AWS's own servers: on ECS the service's events, task failures and task logs; on EKS the namespace's resources, pod logs, events, and CloudWatch logs and metrics. Farik moves the service to the image of the integrated commit, forces a new deployment to restart it, and returns it to the last healthy task definition (ECS) or image (EKS) through the same servers, whose writes the agent is never offered. AWS's ECS server turns its writes and its logs on by environment variables, so a kit's server may now carry fixed settings (ADR 0046's `env`). Out of scope: AWS's hosted servers, which sign in with AWS's own request signing, which Farik's connectors do not do.

## Decisions

- **The servers** (ADR 0020: the official ones), checked 2026-10-05 against the packages' own source, since AWS's documentation page (awslabs.github.io/mcp/servers/ecs-mcp-server) lists ten tools and the package more:
  - **ECS:** `uvx awslabs.ecs-mcp-server==0.1.36` (PyPI, 2026-09-22). Its source names `containerize_app`, `build_and_push_image_to_ecr`, `validate_ecs_express_mode_prerequisites`, `wait_for_service_ready`, `delete_app`, `create_ecs_infrastructure`, `delete_ecs_infrastructure`, `get_deployment_status`, `ecs_troubleshooting_tool` (one `action` among `get_ecs_troubleshooting_guidance`, `fetch_cloudformation_status`, `fetch_service_events`, `fetch_task_failures`, `fetch_task_logs` and more), `ecs_resource_management` (`api_operation` in CamelCase and `api_params`, reads and writes in one tool), and the three `aws_knowledge_aws___search_documentation`, `aws_knowledge_aws___read_documentation`, `aws_knowledge_aws___recommend`. Writes need `ALLOW_WRITE=true` and logs `ALLOW_SENSITIVE_DATA=true`, read from the environment.
  - **EKS:** `uvx awslabs.eks-mcp-server==0.2.1 --allow-write --allow-sensitive-data-access` (PyPI, 2026-09-08). Its sixteen tools are those of its documentation page: `add_inline_policy`, `apply_yaml`, `generate_app_manifest`, `get_cloudwatch_logs`, `get_cloudwatch_metrics`, `get_eks_insights`, `get_eks_metrics_guidance`, `get_eks_vpc_config`, `get_k8s_events`, `get_pod_logs`, `get_policies_for_role`, `list_api_versions`, `list_k8s_resources`, `manage_eks_stacks`, `manage_k8s_resource` (`operation` `create`, `replace`, `patch`, `delete` or `read`, `cluster_name`, `kind`, `api_version`, `name`, `namespace`, `body`), `search_eks_troubleshoot_guide`. The flags follow the pinned package, which ADR 0036 allows.
  Both read the key from `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` and the region from `AWS_REGION`, given as three credential keys, the region being the user's to say. Rejected: AWS's hosted ECS and EKS servers, reached through `mcp-proxy-for-aws`, which signs each request with the key (a second program to run, and the same key).
- **Fixed settings** (ADR 0046, decided in step 12d). A kit connector's new field `env`, a map of variable to value, given to a `stdio` server beside its keys at every listing, call and launch. Names match `^[A-Z][A-Z0-9_]{0,63}$`, are not a credential key, not one of `KEPT_ENV`, and not one that changes which program runs or where it loads from (`LD_*`, `DYLD_*`, `NODE_OPTIONS`, `NPM_CONFIG_*`, `UV_*`, `PIP_*`, `PYTHON*`), each `env_not_allowed`; values are at most 200 characters with no line break; `stdio` only (`env_not_stdio`); in `spec_sha256` when present. A team file cannot add one: a kit entry matches its kit whole (ADR 0036). The ECS entry carries `env: { ALLOW_WRITE: "true", ALLOW_SENSITIVE_DATA: "true", FASTMCP_LOG_LEVEL: "ERROR" }`. Rejected: asking the user to paste "true" as a key.
- **Writes on, for Farik.** With writes on, the server would carry out a write tool's call; the agent is never offered one (they are `denied`, and the hook refuses them), Farik's own adapter calls them, and the key's policy allows only this service's actions, which is the real bound. `ecs_resource_management` mixes reads and writes in one tool, so it is `denied` to the agent, who reads the service through `ecs_troubleshooting_tool`, `get_deployment_status` and `wait_for_service_ready`.
- **What each tag is.** ECS `network`: `ecs_troubleshooting_tool`, `get_deployment_status`, `wait_for_service_ready`, the three `aws_knowledge_aws___*` (6). ECS `denied`: `ecs_resource_management`, `containerize_app`, `build_and_push_image_to_ecr`, `validate_ecs_express_mode_prerequisites`, `delete_app`, `create_ecs_infrastructure`, `delete_ecs_infrastructure` (7). EKS `network`: `list_k8s_resources`, `get_pod_logs`, `get_k8s_events`, `get_cloudwatch_logs`, `get_cloudwatch_metrics`, `get_eks_metrics_guidance`, `get_eks_insights`, `get_eks_vpc_config`, `list_api_versions`, `search_eks_troubleshoot_guide` (10). EKS `denied`: `manage_k8s_resource` (Farik's, and its `read` shares the tool), `apply_yaml`, `generate_app_manifest`, `manage_eks_stacks`, `add_inline_policy`, `get_policies_for_role` (6). A named tool the live listing lacks is removed only if the package's source of that version lacks it too; anything else listed is `denied` unlabelled.
- **ECS's adapter** (`platforms/aws_ecs.rs`), each call `ecs_resource_management { api_operation, api_params }`. `production.service` is `<cluster>/<service>`, or `<cluster>/<service>/<container>` when the task definition has more than one container; `production.image` is required. `live` and `deployments`: `DescribeServices { cluster, services: [service] }`; each of its `deployments` is a `Deployment` whose `id` is its task definition's ARN and whose `version` is the commit whose `image_for` its container's image is (read once per task definition with `DescribeTaskDefinition`), else the image; `rolloutState` `COMPLETED` on the `PRIMARY` one is `Live`, `FAILED` `Failed`, `IN_PROGRESS` `Building`, an `ACTIVE` one `Superseded`. `deploy(commit)`: `DescribeTaskDefinition` of the live one, `RegisterTaskDefinition` with its fields and the container's image set to `image_for(commit)`, then `UpdateService { taskDefinition: <new ARN> }`. `restart`: `UpdateService { forceNewDeployment: true }`. `roll_back(to)`: `UpdateService { taskDefinition: to.id }`, the last healthy task definition. `error_rate`: `None`.
- **EKS's adapter** (`platforms/aws_eks.rs`): `production.service` is `<cluster>/<namespace>/<deployment>[/<container>]`; `manage_k8s_resource { operation: read | patch, cluster_name, kind: "Deployment", api_version: "apps/v1", name, namespace, body? }`, with step 12d's `rollout_state`, `with_image` and `with_restart` building the `patch` body (the container's image; the `restartedAt` annotation). `roll_back(to)` sets the last healthy image. `error_rate`: `None`.
- **The narrow key**, as ADR 0027 asks: on ECS, a user whose policy allows, for this cluster and service, `ecs:DescribeServices`, `ecs:DescribeTaskDefinition`, `ecs:RegisterTaskDefinition`, `ecs:UpdateService`, `ecs:ListTasks`, `ecs:DescribeTasks` and `ecs:DescribeServiceDeployments`, `iam:PassRole` on the service's task roles, and reading its CloudWatch log group; on EKS, a user that may `eks:DescribeCluster` this cluster and read its CloudWatch logs, with an access entry bound to a Kubernetes role limited to one namespace (step 12d's verbs). `RegisterTaskDefinition` cannot be limited to one service in IAM, which the setup copy says plainly.
- **`uv`** (step 10d's decision): the setup's first sentence names it; the first `uvx` run may outlast the 30-second listing, after which the user connects again; the live run records both listing times.
- **The copy.** ECS: title "AWS ECS"; about "Amazon ECS runs your app's containers as services on AWS."; why "So the DevOps Engineer can read your service's events, failed tasks and logs, and Farik can move it to the image of your planned work, restart it, or return it to the last healthy version when a planned deploy or an incident calls for it. The agent itself only reads."; setup "This needs the free program uv on your computer (docs.astral.sh/uv). In AWS, make a user for Farik whose policy allows, on this cluster and service only: describe the service and its task definitions, register a task definition, update the service, list and describe its tasks, pass the service's task roles, and read its CloudWatch logs. Paste its ‘Access key’, ‘Secret access key’ and your region here. In Farik's Settings, the service name is cluster/service." EKS: title "AWS EKS"; about "Amazon EKS runs Kubernetes clusters on AWS."; why "So the DevOps Engineer can read your deployment's pods, logs, events and metrics, and Farik can move it to the image of your planned work, restart it, or move it back when a planned deploy or an incident calls for it. The agent itself only reads."; setup "This needs the free program uv on your computer (docs.astral.sh/uv). In AWS, make a user for Farik that may describe this cluster and read its CloudWatch logs, and give it an access entry on the cluster limited to one namespace: get, list and watch pods, their logs, events and deployments, and patch this one deployment. Paste its ‘Access key’, ‘Secret access key’ and your region here. In Farik's Settings, the service name is cluster/namespace/deployment." `key_page` for both: `https://console.aws.amazon.com/iam/home#/users`.

## File map

```
docs/schemas/kit.schema.json, docs/schemas/team.schema.json  modifies: env (Task 1)
crates/roles/src/kit.rs, crates/core/src/team.rs             modifies: the loader's refusals, CustomServer.env, spec_sha256 (Task 1)
crates/runtime/src/connectors.rs, crates/runtime/src/daemon.rs, crates/cli/src/connector_run.rs   modifies: env at list, call and launch (Task 2)
crates/roles/roles/devops_engineer/kit.yaml                  modifies: aws-ecs, aws-eks (Tasks 3, 5)
crates/runtime/src/platforms/aws_ecs.rs, platforms/aws_eks.rs, platforms.rs   creates/modifies (Tasks 4, 6)
crates/runtime/src/daemon/team.rs, crates/runtime/tests/live_kit_pins.rs   tests, header (Task 7)
docs/SPEC.md, docs/design/role-kits.md, docs/design/devops-engineer.md, docs/plans/project-plan.md   modifies (Task 8)
```

## Interfaces

Consumes: ADR 0046; `CustomServer`, `spec_sha256`, `KEPT_ENV`, `list_tools`, `call_tool`, `launch_spec`, `KeyFile` (12d); `k8s_deployment::{DeploymentObject, rollout_state, with_image, with_restart}` (12d); `Production::image_for` (12c); `ADAPTERS`, `Connection` (12).

Produces:

```rust
pub env: BTreeMap<String, String>                 // CustomServer, farik_core::team; in spec_sha256 when not empty
pub fn platform(connection: Connection, production: &Production) -> Result<Arc<dyn Platform>, PlatformError>;   // platforms::aws_ecs
pub fn platform(connection: Connection, production: &Production) -> Result<Arc<dyn Platform>, PlatformError>;   // platforms::aws_eks
// ADAPTERS gains ("aws-ecs", aws_ecs::platform) and ("aws-eks", aws_eks::platform)
```

## Tasks

### Task 1: `env` in the format

- `a_kit_may_fix_a_servers_settings`: parses, and `custom_server` carries it. RED.
- `refuses_a_setting_that_changes_what_runs`: `LD_PRELOAD`, `NODE_OPTIONS`, `UV_INDEX_URL`, `PYTHONPATH`, `PATH`, a credential key's name, a value with a line break, and `env` on an `http` connector, each with its code. RED.
- `env_changes_the_hash_only_when_present`; `a_team_file_cannot_add_a_setting` (`matches_kit` false). RED each.

- [ ] `feat(roles): let a kit fix a server's settings`

### Task 2: Giving the settings

- `a_server_gets_its_fixed_settings`: a `sh` fixture prints its environment at listing, at a call and through the launcher: the settings, its keys and `KEPT_ENV`, nothing else. RED.

- [ ] `feat(runtime): give a kit's server its fixed settings`

### Task 3: ECS in the kit

`aws-ecs`: `stdio`, `command: uvx`, `args: ["awslabs.ecs-mcp-server==0.1.36"]`, the `env` above, `credential_keys: [AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION]`. Labels: `ecs_troubleshooting_tool` "read the service's events, failures and logs", `get_deployment_status` "read a deployment's state", `wait_for_service_ready` "wait for the service to run", `aws_knowledge_aws___search_documentation` "search AWS's documentation", `aws_knowledge_aws___read_documentation` "read AWS's documentation", `aws_knowledge_aws___recommend` "find related AWS documentation".

- `ecs_only_reads_for_the_agent`: the entry exactly; the 6 `network`, labelled; the 7 `denied`, among them `ecs_resource_management`; the copy exactly. RED.

- [ ] `feat(roles): give the DevOps Engineer AWS ECS`

### Task 4: ECS's adapter

Against a fixture MCP server answering `ecs_resource_management` with recorded ECS shapes.

- `deploys_a_new_task_definition_with_the_commits_image`: `DescribeTaskDefinition`, `RegisterTaskDefinition` with the image of `image_for`, `UpdateService` with the new ARN, in that order. RED.
- `restart_forces_a_new_deployment` and `roll_back_returns_the_healthy_task_definition`. RED each.
- `reads_ecs_deployments`: the four states as decided, and `version` read back from the image. RED.
- `refuses_two_containers_without_a_name`. RED.

- [ ] `feat(runtime): deploy, restart and roll back an ECS service`

### Task 5: EKS in the kit

`aws-eks`: `stdio`, `command: uvx`, `args: ["awslabs.eks-mcp-server==0.2.1", "--allow-write", "--allow-sensitive-data-access"]`, the three keys. Labels: `list_k8s_resources` "list resources", `get_pod_logs` "read a pod's logs", `get_k8s_events` "read events", `get_cloudwatch_logs` "read logs", `get_cloudwatch_metrics` "read metrics", `get_eks_metrics_guidance` "read which metrics exist", `get_eks_insights` "read the cluster's warnings", `get_eks_vpc_config` "read the cluster's network", `list_api_versions` "list the cluster's interfaces", `search_eks_troubleshoot_guide` "search the troubleshooting guide".

- `eks_only_reads_for_the_agent`: the entry exactly; the 10 `network`, labelled; the 6 `denied`, among them `manage_k8s_resource` and `add_inline_policy`; the copy exactly. RED.

- [ ] `feat(roles): give the DevOps Engineer AWS EKS`

### Task 6: EKS's adapter

- `patches_the_deployment_through_manage_k8s_resource`: `read`, then `patch` with the image of `image_for`; the restart's annotation; the healthy image on rollback. RED.

- [ ] `feat(runtime): deploy, restart and roll back an EKS deployment`

### Task 7: Connected by name

- `connects_each_devops_service_by_name` gains `aws-ecs` and `aws-eks`, and the kit's connectors are exactly `vercel`, `render`, `netlify`, `railway`, `fly`, `kubernetes`, `aws-ecs`, `aws-eks`, none `external_effect` and none with an allowance. Guard.
- `live_kit_pins.rs`'s header names both and their variables (`FARIK_KIT_AWS_ECS_AWS_ACCESS_KEY_ID`, `…_AWS_SECRET_ACCESS_KEY`, `…_AWS_REGION`, and the same for `AWS_EKS`).

- [ ] `test(runtime): connect the DevOps Engineer's AWS services by name`

### Task 8: Spec and plan

`docs/SPEC.md` 6.7 (`env`), 8.2 (the launcher gives it), 6.9's kit paragraph (ECS and EKS, writes on for Farik alone, the policy's limit on `RegisterTaskDefinition`); the revision line. `docs/design/role-kits.md` and `docs/design/devops-engineer.md` (the table's AWS row as built). Project plan row 12e, and row 12 marked done when 12 to 12e are.

- [ ] `docs(spec): record AWS ECS and EKS in the DevOps Engineer's kit`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
FARIK_LIVE_TESTS=1 cargo test -p farik-runtime --test live_kit_pins
# expected: ok, AWS ECS and AWS EKS listed with no drift; the first and second uvx listing times in the Execution notes
```

Then, by the founder, on a test ECS service and a test EKS cluster with the narrow users: one planned deploy each, then one broken deploy each restored.

## Execution notes

None yet.
