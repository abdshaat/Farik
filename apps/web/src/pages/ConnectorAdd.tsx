import { Button, Choice, Dialog, Stepper, TextField } from "@farik/ui";
import { useState } from "react";
import { useConnection } from "../app/connection.tsx";
import { type Refusal, refusalsOf } from "../app/refusals.ts";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";
import type { McpServer } from "./setup/TeamSetup.tsx";

export type Tag = "network" | "external_effect" | "denied";
type Listed = { name: string; description: string; usable: boolean };
type Key = { name: string; value: string };
/** A field of step 1 a refusal can be said at; `arg-<n>` is the command's part n, from 0. */
type Field =
	| "name"
	| "command"
	| `arg-${number}`
	| "url"
	| `header:${string}`
	| "keys"
	| "other";

/** Each label, in the order it is offered, with its words (SPEC 5.6). */
export const TAGS: [Tag, keyof typeof en, keyof typeof en][] = [
	["network", "tagNetwork", "tagNetworkNote"],
	["external_effect", "tagExternal", "tagExternalNote"],
	["denied", "tagDenied", "tagDeniedNote"],
];

/** "2 Only reads, 1 Never, 1 Farik can’t use": each label's count, none left out but zeros. */
export function labelsSaid(tags: Tag[], unusable = 0): string {
	const said = TAGS.map(([tag, words]) => [
		tags.filter((x) => x === tag).length,
		t(words),
	])
		.filter(([n]) => n)
		.map(([n, words]) => `${n} ${words}`);
	if (unusable) said.push(t("connectorCantUse", { count: unusable }));
	return said.join(", ");
}

/** A web address that would carry a key into the committed team file: userinfo or a query. */
function holdsSecret(url: string): boolean {
	try {
		const u = new URL(url);
		return Boolean(u.username || u.password || u.search);
	} catch {
		return false;
	}
}

/** Where on step 1 a team refusal at `/agents/<n>/mcp_servers/<m>/<field>` belongs. */
function fieldOf(path: string): Field {
	const [field, index] = path.split("/").slice(5);
	if (field === "name") return "name";
	if (field === "args" && index !== undefined) return `arg-${Number(index)}`;
	if (field === "command" || field === "args") return "command";
	if (field === "url") return "url";
	if (field === "headers") return `header:${index ?? ""}`;
	if (field === "credential_keys") return "keys";
	return "other";
}

/** Each refusal code a connector's entry or labels can get, and its words (SPEC 6.7). */
const BY_CODE: Record<string, keyof typeof en> = {
	connector_name_reserved: "addNameReserved",
	connector_name_twice: "addNameTwice",
	url_holds_secret: "addUrlSecret",
	header_key_unknown: "addHeaderWrong",
	header_holds_secret: "addHeaderSecret",
	command_not_absolute: "addCommandNotAbsolute",
	arg_holds_secret: "addArgSecret",
	state_inside_project: "addStateInsideProject",
	tag_unknown_tool: "addTagUnknown",
};

/** The code a daemon sentence leads with, as `code: words`. */
const codeOf = (message: string) => /^([a-z_]+): /.exec(message)?.[1] ?? "";

/** A refusal's words at its field. The team's own sentence leads with its code; the page never shows it. */
function wordsFor(r: Refusal, field: Field, fill: Record<string, string>) {
	const byField: Record<string, keyof typeof en> = {
		name: "addNameWrong",
		command: "addCommandWrong",
		url: "addUrlWrong",
		keys: "addKeyWrong",
		other: "refuseOther",
	};
	return t(
		BY_CODE[codeOf(r.message)] ??
			byField[field] ??
			(field.startsWith("header:") ? "addHeaderWrong" : "addCommandWrong"),
		fill,
	);
}

/** A failed list or connect, in plain words at the fields it names; never the daemon's own words. */
function refusalsAt(
	e: unknown,
	fill: Record<string, string>,
): Partial<Record<Field, string>> {
	const at: Partial<Record<Field, string>> = {};
	for (const r of refusalsOf(e)) {
		if (r.path) {
			const field = fieldOf(r.path);
			at[field] ??= wordsFor(r, field, fill);
		} else if (BY_CODE[codeOf(r.message)])
			at.other = wordsFor(r, "other", fill);
		else if (r.message.startsWith("the server did not answer"))
			at.other = t("addTimeout", fill);
		else if (/^(its tools could not be listed|the key )/.test(r.message))
			at.other = t("addNotListed", fill);
		else at.other = t("refuseOther");
	}
	return at;
}

/**
 * ConnectorAdd (ADR 0030): how a custom connector starts and its keys, then a label for each of
 * its tools, then where the keys were kept. Keys live in this component's state only, typed into
 * password fields, and are dropped once `connector.connect` is sent, whatever it answers.
 */
export function ConnectorAdd({
	agent,
	name,
	again,
	sandboxed,
	onClose,
}: {
	agent: string;
	/** The agent's display name. */
	name: string;
	/** The team file's entry, when connecting it again. */
	again?: McpServer | undefined;
	/** Whether sessions run in Docker's sandbox, the one mode where no command of the agent reaches its keys. */
	sandboxed: boolean;
	onClose: (changed: boolean) => void;
}) {
	const { client } = useConnection();
	const [step, setStep] = useState(0);
	const [server, setServer] = useState(again?.name ?? "");
	const [transport, setTransport] = useState<"stdio" | "http">(
		again?.transport ?? "stdio",
	);
	const [command, setCommand] = useState(again?.command ?? "");
	// Each part after the program in a field of its own, so one holding a space is kept whole.
	const [args, setArgs] = useState<string[]>(again?.args ?? []);
	const [url, setUrl] = useState(again?.url ?? "");
	// One line per header, "Name: value": Connect again shows each the team file has (N7).
	const [headers, setHeaders] = useState<string[]>(
		again
			? Object.entries(again.headers ?? {}).map(([n, v]) => `${n}: ${v}`)
			: ["Authorization: Bearer {API_KEY}"],
	);
	const [keys, setKeys] = useState<Key[]>(
		again
			? (again.credentialKeys ?? []).map((n) => ({ name: n, value: "" }))
			: [{ name: "", value: "" }],
	);
	const [tools, setTools] = useState<Listed[]>([]);
	const [tags, setTags] = useState<Record<string, Tag>>({});
	const [done, setDone] = useState<{
		storedIn: "keychain" | "file";
		tools: Record<string, Tag>;
	}>();
	const [refused, setRefused] = useState<Partial<Record<Field, string>>>({});
	const [busy, setBusy] = useState(false);

	const fill = { name, server: server.trim() };
	const named = keys.filter((k) => k.name.trim());
	const http = transport === "http";
	const secretInUrl = http && holdsSecret(url);
	const lines = headers.map((line) => {
		const [n, ...value] = line.split(":");
		const name = n?.trim() ?? "";
		const ok = !line.trim() || (value.length > 0 && name !== "");
		return { name, value: value.join(":").trim(), ok };
	});
	/** What is wrong with line `i` before anything is sent: its form, or a name an earlier line has. */
	const lineWrong = (i: number) => {
		const line = lines[i];
		if (!line?.ok) return t("addHeaderWrong");
		const twice = (o: { name: string }) =>
			o.name.toLowerCase() === line.name.toLowerCase();
		return line.name && lines.slice(0, i).some(twice)
			? t("addHeaderTwice")
			: "";
	};
	const lineError = (i: number) =>
		lineWrong(i) || (refused[`header:${lines[i]?.name ?? ""}`] ?? "");
	const headerOk = headers.every((_, i) => !lineWrong(i));
	const sentHeaders = lines.filter((l) => l.name);
	const wire = {
		name: server.trim(),
		transport,
		...(http
			? {
					url: url.trim(),
					...(sentHeaders.length > 0 && {
						headers: Object.fromEntries(
							sentHeaders.map((l) => [l.name, l.value]),
						),
					}),
				}
			: { command: command.trim(), args: args.filter((a) => a !== "") }),
		credentialKeys: named.map((k) => k.name.trim()),
	};
	const keysWire = Object.fromEntries(
		named.map((k) => [k.name.trim(), k.value]),
	);
	const ready =
		server.trim() && (http ? url.trim() : command.trim()) && !secretInUrl;

	const list = async () => {
		if (!client) return;
		setBusy(true);
		setRefused({});
		try {
			const answer = (await client.call("connector.tools", {
				agent,
				server: wire,
				keys: keysWire,
			})) as { tools: Listed[] };
			setTools(answer.tools);
			setTags({});
			setStep(1);
		} catch (e) {
			setRefused(refusalsAt(e, fill));
		}
		setBusy(false);
	};
	const connect = async () => {
		if (!client) return;
		setBusy(true);
		const sending = keysWire;
		// The keys leave the page with this call; none is kept for a second try.
		setKeys(keys.map((k) => ({ ...k, value: "" })));
		try {
			const answer = (await client.call("connector.connect", {
				agent,
				server: wire,
				keys: sending,
				tags: Object.fromEntries(
					tools
						.filter((x) => x.usable)
						.map((x) => [x.name, tags[x.name] ?? "external_effect"]),
				),
			})) as NonNullable<typeof done>;
			setDone(answer);
			setStep(2);
		} catch (e) {
			setRefused(refusalsAt(e, fill));
			setStep(0);
		}
		setBusy(false);
	};

	const title =
		again || step > 0 ? t("addTitleNamed", fill) : t("addTitle", { name });
	const usable = tools.filter((x) => x.usable);
	const counted = labelsSaid(
		Object.values(done?.tools ?? {}),
		tools.length - usable.length,
	);
	const file = done?.storedIn === "file";
	const setArg = (i: number, value: string) =>
		setArgs(args.map((old, j) => (j === i ? value : old)));
	const setKey = (i: number, k: Partial<Key>) =>
		setKeys(keys.map((old, j) => (j === i ? { ...old, ...k } : old)));

	return (
		<Dialog open title={title} onClose={() => onClose(step === 2)}>
			<Stepper
				steps={[t("addStepStart"), t("addStepLabel"), t("addStepDone")]}
				current={step}
			/>
			{step === 0 && (
				<>
					{again && <p className={styles.alert}>{t("addChanged", fill)}</p>}
					<TextField
						id="connector-name"
						label={t("addName")}
						hint={t("addNameHint", { name })}
						value={server}
						error={refused.name ?? ""}
						onChange={setServer}
					/>
					{!again && (
						<Choice
							name="connector-transport"
							legend={t("addHow")}
							value={transport}
							onChange={setTransport}
							options={[
								{
									value: "stdio",
									label: t("addCommandChoice"),
									description: t("addCommandChoiceNote"),
								},
								{
									value: "http",
									label: t("addUrlChoice"),
									description: t("addUrlChoiceNote"),
								},
							]}
						/>
					)}
					{http ? (
						<>
							<TextField
								id="connector-url"
								label={t("addUrl")}
								hint={t("addUrlHint")}
								type="url"
								value={url}
								error={secretInUrl ? t("addUrlSecret") : (refused.url ?? "")}
								onChange={setUrl}
							/>
							{!secretInUrl && (
								<>
									{headers.map((line, i) => (
										// biome-ignore lint/suspicious/noArrayIndexKey: a line is its place in the list
										<div key={i} className={styles.keyRow}>
											<TextField
												id={`connector-header-${i}`}
												label={
													i === 0
														? t("addHeader")
														: t("addHeaderN", { count: i + 1 })
												}
												hint={i === 0 ? t("addHeaderHint") : ""}
												value={line}
												error={lineError(i)}
												onChange={(value) =>
													setHeaders(
														headers.map((old, j) => (j === i ? value : old)),
													)
												}
											/>
											<Button
												kind="quiet"
												onClick={() =>
													setHeaders(headers.filter((_, j) => j !== i))
												}
											>
												{t("addKeyRemove")}{" "}
												<span className={styles.hidden}>
													{t("addHeaderRemoveLabel", { count: i + 1 })}
												</span>
											</Button>
										</div>
									))}
									{headers.length < 16 && (
										<span>
											<Button onClick={() => setHeaders([...headers, ""])}>
												{t("addHeaderMore")}
											</Button>
										</span>
									)}
								</>
							)}
						</>
					) : (
						<>
							<TextField
								id="connector-command"
								label={t("addCommand")}
								hint={t("addCommandHint")}
								value={command}
								error={refused.command ?? ""}
								onChange={setCommand}
							/>
							<fieldset className={styles.group}>
								<legend className={styles.subheading}>{t("addArgs")}</legend>
								<p className={styles.muted}>{t("addArgsHint")}</p>
								{args.map((a, i) => (
									// biome-ignore lint/suspicious/noArrayIndexKey: a part is its place in the command
									<div key={i} className={styles.keyRow}>
										<TextField
											id={`connector-arg-${i}`}
											label={t("addArg", { count: i + 1 })}
											value={a}
											error={refused[`arg-${i}`] ?? ""}
											onChange={(value) => setArg(i, value)}
										/>
										<Button
											kind="quiet"
											onClick={() => setArgs(args.filter((_, j) => j !== i))}
										>
											{t("addKeyRemove")}{" "}
											<span className={styles.hidden}>
												{t("addArgRemoveLabel", { count: i + 1 })}
											</span>
										</Button>
									</div>
								))}
								<span>
									<Button onClick={() => setArgs([...args, ""])}>
										{t("addArgMore")}
									</Button>
								</span>
							</fieldset>
						</>
					)}
					{!secretInUrl && (
						<fieldset className={styles.group}>
							<legend className={styles.subheading}>{t("addKeys")}</legend>
							<p className={styles.muted}>
								{t(again ? "addKeysAgain" : "addKeysHint", { name })}
							</p>
							{keys.map((k, i) => (
								// biome-ignore lint/suspicious/noArrayIndexKey: a row is its place in the list
								<div key={i} className={styles.keyRow}>
									<TextField
										id={`connector-key-name-${i}`}
										label={t("addKeyName")}
										value={k.name}
										onChange={(n) => setKey(i, { name: n })}
									/>
									<TextField
										id={`connector-key-${i}`}
										label={t("addKeyValue")}
										type="password"
										value={k.value}
										onChange={(value) => setKey(i, { value })}
									/>
									<Button
										kind="quiet"
										onClick={() => setKeys(keys.filter((_, j) => j !== i))}
									>
										{t("addKeyRemove")}{" "}
										<span className={styles.hidden}>
											{t("addKeyRemoveLabel", { count: i + 1 })}
										</span>
									</Button>
								</div>
							))}
							{refused.keys && (
								<p role="alert" className={styles.alert}>
									{refused.keys}
								</p>
							)}
							{keys.length < 8 && (
								<span>
									<Button
										onClick={() => setKeys([...keys, { name: "", value: "" }])}
									>
										{t("addKeyMore")}
									</Button>
								</span>
							)}
						</fieldset>
					)}
					{refused.other && (
						<p role="alert" className={styles.alert}>
							{refused.other}
						</p>
					)}
					<div className={styles.actions}>
						<Button
							kind="primary"
							busy={busy}
							disabled={!ready || !headerOk}
							onClick={list}
						>
							{t("addNext")}
						</Button>
						<Button onClick={() => onClose(false)}>{t("agentCancel")}</Button>
					</div>
					<p className={styles.muted}>{t("addNextNote", { name })}</p>
				</>
			)}
			{step === 1 && (
				<>
					<p>{t("addListLead", { ...fill, count: tools.length })}</p>
					<dl className={styles.tagWords}>
						{TAGS.map(([tag, label, note]) => (
							<div key={tag}>
								<dt>{t(label)}</dt>
								<dd>{t(note, { name })}</dd>
							</div>
						))}
					</dl>
					<p className={styles.muted}>{t("addFrom", fill)}</p>
					<ul className={styles.ruled}>
						{tools.map((tool) => (
							<li key={tool.name}>
								{tool.usable ? (
									<fieldset className={styles.group}>
										<legend className={styles.code}>{tool.name}</legend>
										{tool.description && (
											<p className={styles.muted}>{tool.description}</p>
										)}
										<div className={styles.labels}>
											{TAGS.map(([tag, label]) => (
												<label key={tag}>
													<input
														type="radio"
														name={`tag-${tool.name}`}
														checked={
															(tags[tool.name] ?? "external_effect") === tag
														}
														onChange={() =>
															setTags({ ...tags, [tool.name]: tag })
														}
													/>
													{t(label)}
												</label>
											))}
										</div>
									</fieldset>
								) : (
									<>
										<p className={styles.code}>{tool.name}</p>
										{tool.description && (
											<p className={styles.muted}>{tool.description}</p>
										)}
										<p>{t("addUnusable", { name })}</p>
									</>
								)}
							</li>
						))}
					</ul>
					<div className={styles.actions}>
						<Button kind="primary" busy={busy} onClick={connect}>
							{t("addTitleNamed", fill)}
						</Button>
						<Button
							onClick={() => {
								setRefused({});
								setStep(0);
							}}
						>
							{t("addBack")}
						</Button>
					</div>
				</>
			)}
			{step === 2 && (
				<>
					<p role="status">
						<strong>{t("addDone", fill)}</strong>
					</p>
					{named.length > 0 && (
						<p>
							<strong>{t(file ? "addFile" : "addKeychain", fill)}</strong>{" "}
							{t(
								sandboxed
									? file
										? "addFileNote"
										: "addKeychainNote"
									: file
										? "addFileNoteNoSandbox"
										: "addKeychainNoteNoSandbox",
								fill,
							)}
						</p>
					)}
					<ul>
						<li>{counted}</li>
						<li>{t("addNextWork", { name })}</li>
						<li>{t("addOnly", fill)}</li>
					</ul>
					<span>
						<Button kind="primary" onClick={() => onClose(true)}>
							{t("addBackTo", { name })}
						</Button>
					</span>
				</>
			)}
		</Dialog>
	);
}
