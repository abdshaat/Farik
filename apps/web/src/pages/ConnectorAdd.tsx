import { Button, Choice, Dialog, Stepper, TextField } from "@farik/ui";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useConnection } from "../app/connection.tsx";
import { type Refusal, refusalsOf } from "../app/refusals.ts";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";
import type { McpServer } from "./setup/TeamSetup.tsx";

export type Tag = "network" | "external_effect" | "denied";
type Listed = { name: string; description: string; usable: boolean };
type Key = { name: string; value: string };
/** One of Farik's own apps signing the user in (phase 7 step 03b): its name, the code to type, where to install it. */
type AppSignIn = { provider: string; userCode?: string; installUrl?: string };
/** How signing in to the service stands (ADR 0033). `keys` is the key fields, with a sentence when the service said why. */
type SignIn =
	| { kind: "idle" }
	| {
			kind: "offered";
			attempt: string;
			address: string;
			issuer: string;
			app?: AppSignIn;
	  }
	| {
			kind: "waiting";
			attempt: string;
			address: string;
			issuer: string;
			app?: AppSignIn;
	  }
	| { kind: "signedIn"; attempt: string; issuer: string; app?: AppSignIn }
	| { kind: "failed"; code: string; issuer: string; app?: AppSignIn }
	| { kind: "keys"; probed: boolean; sentence?: string };
/** How often the page asks whether the user has said yes. */
const POLL_MS = 2000;

/** The host of a web address, for a sentence: `mcp.notion.com` of `https://mcp.notion.com/mcp`. */
export function hostOf(url: string | undefined): string {
	try {
		return new URL(url ?? "").hostname;
	} catch {
		return url ?? "";
	}
}

/** The card the boards draw around one of Farik's own apps signing the user in; no card for a service that signs in by itself. */
function SigningIn({ app, children }: { app: boolean; children: ReactNode }) {
	if (!app) return children;
	return (
		<section aria-label={t("addSigningIn")} className={styles.card}>
			{children}
		</section>
	);
}

/** `github.com/login/device` of `https://github.com/login/device`: a page, as the button names it. */
export function pageOf(address: string): string {
	try {
		const page = new URL(address);
		return `${page.hostname}${page.pathname.replace(/\/$/, "")}`;
	} catch {
		return address;
	}
}

/** The app a sign-in's answer names, when it names one. */
function appOf(answer: {
	provider?: string;
	userCode?: string;
	installUrl?: string;
}): { app: AppSignIn } | Record<string, never> {
	if (!answer.provider) return {};
	return {
		app: {
			provider: answer.provider,
			...(answer.userCode && { userCode: answer.userCode }),
			...(answer.installUrl && { installUrl: answer.installUrl }),
		},
	};
}

/** "notion" as "Notion": the name the user gave, said as a service's. */
const serviceOf = (server: string) =>
	server ? server.charAt(0).toUpperCase() + server.slice(1) : server;
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
	ended,
	sandboxed,
	onClose,
}: {
	agent: string;
	/** The agent's display name. */
	name: string;
	/** The team file's entry, when connecting it again. */
	again?: McpServer | undefined;
	/** The service ended the sign-in `again` holds, so the page says so and signs in at once. */
	ended?: boolean | undefined;
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
		signedIn?: boolean;
	}>();
	// A server connected with keys goes straight to its keys; one that signs in asks to again.
	const [sign, setSign] = useState<SignIn>(
		again && !again.oauth ? { kind: "keys", probed: false } : { kind: "idle" },
	);
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
	// An empty part is not sent: the place of each sent part, so a refusal at `args/<n>` lands on
	// the part the user sees (re-review 2 m5).
	const sentArgs = args.flatMap((a, i) => (a === "" ? [] : [i]));
	// A program's name holds no space; a whole line pasted into Command is said at once. A full
	// path may hold one.
	const commandWhole =
		!http && !command.trim().startsWith("/") && /\s/.test(command.trim());
	const atParts = (at: Partial<Record<Field, string>>) =>
		Object.fromEntries(
			Object.entries(at).map(([field, words]) => {
				const n = /^arg-(\d+)$/.exec(field)?.[1];
				return [n === undefined ? field : `arg-${sentArgs[Number(n)]}`, words];
			}),
		) as Partial<Record<Field, string>>;
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
			: { command: command.trim(), args: sentArgs.map((i) => args[i]) }),
		credentialKeys: named.map((k) => k.name.trim()),
	};
	const keysWire = Object.fromEntries(
		named.map((k) => [k.name.trim(), k.value]),
	);
	// What signing in asks about: the server, its sign-in settings, no key, and no Authorization
	// header, which the sign-in sends itself.
	const signInHeaders = sentHeaders.filter(
		(l) => l.name.toLowerCase() !== "authorization",
	);
	const signInWire = {
		name: server.trim(),
		transport: "http",
		url: url.trim(),
		...(signInHeaders.length > 0 && {
			headers: Object.fromEntries(signInHeaders.map((l) => [l.name, l.value])),
		}),
		oauth: again?.oauth ?? {},
	};
	const urlHost = hostOf(url.trim());
	const signedIn = http && sign.kind === "signedIn";
	const ready =
		server.trim() &&
		(http ? url.trim() : command.trim()) &&
		!secretInUrl &&
		!commandWhole;

	const listed = (answer: { tools: Listed[] }) => {
		setTools(answer.tools);
		setTags({});
		setStep(1);
	};
	const list = async () => {
		if (!client) return;
		setBusy(true);
		setRefused({});
		try {
			if (signedIn && sign.kind === "signedIn") {
				listed(
					(await client.call("connector.tools", {
						agent,
						server: signInWire,
						attempt: sign.attempt,
					})) as { tools: Listed[] },
				);
			} else {
				listed(
					(await client.call("connector.tools", {
						agent,
						server: wire,
						keys: keysWire,
					})) as { tools: Listed[] },
				);
			}
		} catch (e) {
			// A sign-in that is gone (it lasts ten minutes) is said as that, and asked for again.
			if (
				signedIn &&
				codeOf(refusalsOf(e)[0]?.message ?? "") === "sign_in_unknown"
			)
				setSign({ kind: "failed", code: "sign_in_timed_out", issuer: "" });
			else setRefused(atParts(refusalsAt(e, fill)));
		}
		setBusy(false);
	};
	/**
	 * Asks the service whether it signs the user in: what Next does for a web address with no key
	 * typed. A service that offers no sign-in, and so needs a key the user has not typed, is
	 * listed at once when `thenList`: there is nothing to wait for.
	 */
	const ask = async (thenList = false) => {
		if (!client) return;
		setBusy(true);
		setRefused({});
		try {
			const answer = (await client.call("connector.sign_in", {
				agent,
				server: signInWire,
			})) as {
				attempt: string;
				authorizeUrl: string;
				issuer: string;
				provider?: string;
				userCode?: string;
				installUrl?: string;
			};
			setSign({
				kind: "offered",
				attempt: answer.attempt,
				address: answer.authorizeUrl,
				issuer: answer.issuer,
				...appOf(answer),
			});
		} catch (e) {
			const code = codeOf(refusalsOf(e)[0]?.message ?? "");
			if (code === "sign_in_not_offered") {
				setSign({ kind: "keys", probed: true });
				if (thenList) await list();
			} else if (code === "sign_in_not_supported")
				setSign({
					kind: "keys",
					probed: true,
					sentence: t("addNotSupported", { host: urlHost }),
				});
			else if (code === "pkce_not_supported" || code === "sign_in_failed")
				setSign({
					kind: "keys",
					probed: true,
					sentence: t("addSignInCouldNot", { host: urlHost }),
				});
			else setRefused(atParts(refusalsAt(e, fill)));
		}
		setBusy(false);
	};
	// "Sign in again" opens at the sign-in: the service is asked as soon as the page is up.
	const asked = useRef(false);
	// biome-ignore lint/correctness/useExhaustiveDependencies: once, when the page opens
	useEffect(() => {
		if (asked.current || !again?.oauth) return;
		asked.current = true;
		void ask();
	}, []);
	// Whether the user has said yes, asked every 2 seconds.
	const waitingFor = sign.kind === "waiting" ? sign.attempt : "";
	const waitingIssuer = sign.kind === "waiting" ? sign.issuer : "";
	const waitingApp = sign.kind === "waiting" ? sign.app : undefined;
	// biome-ignore lint/correctness/useExhaustiveDependencies: the attempt is the dependency
	useEffect(() => {
		if (!waitingFor || !client) return;
		let live = true;
		let timer: ReturnType<typeof setTimeout> | undefined;
		const poll = async () => {
			try {
				const answer = (await client.call("connector.sign_in_status", {
					attempt: waitingFor,
				})) as { state: string; reason?: { code: string } };
				if (!live) return;
				if (answer.state === "signed_in")
					return setSign({
						kind: "signedIn",
						attempt: waitingFor,
						issuer: waitingIssuer,
						...(waitingApp && { app: waitingApp }),
					});
				if (answer.state === "failed")
					return setSign({
						kind: "failed",
						code: answer.reason?.code ?? "sign_in_failed",
						issuer: waitingIssuer,
						...(waitingApp && { app: waitingApp }),
					});
			} catch {
				// The attempt is gone: it lasts ten minutes.
				if (live)
					return setSign({
						kind: "failed",
						code: "sign_in_timed_out",
						issuer: waitingIssuer,
					});
			}
			if (live) timer = setTimeout(poll, POLL_MS);
		};
		timer = setTimeout(poll, POLL_MS);
		return () => {
			live = false;
			if (timer) clearTimeout(timer);
		};
	}, [waitingFor]);
	/** The service's page, in a new tab: run in the click itself, so a pop-up blocker lets it through. */
	const openPage = (address: string) =>
		window.open(address, "_blank", "noopener");
	/** Next: ask the service first when a web address has no key typed; else list the tools. */
	const next = () => {
		if (http && sign.kind === "idle" && named.length === 0) return ask(true);
		return list();
	};
	const connect = async () => {
		if (!client) return;
		setBusy(true);
		const sending = keysWire;
		const labels = Object.fromEntries(
			tools
				.filter((x) => x.usable)
				.map((x) => [x.name, tags[x.name] ?? "external_effect"]),
		);
		// The keys leave the page with this call; none is kept for a second try.
		setKeys(keys.map((k) => ({ ...k, value: "" })));
		try {
			const answer = (await client.call(
				"connector.connect",
				signedIn && sign.kind === "signedIn"
					? { agent, server: signInWire, attempt: sign.attempt, tags: labels }
					: { agent, server: wire, keys: sending, tags: labels },
			)) as NonNullable<typeof done>;
			setDone({ ...answer, signedIn });
			setStep(2);
		} catch (e) {
			setRefused(atParts(refusalsAt(e, fill)));
			setStep(0);
		}
		setBusy(false);
	};
	/** Why a sign-in failed, in the words of the code and not the daemon's. */
	const failedWords = (code: string, issuer: string, provider?: string) =>
		t(
			(
				{
					access_denied: "addSignInDenied",
					sign_in_timed_out: "addSignInTimedOut",
					sign_in_mismatch: "addSignInMismatch",
				} as const
			)[code as "access_denied"] ?? "addSignInFailed",
			{ host: provider ?? (hostOf(issuer) || urlHost) },
		);
	// Anything the sign-in was made for changing makes it another: the user signs in again.
	const startOver = () => {
		if (["offered", "waiting", "signedIn", "failed"].includes(sign.kind))
			setSign({ kind: "idle" });
		else if (sign.kind === "keys" && sign.probed) setSign({ kind: "idle" });
	};

	// The key fields show unless the service offers a sign-in, which stands in for them.
	const showsKeys = !http || sign.kind === "idle" || sign.kind === "keys";
	const issuerHost = "issuer" in sign ? hostOf(sign.issuer) : "";
	/** Who signs the user in: one of Farik's own apps by name, else the issuer's host. */
	const signer = "app" in sign && sign.app ? sign.app.provider : issuerHost;
	/** Copies the code the user types, where the browser lets a page write to the clipboard. */
	const copy = (code: string) => {
		void navigator.clipboard?.writeText(code).catch(() => {});
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
					{again && ended && (
						<p className={styles.alert}>
							{t("addSignInEnded", { host: urlHost, server: fill.server })}
						</p>
					)}
					{again && !ended && (
						<p className={styles.alert}>{t("addChanged", fill)}</p>
					)}
					<TextField
						id="connector-name"
						label={t("addName")}
						hint={t("addNameHint", { name })}
						value={server}
						error={refused.name ?? ""}
						onChange={(value) => {
							setServer(value);
							startOver();
						}}
					/>
					{!again && (
						<Choice
							name="connector-transport"
							legend={t("addHow")}
							value={transport}
							onChange={(value) => {
								setTransport(value);
								startOver();
							}}
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
								onChange={(value) => {
									setUrl(value);
									startOver();
								}}
							/>
							{!secretInUrl && sign.kind === "offered" && (
								<SigningIn app={Boolean(sign.app)}>
									<p>
										{t("addSignInLead", {
											host: sign.app?.provider ?? urlHost,
										})}
									</p>
									<span>
										<Button
											kind="primary"
											onClick={() => {
												// A code is typed on a page the user opens from the next board.
												if (!sign.app?.userCode) openPage(sign.address);
												setSign({ ...sign, kind: "waiting" });
											}}
										>
											{t("addSignInButton", { host: signer })}
										</Button>
									</span>
									{!sign.app && issuerHost !== urlHost && (
										<p className={styles.muted}>
											{t("addSignInFor", { host: urlHost })}
										</p>
									)}
									<p className={styles.muted}>
										{sign.app?.userCode
											? t("addSignInCodeNote", { provider: sign.app.provider })
											: t("addSignInNote")}
									</p>
									<span>
										<Button
											kind="quiet"
											onClick={() => setSign({ kind: "keys", probed: true })}
										>
											{t("addUseAKey")}
										</Button>
									</span>
								</SigningIn>
							)}
							{!secretInUrl &&
								sign.kind === "waiting" &&
								sign.app?.userCode && (
									<SigningIn app>
										<p>{t("addCodeLead", { provider: sign.app.provider })}</p>
										<p className={styles.userCode}>{sign.app.userCode}</p>
										<p>{t("addCodeWarning")}</p>
										<div className={styles.codeActions}>
											<Button onClick={() => copy(sign.app?.userCode ?? "")}>
												{t("addCodeCopy")}
											</Button>
											<Button
												kind="primary"
												onClick={() => openPage(sign.address)}
											>
												{t("addCodeOpen", { page: pageOf(sign.address) })}
											</Button>
										</div>
										<p role="status">
											{t("addCodeWaiting", { provider: sign.app.provider })}
										</p>
									</SigningIn>
								)}
							{!secretInUrl &&
								sign.kind === "waiting" &&
								!sign.app?.userCode && (
									<>
										<p role="status">{t("addWaiting", { host: signer })}</p>
										<span>
											<Button onClick={() => openPage(sign.address)}>
												{t("addOpenAgain")}
											</Button>
										</span>
									</>
								)}
							{!secretInUrl && sign.kind === "signedIn" && (
								<SigningIn app={Boolean(sign.app)}>
									<p role="status">
										<strong>{t("addSignedInTo", { host: signer })}</strong>
									</p>
									{sign.app?.installUrl?.startsWith("https://") && (
										<>
											<p>
												{t("addInstallLine", {
													name,
													provider: sign.app.provider,
												})}
											</p>
											<p>
												<a
													href={sign.app.installUrl}
													target="_blank"
													rel="noopener noreferrer"
												>
													{t("addInstallLink", { provider: sign.app.provider })}
												</a>
											</p>
										</>
									)}
								</SigningIn>
							)}
							{!secretInUrl && sign.kind === "failed" && (
								<>
									<p role="alert" className={styles.alert}>
										{failedWords(sign.code, sign.issuer, sign.app?.provider)}
									</p>
									<span>
										<Button kind="primary" busy={busy} onClick={() => ask()}>
											{t("addTryAgain")}
										</Button>
									</span>
								</>
							)}
							{!secretInUrl && sign.kind === "keys" && sign.sentence && (
								<p role="alert" className={styles.alert}>
									{sign.sentence}
								</p>
							)}
							{!secretInUrl && showsKeys && (
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
								error={
									commandWhole ? t("addCommandWhole") : (refused.command ?? "")
								}
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
					{!secretInUrl && showsKeys && (
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
						{(showsKeys || sign.kind === "signedIn") && (
							<Button
								kind="primary"
								busy={busy}
								disabled={!ready || !headerOk}
								onClick={next}
							>
								{t("addNext")}
							</Button>
						)}
						<Button onClick={() => onClose(false)}>{t("agentCancel")}</Button>
					</div>
					{showsKeys && (
						<p className={styles.muted}>{t("addNextNote", { name })}</p>
					)}
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
					{done?.signedIn && (
						<>
							<p>
								<strong>{t("addSignedIn")}</strong>
							</p>
							<p>
								{t(file ? "addSignedInFile" : "addSignedInKeychain", {
									name,
									service: serviceOf(fill.server),
								})}
								{sandboxed ? "" : ` ${t("addSignedInNoSandbox", { name })}`}
							</p>
						</>
					)}
					{!done?.signedIn && named.length > 0 && (
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
						<li>{t(done?.signedIn ? "addOnlySigned" : "addOnly", fill)}</li>
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
