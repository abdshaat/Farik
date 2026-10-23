import { Button } from "@catervas/ui";
import { useState } from "react";
import { Link } from "react-router";
import { t } from "../../strings/t.ts";
import { useStartLooking, WhatIsRead } from "./FindOffer.tsx";
import styles from "./find.module.css";
import {
	counted,
	FIND_NOT_NOW,
	FOUND_SEEN,
	keep,
	kept,
	useFindingStatus,
	useFoundList,
} from "./finding.ts";

/**
 * What All teams says of finding projects: the offer while the index is empty, a scan that stopped
 * halfway, and the projects found since the owner last opened the found list.
 */
export function AllTeamsFinding() {
	const { status } = useFindingStatus();
	const { list } = useFoundList(status?.version);
	const looking = useStartLooking(false);
	const [notNow, setNotNow] = useState(kept(FIND_NOT_NOW) !== undefined);
	if (!status) return null;

	if (!status.indexExists && !status.paused && status.phase === "idle") {
		if (notNow) return null;
		return (
			<section className={styles.card} aria-labelledby="find-more">
				<h2 id="find-more">{t("findMoreTitle")}</h2>
				<WhatIsRead />
				<ul>
					<li>{t("findOfferRead")}</li>
					<li>{t("findMoreChoose")}</li>
					<li>{t("findOfferLocal")}</li>
				</ul>
				<div className={styles.actions}>
					<Button kind="primary" busy={looking.busy} onClick={looking.start}>
						{t("findOfferLook")}
					</Button>
					<Button
						onClick={() => {
							keep(FIND_NOT_NOW, new Date().toISOString());
							setNotNow(true);
						}}
					>
						{t("findNotNow")}
					</Button>
				</div>
				{looking.said && <p role="alert">{looking.said}</p>}
			</section>
		);
	}

	if (status.phase === "stopped") {
		return (
			<section className={styles.card} aria-labelledby="find-stopped">
				<h2 id="find-stopped">{t("scanStopped")}</h2>
				<p>
					{counted(status.projects, t("scanStoppedOne"), (count) =>
						t("scanStoppedLead", { count }),
					)}
				</p>
				<div className={styles.actions}>
					<Button kind="primary" busy={looking.busy} onClick={looking.start}>
						{t("scanKeepLooking")}
					</Button>
					<Link to="/find/found">{t("scanSeeFound")}</Link>
				</div>
				{looking.said && <p role="alert">{looking.said}</p>}
			</section>
		);
	}

	// What was found since the owner last opened the found list, and has no answer or team yet.
	const seen = kept(FOUND_SEEN);
	const found = (list?.main ?? []).filter(
		(row) =>
			!row.alreadyTeam &&
			!row.choice &&
			(seen === undefined ||
				(row.lastAt !== null && Date.parse(row.lastAt) > Date.parse(seen))),
	).length;
	if (status.phase !== "running" && found > 0) {
		return (
			<p className={styles.strip}>
				{counted(found, t("findNewFoundOne"), (count) =>
					t("findNewFound", { count }),
				)}
				<Link to="/find/found">{t("findSeeThem")}</Link>
			</p>
		);
	}
	return null;
}
