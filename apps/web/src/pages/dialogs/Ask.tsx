import { TextArea } from "@catervas/ui";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";

/**
 * A tick box that offers to send the team a request with the step the owner is taking, and, while
 * it is ticked, the request in the owner's words for them to change first (spec 6.10).
 */
export function Ask({
	id,
	label,
	checked,
	onChecked,
	text,
	onText,
}: {
	id: string;
	label: string;
	checked: boolean;
	onChecked: (checked: boolean) => void;
	text: string;
	onText: (text: string) => void;
}) {
	return (
		<>
			<label className={styles.tick}>
				<input
					type="checkbox"
					checked={checked}
					onChange={(e) => onChecked(e.target.checked)}
				/>
				{label}
			</label>
			{checked && (
				<div className={styles.orderForm}>
					<TextArea
						id={`${id}-text`}
						label={t("askTeamText")}
						value={text}
						onChange={onText}
						rows={3}
					/>
					<p className={styles.orderHint}>{t("askTeamBody")}</p>
				</div>
			)}
		</>
	);
}
