import { Avatar, type AvatarKey, Button, TextArea } from "@catervas/ui";
import { useId, useState } from "react";
import type { TaskStatus } from "../app/words.ts";
import { DocumentFrame } from "../components/DocumentFrame.tsx";
import { Markdown } from "../components/Markdown.tsx";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";
import styles from "./DocumentChanges.module.css";
import type { Integration } from "./DocumentChanges.tsx";
import { useCommand } from "./dialogs/StartSprint.tsx";
import { docKey, isHtml } from "./files.ts";
import card from "./ProductPlanCard.module.css";

/** `product_plan.state`, in camelCase. */
export type ProductPlanState = {
	newProject: { name: string } | null;
	approved: boolean;
	task: { taskId: string; status: TaskStatus; createdAt: string } | null;
	documents: { path: string; text: string }[];
};

const NAMES: Record<string, keyof typeof en> = {
	"product/spec": "docNameSpec",
	"product/roadmap": "docNameRoadmap",
};

/** What approving does, said under the buttons; under `auto_merge` nothing needs saying. */
const POLICY: Partial<Record<Integration, keyof typeof en>> = {
	pull_request: "docChangesPullRequest",
	manual: "docChangesManual",
};

const startOfDay = (at: Date) =>
	new Date(at.getFullYear(), at.getMonth(), at.getDate()).getTime();

/** When the plan was written, in the viewer's time: "this morning", "yesterday", "on Wednesday". */
export function writtenWhen(at: string, now: Date): string {
	const written = new Date(at);
	const days = Math.round((startOfDay(now) - startOfDay(written)) / 86_400_000);
	if (days === 1) return t("planWhenYesterday");
	if (days > 1)
		return t("planWhenOn", {
			weekday: written.toLocaleDateString("en-GB", { weekday: "long" }),
		});
	const hour = written.getHours();
	return t(
		hour < 12
			? "planWhenMorning"
			: hour < 18
				? "planWhenAfternoon"
				: "planWhenEvening",
	);
}

/** A project's folder name as its owner says it: `little-oak-cakes` is "Little Oak Cakes". */
export function projectName(name: string): string {
	return name
		.split("-")
		.map((word) => word.charAt(0).toUpperCase() + word.slice(1))
		.join(" ");
}

/**
 * A new project's product plan on Today: both documents in full, and the human's acceptance of
 * the task that wrote them, as "Approve" and "Send back".
 */
export function ProductPlanCard({
	state,
	name,
	avatar,
	integration,
	again,
}: {
	state: ProductPlanState;
	name: string;
	avatar?: AvatarKey | undefined;
	integration: Integration;
	again: () => void;
}) {
	const id = useId();
	const [notes, setNotes] = useState("");
	const [needNotes, setNeedNotes] = useState(false);
	const { busy, refusal, send } = useCommand(again);
	const { task } = state;
	if (!task) return null;
	const message = notes.trim();
	const approve = () =>
		send({
			command: "human_accept",
			body: { taskId: task.taskId, subject: "result" },
		});
	const sendBack = () => {
		if (message === "") {
			setNeedNotes(true);
			return;
		}
		setNeedNotes(false);
		send({
			command: "human_send_back",
			body: {
				taskId: task.taskId,
				subject: "result",
				message,
				failedCriteria: [],
			},
		});
	};
	const policy = POLICY[integration];
	return (
		<section className={styles.card} aria-labelledby={`${id}-title`}>
			<div className={styles.head}>
				{avatar && <Avatar avatarKey={avatar} name={name} size={48} />}
				<div className={styles.headText}>
					<strong id={`${id}-title`}>{t("planToApprove")}</strong>
					<p className={styles.muted}>
						{t("planFrom", {
							name,
							when: writtenWhen(task.createdAt, new Date()),
						})}
					</p>
					<p>{t("planIntro")}</p>
				</div>
			</div>
			{state.documents.map((one) => {
				const named = NAMES[docKey(one.path)];
				return (
					<div key={one.path} className={styles.document}>
						<p className={styles.docName}>{named ? t(named) : one.path}</p>
						{isHtml(one.path) ? (
							<DocumentFrame
								path={one.path}
								task={task.taskId}
								title={named ? t(named) : one.path}
								className={card.frame}
							/>
						) : (
							<div className={card.read}>
								<Markdown text={one.text} />
							</div>
						)}
					</div>
				);
			})}
			<p className={card.gate}>{t("planGate")}</p>
			<TextArea
				id={`${id}-notes`}
				label={t("docChangesNotes", { name })}
				hint={t("docChangesNotesHint")}
				value={notes}
				onChange={setNotes}
				{...(needNotes ? { error: t("docChangesNeedNotes", { name }) } : {})}
				rows={3}
			/>
			<div className={styles.acts}>
				<Button kind="primary" busy={busy} onClick={approve}>
					{t("docChangesApprove")}
				</Button>
				<Button busy={busy} onClick={sendBack}>
					{t("docChangesSendBack")}
				</Button>
				{policy && <span className={styles.policy}>{t(policy)}</span>}
			</div>
			{refusal && (
				<p role="alert" className={styles.alert}>
					{refusal}
				</p>
			)}
		</section>
	);
}
