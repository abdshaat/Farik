import { Outlet, useLocation } from "react-router";
import { Link, Navigate, NavLink } from "../../app/links.tsx";
import { useQuery } from "../../app/store.ts";
import { pageOf } from "../../app/teams.ts";
import { useWide } from "../../shell/Shell.tsx";
import type { en } from "../../strings/en.ts";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import type { Team } from "../setup/TeamSetup.tsx";

type Section = {
	path: string;
	name: keyof typeof en;
	summary: keyof typeof en;
	needsDesigner?: true;
};

/** The sections, in the order the list shows them. */
export const SECTIONS: readonly Section[] = [
	{ path: "general", name: "sectionGeneral", summary: "sectionGeneralSummary" },
	{
		path: "allowed-actions",
		name: "sectionAllowedActions",
		summary: "sectionAllowedActionsSummary",
	},
	{ path: "rules", name: "sectionRules", summary: "sectionRulesSummary" },
	{
		path: "spending",
		name: "sectionSpending",
		summary: "sectionSpendingSummary",
	},
	{ path: "work", name: "sectionWork", summary: "sectionWorkSummary" },
	{
		path: "repositories",
		name: "sectionRepositories",
		summary: "sectionRepositoriesSummary",
	},
	{
		path: "your-app",
		name: "sectionYourApp",
		summary: "sectionYourAppSummary",
		needsDesigner: true,
	},
	{
		path: "saved-teams",
		name: "sectionSavedTeams",
		summary: "sectionSavedTeamsSummary",
	},
	{
		path: "computer",
		name: "sectionComputer",
		summary: "sectionComputerSummary",
	},
	{
		path: "account",
		name: "sectionAccount",
		summary: "sectionAccountSummary",
	},
];

/** The list of sections: a side list on a wide screen, rows with a summary on a phone. */
function SectionList() {
	const wide = useWide();
	const { data } = useQuery<{ team: Team }>("team.get", {});
	const designer = data?.team.agents.some(
		(a) => a.role === "ui_ux_designer" && a.status !== "retired",
	);
	return (
		<nav
			aria-label={t("settingsSections")}
			className={wide ? styles.settingsSide : styles.settingsRows}
		>
			<ul>
				{SECTIONS.filter((s) => !s.needsDesigner || designer).map((s) => (
					<li key={s.path}>
						<NavLink to={`/settings/${s.path}`}>
							<span>{t(s.name)}</span>
							{!wide && (
								<>
									<span className={styles.muted}>{t(s.summary)}</span>
									<span aria-hidden="true" className={styles.chevron}>
										›
									</span>
								</>
							)}
						</NavLink>
					</li>
				))}
			</ul>
		</nav>
	);
}

/** `/settings`: the list on a phone; on a wide screen General, and the old preview link its page. */
export function SettingsIndex() {
	const wide = useWide();
	const { hash } = useLocation();
	if (hash === "#preview") return <Navigate to="/settings/your-app" replace />;
	if (wide) return <Navigate to="/settings/general" replace />;
	return <SectionList />;
}

export function SettingsLayout() {
	const wide = useWide();
	const atList =
		pageOf(useLocation().pathname).replace(/\/$/, "") === "/settings";
	return (
		<div className={`${styles.page} ${styles.settingsPage}`}>
			<h1 className={styles.title}>{t("settings")}</h1>
			<div className={styles.settings}>
				{wide && <SectionList />}
				<div className={styles.settingsBody}>
					{!wide && !atList && (
						<Link to="/settings" className={styles.settingsBack}>
							<span aria-hidden="true">←</span> {t("settingsBack")}
						</Link>
					)}
					<Outlet />
				</div>
			</div>
		</div>
	);
}
