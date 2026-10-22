import { Button, InfoTip } from "@catervas/ui";
import { useState } from "react";
import { useNavigate } from "react-router";
import { useHub } from "../../app/connection.tsx";
import { commandSaid } from "../../app/refusals.ts";
import { t } from "../../strings/t.ts";
import styles from "./find.module.css";

/** What is read, kept and never touched, word for word from the mockup. */
export function WhatIsRead() {
	return (
		<>
			<p>
				<strong>{t("findWeRead")}</strong> {t("findWeReadWhat")}
			</p>
			<p>
				<strong>{t("findWeKeep")}</strong> {t("findWeKeepWhat")}
			</p>
			<p>
				<strong>{t("findNeverKept")}</strong> {t("findNeverKeptWhat")}
			</p>
			<p>
				<strong>{t("findNeverRead")}</strong> {t("findNeverReadWhat")}
			</p>
			<p>{t("findNamesNote")}</p>
			<p>{t("findPauseNote")}</p>
		</>
	);
}

/** Starts the scan and goes to its page; `setup` says it was asked for from the setup's pages. */
export function useStartLooking(setup: boolean) {
	const { client } = useHub();
	const navigate = useNavigate();
	const [busy, setBusy] = useState(false);
	const [said, setSaid] = useState<string>();
	const start = async () => {
		if (!client) return;
		setBusy(true);
		setSaid(undefined);
		try {
			await client.call("finding.start", {});
			navigate("/find/scanning", { state: { setup } });
		} catch (e) {
			setSaid(
				commandSaid(e instanceof Error ? e.message : "", {}) || t("findStart"),
			);
		} finally {
			setBusy(false);
		}
	};
	return { start, busy, said };
}

/** The computer check's offer to look through Claude Code's history (journey A, step 1). */
export function FindOffer(props: { onSkip: () => void }) {
	const { start, busy, said } = useStartLooking(true);
	return (
		<section className={styles.card} aria-labelledby="find-offer">
			<h2 id="find-offer">{t("findOfferTitle")}</h2>
			<h3>
				{t("findOfferAsk")}{" "}
				<InfoTip id="find-offer-read" label={t("findOfferInfoLabel")}>
					<WhatIsRead />
				</InfoTip>
			</h3>
			<ul>
				<li>{t("findOfferRead")}</li>
				<li>{t("findOfferList")}</li>
				<li>{t("findOfferLocal")}</li>
			</ul>
			<div className={styles.actions}>
				<Button kind="primary" busy={busy} onClick={start}>
					{t("findOfferLook")}
				</Button>
				<Button onClick={props.onSkip}>{t("findOfferSkip")}</Button>
			</div>
			{said && <p role="alert">{said}</p>}
		</section>
	);
}
