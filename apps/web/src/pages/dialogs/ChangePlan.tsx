import { Button, Dialog, TextArea } from "@catervas/ui";
import { useState } from "react";
import { useCommand } from "../../app/command.ts";
import { t } from "../../strings/t.ts";

/** Sending a ready task back to its plan, with the owner's words for the Product Manager. */
export function ChangePlan({
	id,
	title,
	onClose,
}: {
	id: string;
	title: string;
	onClose: () => void;
}) {
	const [why, setWhy] = useState("");
	const { busy, refusal, send } = useCommand(onClose);
	const reason = why.trim();
	return (
		<Dialog
			open
			title={t("taskChangePlanTitle").replace("{title}", title)}
			onClose={onClose}
			actions={
				<>
					<Button onClick={onClose}>{t("taskCancelKeep")}</Button>
					<Button
						kind="primary"
						busy={busy}
						disabled={reason === ""}
						onClick={() =>
							send({
								command: "task_transition",
								body: { taskId: id, to: "refining", reason },
							})
						}
					>
						{t("taskChangePlan")}
					</Button>
				</>
			}
		>
			<TextArea
				id="change-plan-why"
				label={t("taskChangePlanWhy")}
				value={why}
				onChange={setWhy}
				required
			/>
			{refusal && <p role="alert">{refusal}</p>}
		</Dialog>
	);
}
