import {
	type ChildProcessByStdio,
	execFileSync,
	spawn,
} from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { type AddressInfo, createServer } from "node:net";
import { tmpdir } from "node:os";
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

/** Runs `farik <args>` in the project and answers what it printed. */
export function farik(project: string, args: string[]): string {
	return execFileSync(join(target, "farik"), args, {
		cwd: project,
		env,
		encoding: "utf8",
	});
}

/** The project's events, as `farik --json log` prints them. */
export function events(
	project: string,
): { kind: string; body: Record<string, unknown> }[] {
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

/** Makes `folder` a git repository with one commit. */
export function gitProject(folder: string): void {
	mkdirSync(folder, { recursive: true });
	const git = (...args: string[]) => execFileSync("git", args, { cwd: folder });
	git("init", "-b", "main");
	writeFileSync(join(folder, "README.md"), "the first line\n");
	git("add", "README.md");
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
}): Promise<{
	url: string;
	port: number;
	project: string;
	stop(): Promise<void>;
}> {
	const project = mkdtempSync(join(tmpdir(), "farik-e2e-project-"));
	const args: string[] = [];
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
		setUp(project, o.setupPending === true);
		if (o.team) writeTeam(project, o.team.endsWith("-designer"));
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
		},
	};
}

/** A project with its team, and sandboxing off, as `startServe` serves by default. */
function setUp(project: string, setupPending: boolean): void {
	gitProject(project);
	farik(project, ["init"]);
	// What the CLI tests' `no_sandbox` writes: this machine runs tasks without Docker.
	mkdirSync(join(project, ".farik/local"), { recursive: true });
	writeFileSync(
		join(project, ".farik/local/settings.json"),
		'{"sandbox":"none"}',
	);
	if (setupPending) {
		farik(project, ["pause"]);
		writeFileSync(join(project, ".farik/local/setup-pending"), "");
	}
}

/**
 * Mira (Product Manager), Ada (Architect, who checks plans and reviews Theo) and Theo (Developer),
 * and Iris (UI/UX Designer) when `designer`.
 */
function writeTeam(project: string, designer: boolean): void {
	const agent = (id: string, name: string, role: string) =>
		`- display_name: ${name}\n  id: ${id}\n  model:\n    effort: high\n    id: claude-opus-5-5\n  persona: ${name}.\n  role: ${role}\n  status: active\n`;
	const path = join(project, ".farik/team.yaml");
	const yaml = readFileSync(path, "utf8");
	const agents =
		agent("mira", "Mira", "product_manager") +
		agent("ada", "Ada", "architect") +
		agent("theo", "Theo", "software_developer") +
		(designer ? agent("iris", "Iris", "ui_ux_designer") : "");
	writeFileSync(
		path,
		yaml.replace(/^agents:\n[\s\S]*?(?=^budgets:)/m, `agents:\n${agents}`),
	);
}
