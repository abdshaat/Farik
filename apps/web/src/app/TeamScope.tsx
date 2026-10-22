import { useEffect, useState } from "react";
import { Link, Navigate, Outlet, useLocation, useParams } from "react-router";
import { NotFound } from "../pages/NotFound.tsx";
import { Opening } from "../pages/Opening.tsx";
import { t } from "../strings/t.ts";
import { ConnectionProvider, useConnection } from "./connection.tsx";
import { type ServeStatus, useHubQuery } from "./store.ts";
import {
	isTeamAddress,
	ON_CURRENT,
	redirectTarget,
	rememberTeam,
	type TeamList,
} from "./teams.ts";

const GONE_CHECK_MS = 1500;

/**
 * The team in the address: its own connection, closed and replaced when the address changes team
 * (the `key`). The hub's stays open beside it.
 */
export function TeamScope() {
	const { team = "" } = useParams();
	return (
		<ConnectionProvider key={team} inner path={`/t/${team}/rpc`}>
			<TeamGate team={team} />
		</ConnectionProvider>
	);
}

/** Remembers the team viewed, and leaves a team that is gone or is not driven by this serve. */
function TeamGate({ team }: { team: string }) {
	const { data, again } = useHubQuery<TeamList>("teams.list", {});
	const { status } = useConnection();
	// While this team's socket is down, the hub says whether the team is gone.
	useEffect(() => {
		if (status !== "lost" && status !== "reopening") return;
		const timer = setInterval(again, GONE_CHECK_MS);
		return () => clearInterval(timer);
	}, [status, again]);
	const row = data?.teams.find((one) => one.slug === team);
	useEffect(() => {
		if (row) rememberTeam(team);
	}, [row, team]);
	// A team that was removed: the same page of another, or All teams.
	if (data && !row) return <Navigate to="/" replace />;
	if (row && row.status !== "running" && row.status !== "paused")
		return (
			<main>
				<h1>{row.name}</h1>
				<p role="status">{t("teamNotRunning")}</p>
				<p>
					<Link to="/teams">{t("allTeams")}</Link>
				</p>
			</main>
		);
	return <Outlet />;
}

/** The steps of setup after the project talk to the team just taken on: `/rpc`, the current one. */
export function CurrentScope() {
	return (
		<ConnectionProvider inner path="/rpc">
			<Outlet />
		</ConnectionProvider>
	);
}

/** How long, and how often, the page asks for the team it has just taken on. */
const TAKE_ON_WAIT_MS = 30_000;
const TAKE_ON_EVERY_MS = 500;

/** An address with no `/t/`, an old one or a bookmark: the same page of the team it belongs to. */
export function TeamRedirect() {
	const location = useLocation();
	const { data: list, again } = useHubQuery<TeamList>("teams.list", {});
	const { data: status } = useHubQuery<ServeStatus>("serve.status", {});
	const state = location.state as { team?: string } | null;
	const onCurrent = state?.team === ON_CURRENT.team;
	const [gaveUp, setGaveUp] = useState(false);
	// A project just chosen in setup is a team only once its drive has started, after the hub is
	// back: until it is listed, ask again rather than say there is none.
	const waiting =
		onCurrent &&
		!gaveUp &&
		list?.teams.length === 0 &&
		status !== undefined &&
		!status.takeOnError &&
		!status.leaving;
	useEffect(() => {
		if (!waiting) return;
		const timer = setInterval(again, TAKE_ON_EVERY_MS);
		const stop = setTimeout(() => setGaveUp(true), TAKE_ON_WAIT_MS);
		return () => {
			clearInterval(timer);
			clearTimeout(stop);
		};
	}, [waiting, again]);
	if (!isTeamAddress(location.pathname)) return <NotFound />;
	if (!list || !status || waiting) return <Opening />;
	const address = `${location.pathname}${location.search}${location.hash}`;
	return (
		<Navigate to={redirectTarget(address, list, status, onCurrent)} replace />
	);
}
