import { Button, Dialog, Stepper, TextField } from "@catervas/ui";
import { useEffect, useState } from "react";
import { useConnection } from "../app/connection.tsx";
import { refusalsOf } from "../app/refusals.ts";
import { t } from "../strings/t.ts";
import { AllowanceFields, listed, numberOf, offersOf } from "./allowances.tsx";
import { CodeCard, hostOf, SigningIn, type Tag } from "./ConnectorAdd.tsx";
import styles from "./pages.module.css";
import type { KitService } from "./Team.tsx";

/** How often the page asks whether the user has said yes. */
const POLL_MS = 2000;
/** One of Catervas's own apps signing the user in by a code (phase 7 step 03b): its name and the code to type. */
type AppCode = { provider: string; userCode: string };
/** How signing in to the service stands. */
type SignIn =
	| { kind: "asking" }
	| {
			kind: "offered";
			attempt: string;
			address: string;
			issuer: string;
			app?: AppCode;
	  }
	| {
			kind: "waiting";
			attempt: string;
			address: string;
			issuer: string;
			app?: AppCode;
	  }
	| {
			kind: "signed";
			attempt: string;
			address: string;
			issuer: string;
			app?: AppCode;
	  }
	| { kind: "failed"; code: string };

/** The code a daemon sentence leads with, as `code: words`. */
const codeOf = (message: string) => /^([a-z_]+): /.exec(message)?.[1] ?? "";

/** A tool as the user reads it: the kit's label, else its name with `_` and `-` read as spaces. */
const toolSaid = (labels: Record<string, string>, tool: string) =>
	labels[tool] ?? tool.replace(/[_-]+/g, " ");

/**
 * Connecting a service of the agent's role's kit (ADR 0036): what it is, why the role wants it and
 * what to do, then a key or a sign-in, and nothing to label, because Catervas labelled every tool.
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
		/** The calls each sprint the user gave each spending tool. */
		allowed: Record<string, number>;
	}>();
	// A kit that gives spending tools a number asks how many between the key or sign-in and done.
	const offers = offersOf(service);
	const asksHowMany = offers.length > 0;
	const [step, setStep] = useState<"connect" | "how">("connect");
	const [numbers, setNumbers] = useState<Record<string, string>>(
		Object.fromEntries(
			(service.allowances ?? []).map((offer) => [
				offer.tool,
				String(offer.calls),
			]),
		),
	);
	const [shown, setShown] = useState(false);

	const fill = { name, service: service.title };
	const server = { name: service.name, source: "kit" };

	/** What a failed ask or connect says, in Catervas's words and never the daemon's. */
	const said = (e: unknown) => {
		const message = refusalsOf(e)[0]?.message ?? "";
		if (codeOf(message) === "connector_not_in_kit")
			return t("kitChanged", fill);
		if (message.startsWith("the server did not answer"))
			return t("kitTimeout", { service: service.title });
		return t("kitRefused", fill);
	};
	const connect = async (attempt?: string) => {
		if (!client) return;
		const allowed = Object.fromEntries(
			offers.map((offer) => [offer.tool, numberOf(numbers[offer.tool] ?? "")]),
		);
		if (asksHowMany && Object.values(allowed).some((n) => n === undefined)) {
			setShown(true);
			return;
		}
		const allowances = asksHowMany
			? { allowances: allowed as Record<string, number> }
			: {};
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
					? { agent, server, attempt, ...allowances, tags: {} }
					: { agent, server, keys: sending, ...allowances, tags: {} },
			)) as { storedIn: "keychain" | "file"; tools: Record<string, Tag> };
			setDone({
				...answer,
				signedIn: Boolean(attempt),
				allowed: (allowances.allowances ?? {}) as Record<string, number>,
			});
		} catch (e) {
			setRefused(said(e));
			if (attempt) setSign({ kind: "failed", code: "" });
			// A refusal of the numbers or the key is mended where it was typed.
			if (asksHowMany && !attempt) setStep("connect");
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
				})) as {
					attempt: string;
					authorizeUrl: string;
					issuer: string;
					provider?: string;
					userCode?: string;
				};
				if (live)
					setSign({
						kind: "offered",
						attempt: answer.attempt,
						address: answer.authorizeUrl,
						issuer: answer.issuer,
						...(answer.userCode && {
							app: {
								provider: answer.provider ?? service.title,
								userCode: answer.userCode,
							},
						}),
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
				if (answer.state === "signed_in") {
					if (!asksHowMany || sign.kind !== "waiting")
						return void connect(waitingFor);
					setSign({ ...sign, kind: "signed" });
					return setStep("how");
				}
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
	/**
	 * Closes the dialog and, with it, the sign-in under way, which the daemon stops waiting for, so
	 * that a yes given on the service's page after the user left is never kept. Best effort: an
	 * attempt that is gone is already ended.
	 */
	const leave = () => {
		if (client && !done && "attempt" in sign)
			void client
				.call("connector.sign_in_cancel", { attempt: sign.attempt })
				.catch(() => {});
		onClose(done !== undefined);
	};
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
	const steps = asksHowMany
		? [t("allowStepConnect"), t("allowStepHowMany"), t("allowStepDone")]
		: [t("kitStepConnect"), t("kitStepDone")];
	// A spending tool given a number runs up to it unasked; beyond it, and at 0, it asks.
	const given = offers.filter((offer) => (done?.allowed[offer.tool] ?? 0) > 0);
	const group = (heading: string, items: string[]) =>
		items.length === 0 ? null : (
			<div>
				<h3 className={styles.subheading}>{heading}</h3>
				<ul>
					{items.map((item) => (
						<li key={item}>{item}</li>
					))}
				</ul>
			</div>
		);
	const toolsSaid = (tags: Tag[], skip: string[] = []) =>
		Object.entries(done?.tools ?? {})
			.filter(([tool, tag]) => tags.includes(tag) && !skip.includes(tool))
			.map(([tool]) => toolSaid(service.labels, tool));

	return (
		<Dialog open title={t("kitTitle", fill)} onClose={leave}>
			<Stepper
				steps={steps}
				current={done ? steps.length - 1 : step === "how" ? 1 : 0}
			/>
			{!done && step === "how" && (
				<>
					{signsIn && (
						<p role="status">
							<strong>{t("allowSignedIn", { service: service.title })}</strong>
						</p>
					)}
					<h3 className={styles.subheading}>{t("allowQuestion", fill)}</h3>
					<p>{t("allowSpends", { name, service: service.title })}</p>
					<AllowanceFields
						offers={offers}
						values={numbers}
						shown={shown}
						onChange={(tool, text) => setNumbers({ ...numbers, [tool]: text })}
					/>
					<p className={styles.muted}>{t("allowRange", fill)}</p>
					<p className={styles.muted}>{t("allowAlwaysAsk")}</p>
					<p className={styles.muted}>{t("allowDay")}</p>
					{refused && (
						<p role="alert" className={styles.alert}>
							{refused}
						</p>
					)}
					<div className={styles.actions}>
						<Button
							kind="primary"
							busy={busy}
							onClick={() =>
								connect(sign.kind === "signed" ? sign.attempt : undefined)
							}
						>
							{t("kitConnect")}
						</Button>
						<Button
							onClick={() => {
								setShown(false);
								// The sign-in is done; going back offers its page again.
								if (sign.kind === "signed")
									setSign({ ...sign, kind: "offered" });
								setStep("connect");
							}}
						>
							{t("allowBack")}
						</Button>
					</div>
				</>
			)}
			{!done && step === "connect" && (
				<>
					<p>{service.about}</p>
					<h3 className={styles.subheading}>{t("kitWhy", fill)}</h3>
					<p>{service.why}</p>
					<h3 className={styles.subheading}>{t("kitWhat")}</h3>
					<p>{service.setup}</p>
					{signsIn ? (
						<>
							{sign.kind === "offered" && (
								<SigningIn app={Boolean(sign.app)}>
									<span>
										<Button
											kind="primary"
											onClick={() => {
												// A code is typed on a page the user opens from the next board.
												if (!sign.app)
													window.open(sign.address, "_blank", "noopener");
												setSign({ ...sign, kind: "waiting" });
											}}
										>
											{t("kitSignIn", fill)}
										</Button>
									</span>
									<p className={styles.muted}>
										{sign.app
											? t("addSignInCodeNote", { provider: sign.app.provider })
											: t("kitSignInNote", fill)}
									</p>
								</SigningIn>
							)}
							{sign.kind === "waiting" && sign.app && (
								<CodeCard
									provider={sign.app.provider}
									userCode={sign.app.userCode}
									address={sign.address}
								/>
							)}
							{sign.kind === "waiting" && !sign.app && (
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
							{sign.kind === "failed" && !refused && (
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
					{refused && (
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
								onClick={() => (asksHowMany ? setStep("how") : connect())}
							>
								{t(asksHowMany ? "allowNext" : "kitConnect")}
							</Button>
						)}
						<Button onClick={leave}>{t("agentCancel")}</Button>
					</div>
				</>
			)}
			{done && (
				<>
					<p role="status">
						<strong>{t("kitDone", fill)}</strong>
					</p>
					{group(t("kitCan", fill), [
						...toolsSaid(["network"]),
						...(given.length > 0
							? [
									t("allowMakeUpTo", {
										list: listed(
											given.map(
												(offer) => `${done?.allowed[offer.tool]} ${offer.what}`,
											),
										),
									}),
								]
							: []),
					])}
					{group(t("kitAsks", fill), [
						...(given.length > 0 ? [t("allowMore")] : []),
						...toolsSaid(
							["external_effect"],
							given.map((offer) => offer.tool),
						),
					])}
					{group(t("kitNever"), toolsSaid(["denied"]))}
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
