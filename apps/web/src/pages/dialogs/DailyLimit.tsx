import { Button, Dialog } from "@farik/ui";
import { useState } from "react";
import { useConnection } from "../../app/connection.tsx";
import { saidAll } from "../../app/refusals.ts";
import { t } from "../../strings/t.ts";
import { useDailyLimit } from "../setup/SetupSpending.tsx";
import type { Team } from "../setup/TeamSetup.tsx";

/** "Set a daily limit": setup's own control, saved as `budgets.daily_usd` through `team.save`. */
export function DailyLimit({
	team,
	onClose,
}: {
	team: Team;
	onClose: () => void;
}) {
	const { client } = useConnection();
	const { wrong, budgets, fields } = useDailyLimit(team.budgets.dailyUsd);
	const [busy, setBusy] = useState(false);
	const [refused, setRefused] = useState<string>();
	const save = async () => {
		if (!client) return;
		setBusy(true);
		setRefused(undefined);
		try {
			await client.call("team.save", {
				team: { ...team, budgets: budgets(team.budgets) },
			});
			onClose();
		} catch (e) {
			setRefused(saidAll(e));
			setBusy(false);
		}
	};
	return (
		<Dialog
			open
			title={t("costsSetLimit")}
			onClose={onClose}
			actions={
				<>
					<Button onClick={onClose}>{t("sprintNotNow")}</Button>
					<Button kind="primary" busy={busy} disabled={wrong} onClick={save}>
						{t("limitSave")}
					</Button>
				</>
			}
		>
			{fields}
			{refused && <p role="alert">{refused}</p>}
		</Dialog>
	);
}
