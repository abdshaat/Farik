import { Button, Dialog } from "@catervas/ui";
import { t } from "../../strings/t.ts";
import { useCommand } from "./StartSprint.tsx";

/** Ending the sprint early: its unfinished tasks go back on the board, or wait in the Backlog when
 * the team plans in sprints, and `planner` still runs the review and the look back. */
export function EndSprint({
	n,
	unfinished,
	planner,
	backlog,
	onClose,
}: {
	n: string;
	unfinished: number;
	planner: string;
	/** Whether the team plans in sprints (`backlog.summary`). */
	backlog: boolean;
	onClose: () => void;
}) {
	const one = unfinished === 1;
	const { busy, refusal, send } = useCommand(onClose);
	return (
		<Dialog
			open
			title={t("sprintEndTitle").replace("{n}", n)}
			onClose={onClose}
			actions={
				<>
					<Button onClick={onClose}>{t("sprintKeep")}</Button>
					<Button
						kind="primary"
						busy={busy}
						onClick={() => send({ command: "sprint_end", body: {} })}
					>
						{t("sprintEnd").replace("{n}", n)}
					</Button>
				</>
			}
		>
			<p>
				{t(
					backlog
						? one
							? "sprintEndBacklogOne"
							: "sprintEndBacklog"
						: one
							? "sprintEndLeavesOne"
							: "sprintEndLeaves",
				)
					.replace("{count}", String(unfinished))
					.replace("{name}", planner)}
			</p>
			{refusal && <p role="alert">{refusal}</p>}
		</Dialog>
	);
}
