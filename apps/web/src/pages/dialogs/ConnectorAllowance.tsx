import { Button, Dialog } from "@farik/ui";
import { useState } from "react";
import { useConnection } from "../../app/connection.tsx";
import { refusalsOf } from "../../app/refusals.ts";
import { t } from "../../strings/t.ts";
import {
	AllowanceFields,
	type AllowanceRow,
	numberOf,
	offersOf,
} from "../allowances.tsx";
import styles from "../pages.module.css";
import type { KitService } from "../Team.tsx";

/**
 * "Change how many": the numbers an agent may make of a kit service's spending tools each sprint
 * without asking, from the agent's page (ADR 0037). It starts at the numbers the entry holds, says
 * how many were made so far, and refuses a number over 1,000 at its field. When Farik has changed
 * the service since it was connected, the numbers cannot change until it is connected again.
 */
export function ConnectorAllowance({
	agent,
	name,
	service,
	rows,
	onClose,
	onAgain,
}: {
	agent: string;
	/** The agent's display name. */
	name: string;
	service: KitService;
	/** What the agent has made of each tool this period, with the number now allowed. */
	rows: AllowanceRow[];
	onClose: (changed: boolean) => void;
	/** Connects the service again, for a kit that changed it. */
	onAgain: () => void;
}) {
	const { client } = useConnection();
	const offers = offersOf(service);
	const [numbers, setNumbers] = useState<Record<string, string>>(
		Object.fromEntries(
			offers.map((offer) => [
				offer.tool,
				String(
					rows.find((row) => row.tool === offer.tool)?.of ??
						service.allowances?.find((given) => given.tool === offer.tool)
							?.calls ??
						0,
				),
			]),
		),
	);
	const made = Object.fromEntries(rows.map((row) => [row.tool, row.used]));
	const [shown, setShown] = useState(false);
	const [busy, setBusy] = useState(false);
	const [refused, setRefused] = useState<string>();
	const [again, setAgain] = useState(false);
	const fill = { name, service: service.title };

	const save = async () => {
		if (!client) return;
		const allowed = Object.fromEntries(
			offers.map((offer) => [offer.tool, numberOf(numbers[offer.tool] ?? "")]),
		);
		if (Object.values(allowed).some((n) => n === undefined)) {
			setShown(true);
			return;
		}
		setBusy(true);
		setRefused(undefined);
		try {
			await client.call("connector.allowances", {
				agent,
				server: service.name,
				allowances: allowed,
			});
			onClose(true);
			return;
		} catch (e) {
			const message = refusalsOf(e)[0]?.message ?? "";
			if (message.startsWith("connector_not_in_kit")) setAgain(true);
			else setRefused(t("allowRefused", fill));
		}
		setBusy(false);
	};

	if (again)
		return (
			<Dialog
				open
				title={t("allowAgainTitle", fill)}
				onClose={() => onClose(false)}
				actions={
					<>
						<Button onClick={() => onClose(false)}>{t("agentCancel")}</Button>
						<Button kind="primary" onClick={onAgain}>
							{t("connectorAgainButton")}
						</Button>
					</>
				}
			>
				<p>{t("allowAgain")}</p>
			</Dialog>
		);
	return (
		<Dialog
			open
			title={t("allowQuestion", fill)}
			onClose={() => onClose(false)}
			actions={
				<>
					<Button onClick={() => onClose(false)}>{t("agentCancel")}</Button>
					<Button kind="primary" busy={busy} onClick={save}>
						{t("allowSave")}
					</Button>
				</>
			}
		>
			<AllowanceFields
				offers={offers}
				values={numbers}
				made={made}
				shown={shown}
				onChange={(tool, text) => setNumbers({ ...numbers, [tool]: text })}
			/>
			<p className={styles.muted}>
				{t("allowRange", fill)} {t("allowAlwaysAsk")}
			</p>
			<p className={styles.muted}>{t("allowApplies", fill)}</p>
			{refused && (
				<p role="alert" className={styles.alert}>
					{refused}
				</p>
			)}
		</Dialog>
	);
}
