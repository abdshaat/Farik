import { useSyncExternalStore } from "react";
import { Navigate, NavLink, Outlet, useLocation } from "react-router";
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

/** Whether the screen is 1024 px or wider: the rail, else the top and bottom bars. */
function useWide(): boolean {
	return useSyncExternalStore(subscribe, () => matchMedia(WIDE).matches);
}

export function Shell() {
	const wide = useWide();
	const { status } = useConnection();
	const { data } = useQuery<ServeStatus>("serve.status", {});
	const home = useLocation().pathname === "/";
	// "/" shows nothing, and asks nothing of the project, until Farik says where it stands.
	if (!data && home) return null;
	// During setup, every path goes where "/" would.
	if (data && landing(data) !== "/")
		return <Navigate to={landing(data)} replace />;
	const places = (
		<ul className={styles.places}>
			<li>
				<NavLink to="/" end>
					{t("today")}
				</NavLink>
			</li>
			<li>
				<NavLink to="/team">{t("team")}</NavLink>
			</li>
			<li>
				<NavLink to="/settings">{t("settings")}</NavLink>
			</li>
		</ul>
	);
	const pause = data && <PauseControl paused={data.paused} short={!wide} />;
	return (
		<div className={wide ? styles.wide : styles.narrow}>
			{wide ? (
				<header className={styles.rail}>
					<p className={styles.logo}>{t("brand")}</p>
					<nav aria-label={t("navRail")}>{places}</nav>
					<div className={styles.foot}>
						<p className={styles.conn}>
							<span className={styles.dot} aria-hidden="true" />
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
			</main>
			{!wide && (
				<nav aria-label={t("navBar")} className={styles.bar}>
					{places}
				</nav>
			)}
		</div>
	);
}
