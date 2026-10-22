import { Navigate, Route, Routes } from "react-router";
import { AgentEdit } from "../pages/AgentEdit.tsx";
import { AllTeams } from "../pages/AllTeams.tsx";
import { Board } from "../pages/Board.tsx";
import { Chats } from "../pages/Chats.tsx";
import { ChatsAlias } from "../pages/ChatsAlias.tsx";
import { Connect } from "../pages/Connect.tsx";
import { Costs } from "../pages/Costs.tsx";
import { Events } from "../pages/Events.tsx";
import { Files } from "../pages/Files.tsx";
import { ConnectGithub } from "../pages/find/ConnectGithub.tsx";
import { ExportDrop } from "../pages/find/ExportDrop.tsx";
import { Found } from "../pages/find/Found.tsx";
import { Scanning } from "../pages/find/Scanning.tsx";
import { Services } from "../pages/find/Services.tsx";
import { Together } from "../pages/find/Together.tsx";
import { WhereClaude } from "../pages/find/WhereClaude.tsx";
import { Gate } from "../pages/Gate.tsx";
import { HelpNeeded } from "../pages/HelpNeeded.tsx";
import { MarketingPlan } from "../pages/MarketingPlan.tsx";
import { NotFound } from "../pages/NotFound.tsx";
import { Opening } from "../pages/Opening.tsx";
import { PlanEditor } from "../pages/PlanEditor.tsx";
import { PlanPage } from "../pages/PlanPage.tsx";
import { ProcurementMailbox } from "../pages/ProcurementMailbox.tsx";
import { Questions } from "../pages/Questions.tsx";
import { RequestFiled } from "../pages/RequestFiled.tsx";
import { Account } from "../pages/settings/Account.tsx";
import { AllowedActions } from "../pages/settings/AllowedActions.tsx";
import { Computer } from "../pages/settings/Computer.tsx";
import { General } from "../pages/settings/General.tsx";
import { Rules } from "../pages/settings/Rules.tsx";
import { SavedTeamsPage } from "../pages/settings/SavedTeamsPage.tsx";
import {
	SettingsIndex,
	SettingsLayout,
} from "../pages/settings/SettingsLayout.tsx";
import { Spending } from "../pages/settings/Spending.tsx";
import { Work } from "../pages/settings/Work.tsx";
import { YourApp } from "../pages/settings/YourApp.tsx";
import { SetupAccount } from "../pages/setup/SetupAccount.tsx";
import { SetupComputer } from "../pages/setup/SetupComputer.tsx";
import { SetupFinish } from "../pages/setup/SetupFinish.tsx";
import { SetupPermissions } from "../pages/setup/SetupPermissions.tsx";
import { SetupProject } from "../pages/setup/SetupProject.tsx";
import { SetupReady } from "../pages/setup/SetupReady.tsx";
import { SetupScan } from "../pages/setup/SetupScan.tsx";
import { SetupSpending } from "../pages/setup/SetupSpending.tsx";
import { SetupTeam } from "../pages/setup/SetupTeam.tsx";
import { TeamSetup } from "../pages/setup/TeamSetup.tsx";
import { TaskDetail } from "../pages/TaskDetail.tsx";
import { Team } from "../pages/Team.tsx";
import { ThisComputer } from "../pages/ThisComputer.tsx";
import { Today } from "../pages/Today.tsx";
import { Shell } from "../shell/Shell.tsx";
import { useConnection } from "./connection.tsx";
import { Navigate as TeamNavigate } from "./links.tsx";
import { CurrentScope, TeamRedirect, TeamScope } from "./TeamScope.tsx";
import { useTheme } from "./theme.ts";

export function App() {
	const { status } = useConnection();
	// The one theme state: Settings changes it, and it applies on every page.
	const [theme, setTheme] = useTheme();
	// Without a session, or while Catervas is not answering, every path shows why.
	if (status === "no_session" || status === "lost") return <Connect />;
	if (status === "checking" || status === "connecting") return <Opening />;
	return (
		<Routes>
			{/* With a live session, the connect page has nothing to say: a used link goes home. */}
			<Route
				path="/connect"
				element={status === "open" ? <Navigate to="/" replace /> : <Connect />}
			/>
			{/* Setup has its own two-panel layout, without the rail and bars. */}
			<Route path="/setup/computer" element={<SetupComputer />} />
			<Route path="/setup/account" element={<SetupAccount />} />
			<Route path="/setup/project" element={<SetupProject />} />
			{/* Finding the owner's projects (ADR 0059) is the computer's, so it talks to the hub. */}
			<Route path="/setup/where" element={<WhereClaude />} />
			<Route path="/find/scanning" element={<Scanning />} />
			<Route path="/find/found" element={<Found />} />
			<Route path="/find/export" element={<ExportDrop />} />
			<Route path="/find/github" element={<ConnectGithub />} />
			<Route path="/find/services" element={<Services />} />
			<Route path="/find/together" element={<Together />} />
			{/* The steps after the project talk to the team just taken on, the current one. */}
			<Route element={<CurrentScope />}>
				<Route path="/setup/scan" element={<SetupScan />} />
				{/* The offer to document an existing project, after the team starts; it needs no draft. */}
				<Route path="/setup/ready" element={<SetupReady />} />
				{/* The team screens share one draft, which "Start the team" sends. */}
				<Route element={<TeamSetup />}>
					<Route path="/setup/team" element={<SetupTeam />} />
					<Route path="/setup/permissions" element={<SetupPermissions />} />
					<Route path="/setup/spending" element={<SetupSpending />} />
					<Route path="/setup/finish" element={<SetupFinish />} />
					<Route
						path="/setup/advanced"
						element={<Navigate to="/setup/finish" replace />}
					/>
				</Route>
			</Route>
			{/* All teams and This computer have the rail of the computer, not of a team. */}
			<Route element={<Shell />}>
				<Route path="/teams" element={<AllTeams />} />
				<Route path="/computer" element={<ThisComputer />} />
			</Route>
			{/* Each team's pages are under its own address (ADR 0059). */}
			<Route path="/t/:team" element={<TeamScope />}>
				<Route element={<Shell />}>
					<Route index element={<Today />} />
					<Route path="board" element={<Board />} />
					<Route path="channel" element={<Chats />} />
					<Route path="channel/:id" element={<Chats />} />
					<Route
						path="chats"
						element={<TeamNavigate to="/channel" replace />}
					/>
					<Route path="chats/:id" element={<ChatsAlias />} />
					<Route path="files" element={<Files />} />
					<Route path="files/*" element={<Files />} />
					<Route path="costs" element={<Costs />} />
					<Route path="events" element={<Events />} />
					<Route path="marketing/plans/:id" element={<MarketingPlan />} />
					<Route path="requests/:id" element={<RequestFiled />} />
					<Route path="tasks/:id" element={<TaskDetail />} />
					<Route path="tasks/:id/questions" element={<Questions />} />
					<Route path="tasks/:id/plan" element={<PlanPage />} />
					<Route path="tasks/:id/plan/edit" element={<PlanEditor />} />
					<Route path="tasks/:id/accept" element={<Gate />} />
					<Route path="tasks/:id/help" element={<HelpNeeded />} />
					<Route path="team" element={<Team />} />
					<Route path="team/:id" element={<AgentEdit />} />
					<Route path="team/:id/mailbox" element={<ProcurementMailbox />} />
					<Route path="settings" element={<SettingsLayout />}>
						<Route index element={<SettingsIndex />} />
						<Route
							path="general"
							element={<General theme={theme} onTheme={setTheme} />}
						/>
						<Route path="allowed-actions" element={<AllowedActions />} />
						<Route path="rules" element={<Rules />} />
						<Route path="spending" element={<Spending />} />
						<Route path="work" element={<Work />} />
						<Route path="your-app" element={<YourApp />} />
						<Route path="saved-teams" element={<SavedTeamsPage />} />
						<Route path="computer" element={<Computer />} />
						<Route path="account" element={<Account />} />
					</Route>
					<Route path="*" element={<NotFound />} />
				</Route>
			</Route>
			{/* Any other address is an old one, or a bookmark: it goes to a team's page. */}
			<Route path="*" element={<TeamRedirect />} />
		</Routes>
	);
}
