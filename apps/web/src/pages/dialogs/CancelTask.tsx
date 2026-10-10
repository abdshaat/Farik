import { Button, Dialog, TextArea } from "@catervas/ui";
import { useState } from "react";
import { t } from "../../strings/t.ts";
import { useCommand } from "./StartSprint.tsx";

/** Cancelling a task, with its reason; an escalated task cancels by resolving its escalation. */
export function CancelTask({
	id,
	title,
	escalated,
	onClose,
}: {
	id: string;
	title: string;
	escalated: boolean;
	onClose: () => void;
}) {
	const [why, setWhy] = useState("");
	const { busy, refusal, send } = useCommand(onClose);
	const reason = why.trim();
	return (
		<Dialog
			open
			title={t("taskCancelTitle").replace("{title}", title)}
			onClose={onClose}
			actions={
				<>
					<Button onClick={onClose}>{t("taskCancelKeep")}</Button>
					<Button
						kind="primary"
						busy={busy}
						disabled={reason === ""}
						onClick={() =>
							send(
								escalated
									? {
											command: "escalation_resolve",
											body: { taskId: id, to: "cancelled", message: reason },
										}
									: {
											command: "task_transition",
											body: { taskId: id, to: "cancelled", reason },
										},
							)
						}
					>
						{t("taskCancel")}
					</Button>
				</>
			}
		>
			<TextArea
				id="cancel-why"
				label={t("taskCancelWhy")}
				value={why}
				onChange={setWhy}
				required
			/>
			{refusal && <p role="alert">{refusal}</p>}
		</Dialog>
	);
}
