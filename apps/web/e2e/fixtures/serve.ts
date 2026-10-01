import {
	type ChildProcessByStdio,
	execFileSync,
	spawn,
} from "node:child_process";
import {
	chmodSync,
	copyFileSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	writeFileSync,
} from "node:fs";
import { type AddressInfo, createServer } from "node:net";
import { tmpdir, userInfo } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import type { Readable } from "node:stream";

// Both built by `cargo xtask check --integration`: farik by its cargo tests, the server by its build step.
const target = resolve(import.meta.dirname, "../../../../target/debug");

// Farik's state folder for these runs, so the developer's own is never read or written.
const env = {
	...process.env,
	XDG_CONFIG_HOME: mkdtempSync(join(tmpdir(), "farik-e2e-config-")),
};

/** Farik's state folder, which every serve with a project shares: saved teams are kept in it. */
export const stateFolder = join(env.XDG_CONFIG_HOME, "farik");

/** Runs `farik <args>` in the project and answers what it printed. */
export function farik(project: string, args: string[]): string {
	return execFileSync(join(target, "farik"), args, {
		cwd: project,
		env,
		encoding: "utf8",
	});
}

/** The project's events, as `farik --json log` prints them. */
export function events(project: string): {
	kind: string;
	task_id?: string;
	agent_id?: string;
	body: Record<string, unknown>;
}[] {
	return farik(project, ["--json", "log"])
		.trim()
		.split("\n")
		.map((line) => JSON.parse(line));
}

function freePort(): Promise<number> {
	return new Promise((found, failed) => {
		const server = createServer();
		server.on("error", failed);
		server.listen(0, "127.0.0.1", () => {
			const { port } = server.address() as AddressInfo;
			server.close(() => found(port));
		});
	});
}

/** The start link the server prints, or the reason it never did. */
function link(
	server: ChildProcessByStdio<null, Readable, null>,
): Promise<string> {
	return new Promise((found, failed) => {
		const lines = createInterface({ input: server.stdout });
		lines.on("line", (line) => {
			const said = line.match(/^open (http:\S+) in your browser$/);
			if (said?.[1]) found(said[1]);
		});
		server.on("exit", (code) =>
			failed(new Error(`farik-e2e-serve exited with ${code}`)),
		);
	});
}

/**
 * Makes `folder` a git repository with one commit, of `README.md`, `files` written, and `present`,
 * files already in the folder.
 */
export function gitProject(
	folder: string,
	files: Record<string, string> = {},
	present: string[] = [],
): void {
	mkdirSync(folder, { recursive: true });
	const git = (...args: string[]) => execFileSync("git", args, { cwd: folder });
	git("init", "-b", "main");
	writeFileSync(join(folder, "README.md"), "the first line\n");
	for (const [name, text] of Object.entries(files)) {
		mkdirSync(join(folder, name, ".."), { recursive: true });
		writeFileSync(join(folder, name), text);
	}
	git("add", "README.md", ...Object.keys(files), ...present);
	git(
		"-c",
		"user.name=t",
		"-c",
		"user.email=t@example.com",
		"commit",
		"-m",
		"the first commit",
	);
}

/**
 * `farik-e2e-serve` on a free port. By default on a new project with its team, sandboxing off.
 * With `project: false` it starts in an empty folder, for the first-run wizard: `HOME` is `home`,
 * which holds Farik's state folder too, the fake `claude` and `docker` come first on `PATH`, no
 * key is in the environment, and the key is kept in a file, never the keychain.
 */
export async function startServe(o: {
	transcripts: string[];
	project?: boolean;
	/** A paused project that still waits for the team's setup, as a fresh take-on leaves it. */
	setupPending?: boolean;
	home?: string;
	/** The team to write in place of `farik init`'s two. */
	team?: "pm-architect-developer" | "pm-architect-developer-designer";
	/** How long each recorded session waits before it plays, so a page sees each state it leaves. */
	paceMs?: number;
	/** Whether `team`'s team plans its work in sprints (step 15); it does not by default. */
	sprints?: boolean;
}): Promise<{
	url: string;
	port: number;
	project: string;
	stop(): Promise<void>;
}> {
	const project = mkdtempSync(join(tmpdir(), "farik-e2e-project-"));
	const args: string[] = [];
	const docker = o.project !== false && o.team?.endsWith("-designer") === true;
	let serveEnv: NodeJS.ProcessEnv = env;
	if (o.project === false) {
		const {
			ANTHROPIC_API_KEY: _key,
			CLAUDE_CODE_OAUTH_TOKEN: _token,
			XDG_CONFIG_HOME: _config,
			...rest
		} = process.env;
		serveEnv = {
			...rest,
			HOME: o.home ?? mkdtempSync(join(tmpdir(), "farik-e2e-home-")),
			PATH: `${resolve(import.meta.dirname, "fake-bin")}:${process.env.PATH}`,
		};
		args.push("--no-keychain");
	} else {
		setUp(project, o.setupPending === true, docker);
		if (o.team) writeTeam(project, docker, o.sprints === true);
		// The Designer has no browser without Docker's sandbox (D3), so its team runs in it.
		if (docker) args.push("--sandbox-image", SANDBOX_IMAGE);
	}

	const port = await freePort();
	args.push("--port", String(port));
	if (o.transcripts.length > 0)
		args.push("--transcripts", o.transcripts.join(","));
	if (o.paceMs) args.push("--pace", String(o.paceMs));
	const server = spawn(join(target, "farik-e2e-serve"), args, {
		cwd: project,
		env: serveEnv,
		stdio: ["ignore", "pipe", "inherit"],
	});
	const url = await link(server);
	const exited = new Promise((done) => server.on("exit", done));
	return {
		url,
		port,
		project,
		async stop() {
			// The first Ctrl-C waits for the session running now; once the named sessions are
			// played it never ends, and the second aborts it.
			server.kill("SIGINT");
			const settled = setTimeout(() => server.kill("SIGINT"), 2000);
			await exited;
			clearTimeout(settled);
			if (docker) removeContainers(project);
		},
	};
}

/**
 * A project with its team, and sandboxing off, as `startServe` serves by default; with `designer`,
 * in Docker's sandbox instead, and holding the two-file site its preview serves.
 */
function setUp(
	project: string,
	setupPending: boolean,
	designer: boolean,
): void {
	if (designer) {
		copyFileSync(busybox(), join(project, "busybox"));
		chmodSync(join(project, "busybox"), 0o755);
		gitProject(project, SITE, ["busybox"]);
	} else {
		gitProject(project);
	}
	farik(project, ["init"]);
	// What the CLI tests' `no_sandbox` writes: this machine runs tasks without Docker.
	mkdirSync(join(project, ".farik/local"), { recursive: true });
	writeFileSync(
		join(project, ".farik/local/settings.json"),
		designer ? '{"sandbox":"docker"}' : '{"sandbox":"none"}',
	);
	if (setupPending) {
		farik(project, ["pause"]);
		writeFileSync(join(project, ".farik/local/setup-pending"), "");
	}
}

/**
 * Mira (Product Manager), Ada (Architect, who checks plans and reviews Theo) and Theo (Developer),
 * and Iris (UI/UX Designer) when `designer`: with the Playwright connector, no plan checked
 * before work starts, and the site's preview, busybox's `httpd` on port 4401. The team does not
 * plan its work in sprints unless `sprints`, so a journey's work flows as soon as it is ready.
 */
function writeTeam(project: string, designer: boolean, sprints: boolean): void {
	const agent = (id: string, name: string, role: string, extra = "") =>
		`- display_name: ${name}\n  id: ${id}\n${extra}  model:\n    effort: high\n    id: claude-opus-5-5\n  persona: ${name}.\n  role: ${role}\n  status: active\n`;
	const path = join(project, ".farik/team.yaml");
	let yaml = readFileSync(path, "utf8");
	const agents =
		agent("mira", "Mira", "product_manager") +
		agent("ada", "Ada", "architect") +
		agent("theo", "Theo", "software_developer") +
		(designer
			? agent(
					"iris",
					"Iris",
					"ui_ux_designer",
					"  mcp_servers:\n  - name: playwright\n    source: builtin\n",
				)
			: "");
	yaml = yaml
		.replace(/^agents:\n[\s\S]*?(?=^budgets:)/m, `agents:\n${agents}`)
		.replace("plan_in_sprints: true", `plan_in_sprints: ${sprints}`);
	if (designer) {
		yaml = yaml.replace("    required: always\n", "    required: never\n");
		// Alpine's own busybox leaves `httpd` out; the project carries busybox-extras' as `busybox`.
		yaml +=
			"preview:\n  start: ./busybox httpd -f -p 4401 -h site\n  port: 4401\n";
	}
	writeFileSync(path, yaml);
}

/** The image the Designer's team's sandbox and preview run in. */
const SANDBOX_IMAGE = "alpine:3.22";

/** The site the Designer's preview serves. */
const SITE = {
	"site/index.html":
		'<!doctype html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n<meta name="viewport" content="width=device-width, initial-scale=1">\n<title>Sign in</title>\n<link rel="stylesheet" href="style.css">\n</head>\n<body>\n<main>\n<h1>Sign in</h1>\n<p>Welcome back.</p>\n</main>\n</body>\n</html>\n',
	"site/style.css":
		"body { font-family: sans-serif; margin: 2rem; }\nh1 { color: #4a5a6c; }\n",
};

/**
 * busybox-extras' binary, which has the `httpd` applet `alpine:3.22`'s own busybox leaves out,
 * fetched once per machine by a container with the network on.
 */
function busybox(): string {
	const file = join(tmpdir(), "farik-e2e-busybox-extras");
	if (!existsSync(file)) {
		const out = mkdtempSync(join(tmpdir(), "farik-e2e-busybox-"));
		const { uid, gid } = userInfo();
		execFileSync("docker", [
			"run",
			"--rm",
			"--mount",
			`type=bind,src=${out},dst=/out`,
			SANDBOX_IMAGE,
			"sh",
			"-c",
			`apk add -q --no-cache busybox-extras && cp /bin/busybox-extras /out/busybox && chown ${uid}:${gid} /out/busybox`,
		]);
		copyFileSync(join(out, "busybox"), file);
	}
	return file;
}

/** Removes every container Farik left for `project`: a task that still waits keeps its sandbox. */
function removeContainers(project: string): void {
	const first = farik(project, ["--json", "log"]).split("\n")[0];
	const id = first ? JSON.parse(first).project_id : undefined;
	if (!id) return;
	const names = execFileSync(
		"docker",
		["ps", "-aq", "--filter", `label=farik.project=${id}`],
		{ encoding: "utf8" },
	)
		.split("\n")
		.filter(Boolean);
	if (names.length > 0) execFileSync("docker", ["rm", "-f", ...names]);
}
