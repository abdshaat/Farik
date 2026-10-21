import { Button, Dialog, InfoTip, Switch } from "@catervas/ui";
import { useState } from "react";
import { Link } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { saidAll } from "../app/refusals.ts";
import { Bullets } from "../components/Bullets.tsx";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";
import type { Part } from "./TeamRules.tsx";

/**
 * Autopilot (ADR 0082): the switch that lets the team run the product. It needs a daily
 * spending limit, which is its stop. Turning it on shows the warning first, and only the
 * dialog's Turn on sends the confirmation; turning it off is one press.
 */
export function AutopilotSwitch({ saved, autopilot, done }: Part) {
	const { client } = useConnection();
	const [asking, setAsking] = useState(false);
	const [busy, setBusy] = useState(false);
	const [refused, setRefused] = useState<string>();
	const limit = saved.budgets.dailyUsd;
	const set = async (params: { on: boolean; confirmed?: boolean }) => {
		if (!client) return;
		setBusy(true);
		setRefused(undefined);
		try {
			await client.call("autopilot.set", params);
			done();
		} catch (e) {
			setRefused(saidAll(e));
			setBusy(false);
			setAsking(false);
		}
	};
	const warning = t("autopilotWarn", {
		limit: limit ?? "",
		perHour: saved.policy.paidCallsPerHour ?? 30,
	});
	return (
		<div className={styles.section}>
			<Switch
				id="autopilot"
				label={t("autopilotSwitch")}
				info={<Bullets text={t("autopilotSwitchNote")} />}
				checked={Boolean(autopilot)}
				disabled={(limit === undefined && !autopilot) || busy}
				onChange={(on) => (on ? setAsking(true) : set({ on: false }))}
			/>
			{limit === undefined && !autopilot && (
				<p className={styles.muted}>
					{t("autopilotNeedsLimit")}{" "}
					<Link to="/settings/spending">{t("autopilotNeedsLimitLink")}</Link>
				</p>
			)}
			{refused && (
				<p role="alert" className={styles.alert}>
					{refused}
				</p>
			)}
			{asking && (
				<Dialog
					open
					title={t("autopilotWarnTitle")}
					onClose={() => setAsking(false)}
					actions={
						<>
							<Button onClick={() => setAsking(false)}>
								{t("autopilotCancel")}
							</Button>
							<Button
								kind="primary"
								busy={busy}
								onClick={() => set({ on: true, confirmed: true })}
							>
								{t("autopilotTurnOn")}
							</Button>
						</>
					}
				>
					<Bullets text={warning} />
					<InfoTip id="autopilot-warn-info">
						<Bullets text={t("autopilotWarnNote")} />
					</InfoTip>
				</Dialog>
			)}
		</div>
	);
}
