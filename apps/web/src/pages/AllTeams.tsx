import { connect } from "@catervas/protocol-client";
import { Button, InfoTip } from "@catervas/ui";
import { useState } from "react";
import { Link, useLocation, useNavigate } from "react-router";
import { useHub, useSocketFactory } from "../app/connection.tsx";
import { commandSaid, saidAll } from "../app/refusals.ts";
import type { TeamRow } from "../app/teams.ts";
import { useTeamList } from "../app/useTeamList.ts";
import { sentence } from "../app/words.ts";
import { t } from "../strings/t.ts";
import styles from "./AllTeams.module.css";
import { AllTeamsFinding } from "./find/AllTeamsFinding.tsx";
import { ExportCard } from "./find/ExportCard.tsx";
import { findingSaid, useFoundList } from "./find/finding.ts";
import { dollars } from "./Gate.tsx";
import reminderStyles from "./NeedsYou.module.css";
import { useDigest } from "./reminders.ts";

/** What a team's badge says. */
function badge(team: TeamRow): string {
	if (team.status === "elsewhere") return t("teamsBadgeElsewhere");
	if (team.status === "missing") return t("teamsBadgeMissing");
	if (team.status === "failed") return t("teamsBadgeStopped");
	if (team.waiting > 0) return t("teamsBadgeWaiting", { count: team.waiting });
	if (team.workingAgents > 0) return t("teamsBadgeWorking");
	return team.status === "paused" ? t("teamsBadgePaused") : "";
}

/** All teams: a card for each team on this computer, and what waits on the owner (ADR 0059). */
export function AllTeams() {
	const { list, again } = useTeamList();
	const hub = useHub();
	const factory = useSocketFactory();
	const navigate = useNavigate();
	const created = (useLocation().state as { created?: number } | null)?.created;
	const { list: found, again: lookAgain } = useFoundList(undefined);
	const { digest } = useDigest();
	const [asking, setAsking] = useState<string>();
	const [undoing, setUndoing] = useState<string>();
	const [said, setSaid] = useState<Record<string, string>>({});
	const [busy, setBusy] = useState<string>();

	const say = (slug: string, words: string | undefined) =>
		setSaid((all) => {
			const next = { ...all };
			if (words === undefined) delete next[slug];
			else next[slug] = words;
			return next;
		});
	// Sends `command` to the team on a socket of its own, which closes after: the page has no
	// connection to it.
	const send = async (
		slug: string,
		command: "team_resume" | "catervafication_start",
	) => {
		setBusy(slug);
		say(slug, undefined);
		const url = `ws://${location.host}/t/${slug}/rpc`;
		const client = connect(url, factory(url));
		try {
			const reply = await client.command({ command, body: {} });
			if ("error" in reply)
				say(
					slug,
					commandSaid(reply.error.detail, {}) || sentence(reply.error.detail),
				);
		} catch (e) {
			say(slug, saidAll(e));
		} finally {
			client.close();
			setBusy(undefined);
			again();
		}
	};
	const start = (slug: string) => send(slug, "team_resume");
	const undo = async (slug: string) => {
		setBusy(slug);
		say(slug, undefined);
		try {
			const answer = (await hub.client?.call("teams.undo", { slug })) as
				| { left?: string }
				| undefined;
			setUndoing(undefined);
			if (answer?.left) say(slug, answer.left);
		} catch (e) {
			say(slug, findingSaid(e, "refuseOther"));
		} finally {
			setBusy(undefined);
			again();
			lookAgain();
		}
	};
	const choose = async (key: string, choice: "team" | "ignore") => {
		try {
			await hub.client?.call("finding.choose", { key, choice });
		} catch {}
		if (choice === "team") navigate("/find/teams");
		else lookAgain();
	};
	const watched = (found?.main ?? []).filter(
		(row) => (row.choice ?? row.suggested) === "watch" && !row.alreadyTeam,
	);
	const remove = async (slug: string) => {
		setBusy(slug);
		say(slug, undefined);
		try {
			await hub.client?.call("teams.remove", { slug, confirmed: true });
			setAsking(undefined);
		} catch (e) {
			say(slug, saidAll(e));
		} finally {
			setBusy(undefined);
			again();
		}
	};

	const waiting = list?.teams.filter((team) => team.waiting > 0) ?? [];
	const asleep = list?.accountAsleepUntil
		? new Date(list.accountAsleepUntil)
				.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" })
				.toLowerCase()
		: undefined;
	return (
		<div className={styles.page}>
			<h1 className={styles.title}>{t("allTeams")}</h1>
			{asleep && <p role="status">{t("teamsAsleep", { time: asleep })}</p>}
			{created !== undefined && created > 0 && (
				<p role="status">
					{created === 1
						? t("teamsCreatedOne")
						: t("teamsCreated", { count: created })}
				</p>
			)}
			{digest && digest.count > 0 && (
				<section
					className={reminderStyles.band}
					aria-label={t(
						digest.count === 1 ? "needsYouBandOne" : "needsYouBand",
						{
							count: digest.count,
						},
					)}
				>
					<p>
						<strong>
							{t(digest.count === 1 ? "needsYouBandOne" : "needsYouBand", {
								count: digest.count,
							})}
						</strong>{" "}
						{digest.headline}
					</p>
					<Link to="/needs-you">{t("needsYouSeeAll")}</Link>
				</section>
			)}
			<AllTeamsFinding />
			<ExportCard />
			{list?.teams.length === 0 && <p>{t("teamsNone")}</p>}
			<div className={styles.cards}>
				{list?.teams.map((team) => (
					<article key={team.slug} className={styles.card}>
						<h2>
							<Link to={`/t/${team.slug}/`}>{team.name}</Link>
						</h2>
						{badge(team) && <p className={styles.badge}>{badge(team)}</p>}
						{team.status === "failed" && team.failed && (
							<p className={styles.muted}>{team.failed}</p>
						)}
						<ul className={styles.facts}>
							<li>
								{t(
									team.workingAgents === 1
										? "teamsAgentsOne"
										: "teamsAgentsMany",
									{
										count: team.workingAgents,
									},
								)}
							</li>
							<li>{t("teamsToday", { amount: dollars(team.todayUsd) })}</li>
							<li>{t("teamsMonth", { amount: dollars(team.monthUsd) })}</li>
						</ul>
						{team.status === "paused" &&
							(undoing === team.slug ? (
								<div>
									<p>{t("teamsUndoConfirm", { name: team.name })}</p>
									<Button
										busy={busy === team.slug}
										onClick={() => undo(team.slug)}
									>
										{t("teamsUndoYes")}
									</Button>{" "}
									<Button onClick={() => setUndoing(undefined)}>
										{t("agentCancel")}
									</Button>
								</div>
							) : (
								<div className={styles.actions}>
									<Button
										busy={busy === team.slug}
										onClick={() => start(team.slug)}
									>
										{t("teamStart")}
									</Button>
									{team.undoable && (
										<>
											<Button
												disabled={busy === team.slug}
												onClick={() => send(team.slug, "catervafication_start")}
											>
												{t("teamsDocument")}
											</Button>
											<InfoTip
												id={`document-${team.slug}`}
												label={t("teamsDocumentMore")}
											>
												<ul>
													<li>{t("teamsDocumentRead")}</li>
													<li>{t("teamsDocumentApprove")}</li>
													<li>{t("teamsDocumentThen")}</li>
												</ul>
											</InfoTip>
											<Button
												kind="quiet"
												disabled={busy === team.slug}
												onClick={() => setUndoing(team.slug)}
											>
												{t("teamsUndo")}
											</Button>
										</>
									)}
								</div>
							))}
						{team.status === "missing" &&
							(asking === team.slug ? (
								<div>
									<p>{t("teamRemoveConfirm", { name: team.name })}</p>
									<Button
										busy={busy === team.slug}
										onClick={() => remove(team.slug)}
									>
										{t("teamRemoveYes")}
									</Button>{" "}
									<Button onClick={() => setAsking(undefined)}>
										{t("agentCancel")}
									</Button>
								</div>
							) : (
								<div>
									<Button onClick={() => setAsking(team.slug)}>
										{t("teamRemove")}
									</Button>
								</div>
							))}
						{said[team.slug] && <p role="alert">{said[team.slug]}</p>}
					</article>
				))}
			</div>
			{watched.length > 0 && (
				<section aria-labelledby="watched-heading" className={styles.waiting}>
					<h2 id="watched-heading">{t("teamsWatchedTitle")}</h2>
					<ul>
						<li>{t("teamsWatchedReminders")}</li>
						<li>{t("teamsWatchedNothing")}</li>
					</ul>
					<ul>
						{watched.map((row) => (
							<li key={row.key}>
								<span>{row.name}</span>{" "}
								<Button kind="quiet" onClick={() => choose(row.key, "ignore")}>
									{t("teamsNotMine")}
								</Button>{" "}
								<Button onClick={() => choose(row.key, "team")}>
									{t("teamsMakeATeam")}
								</Button>
							</li>
						))}
					</ul>
				</section>
			)}
			{waiting.length > 0 && (
				<section aria-labelledby="waiting-heading" className={styles.waiting}>
					<h2 id="waiting-heading">{t("teamsWaitingTitle")}</h2>
					<ul>
						{waiting.map((team) => (
							<li key={team.slug}>
								<Link to={`/t/${team.slug}/`}>
									{t(
										team.waiting === 1 ? "teamsWaitingOne" : "teamsWaitingMany",
										{
											name: team.name,
											count: team.waiting,
										},
									)}
								</Link>
							</li>
						))}
					</ul>
				</section>
			)}
		</div>
	);
}
