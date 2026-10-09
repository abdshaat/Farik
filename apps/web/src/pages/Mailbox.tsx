import { Button, Dialog } from "@farik/ui";
import { useState } from "react";
import { Link } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { said } from "../app/refusals.ts";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import { visibly } from "./dialogs/ToolApproval.tsx";
import { calendarDay } from "./orders.ts";
import styles from "./pages.module.css";
import { type Mailbox, timeWords } from "./sellerMail.ts";

const codeOfMessage = (e: unknown) =>
	/^([a-z_]+): /.exec(e instanceof Error ? e.message : "")?.[1];

/**
 * "Procurement mailbox" on the Procurement Specialist's page (spec 6.10): where it writes from,
 * when Farik last read the replies or why it could not, and "Check now", "Change" and
 * "Disconnect"; or the way to connect one.
 */
export function MailboxSection({ id, name }: { id: string; name: string }) {
	const { client } = useConnection();
	const { data: mailbox, again } = useQuery<Mailbox>(
		"procurement_mailbox.get",
		{},
	);
	const [asking, setAsking] = useState(false);
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const act = async (method: "check" | "disconnect") => {
		if (!client) return;
		setBusy(true);
		setRefusal(undefined);
		try {
			await client.call(`procurement_mailbox.${method}`, {});
			setAsking(false);
			again();
		} catch (e) {
			setRefusal(said(codeOfMessage(e), {}, "refuseCommand"));
		} finally {
			setBusy(false);
		}
	};
	return (
		<section className={styles.section} aria-labelledby="mailbox-heading">
			<h2 id="mailbox-heading">{t("mailboxTitle")}</h2>
			{mailbox?.connected ? (
				<>
					<p>
						{t("mailboxFrom", {
							name,
							address: visibly(mailbox.address ?? ""),
						})}
					</p>
					{mailbox.error ? (
						<p role="alert">
							{t("mailboxError", {
								time: mailbox.checkedAt ? timeWords(mailbox.checkedAt) : "",
								why: visibly(mailbox.error),
							})}
						</p>
					) : (
						<p>
							{mailbox.checkedAt
								? t("mailboxChecked", { time: timeWords(mailbox.checkedAt) })
								: t("mailboxEvery")}
						</p>
					)}
					{mailbox.restartedAt && (
						<p>
							{t("mailboxRestarted", {
								day: calendarDay(mailbox.restartedAt, new Date()),
							})}
						</p>
					)}
					{refusal && <p role="alert">{refusal}</p>}
					<div className={styles.actions}>
						<Button busy={busy} onClick={() => act("check")}>
							{t("mailboxCheckNow")}
						</Button>
						<Link to={`/team/${id}/mailbox`}>{t("mailboxChange")}</Link>
						<Button onClick={() => setAsking(true)}>
							{t("mailboxDisconnect")}
						</Button>
					</div>
					{asking && (
						<Dialog
							open
							fillsPhone
							title={t("mailboxDisconnectTitle", {
								address: visibly(mailbox.address ?? ""),
							})}
							onClose={() => setAsking(false)}
							actions={
								<>
									<Button onClick={() => setAsking(false)}>
										{t("mailboxKeep")}
									</Button>
									<Button
										kind="primary"
										busy={busy}
										onClick={() => act("disconnect")}
									>
										{t("mailboxDisconnect")}
									</Button>
								</>
							}
						>
							<p>{t("mailboxDisconnectBody", { name })}</p>
						</Dialog>
					)}
				</>
			) : (
				<>
					<p>{t("mailboxNone", { name })}</p>
					<Link to={`/team/${id}/mailbox`}>{t("mailboxConnectLink")}</Link>
				</>
			)}
		</section>
	);
}
