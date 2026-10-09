import { RpcError } from "@farik/protocol-client";
import { Button, Choice, TextArea, TextField } from "@farik/ui";
import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { said } from "../app/refusals.ts";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";
import type { Mailbox } from "./sellerMail.ts";
import type { Team } from "./setup/TeamSetup.tsx";

type Provider = "gmail" | "icloud" | "fastmail" | "other" | "microsoft";
type Security = "tls" | "starttls";
type Server = { host: string; port: string; security: Security };

/** The servers of a provider Farik knows: reading, then sending. */
const KNOWN: Record<
	Exclude<Provider, "other" | "microsoft">,
	[Server, Server]
> = {
	gmail: [
		{ host: "imap.gmail.com", port: "993", security: "tls" },
		{ host: "smtp.gmail.com", port: "465", security: "tls" },
	],
	icloud: [
		{ host: "imap.mail.me.com", port: "993", security: "tls" },
		{ host: "smtp.mail.me.com", port: "587", security: "starttls" },
	],
	fastmail: [
		{ host: "imap.fastmail.com", port: "993", security: "tls" },
		{ host: "smtp.fastmail.com", port: "465", security: "tls" },
	],
};

const NONE: Server = { host: "", port: "", security: "tls" };

/** Where a provider explains its app passwords. */
const HELP: Record<string, string> = {
	gmail: "https://support.google.com/accounts/answer/185833",
	icloud: "https://support.apple.com/102654",
	fastmail: "https://www.fastmail.help/hc/articles/360058752854",
};

/** The steps of a provider's app password, in Farik's words. */
const STEPS: Record<string, Parameters<typeof t>[0][]> = {
	gmail: ["mailboxHowGmail1", "mailboxHowGmail2", "mailboxHowGmail3"],
	icloud: ["mailboxHowIcloud1", "mailboxHowIcloud2", "mailboxHowIcloud3"],
	fastmail: [
		"mailboxHowFastmail1",
		"mailboxHowFastmail2",
		"mailboxHowFastmail3",
	],
};
const NOTE: Record<string, Parameters<typeof t>[0]> = {
	gmail: "mailboxHowGmail",
	fastmail: "mailboxHowFastmail",
	other: "mailboxHowOther",
};
const PROVIDER_NAME: Record<string, string> = {
	gmail: "Gmail",
	icloud: "iCloud",
	fastmail: "Fastmail",
};

/** The provider an address names by its domain, for the domains Farik knows. */
const DOMAINS: Record<string, Provider> = {
	"gmail.com": "gmail",
	"googlemail.com": "gmail",
	"icloud.com": "icloud",
	"me.com": "icloud",
	"mac.com": "icloud",
	"fastmail.com": "fastmail",
	"fastmail.fm": "fastmail",
	"outlook.com": "microsoft",
	"hotmail.com": "microsoft",
	"live.com": "microsoft",
	"msn.com": "microsoft",
};

/** Whether a server is named: a host and a port that is a number from 1 to 65535. */
const named = (server: Server) =>
	server.host.trim() !== "" &&
	/^\d+$/.test(server.port.trim()) &&
	Number(server.port) >= 1 &&
	Number(server.port) <= 65535;

/** The code a daemon refusal leads with, as `code: words`. */
const codeOfMessage = (e: unknown) =>
	/^([a-z_]+): /.exec(e instanceof Error ? e.message : "")?.[1];

/** JSON-RPC's code for params that do not fit the method's schema. */
const INVALID_PARAMS = -32602;

/** A refused connect in plain words: params the schema refuses carry no code, and are settings that do not fit. */
const refusedWords = (e: unknown) =>
	e instanceof RpcError && e.code === INVALID_PARAMS
		? said("mailbox_settings_invalid")
		: said(codeOfMessage(e), {}, "refuseCommand");

/** One server's three fields. */
function ServerFields({
	id,
	title,
	server,
	onChange,
	locked,
}: {
	id: string;
	title: string;
	server: Server;
	onChange: (server: Server) => void;
	locked: boolean;
}) {
	return (
		<fieldset className={styles.orderForm} disabled={locked}>
			<legend>{title}</legend>
			<TextField
				id={`${id}-host`}
				label={t("mailboxHost")}
				value={server.host}
				onChange={(host) => onChange({ ...server, host })}
			/>
			<TextField
				id={`${id}-port`}
				label={t("mailboxPort")}
				value={server.port}
				onChange={(port) => onChange({ ...server, port })}
			/>
			<Choice<Security>
				name={`${id}-security`}
				legend={t("mailboxEncryption")}
				value={server.security}
				onChange={(security) => onChange({ ...server, security })}
				options={[
					{ value: "tls", label: t("mailboxTls") },
					{ value: "starttls", label: t("mailboxStarttls") },
				]}
			/>
		</fieldset>
	);
}

/**
 * "Connect a procurement mailbox" (spec 6.10): the address the Procurement Specialist writes
 * from and Farik reads replies at, an app password, and the servers. Connecting signs in to both
 * servers and sends nothing; the password is sent once and dropped from this page.
 */
export function ProcurementMailbox() {
	const { id } = useParams();
	const navigate = useNavigate();
	const { client } = useConnection();
	const { data: teamData } = useQuery<{ team: Team }>("team.get", {});
	const { data: current } = useQuery<Mailbox>("procurement_mailbox.get", {});
	const agent = teamData?.team.agents.find((a) => a.id === id);
	const name = agent?.displayName ?? id ?? "";
	const [address, setAddress] = useState("");
	const [yourName, setYourName] = useState("");
	const [provider, setProvider] = useState<Provider>("gmail");
	const [username, setUsername] = useState<string>();
	const [password, setPassword] = useState("");
	const [imap, setImap] = useState<Server>(KNOWN.gmail[0]);
	const [smtp, setSmtp] = useState<Server>(KNOWN.gmail[1]);
	const [folder, setFolder] = useState("INBOX");
	const [signature, setSignature] = useState("");
	const [disclose, setDisclose] = useState(true);
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const [filled, setFilled] = useState(false);
	// The mailbox already connected fills the form once, for "Change".
	useEffect(() => {
		if (filled || !current?.connected) return;
		setFilled(true);
		setAddress(current.address ?? "");
		setYourName(current.name ?? "");
		setFolder(current.folder ?? "INBOX");
		setSignature(current.signature ?? "");
		setDisclose(current.discloseAi !== false);
		const known = current.provider as Provider | undefined;
		if (known) choose(known);
	});
	const choose = (next: Provider) => {
		setProvider(next);
		if (next === "other") {
			setImap({ ...NONE });
			setSmtp({ ...NONE });
		} else if (next !== "microsoft") {
			setImap(KNOWN[next][0]);
			setSmtp(KNOWN[next][1]);
		}
	};
	// The domain of an address Farik knows chooses the provider; any other leaves the choice be.
	const typeAddress = (next: string) => {
		setAddress(next);
		const found = next.includes("@")
			? DOMAINS[
					next
						.slice(next.lastIndexOf("@") + 1)
						.trim()
						.toLowerCase()
				]
			: undefined;
		if (found && found !== provider) choose(found);
	};
	const microsoft = provider === "microsoft";
	const known = provider !== "other" && provider !== "microsoft";
	const connect = async () => {
		if (!client || microsoft) return;
		setBusy(true);
		setRefusal(undefined);
		const server = (one: Server) => ({
			host: one.host.trim(),
			port: Number(one.port),
			security: one.security,
		});
		try {
			await client.call("procurement_mailbox.connect", {
				address: address.trim(),
				name: yourName.trim(),
				provider,
				imap: server(imap),
				smtp: server(smtp),
				username: (username ?? address).trim(),
				password,
				folder: folder.trim() || "INBOX",
				signature: signature.trim(),
				discloseAi: disclose,
			});
			navigate(`/team/${id}`);
		} catch (e) {
			setRefusal(refusedWords(e));
		} finally {
			// The password is not kept here once it has been sent, whatever the answer.
			setPassword("");
			setBusy(false);
		}
	};
	return (
		<div className={styles.page}>
			<p>
				<Link to={`/team/${id}`}>{t("mailboxBack", { name })}</Link>
			</p>
			<h1>{t("mailboxPageTitle")}</h1>
			<p>{t("mailboxLead", { name })}</p>
			<p role="note">{t("mailboxAlias")}</p>
			<p>{t("mailboxSendAs")}</p>
			<div className={styles.orderForm}>
				<TextField
					id="mailbox-address"
					label={t("mailboxAddress")}
					value={address}
					onChange={typeAddress}
				/>
				<TextField
					id="mailbox-name"
					label={t("mailboxName")}
					value={yourName}
					onChange={setYourName}
				/>
				<Choice<Provider>
					name="mailbox-provider"
					legend={t("mailboxProvider")}
					value={provider}
					onChange={choose}
					options={[
						{ value: "gmail", label: t("mailboxGmail") },
						{ value: "icloud", label: t("mailboxIcloud") },
						{ value: "fastmail", label: t("mailboxFastmail") },
						{ value: "other", label: t("mailboxOther") },
						{
							value: "microsoft",
							label: `${t("mailboxMicrosoftChoice")} (${t("mailboxSoon")})`,
						},
					]}
				/>
				{microsoft ? (
					<p role="alert">{t("mailboxMicrosoft")}</p>
				) : (
					<>
						<h2>{t("mailboxHowTitle")}</h2>
						{STEPS[provider] && (
							<ol>
								{STEPS[provider].map((step) => (
									<li key={step}>{t(step)}</li>
								))}
							</ol>
						)}
						{NOTE[provider] && <p>{t(NOTE[provider])}</p>}
						{HELP[provider] && (
							<a
								href={HELP[provider]}
								target="_blank"
								rel="noopener noreferrer"
							>
								{t("mailboxHowLink", {
									provider: PROVIDER_NAME[provider] ?? "",
								})}
							</a>
						)}
						<TextField
							id="mailbox-username"
							label={t("mailboxUsername")}
							value={username ?? address}
							onChange={setUsername}
						/>
						<TextField
							id="mailbox-password"
							type="password"
							label={t("mailboxPassword")}
							hint={t("mailboxPasswordHint", { name })}
							value={password}
							onChange={setPassword}
						/>
						<details>
							<summary>{t("mailboxServers")}</summary>
							<ServerFields
								id="mailbox-imap"
								title={t("mailboxImap")}
								server={imap}
								onChange={setImap}
								locked={known}
							/>
							<ServerFields
								id="mailbox-smtp"
								title={t("mailboxSmtp")}
								server={smtp}
								onChange={setSmtp}
								locked={known}
							/>
							<TextField
								id="mailbox-folder"
								label={t("mailboxFolder")}
								value={folder}
								onChange={setFolder}
							/>
						</details>
						<TextArea
							id="mailbox-signature"
							label={t("mailboxSignature")}
							value={signature}
							onChange={setSignature}
							rows={3}
						/>
						<p className={styles.orderHint}>{t("mailboxSignatureHint")}</p>
						<label className={styles.tick}>
							<input
								type="checkbox"
								checked={disclose}
								onChange={(e) => setDisclose(e.target.checked)}
							/>
							{t("mailboxDisclose")}
						</label>
						{disclose && (
							<p className={styles.orderHint}>
								{t("mailboxDisclosure", { name: yourName })}
							</p>
						)}
						{refusal && <p role="alert">{refusal}</p>}
						<div>
							<Button
								kind="primary"
								busy={busy}
								disabled={
									address.trim() === "" ||
									yourName.trim() === "" ||
									password === "" ||
									!named(imap) ||
									!named(smtp)
								}
								onClick={connect}
							>
								{t("mailboxConnect")}
							</Button>
						</div>
						<p className={styles.orderHint}>
							{busy ? t("mailboxChecking") : t("mailboxSendsNothing")}
						</p>
					</>
				)}
			</div>
		</div>
	);
}
