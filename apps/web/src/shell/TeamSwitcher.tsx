import { useEffect, useRef, useState } from "react";
import { useLocation } from "react-router";
import { useHub } from "../app/connection.tsx";
import { useNavigate } from "../app/links.tsx";
import { pageOf, samePage, type TeamRow, teamPath } from "../app/teams.ts";
import { useTeamList } from "../app/useTeamList.ts";
import { t } from "../strings/t.ts";
import styles from "./Shell.module.css";

/** What a team is doing, in the words of the menu's second line; nothing for a team at rest. */
export function statusLine(team: TeamRow): string {
	if (team.status === "elsewhere") return t("teamsElsewhere");
	if (team.status === "missing") return t("teamsMissing");
	if (team.status === "failed")
		return t("teamsStopped", { why: (team.failed ?? "").slice(0, 40) });
	if (team.waiting > 0) return t("teamsWaiting", { count: team.waiting });
	if (team.workingAgents > 0) return t("teamsWorking");
	return team.status === "paused" ? t("teamsPaused") : "";
}

/**
 * The team switcher at the top of the rail: a button with the current team's name, or "All teams"
 * off a team, and a menu of every team on this computer (ADR 0059).
 */
export function TeamSwitcher({ team }: { team: string | undefined }) {
	const { list } = useTeamList();
	const hub = useHub();
	const navigate = useNavigate();
	const page = pageOf(useLocation().pathname);
	const [open, setOpen] = useState(false);
	const wrap = useRef<HTMLDivElement>(null);
	const first = useRef<HTMLButtonElement>(null);
	const name =
		team === undefined
			? t("allTeams")
			: (list?.teams.find((one) => one.slug === team)?.name ?? team);

	useEffect(() => {
		if (!open) return;
		first.current?.focus();
		const away = (e: MouseEvent) => {
			if (!wrap.current?.contains(e.target as Node)) setOpen(false);
		};
		const onKey = (e: KeyboardEvent) => {
			if (e.key !== "Escape") return;
			setOpen(false);
			wrap.current?.querySelector("button")?.focus();
		};
		document.addEventListener("mousedown", away);
		document.addEventListener("keydown", onKey);
		return () => {
			document.removeEventListener("mousedown", away);
			document.removeEventListener("keydown", onKey);
		};
	}, [open]);

	const go = (to: string) => {
		setOpen(false);
		navigate(to);
	};
	const choose = async (slug: string) => {
		setOpen(false);
		// `/rpc` and `catervas open` follow the owner to the team chosen.
		try {
			await hub.client?.call("teams.current", { slug });
		} catch {
			// The team went away meanwhile: its address says so.
		}
		navigate(teamPath(slug, samePage(page)));
	};

	return (
		<div ref={wrap} className={styles.switcher}>
			<button
				type="button"
				className={styles.switcherButton}
				aria-haspopup="menu"
				aria-expanded={open}
				onClick={() => setOpen(!open)}
			>
				{name}
			</button>
			{open && (
				<div role="menu" aria-label={t("teamsMenu")} className={styles.menu}>
					<button
						type="button"
						role="menuitem"
						ref={first}
						aria-current={team === undefined ? "page" : undefined}
						onClick={() => go("/teams")}
					>
						{t("allTeams")}
					</button>
					<hr />
					{list?.teams.map((one) => (
						<button
							key={one.slug}
							type="button"
							role="menuitem"
							aria-current={one.slug === team ? "page" : undefined}
							onClick={() => choose(one.slug)}
						>
							{one.name}
							{statusLine(one) && <span>{statusLine(one)}</span>}
						</button>
					))}
					<hr />
					<button
						type="button"
						role="menuitem"
						onClick={() => go("/setup/project")}
					>
						{t("teamsAdd")}
					</button>
				</div>
			)}
		</div>
	);
}
