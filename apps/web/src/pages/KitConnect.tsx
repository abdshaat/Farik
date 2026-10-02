import { Button, Dialog, Stepper, TextField } from "@farik/ui";
import { useEffect, useState } from "react";
import { useConnection } from "../app/connection.tsx";
import { refusalsOf } from "../app/refusals.ts";
import { t } from "../strings/t.ts";
import { hostOf, type Tag } from "./ConnectorAdd.tsx";
import styles from "./pages.module.css";
import type { KitService } from "./Team.tsx";

/** How often the page asks whether the user has said yes. */
const POLL_MS = 2000;
/** How signing in to the service stands. */
type SignIn =
	| { kind: "asking" }
	| { kind: "offered"; attempt: string; address: string; issuer: string }
	| { kind: "waiting"; attempt: string; address: string; issuer: string }
	| { kind: "failed"; code: string };

/** The code a daemon sentence leads with, as `code: words`. */
const codeOf = (message: string) => /^([a-z_]+): /.exec(message)?.[1] ?? "";

/** A tool as the user reads it: the kit's label, else its name with `_` and `-` read as spaces. */
const toolSaid = (labels: Record<string, string>, tool: string) =>
	labels[tool] ?? tool.replace(/[_-]+/g, " ");

/**
 * Connecting a service of the agent's role's kit (ADR 0036): what it is, why the role wants it and
 * what to do, then a key or a sign-in, and nothing to label, because Farik labelled every tool.
 * Keys live in this component's state only, typed into password fields, and are dropped once
 * `connector.connect` is sent, whatever it answers.
 */
export function KitConnect({
	agent,
	name,
	service,
	sandboxed,
	onClose,
}: {
	agent: string;
	/** The agent's display name. */
	name: string;
	service: KitService;
	/** Whether sessions run in Docker's sandbox, the one mode where no command of the agent reaches its keys. */
	sandboxed: boolean;
	onClose: (changed: boolean) => void;
}) {
	const { client } = useConnection();
	const signsIn = service.auth === "oauth";
	const [values, setValues] = useState<string[]>(
		service.credentialKeys.map(() => ""),
	);
	const [sign, setSign] = useState<SignIn>({ kind: "asking" });
	const [refused, setRefused] = useState<string>();
	const [busy, setBusy] = useState(false);
	const [done, setDone] = useState<{
		storedIn: "keychain" | "file";
		tools: Record<string, Tag>;
		signedIn: boolean;
	}>();
	const fill = { name, service: service.title };
	const server = { name: service.name, source: "kit" };

	/** What a failed ask or connect says, in Farik's words and never the daemon's. */
	const said = (e: unknown) => {
		const message = refusalsOf(e)[0]?.message ?? "";
		if (codeOf(message) === "connector_not_in_kit")
			return t("kitChanged", fill);
		if (message.startsWith("the server did not answer"))
			return t("addTimeout", { server: service.title });
		return t("kitRefused", fill);
	};
	const connect = async (attempt?: string) => {
		if (!client) return;
		setBusy(true);
		setRefused(undefined);
		const sending = Object.fromEntries(
			service.credentialKeys.map((key, i) => [key, values[i] ?? ""]),
		);
		// The keys leave the page with this call; none is kept for a second try.
		setValues(values.map(() => ""));
		try {
			const answer = (await client.call(
				"connector.connect",
				attempt
					? { agent, server, attempt, tags: {} }
					: { agent, server, keys: sending, tags: {} },
			)) as { storedIn: "keychain" | "file"; tools: Record<string, Tag> };
			setDone({ ...answer, signedIn: Boolean(attempt) });
		} catch (e) {
			setRefused(said(e));
			if (attempt) setSign({ kind: "failed", code: "sign_in_timed_out" });
		}
		setBusy(false);
	};

	// A service that signs in is asked at once, so the button opens its page in the click itself.
	// biome-ignore lint/correctness/useExhaustiveDependencies: once, when the page opens
	useEffect(() => {
		if (!signsIn || !client) return;
		let live = true;
		void (async () => {
			try {
				const answer = (await client.call("connector.sign_in", {
					agent,
					server,
				})) as { attempt: string; authorizeUrl: string; issuer: string };
				if (live)
					setSign({
						kind: "offered",
						attempt: answer.attempt,
						address: answer.authorizeUrl,
						issuer: answer.issuer,
					});
			} catch (e) {
				if (live) {
					setSign({
						kind: "failed",
						code: codeOf(refusalsOf(e)[0]?.message ?? ""),
					});
					setRefused(said(e));
				}
			}
		})();
		return () => {
			live = false;
		};
	}, []);
	// Whether the user has said yes, asked every 2 seconds; yes is connected.
	const waitingFor = sign.kind === "waiting" ? sign.attempt : "";
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
				if (answer.state === "signed_in") return void connect(waitingFor);
				if (answer.state === "failed")
					return setSign({
						kind: "failed",
						code: answer.reason?.code ?? "sign_in_failed",
					});
			} catch {
				// The attempt is gone: it lasts ten minutes.
				if (live) return setSign({ kind: "failed", code: "sign_in_timed_out" });
			}
			if (live) timer = setTimeout(poll, POLL_MS);
		};
		timer = setTimeout(poll, POLL_MS);
		return () => {
			live = false;
			if (timer) clearTimeout(timer);
		};
	}, [waitingFor]);
	const failedWords = (code: string) =>
		t(
			(
				{
					access_denied: "addSignInDenied",
					sign_in_timed_out: "addSignInTimedOut",
					sign_in_mismatch: "addSignInMismatch",
				} as const
			)[code as "access_denied"] ?? "addSignInFailed",
			{ host: service.title },
		);

	const keyHost = hostOf(service.keyPage);
	const file = done?.storedIn === "file";
	const group = (heading: string, tags: Tag[]) => {
		const tools = Object.entries(done?.tools ?? {}).filter(([, tag]) =>
			tags.includes(tag),
		);
		return tools.length === 0 ? null : (
			<div>
				<h3 className={styles.subheading}>{heading}</h3>
				<ul>
					{tools.map(([tool]) => (
						<li key={tool}>{toolSaid(service.labels, tool)}</li>
					))}
				</ul>
			</div>
		);
	};

	return (
		<Dialog
			open
			title={t("kitTitle", fill)}
			onClose={() => onClose(done !== undefined)}
		>
			<Stepper
				steps={[t("kitStepConnect"), t("kitStepDone")]}
				current={done ? 1 : 0}
			/>
			{!done && (
				<>
					<p>{service.about}</p>
					<h3 className={styles.subheading}>{t("kitWhy", fill)}</h3>
					<p>{service.why}</p>
					<h3 className={styles.subheading}>{t("kitWhat")}</h3>
					<p>{service.setup}</p>
					{signsIn ? (
						<>
							{sign.kind === "offered" && (
								<span>
									<Button
										kind="primary"
										onClick={() => {
											window.open(sign.address, "_blank", "noopener");
											setSign({ ...sign, kind: "waiting" });
										}}
									>
										{t("kitSignIn", fill)}
									</Button>
								</span>
							)}
							{sign.kind === "offered" && (
								<p className={styles.muted}>{t("kitSignInNote", fill)}</p>
							)}
							{sign.kind === "waiting" && (
								<>
									<p role="status">{t("kitWaiting", fill)}</p>
									<span>
										<Button
											onClick={() =>
												window.open(sign.address, "_blank", "noopener")
											}
										>
											{t("addOpenAgain")}
										</Button>
									</span>
								</>
							)}
							{sign.kind === "failed" && (
								<p role="alert" className={styles.alert}>
									{failedWords(sign.code)}
								</p>
							)}
						</>
					) : (
						<>
							{service.keyPage && (
								<>
									<p>
										<a
											href={service.keyPage}
											target="_blank"
											rel="noopener noreferrer"
										>
											{t("kitGetKey")}
										</a>
									</p>
									<p className={styles.muted}>
										{t("kitGetKeyNote", { host: keyHost })}
									</p>
								</>
							)}
							{service.credentialKeys.map((key, i) => (
								<TextField
									key={key}
									id={`kit-key-${i}`}
									label={
										service.credentialKeys.length === 1
											? t("kitKeyLabel", fill)
											: t("kitKeyLabelOf", { ...fill, key })
									}
									hint={t("kitKeyHint", fill)}
									type="password"
									value={values[i] ?? ""}
									onChange={(value) =>
										setValues(values.map((old, j) => (j === i ? value : old)))
									}
								/>
							))}
						</>
					)}
					{refused && sign.kind !== "failed" && (
						<p role="alert" className={styles.alert}>
							{refused}
						</p>
					)}
					<div className={styles.actions}>
						{!signsIn && (
							<Button
								kind="primary"
								busy={busy}
								disabled={values.some((value) => !value.trim())}
								onClick={() => connect()}
							>
								{t("kitConnect")}
							</Button>
						)}
						<Button onClick={() => onClose(false)}>{t("agentCancel")}</Button>
					</div>
				</>
			)}
			{done && (
				<>
					<p role="status">
						<strong>{t("kitDone", fill)}</strong>
					</p>
					{group(t("kitCan", fill), ["network"])}
					{group(t("kitAsks", fill), ["external_effect"])}
					{group(t("kitNever"), ["denied"])}
					<p>
						{done.signedIn
							? `${t(file ? "addSignedInFile" : "addSignedInKeychain", {
									name,
									service: service.title,
								})}${sandboxed ? "" : ` ${t("addSignedInNoSandbox", { name })}`}`
							: t(
									file
										? sandboxed
											? "kitFile"
											: "kitFileNoSandbox"
										: sandboxed
											? "kitKeychain"
											: "kitKeychainNoSandbox",
									fill,
								)}
					</p>
					<p>{t("kitUse", fill)}</p>
					<p>{t("kitOnly", fill)}</p>
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
