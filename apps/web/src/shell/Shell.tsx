import icon from "@catervas/brand/assets/icons/icon-48.png";
import { Button } from "@catervas/ui";
import { useCallback, useSyncExternalStore } from "react";
import { Outlet, useLocation, useParams } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { landing } from "../app/landing.ts";
import { Link, Navigate, NavLink, useNavigate } from "../app/links.tsx";
import { type ServeStatus, useQuery } from "../app/store.ts";
import { pageOf } from "../app/teams.ts";
import { Opening } from "../pages/Opening.tsx";
import type { Team } from "../pages/setup/TeamSetup.tsx";
import { t } from "../strings/t.ts";
import { PauseControl } from "./PauseControl.tsx";
import styles from "./Shell.module.css";
import { TeamSwitcher } from "./TeamSwitcher.tsx";

const WIDE = "(min-width: 1024px)";

/** The rail's places, in order (steps 09 and 10). */
const PLACES = [
	["/", "today"],
	["/board", "board"],
	["/channel", "chats"],
	["/files", "files"],
	["/team", "team"],
	["/costs", "costs"],
	["/settings", "settings"],
] as const;

/** The places a phone's bar leaves out; Team's page links to them. */
const UNDER_TEAM: string[] = ["/files", "/settings"];

/** Whether the screen matches the media `query`, and when it starts or stops. */
export function useMedia(query: string): boolean {
	const subscribe = useCallback(
		(changed: () => void) => {
			const list = matchMedia(query);
			list.addEventListener("change", changed);
			return () => list.removeEventListener("change", changed);
		},
		[query],
	);
	return useSyncExternalStore(subscribe, () => matchMedia(query).matches);
}

/** Whether the screen is 1024 px or wider: the rail, else the top and bottom bars. */
export function useWide(): boolean {
	return useMedia(WIDE);
}

/** The places off a team: every team, and this computer's own settings. */
const COMPUTER_PLACES = [
	["/teams", "allTeams"],
	["/computer", "computerPlace"],
] as const;

export function Shell() {
	const wide = useWide();
	const { status, client } = useConnection();
	const navigate = useNavigate();
	// Inside a team the rail is the team's; on All teams and This computer it is the computer's.
	const { team } = useParams();
	const inTeam = team !== undefined;
	const { data, again } = useQuery<ServeStatus>("serve.status", {}, !inTeam);
	// One agent alone is paused and resumed by its name, not as a team (ADR 0071).
	// Asked only once a project is open: before that there is no team.
	const { data: held } = useQuery<{ team: Team }>(
		"team.get",
		{},
		!inTeam || !data?.projectRoot,
	);
	const solo = held?.team?.policy?.solo
		? held.team.agents.find((a) => a.status !== "retired")?.displayName
		: undefined;
	const path = pageOf(useLocation().pathname);
	// The chats fill the window and scroll inside it, so the window itself does not.
	const filled = path === "/channel" || path.startsWith("/channel/");
	// Nothing shows, and nothing is asked of the project, until Catervas says where it stands.
	if (inTeam && !data) return <Opening />;
	// During setup, every path goes where "/" would.
	if (data && landing(data) !== "/")
		return <Navigate to={landing(data)} replace />;
	// Files and Settings sit under Team on a phone (web-ui.md), so the bar has five places.
	const shown = inTeam ? PLACES : COMPUTER_PLACES;
	const places = (
		<ul className={styles.places}>
			{shown
				.filter(([to]) => wide || !UNDER_TEAM.includes(to))
				.map(([to, word]) => (
					<li key={to}>
						{!wide && to === "/team" && path.startsWith("/files") ? (
							// Files sits under Team on a phone: Team stays marked (NavLink marks only its own path).
							<Link to={to} aria-current="page">
								{t(word)}
							</Link>
						) : (
							<NavLink to={to} end={to === "/"}>
								{t(word)}
							</NavLink>
						)}
					</li>
				))}
		</ul>
	);
	const dismissKeys = async (thenTeam: boolean) => {
		try {
			await client?.call("keys_copied.dismiss", {});
			// The dismissal records no event, so nothing else asks the status again.
			again();
			if (thenTeam) navigate("/team");
		} catch {
			// Closed connection or a refusal: the notice stays, so the user can choose again.
		}
	};
	const copied = data?.keysCopied;
	const pause = data && inTeam && (
		<PauseControl paused={data.paused} short={!wide} solo={solo} />
	);
	return (
		<div
			className={`${wide ? styles.wide : styles.narrow}${filled ? ` ${styles.fill}` : ""}`}
		>
			{wide ? (
				<header className={styles.rail}>
					<p className={styles.logo}>
						<img src={icon} alt="" width="24" height="24" />
						{t("brand")}
					</p>
					<TeamSwitcher team={team} />
					<nav aria-label={t("navRail")}>{places}</nav>
					<div className={styles.foot}>
						<p className={styles.conn}>
							<span
								className={`${styles.dot}${status === "open" ? ` ${styles.live}` : ""}`}
								aria-hidden="true"
							/>
							{t(status === "open" ? "connected" : "connecting")}
						</p>
						{data && inTeam && (
							<p>
								{solo
									? t(data.paused ? "soloPaused" : "soloWorking", {
											name: solo,
										})
									: t(data.paused ? "teamPaused" : "teamWorking")}
							</p>
						)}
						{pause}
					</div>
				</header>
			) : (
				<header className={styles.top}>
					<div className={styles.topGroup}>
						<TeamSwitcher team={team} />
					</div>
					{pause}
				</header>
			)}
			<main className={styles.content}>
				{data?.paused && (
					<p className={styles.banner}>
						<strong>{t("pausedLead")}</strong> {t("pausedNothingNew")}
						{wide && ` ${t("pausedStillYours")}`}
					</p>
				)}
				{copied && (
					<div role="status" className={styles.banner}>
						<p>
							{t(copied.count === 1 ? "keysCopiedOne" : "keysCopied", {
								from: copied.from.split(/[\\/]/).at(-1) ?? copied.from,
								count: copied.count,
							})}
						</p>
						<Button onClick={() => dismissKeys(false)}>{t("keysKeep")}</Button>{" "}
						<Button onClick={() => dismissKeys(true)}>{t("keysChoose")}</Button>
					</div>
				)}
				<Outlet />
				{!wide && inTeam && path.startsWith("/team") && (
					<p className={styles.under}>
						<Link to="/files">{t("files")}</Link>
						<Link to="/settings">{t("settings")}</Link>
					</p>
				)}
			</main>
			{!wide && (
				<nav aria-label={t("navBar")} className={styles.bar}>
					{places}
				</nav>
			)}
		</div>
	);
}
