import {
	type ChildProcessByStdio,
	execFileSync,
	spawn,
} from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
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
	home?: string;
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
	} else setUp(project);

	const port = await freePort();
	args.push("--port", String(port));
	if (o.transcripts.length > 0)
		args.push("--transcripts", o.transcripts.join(","));
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
			server.kill("SIGINT");
			await exited;
		},
	};
}

/** A project with its team, and sandboxing off, as `startServe` serves by default. */
function setUp(project: string): void {
	gitProject(project);
	farik(project, ["init"]);
	// What the CLI tests' `no_sandbox` writes: this machine runs tasks without Docker.
	mkdirSync(join(project, ".farik/local"), { recursive: true });
	writeFileSync(
		join(project, ".farik/local/settings.json"),
		'{"sandbox":"none"}',
	);
}
