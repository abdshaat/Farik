import { Navigate, Route, Routes } from "react-router";
import { AgentEdit } from "../pages/AgentEdit.tsx";
import { Connect } from "../pages/Connect.tsx";
import { Events } from "../pages/Events.tsx";
import { NotFound } from "../pages/NotFound.tsx";
import { Settings } from "../pages/Settings.tsx";
import { SetupAccount } from "../pages/setup/SetupAccount.tsx";
import { SetupAdvanced } from "../pages/setup/SetupAdvanced.tsx";
import { SetupComputer } from "../pages/setup/SetupComputer.tsx";
import { SetupFinish } from "../pages/setup/SetupFinish.tsx";
import { SetupPermissions } from "../pages/setup/SetupPermissions.tsx";
import { SetupProject } from "../pages/setup/SetupProject.tsx";
import { SetupScan } from "../pages/setup/SetupScan.tsx";
import { SetupSpending } from "../pages/setup/SetupSpending.tsx";
import { SetupTeam } from "../pages/setup/SetupTeam.tsx";
import { TeamSetup } from "../pages/setup/TeamSetup.tsx";
import { Team } from "../pages/Team.tsx";
import { Shell } from "../shell/Shell.tsx";
import { useConnection } from "./connection.tsx";
import { type ServeStatus, useQuery } from "./store.ts";
import { useTheme } from "./theme.ts";

/** The start page: setup until Farik has a project and its team, the event list after. */
function Home() {
	const { data } = useQuery<ServeStatus>("serve.status", {});
	if (!data) return null;
	if (data.projectRoot !== null)
		return (
			<Navigate to={data.setupPending ? "/setup/scan" : "/events"} replace />
		);
	// A project was chosen but could not be taken on: its screen says why.
	return (
		<Navigate
			to={data.takeOnError ? "/setup/project" : "/setup/computer"}
			replace
		/>
	);
}

export function App() {
	const { status } = useConnection();
	// The one theme state: Settings changes it, and it applies on every page.
	const [theme, setTheme] = useTheme();
	// Without a session, or while Farik is not answering, every path shows why.
	if (status === "no_session" || status === "lost") return <Connect />;
	return (
		<Routes>
			{/* With a live session, the connect page has nothing to say: a used link goes home. */}
			<Route
				path="/connect"
				element={status === "open" ? <Navigate to="/" replace /> : <Connect />}
			/>
			<Route path="/" element={<Home />} />
			{/* Setup has its own two-panel layout, without the rail and bars. */}
			<Route path="/setup/computer" element={<SetupComputer />} />
			<Route path="/setup/account" element={<SetupAccount />} />
			<Route path="/setup/project" element={<SetupProject />} />
			<Route path="/setup/scan" element={<SetupScan />} />
			{/* The team screens share one draft, which "Start the team" sends. */}
			<Route element={<TeamSetup />}>
				<Route path="/setup/team" element={<SetupTeam />} />
				<Route path="/setup/permissions" element={<SetupPermissions />} />
				<Route path="/setup/spending" element={<SetupSpending />} />
				<Route path="/setup/finish" element={<SetupFinish />} />
				<Route path="/setup/advanced" element={<SetupAdvanced />} />
			</Route>
			<Route element={<Shell />}>
				<Route path="/events" element={<Events />} />
				<Route path="/team" element={<Team />} />
				<Route path="/team/:id" element={<AgentEdit />} />
				<Route
					path="/settings"
					element={<Settings theme={theme} onTheme={setTheme} />}
				/>
				<Route path="*" element={<NotFound />} />
			</Route>
		</Routes>
	);
}
