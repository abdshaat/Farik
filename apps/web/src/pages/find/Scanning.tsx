import { Button } from "@catervas/ui";
import { useState } from "react";
import { Link, useLocation, useNavigate } from "react-router";
import { useHub } from "../../app/connection.tsx";
import { t } from "../../strings/t.ts";
import { Wizard } from "../setup/Wizard.tsx";
import { useStartLooking } from "./FindOffer.tsx";
import styles from "./find.module.css";
import {
	counted,
	type FindingStatus,
	findingSaid,
	useFindingStatus,
} from "./finding.ts";

/** How long is left, in words. */
function timeLeft(seconds: number): string {
	if (seconds < 45) return t("scanLeftSeconds");
	if (seconds < 90) return t("scanLeftMinute");
	return t("scanLeftMinutes", { minutes: Math.round(seconds / 60) });
}

/** The three counts of a scan. */
export function Counts(props: { status: FindingStatus }) {
	const { projects, jobs, sessions } = props.status;
	return (
		<ul className={styles.counts}>
			<li className={styles.count}>
				{counted(projects, t("scanProjectsOne"), (count) =>
					t("scanProjects", { count }),
				)}
			</li>
			<li className={styles.count}>
				{counted(jobs, t("scanJobsOne"), (count) => t("scanJobs", { count }))}
			</li>
			<li className={styles.count}>
				{counted(sessions, t("scanSessionsOne"), (count) =>
					t("scanSessions", { count }),
				)}
			</li>
		</ul>
	);
}

/** Sessions Catervas could not read, said once the scan is done. */
function Unreadable(props: { count: number }) {
	return (
		<section className={styles.card} aria-labelledby="find-unreadable">
			<h2 id="find-unreadable">
				{counted(props.count, t("scanUnreadableOne"), (count) =>
					t("scanUnreadable", { count }),
				)}
			</h2>
			<ul>
				<li>{t("scanUnreadableNewer")}</li>
				<li>{t("scanUnreadableRest")}</li>
				<li>{t("scanUnreadableUpdate")}</li>
			</ul>
			<p>{t("scanUnreadableDone")}</p>
		</section>
	);
}

/** "Looking through your history": live counts, setup meanwhile, and the scan's end (journey A, step 2). */
export function Scanning() {
	const { client } = useHub();
	const navigate = useNavigate();
	const fromSetup =
		(useLocation().state as { setup?: boolean } | null)?.setup === true;
	const { status, again } = useFindingStatus();
	const [said, setSaid] = useState<string>();
	const looking = useStartLooking(fromSetup);
	const stop = async () => {
		setSaid(undefined);
		try {
			await client?.call("finding.stop", {});
		} catch (e) {
			setSaid(findingSaid(e, "refuseOther"));
		}
		again();
	};
	const phase = status?.phase;
	const title =
		phase === "done"
			? t("scanDone")
			: phase === "stopped"
				? t("scanStopped")
				: t("scanningTitle");
	const lead =
		phase === "running" && status?.secondsLeft != null
			? timeLeft(status.secondsLeft)
			: phase === "done"
				? t("scanDoneLead")
				: phase === "stopped"
					? status?.error
						? t("scanFailed")
						: counted(status?.projects ?? 0, t("scanStoppedOne"), (count) =>
								t("scanStoppedLead", { count }),
							)
					: "";
	return (
		<Wizard step={0} title={title} lead={lead} nav={false}>
			{status && phase !== "idle" && <Counts status={status} />}
			{phase === "running" && status && (
				<>
					{status.reading && (
						<p className={styles.reading}>
							<span className={styles.prompt} aria-hidden="true">
								&gt;{" "}
							</span>
							{t("scanReading", { what: status.reading })}
						</p>
					)}
					<p className={styles.muted}>
						{t("scanMeanwhile")} {t("scanTellYou")}
					</p>
					<div className={styles.actions}>
						{fromSetup ? (
							<Button onClick={() => navigate("/setup/computer")}>
								{t("scanContinueSetup")}
							</Button>
						) : (
							<Button onClick={() => navigate("/teams")}>
								{t("scanBackToTeams")}
							</Button>
						)}
						<Button onClick={stop}>{t("scanStop")}</Button>
					</div>
					<ul className={styles.muted}>
						<li>{t("scanKept")}</li>
						<li>{t("scanPickUp")}</li>
					</ul>
				</>
			)}
			{phase === "done" && status && (
				<>
					{status.unreadable > 0 && <Unreadable count={status.unreadable} />}
					<div className={styles.actions}>
						<Button kind="primary" onClick={() => navigate("/find/found")}>
							{t("scanSee")}
						</Button>
					</div>
				</>
			)}
			{phase === "stopped" && (
				<div className={styles.actions}>
					<Button kind="primary" busy={looking.busy} onClick={looking.start}>
						{t("scanKeepLooking")}
					</Button>
					<Link to="/find/found">{t("scanSeeFound")}</Link>
				</div>
			)}
			{phase === "idle" && (
				<>
					<p>{t("scanNone")}</p>
					<div className={styles.actions}>
						<Button kind="primary" busy={looking.busy} onClick={looking.start}>
							{t("findOfferLook")}
						</Button>
					</div>
				</>
			)}
			{(said || looking.said) && <p role="alert">{said ?? looking.said}</p>}
		</Wizard>
	);
}
