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

/** `farik-e2e-serve` on a new project with its team, sandboxing off, on a free port. */
export async function startServe(o: { transcripts: string[] }): Promise<{
	url: string;
	port: number;
	project: string;
	stop(): Promise<void>;
}> {
	const project = mkdtempSync(join(tmpdir(), "farik-e2e-project-"));
	const git = (...args: string[]) =>
		execFileSync("git", args, { cwd: project });
	git("init", "-b", "main");
	writeFileSync(join(project, "README.md"), "the first line\n");
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
	farik(project, ["init"]);
	// What the CLI tests' `no_sandbox` writes: this machine runs tasks without Docker.
	mkdirSync(join(project, ".farik/local"), { recursive: true });
	writeFileSync(
		join(project, ".farik/local/settings.json"),
		'{"sandbox":"none"}',
	);

	const port = await freePort();
	const args = ["--port", String(port)];
	if (o.transcripts.length > 0)
		args.push("--transcripts", o.transcripts.join(","));
	const server = spawn(join(target, "farik-e2e-serve"), args, {
		cwd: project,
		env,
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
