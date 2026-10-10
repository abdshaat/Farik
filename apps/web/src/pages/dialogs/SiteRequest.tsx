import { Button, Dialog, TextArea } from "@catervas/ui";
import { useState } from "react";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import { useCommand } from "./StartSprint.tsx";

/** One site an agent asks to read, as `waiting.list` gives it. */
export type SiteAsk = {
	request: number;
	host: string;
	/** The page the agent wants, untrusted. */
	url: string;
	/** The agent's reason, in its own words, untrusted. */
	why: string;
};

/** Whether a host has a part written in another alphabet, which Catervas keeps in its `xn--` form. */
export const isScript = (host: string) =>
	host.split(".").some((part) => part.startsWith("xn--"));

/** The warning for a name written in another alphabet: what it is, and what to check. */
export function ScriptWarning({
	className,
}: {
	className?: string | undefined;
}) {
	return (
		<p className={className} role="note">
			<strong>{t("siteRequestScriptWhat")}</strong>{" "}
			{t("siteRequestScriptCheck")}
		</p>
	);
}

/**
 * "Allow" or "Don't allow" the site an agent asked to read, with a note it is told (spec 6.10):
 * allowing lets it read every page of the site until the owner removes it.
 */
export function SiteRequest({
	ask,
	agent,
	allow,
	onClose,
}: {
	ask: SiteAsk;
	agent: string;
	allow: boolean;
	onClose: () => void;
}) {
	const [note, setNote] = useState("");
	const { busy, refusal, send } = useCommand(onClose);
	const decide = () => {
		const said = note.trim();
		send({
			command: "site_decide",
			body: { request: ask.request, allow, ...(said ? { note: said } : {}) },
		});
	};
	return (
		<Dialog
			open
			fillsPhone
			title={t(allow ? "siteAllowTitle" : "siteDeclineTitle", {
				name: agent,
				host: ask.host,
			})}
			onClose={onClose}
			actions={
				<Button kind="primary" busy={busy} onClick={decide}>
					{t(allow ? "siteRequestAllow" : "siteRequestDecline")}
				</Button>
			}
		>
			<div className={styles.siteRequest}>
				{isScript(ask.host) && <ScriptWarning className={styles.siteScript} />}
				<p>
					{t(allow ? "siteAllowBody" : "siteDeclineBody", {
						name: agent,
						host: ask.host,
					})}
				</p>
				<TextArea
					id="site-request-note"
					label={t("siteRequestNote", { name: agent })}
					value={note}
					onChange={setNote}
				/>
				{refusal && <p role="alert">{refusal}</p>}
			</div>
		</Dialog>
	);
}
