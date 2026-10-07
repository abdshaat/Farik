# Phase 7, step 12d: Kubernetes, and a key kept as a file

Status: draft. Its readiness review runs once step 12c has landed.
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 6.7, 6.9, 8.2, 8.6; F9
Depends on: step 12c (`Production.image`, `image_for`); step 12 (`call_tool`, `ADAPTERS`, `Connection`); step 11 (the approved `ConnectorFileKey` board); step 01 (the launcher, `LaunchSpec`, `POST /connector/launch`, `farik connect`); steps 05 and 05b (the kit format, `spec_sha256`, `ConnectorAdd`, `KitConnect`); phase 6 (merged in #19)
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from row 12 (see step 12's header).

## Goal

A team whose service runs on any Kubernetes cluster connects it to the DevOps Engineer by pasting, or choosing, a sign-in file for a service account limited to one namespace. The agent reads the namespace's deployments, pods, their logs and events through a pinned community server, the official choice being none; Farik moves the deployment to the image of the integrated commit, restarts its rollout, and moves it back to the last healthy image through the same server. A kit's connector may now name a key that is a whole file: Farik keeps it in the key store like any key, writes it into the server's own folder when the server starts, and gives the server its path. Out of scope: AWS's EKS, which signs in through AWS (12e).

## Decisions

- **The server** (ADR 0020's second choice), checked 2026-10-05: Kubernetes publishes no MCP server of its own. The maintained community one is `containers/kubernetes-mcp-server` (github.com/containers/kubernetes-mcp-server; npm `kubernetes-mcp-server`, 0.0.67, published 2026-09-18), Go, run by `npx`, reading the standard `KUBECONFIG` variable. Rejected: `mcp-server-kubernetes` (4.1.9), which runs `kubectl` and needs it installed; a server of Farik's own (the community one already reads and patches what Farik needs). Entry: `transport: stdio`, `command: npx`, `args: ["kubernetes-mcp-server@0.0.67"]`, `credential_keys: [KUBECONFIG]`, `file_keys: [KUBECONFIG]`. Its own `read_only` setting stays off, since Farik patches through it; the agent is held by the tags, and the cluster by the account's role.
- **A key kept as a file** (ADR 0046, Task 1). A kit connector's new field `file_keys` names credential keys whose value is a file's text. For each, at every listing (`list_tools`), call (`call_tool`) and launch (`POST /connector/launch` and `farik connector run`), the value is written to `<the server's folder>/<key in lower case>`, 0600, in the 0700 folder made fresh for that start (ADR 0030), and the variable is set to that file's path, not the text. A file key's value is at most 64 KiB of UTF-8 with no NUL (`file_key_too_large`, `file_key_not_text`), checked where keys are read (the web form, `farik connect`) before anything is kept. `file_keys` must be a subset of `credential_keys` (`file_key_unknown`), is `stdio` only (`file_key_not_stdio`), and is in `spec_sha256` when present, so every hash kept before stands (ADR 0036's rule for new fields). A store that refuses so large a value (Windows' credential manager keeps at most 2,560 bytes) fails the connect with the store's own sentence; the user then uses a file without an embedded certificate bundle, as the setup copy says. Rejected: a path to the file as the key (the file would sit outside the key store, readable by anything the user runs); the server's own `--kubeconfig` flag (one path, in `args`, for every user).
- **The screens** (the approved `ConnectorFileKey`): `ConnectorAdd`'s key form shows a file key as "Choose the file" and a box to paste it into, never a one-line field, which would drop its line breaks. `farik connect` takes `--key-file <KEY>=<path>`, which reads the file, for a file key, and refuses `--key-file` for any other.
- **What each tag is**, from the README (read 2026-10-05). `network`: `namespaces_list`, `events_list`, `pods_list_in_namespace`, `pods_get`, `pods_log`, `pods_top`, `resources_list`, `resources_get` (8). `denied`: `pods_exec` and `pods_run` (a shell or a pod in production, which spec 6.9 forbids), `pods_delete`, `resources_create_or_update` (Farik's), `resources_delete`, `resources_scale` (scaling is the human's), `pods_list` (every namespace), `nodes_log`, `nodes_stats_summary`, `nodes_top`, `projects_list`, `configuration_contexts_list`, `targets_list`, `configuration_view` (it shows the sign-in file) (14). Any other tool of the live listing (`helm_*` and the other toolsets when on) is `denied` unlabelled.
- **The adapter** (`platforms/kubernetes.rs`), through `call_tool`. `production.service` is `<namespace>/<deployment>`, or `<namespace>/<deployment>/<container>` when the pod has more than one container; `production.image` is required. Shared with step 12e's EKS in `platforms/k8s_deployment.rs`, pure functions over the deployment object:
  - `live`: `resources_get { apiVersion: "apps/v1", kind: "Deployment", namespace, name }`; `Live` when `status.observedGeneration` equals `metadata.generation` and `updatedReplicas`, `availableReplicas` and `replicas` are equal; `Failed` when its `Progressing` condition is `False` with `ProgressDeadlineExceeded`; else `Building`; its `id` the container's image and its `version` the commit whose `image_for` it is, else the image;
  - `deployments`: the live one alone, since Farik's own records hold the history;
  - `deploy(commit)`: the object read, its container's image set to `image_for(commit)`, sent with `resources_create_or_update` keeping `metadata.resourceVersion`, so a change made since the read is refused rather than overwritten;
  - `restart`: the same with `spec.template.metadata.annotations["kubectl.kubernetes.io/restartedAt"]` set to now, as `kubectl rollout restart` does;
  - `roll_back(to)`: the image set to `to.id`, the last image Farik recorded healthy, which is the design's "undo to the healthy revision" by image rather than by revision number (revision numbers change with every restart);
  - `error_rate`: `None`.
  The live run confirms `resources_create_or_update`'s input shape against 0.0.67; a field it refuses stops the run and the planner decides.
- **The narrow credential**, as ADR 0027 asks: a service account in the one namespace, with a role allowing `get`, `list` and `watch` on pods, `pods/log`, events, replica sets and deployments, and `patch` and `update` on the one deployment by name; the setup copy says so in plain words for whoever runs the cluster.
- **The copy.** Title "Kubernetes"; about "Kubernetes runs your app's containers on a cluster of machines."; why "So the DevOps Engineer can read your deployment, its pods, logs and events, and Farik can move it to the image of your planned work, restart it, or move it back when a planned deploy or an incident calls for it. The agent itself only reads."; setup "This needs Node.js on your computer. Ask whoever runs your cluster for a sign-in file (a kubeconfig) for a service account limited to one namespace: allowed to get, list and watch pods, their logs, events, replica sets and deployments, and to patch and update this one deployment. Choose or paste the file here. In Farik's Settings, the service name is namespace/deployment, and the image name is like registry.example.com/app:{commit}." `key_page: https://kubernetes.io/docs/concepts/configuration/organize-cluster-access-kubeconfig/`, since a kit that takes keys says where they come from (`key_page_missing`).

## File map

```
docs/decisions/0046-fixed-settings-and-file-keys-in-a-kit-server.md   creates (Task 1)
docs/schemas/kit.schema.json, docs/schemas/team.schema.json  modifies: file_keys (Task 2)
crates/roles/src/kit.rs, crates/core/src/team.rs             modifies: the loader's refusals, CustomServer.file_keys, spec_sha256 (Task 2)
crates/runtime/src/connectors.rs, crates/runtime/src/daemon.rs, crates/cli/src/connector_run.rs   modifies: writing the file at list, call and launch (Task 3)
crates/cli/src/connector.rs, apps/web/src/pages/ConnectorAdd.tsx, KitConnect.tsx (+tests)   modifies: the file key's field (Task 4)
crates/roles/roles/devops_engineer/kit.yaml, crates/roles/src/kit.rs   modifies: kubernetes (Task 5)
crates/runtime/src/platforms/k8s_deployment.rs, platforms/kubernetes.rs, platforms.rs   creates/modifies (Task 6)
crates/runtime/src/daemon/team.rs, crates/runtime/tests/live_kit_pins.rs   tests, header (Task 7)
docs/SPEC.md, docs/design/role-kits.md, docs/plans/project-plan.md   modifies (Task 8)
```

## Interfaces

Consumes: `CustomServer`, `spec_sha256`, `custom_server` (core); `parse_kit`, `KitConnector` (roles); `list_tools`, `call_tool`, `launch_spec`, `LaunchSpec`, `working_folder`, `KEPT_ENV`, `launch_answer` (runtime); `ConnectorCommands`, `ConnectorAdd`, `KitConnect`; `Production::image_for` (12c).

Produces:

```rust
pub file_keys: Vec<String>                         // CustomServer, farik_core::team; in spec_sha256 when not empty
pub struct KeyFile { pub variable: String, pub file: PathBuf, pub text: Secret }        // farik_runtime::connectors
pub fn key_files(server: &CustomServer, keys: &BTreeMap<String, Secret>, folder: &Path) -> Result<Vec<KeyFile>, ConnectorError>;
pub fn write_key_files(files: &[KeyFile]) -> std::io::Result<()>;                      // 0600, in the 0700 folder
pub struct DeploymentObject(serde_json::Value);                                        // platforms::k8s_deployment
pub fn rollout_state(object: &DeploymentObject) -> DeploymentState;
pub fn with_image(object: &DeploymentObject, container: Option<&str>, image: &str) -> Result<DeploymentObject, PlatformError>;
pub fn with_restart(object: &DeploymentObject, now: DateTime<Utc>) -> DeploymentObject;
pub fn platform(connection: Connection, production: &Production) -> Result<Arc<dyn Platform>, PlatformError>;   // platforms::kubernetes
```

## Tasks

### Task 1: ADR 0046

Files: `docs/decisions/0046-fixed-settings-and-file-keys-in-a-kit-server.md` (the next free number after step 11's 0045; if taken by then, the next free one, with steps 12d and 12e's references changed in the same commit). Decides both fields a kit's server may now carry: `file_keys` (this step) and `env`, fixed non-secret variables (step 12e): what each is, its limits, its place in `spec_sha256`, and that neither lets a team file add a variable or a file a kit did not ship (ADR 0036's trust holds: a kit entry matches the kit whole). Rejected for each as above and in step 12e.

- [ ] `docs(decisions): let a kit's server take fixed settings and a key kept as a file`

### Task 2: `file_keys` in the format

- `a_kit_may_name_a_file_key`: parses, and `custom_server` carries it. RED.
- `refuses_a_bad_file_key`: not a credential key, on an `http` connector: each with its code. RED.
- `file_keys_change_the_hash_only_when_present`: a kept literal for an entry without them stands; with them it differs. RED.
- `a_team_file_cannot_add_a_file_key`: a `source: kit` entry with `file_keys` the kit lacks fails `matches_kit`. RED.

- [ ] `feat(roles): let a kit's connector keep a key as a file`

### Task 3: Writing the file

- `listing_writes_the_file_and_gives_its_path`: a `sh` fixture server prints `$KUBECONFIG` and its file's text and mode; the variable is a path inside the server's folder, the file holds the key byte for byte, mode 0600. RED.
- `the_launcher_writes_it_too`: `POST /connector/launch` answers `files` beside `env`; `farik connector run` writes them before it execs, and refuses an answer whose file is outside `cwd`. RED.
- `refuses_a_file_key_too_large_or_not_text`. RED.

- [ ] `feat(runtime): hand a server its key as a file in its own folder`

### Task 4: Giving a file

- `connect_reads_a_key_file` (`farik connect --key-file KUBECONFIG=<path>`) and `refuses_key_file_for_a_plain_key`. RED each.
- `the_key_form_takes_a_file_key_whole` (`ConnectorAdd`, as the board): "Choose the file" and the box, its line breaks kept in what is sent. RED.

- [ ] `feat(web): give a connector a key that is a whole file`

### Task 5: Kubernetes in the kit

`kubernetes` after `fly`; labels: `namespaces_list` "list namespaces", `events_list` "read events", `pods_list_in_namespace` "list pods", `pods_get` "read a pod", `pods_log` "read a pod's logs", `pods_top` "read pods' use", `resources_list` "list resources", `resources_get` "read a resource".

- `kubernetes_only_reads_for_the_agent`: the entry as above; the 8 `network`, labelled; the 14 `denied`, among them `pods_exec`, `resources_scale` and `configuration_view`; the copy exactly. RED.

- [ ] `feat(roles): give the DevOps Engineer Kubernetes`

### Task 6: The adapter

- `reads_a_rollouts_state` (pure): complete, progressing, and past its deadline. RED.
- `sets_the_image_keeping_the_resource_version` and `refuses_two_containers_without_a_name`. RED each.
- `restart_stamps_restarted_at`. RED.
- `deploys_restarts_and_rolls_back_through_the_server` (fixture through `call_tool`): `resources_create_or_update` with the image of `image_for`, then the stamp, then the healthy image. RED.

- [ ] `feat(runtime): deploy, restart and roll back a Kubernetes deployment`

### Task 7: Connected by name

- `connects_each_devops_service_by_name` gains `kubernetes`. Guard.
- `live_kit_pins.rs`'s header names Kubernetes and `FARIK_KIT_KUBERNETES_KUBECONFIG` (the file's text); the run writes it as the launcher does.

- [ ] `test(runtime): connect the DevOps Engineer's Kubernetes by name`

### Task 8: Spec and plan

`docs/SPEC.md` 6.7 (`file_keys`), 8.2 (the launcher writes a file key), 6.9's kit paragraph (Kubernetes); the revision line. `docs/design/role-kits.md`, `docs/design/devops-engineer.md`. Project plan row 12d.

- [ ] `docs(spec): record Kubernetes and file keys`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
FARIK_LIVE_TESTS=1 cargo test -p farik-runtime --test live_kit_pins
# expected: ok, Kubernetes listed with no drift
```

The live run needs Node.js and a cluster (a local `kind` cluster is enough) with the narrow service account. Then, by the founder, on that cluster: a planned deploy of an image pushed for a commit, then a broken one restored.

## Execution notes

None yet.
