import { Button } from "@catervas/ui";
import { useRef } from "react";
import { useNavigate } from "react-router";
import { t } from "../../strings/t.ts";
import { Wizard } from "../setup/Wizard.tsx";
import styles from "./find.module.css";
import {
	counted,
	EXPORT_PENDING,
	type ExportSummary,
	keep,
	useExportUpload,
} from "./finding.ts";
import { FindStepper } from "./Stepper.tsx";

/** What came in, in words: "12 conversations and 2 Projects brought in". */
export function broughtIn(summary: ExportSummary): string {
	return t("exportBroughtIn", {
		conversations: counted(
			summary.conversations,
			t("exportConversationsOne"),
			(count) => t("exportConversations", { count }),
		),
		projects: counted(summary.projects, t("exportProjectsOne"), (count) =>
			t("exportProjects", { count }),
		),
	});
}

/** The file picker and drop zone that send a claude.ai export to the hub, and what the hub said of it. */
export function DropZone(props: {
	/** Only the button, for the card on other pages. */
	compact?: boolean;
}) {
	const { send, busy, summary, refused } = useExportUpload();
	const input = useRef<HTMLInputElement>(null);
	const take = (files: FileList | null | undefined) => {
		const file = files?.[0];
		if (file) void send(file);
	};
	return (
		<>
			{/* biome-ignore lint/a11y/noStaticElementInteractions: a drop target; the button inside does the same by keyboard */}
			<div
				className={styles.drop}
				onDragOver={(e) => e.preventDefault()}
				onDrop={(e) => {
					e.preventDefault();
					take(e.dataTransfer.files);
				}}
			>
				{!props.compact && (
					<>
						<strong>{t("exportDropTitle")}</strong>
						<span className={styles.muted}>{t("exportDropNote")}</span>
					</>
				)}
				<Button busy={busy} onClick={() => input.current?.click()}>
					{refused ? t("exportChooseOther") : t("exportChoose")}
				</Button>
				<input
					ref={input}
					type="file"
					accept=".zip,application/zip"
					className={styles.file}
					tabIndex={-1}
					aria-hidden="true"
					onChange={(e) => {
						take(e.target.files);
						e.target.value = "";
					}}
				/>
			</div>
			{busy && <p role="status">{t("exportReading")}</p>}
			{summary && <p role="status">{broughtIn(summary)}</p>}
			{refused && (
				<section className={styles.alert} role="alert">
					<strong>{t(refused)}</strong>
					{refused === "exportNotAnExport" && (
						<>
							<p>{t("exportNotAnExportHelp")}</p>
							<p>
								<strong>{t("exportExpiredTitle")}</strong>{" "}
								{t("exportExpiredHelp")}
							</p>
						</>
					)}
				</section>
			)}
		</>
	);
}

/** "Bring your claude.ai history": the six steps, and the drop zone (journey C, step 1 of 3). */
export function ExportDrop() {
	const navigate = useNavigate();
	return (
		<Wizard
			step={0}
			title={t("exportTitle")}
			lead={t("exportKicker")}
			nav={<FindStepper current={0} />}
		>
			<ul className={styles.legend}>
				<li>{t("exportKeep")}</li>
				<li>{t("exportLocal")}</li>
				<li>{t("exportNoSignIn")}</li>
			</ul>
			<ol className={styles.legend} aria-label={t("exportStepsLabel")}>
				<li>{t("exportStepOpen")}</li>
				<li>{t("exportStepSettings")}</li>
				<li>{t("exportStepExport")}</li>
				<li>{t("exportStepWait")}</li>
				<li>{t("exportStepDownload")}</li>
				<li>{t("exportStepDrop")}</li>
			</ol>
			<DropZone />
			<div className={styles.foot}>
				<Button
					onClick={() => {
						// Nothing is called: the card on All teams waits for the export.
						keep(EXPORT_PENDING, "1");
						navigate("/find/services");
					}}
				>
					{t("exportLater")}
				</Button>
				{/* GitHub, step 2 of 3, is step 06d's; until it lands this goes to the services. */}
				<Button kind="primary" onClick={() => navigate("/find/services")}>
					{t("exportNext")}
				</Button>
			</div>
		</Wizard>
	);
}
