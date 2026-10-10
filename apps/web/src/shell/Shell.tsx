import icon from "@catervas/brand/assets/icons/icon-48.png";
import { useSyncExternalStore } from "react";
import { Link, Navigate, NavLink, Outlet, useLocation } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { landing } from "../app/landing.ts";
import { type ServeStatus, useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import { PauseControl } from "./PauseControl.tsx";
import styles from "./Shell.module.css";

const WIDE = "(min-width: 1024px)";

function subscribe(changed: () => void) {
	const query = matchMedia(WIDE);
	query.addEventListener("change", changed);
	return () => query.removeEventListener("change", changed);
}

/** The rail's places, in order (steps 09 and 10). */
const PLACES = [
	["/", "today"],
	["/board", "board"],
	["/channel", "chats"],
	["/team", "team"],
	["/costs", "costs"],
	["/settings", "settings"],
] as const;

/** Whether the screen is 1024 px or wider: the rail, else the top and bottom bars. */
export function useWide(): boolean {
	return useSyncExternalStore(subscribe, () => matchMedia(WIDE).matches);
}

export function Shell() {
	const wide = useWide();
	const { status } = useConnection();
	const { data } = useQuery<ServeStatus>("serve.status", {});
	const path = useLocation().pathname;
	// Nothing shows, and nothing is asked of the project, until Catervas says where it stands.
	if (!data) return null;
	// During setup, every path goes where "/" would.
	if (landing(data) !== "/") return <Navigate to={landing(data)} replace />;
	// Settings sits under Team on a phone (web-ui.md), so the bar has five places.
	const places = (
		<ul className={styles.places}>
			{PLACES.filter(([to]) => wide || to !== "/settings").map(([to, word]) => (
				<li key={to}>
					<NavLink to={to} end={to === "/"}>
						{t(word)}
					</NavLink>
				</li>
			))}
		</ul>
	);
	const pause = data && <PauseControl paused={data.paused} short={!wide} />;
	return (
		<div className={wide ? styles.wide : styles.narrow}>
			{wide ? (
				<header className={styles.rail}>
					<p className={styles.logo}>
						<img src={icon} alt="" width="24" height="24" />
						{t("brand")}
					</p>
					<nav aria-label={t("navRail")}>{places}</nav>
					<div className={styles.foot}>
						<p className={styles.conn}>
							<span
								className={`${styles.dot}${status === "open" ? ` ${styles.live}` : ""}`}
								aria-hidden="true"
							/>
							{t(status === "open" ? "connected" : "connecting")}
						</p>
						{data && <p>{t(data.paused ? "teamPaused" : "teamWorking")}</p>}
						{pause}
					</div>
				</header>
			) : (
				<header className={styles.top}>
					<span>{data?.projectRoot?.split(/[\\/]/).at(-1) ?? t("brand")}</span>
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
				<Outlet />
				{!wide && path.startsWith("/team") && (
					<Link to="/settings">{t("settings")}</Link>
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
