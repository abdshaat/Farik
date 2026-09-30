import { Button, Dialog } from "@farik/ui";
import { t } from "../../strings/t.ts";
import { useCommand } from "./StartSprint.tsx";

/** Ending the sprint early: its unfinished tasks go back on the board. */
export function EndSprint({
	n,
	unfinished,
	onClose,
}: {
	n: string;
	unfinished: number;
	onClose: () => void;
}) {
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
			<p>{t("sprintEndUnfinished").replace("{count}", String(unfinished))}</p>
			{refusal && <p role="alert">{refusal}</p>}
		</Dialog>
	);
}
