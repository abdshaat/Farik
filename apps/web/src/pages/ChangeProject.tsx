import { Button } from "@catervas/ui";
import { useState } from "react";
import { useConnection } from "../app/connection.tsx";
import { saidAll } from "../app/refusals.ts";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";

/**
 * The one change-project flow: a trigger, the confirmation naming the folder, and
 * `project.leave` then `reopen()` (Settings, the rail and the phone's top bar).
 * `short` makes the trigger a quiet "Change" whose name is still "Change project".
 */
export function ChangeProject({
	root,
	short = false,
}: {
	root: string;
	short?: boolean;
}) {
	const { client, reopen } = useConnection();
	const [asking, setAsking] = useState(false);
	const [moving, setMoving] = useState(false);
	const [said, setSaid] = useState<string>();
	const change = async () => {
		if (!client) return;
		setSaid(undefined);
		setMoving(true);
		try {
			await client.call("project.leave", {});
			reopen();
		} catch (e) {
			setSaid(saidAll(e));
			setMoving(false);
		}
	};
	if (!asking)
		return (
			<div>
				<Button
					kind={short ? "quiet" : "secondary"}
					onClick={() => setAsking(true)}
				>
					{short ? (
						<>
							{t("changeShort")}{" "}
							<span className={styles.hidden}>{t("projectWord")}</span>
						</>
					) : (
						t("changeProject")
					)}
				</Button>
			</div>
		);
	return (
		<>
			<p>
				{t("changeProjectConfirm").replace(
					"{name}",
					root.split(/[\\/]/).filter(Boolean).at(-1) ?? "",
				)}
			</p>
			<div className={styles.actions}>
				<Button busy={moving} onClick={change}>
					{t("changeProjectYes")}
				</Button>
				<Button kind="quiet" onClick={() => setAsking(false)}>
					{t("agentCancel")}
				</Button>
			</div>
			{said && <p role="alert">{said}</p>}
		</>
	);
}
