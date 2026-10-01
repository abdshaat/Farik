import { Button, Dialog } from "@farik/ui";
import { t } from "../../strings/t.ts";
import { useDailyLimit } from "../setup/SetupSpending.tsx";
import type { Team } from "../setup/TeamSetup.tsx";
import { useChange } from "../TeamRules.tsx";

/**
 * "Set a daily limit": setup's own control, saved as `budgets.daily_usd` through `team.save`,
 * with its effect shown first, as Settings' part shows it.
 */
export function DailyLimit({
	team,
	onClose,
}: {
	team: Team;
	onClose: () => void;
}) {
	const { wrong, budgets, fields } = useDailyLimit(team.budgets.dailyUsd);
	const next = { ...team, budgets: budgets(team.budgets) };
	const { preview, save, busy, blocked } = useChange(
		{ saved: team, base: undefined, done: onClose },
		next.budgets.dailyUsd === team.budgets.dailyUsd ? team : next,
		{ wrong },
	);
	return (
		<Dialog
			open
			title={t("costsSetLimit")}
			onClose={onClose}
			actions={
				<>
					<Button onClick={onClose}>{t("sprintNotNow")}</Button>
					<Button kind="primary" busy={busy} disabled={blocked} onClick={save}>
						{t("limitSave")}
					</Button>
				</>
			}
		>
			{fields}
			{preview}
		</Dialog>
	);
}
