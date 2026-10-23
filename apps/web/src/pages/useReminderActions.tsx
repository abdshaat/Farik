import { useState } from "react";
import { useHub } from "../app/connection.tsx";
import { findingSaid } from "./find/finding.ts";
import { GiveToTeam } from "./GiveToTeam.tsx";
import type { ReminderItemData } from "./reminders.ts";

/**
 * What the buttons of an item do: the calls to the hub, the dialog that gives an item to its team,
 * and `after` once any of them has changed what the digest holds.
 */
export function useReminderActions(after: () => void) {
	const { client } = useHub();
	const [giving, setGiving] = useState<ReminderItemData>();
	const [busy, setBusy] = useState(false);
	const [said, setSaid] = useState<string>();
	const dialog = giving ? (
		<GiveToTeam
			item={giving}
			busy={busy}
			said={said}
			onClose={() => {
				setGiving(undefined);
				setSaid(undefined);
			}}
			onSend={async () => {
				setBusy(true);
				setSaid(undefined);
				try {
					await client?.call("reminders.give", { key: giving.key });
					setGiving(undefined);
					after();
				} catch (e) {
					setSaid(findingSaid(e, "refuseOther"));
				} finally {
					setBusy(false);
				}
			}}
		/>
	) : null;
	const quietly = async (
		method: "reminders.snooze" | "reminders.dismiss",
		params: object,
	) => {
		try {
			await client?.call(method, params);
		} catch {}
		after();
	};
	return {
		dialog,
		onGive: setGiving,
		onSnooze: (item: ReminderItemData, length: "day" | "week") =>
			quietly("reminders.snooze", { key: item.key, for: length }),
		onDismiss: (item: ReminderItemData) =>
			quietly("reminders.dismiss", { key: item.key }),
	};
}
