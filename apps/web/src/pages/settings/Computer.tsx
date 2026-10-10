import { Button, InfoTip } from "@catervas/ui";
import { useState } from "react";
import { Link } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { type ServeStatus, useQuery } from "../../app/store.ts";
import { Bullets } from "../../components/Bullets.tsx";
import { t } from "../../strings/t.ts";
import { ChangeProject } from "../ChangeProject.tsx";
import styles from "../pages.module.css";

/** This computer: where Catervas runs, the project it works on, and Disconnect for this browser. */
export function Computer() {
	const { disconnect } = useConnection();
	const { data } = useQuery<ServeStatus>("serve.status", {});
	const [leaving, setLeaving] = useState(false);
	const leave = async () => {
		setLeaving(true);
		try {
			await disconnect();
		} catch {
			// The request failed: this browser is still connected, and the button can be tried again.
		} finally {
			setLeaving(false);
		}
	};
	return (
		<div className={styles.page}>
			<section className={styles.section} aria-labelledby="computer-heading">
				<h2 id="computer-heading">{t("thisComputer")}</h2>
				<div className={styles.titled}>
					<strong>{t("connectedLead")}</strong>
					<InfoTip id="connectedBody-info">
						<Bullets text={t("connectedBody")} />
					</InfoTip>
				</div>
				{data && (
					<dl className={styles.facts}>
						<dt>{t("projectFolder")}</dt>
						<dd>{data.projectRoot}</dd>
						<dt>{t("address")}</dt>
						<dd>{`127.0.0.1:${data.port}`}</dd>
					</dl>
				)}
				{data?.projectRoot && <ChangeProject root={data.projectRoot} />}
				<div>
					<Button busy={leaving} onClick={leave}>
						{t("disconnect")}
					</Button>
				</div>
			</section>
			<p>
				<Link to="/events">{t("eventsForTesting")}</Link>
			</p>
		</div>
	);
}
