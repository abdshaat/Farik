import { Button, Dialog } from "@catervas/ui";
import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router";
import { useHub } from "../../app/connection.tsx";
import type { TeamList } from "../../app/teams.ts";
import { useTeamList } from "../../app/useTeamList.ts";
import { t } from "../../strings/t.ts";
import { FolderBrowser } from "../setup/FolderBrowser.tsx";
import { Wizard } from "../setup/Wizard.tsx";
import { AddProjectDialog } from "./AddProjectDialog.tsx";
import { ExportCard } from "./ExportCard.tsx";
import styles from "./find.module.css";
import {
	type Choice,
	counted,
	FOUND_SEEN,
	type FoundRow,
	findingSaid,
	keep,
	slugOf,
	tilde,
	useFindingStatus,
	useFoundList,
	whenWorked,
} from "./finding.ts";
import { FindStepper } from "./Stepper.tsx";

const CHOICES: Choice[] = ["team", "watch", "ignore"];
const CHOICE_WORDS = {
	team: "foundChoiceTeam",
	watch: "foundChoiceWatch",
	ignore: "foundChoiceIgnore",
} as const;

/** What the row says under its name: when it was worked on, and the services used through Claude. */
function meta(
	row: FoundRow,
	section: "main" | "explored" | "gone" | "job",
): string {
	if (row.kind === "claude_ai") return t("foundClaudeAiProject");
	if (section === "job") return row.jobs[0]?.schedule ?? "";
	const when = row.lastAt ? whenWorked(row.lastAt) : "";
	if (section === "explored")
		return [when, t("foundExploredNote")].filter(Boolean).join(" · ");
	if (section === "gone")
		return [when, t("foundGoneNote")].filter(Boolean).join(" · ");
	if (row.kind === "hand") return t("foundByHand");
	return [when, row.services.join(", ")].filter(Boolean).join(" · ");
}

/** Team, Watch and Ignore for one row, the chosen one filled. */
function Pick(props: {
	row: FoundRow;
	value: Choice | undefined;
	onChoose: (choice: Choice) => void;
}) {
	// A claude.ai Project or a job is watched; it joins a team only by a proof (docs/design/many-teams.md).
	const choices =
		props.row.kind === "claude_ai" || props.row.kind === "job"
			? CHOICES.filter((choice) => choice !== "team")
			: CHOICES;
	return (
		<fieldset className={styles.pick}>
			<legend>{t("foundChoiceFor", { name: props.row.name })}</legend>
			{choices.map((choice) => (
				<label key={choice}>
					<input
						type="radio"
						name={`choice-${props.row.key}`}
						value={choice}
						checked={props.value === choice}
						onChange={() => props.onChoose(choice)}
					/>
					{t(CHOICE_WORDS[choice])}
				</label>
			))}
		</fieldset>
	);
}

/** One project of the found list. */
function Item(props: {
	row: FoundRow;
	section: "main" | "explored" | "gone" | "job";
	value: Choice | undefined;
	teams: TeamList | undefined;
	onChoose: (choice: Choice) => void;
	onSeparate: (folder: string) => void;
	onLocate: () => void;
}) {
	const { row } = props;
	const [open, setOpen] = useState(false);
	const several = row.folders.length > 1;
	const connector = row.kind === "connector";
	const team =
		row.alreadyTeam &&
		(props.teams?.teams.find((one) => one.slug === row.alreadyTeam)?.name ??
			row.alreadyTeam);
	const panel = `panel-${row.key}`;
	return (
		<li className={styles.row}>
			<div className={styles.rowHead}>
				<strong>{row.name}</strong>
				{several && (
					<span>{t("foundFolders", { count: row.folders.length })}</span>
				)}
				{(several || connector) && (
					<button
						type="button"
						className={styles.more}
						aria-label={
							several
								? t("foundMoreAbout", {
										name: t("foundFolders", { count: row.folders.length }),
									})
								: t("foundMoreAbout", { name: row.name })
						}
						aria-expanded={open}
						aria-controls={panel}
						onClick={() => setOpen((on) => !on)}
					>
						ⓘ
					</button>
				)}
				{row.jobs.length > 0 && (
					<span className={styles.badge}>
						{counted(row.jobs.length, t("foundJobOne"), (count) =>
							t("foundJobs", { count }),
						)}
					</span>
				)}
			</div>
			{team ? (
				<span>{t("foundAlreadyTeam", { name: team })}</span>
			) : props.section === "gone" ? (
				<span className={styles.actions}>
					<Button onClick={props.onLocate}>{t("foundLocate")}</Button>
					<Button onClick={() => props.onChoose("ignore")}>
						{t("foundNotMine")}
					</Button>
				</span>
			) : (
				<Pick row={row} value={props.value} onChoose={props.onChoose} />
			)}
			<p className={styles.meta}>{meta(row, props.section)}</p>
			{open && several && (
				<div id={panel} className={styles.panel}>
					<p>
						{t("foundSameRepository", {
							folders: row.folders.map(tilde).join(" and "),
						})}
					</p>
					<Button
						onClick={() => {
							const folder = row.folders.at(-1);
							if (folder) props.onSeparate(folder);
						}}
					>
						{t("foundDifferent")}
					</Button>
				</div>
			)}
			{open && connector && (
				<div id={panel} className={styles.panel}>
					<ul>
						<li>{t("foundConnectorNotFolder")}</li>
						<li>
							{t("foundConnectorUsed", { services: row.services.join(", ") })}
						</li>
						<li>{t("foundConnectorFolder", { slug: slugOf(row.name) })}</li>
					</ul>
				</div>
			)}
		</li>
	);
}

/** A section the page tucks away until the owner opens it. */
function Tucked(props: { label: string; children: React.ReactNode }) {
	const [open, setOpen] = useState(false);
	return (
		<section className={styles.section}>
			<Button expanded={open} onClick={() => setOpen((on) => !on)}>
				{props.label}
			</Button>
			{open && props.children}
		</section>
	);
}

/** "Here's what you work on": a project each, with Team, Watch or Ignore already chosen (journey A, step 3). */
export function Found() {
	const { client } = useHub();
	const navigate = useNavigate();
	const first = useLocation().key === "default";
	const { status } = useFindingStatus();
	const { list, again } = useFoundList(status?.version);
	const { list: teams } = useTeamList();
	const [chosen, setChosen] = useState<Record<string, Choice>>({});
	const [said, setSaid] = useState<string>();
	const [locating, setLocating] = useState<FoundRow>();
	const [adding, setAdding] = useState(false);

	// What the owner has seen of the list is what "new projects found" counts from.
	useEffect(() => {
		keep(FOUND_SEEN, new Date().toISOString());
	}, []);

	const choose = async (row: FoundRow, choice: Choice) => {
		setSaid(undefined);
		const before = chosen[row.key];
		setChosen((all) => ({ ...all, [row.key]: choice }));
		try {
			await client?.call("finding.choose", { key: row.key, choice });
		} catch (e) {
			setChosen((all) => {
				const next = { ...all };
				if (before === undefined) delete next[row.key];
				else next[row.key] = before;
				return next;
			});
			setSaid(findingSaid(e, "foundChooseFailed"));
		}
	};
	const separate = async (row: FoundRow, folder: string) => {
		setSaid(undefined);
		try {
			await client?.call("finding.separate", { key: row.key, folder });
			again();
		} catch (e) {
			setSaid(findingSaid(e, "foundChooseFailed"));
		}
	};
	const currentChoice = (row: FoundRow) =>
		chosen[row.key] ?? row.choice ?? row.suggested ?? undefined;

	const nothing =
		list !== undefined &&
		status?.phase !== "running" &&
		list.main.length +
			list.explored.length +
			list.gone.length +
			list.computerJobs.length ===
			0;
	const items = (
		rows: FoundRow[],
		section: "main" | "explored" | "gone" | "job",
	) => (
		<ul className={styles.list}>
			{rows.map((row) => (
				<Item
					key={row.key}
					row={row}
					section={section}
					value={currentChoice(row)}
					teams={teams}
					onChoose={(choice) => choose(row, choice)}
					onSeparate={(folder) => separate(row, folder)}
					onLocate={() => setLocating(row)}
				/>
			))}
		</ul>
	);
	return (
		<Wizard
			step={0}
			title={nothing ? t("foundNothing") : t("foundTitle")}
			lead=""
			nav={<FindStepper current={0} />}
		>
			{nothing ? (
				<>
					<p>{t("foundNothingLead")}</p>
					<p>{t("foundNothingWeb")}</p>
					<div className={styles.actions}>
						<Button onClick={() => navigate("/find/export")}>
							{t("foundIUseClaudeAi")}
						</Button>
						<Button onClick={() => navigate("/setup/account")}>
							{t("foundSetUpOne")}
						</Button>
						<Button onClick={() => setAdding(true)}>
							{t("foundAddByHand")}
						</Button>
					</div>
				</>
			) : (
				<>
					<ul className={styles.legend}>
						<li>
							<strong>{t("foundTeam")}</strong> {t("foundTeamNote")}
						</li>
						<li>
							<strong>{t("foundWatch")}</strong> {t("foundWatchNote")}
						</li>
						<li>
							<strong>{t("foundIgnore")}</strong> {t("foundIgnoreNote")}
						</li>
					</ul>
					<p>{t("foundPicked")}</p>
					<ExportCard />
					{list && items(list.main, "main")}
					{list && list.explored.length > 0 && (
						<Tucked label={t("foundExplored", { count: list.explored.length })}>
							{items(list.explored, "explored")}
						</Tucked>
					)}
					{list && list.gone.length > 0 && (
						<Tucked label={t("foundGone", { count: list.gone.length })}>
							{items(list.gone, "gone")}
						</Tucked>
					)}
					{list && list.home.sessions > 0 && (
						<Tucked label={t("foundHome")}>
							<p>{t("foundHomeSessions", { count: list.home.sessions })}</p>
							{list.home.formedInto.length > 0 && (
								<p>
									{t("foundHomeFormed", {
										names: list.home.formedInto.join(", "),
									})}
								</p>
							)}
							<p>{t("foundHomeRest")}</p>
						</Tucked>
					)}
					{list && list.computerJobs.length > 0 && (
						<Tucked
							label={t("foundComputerJobs", {
								count: list.computerJobs.length,
							})}
						>
							{items(list.computerJobs, "job")}
						</Tucked>
					)}
				</>
			)}
			{said && <p role="alert">{said}</p>}
			<div className={styles.foot}>
				<Button onClick={() => (first ? navigate("/teams") : navigate(-1))}>
					{t("back")}
				</Button>
				{!nothing && (
					// Step 06b's "Do these work together?" comes next; until it lands, All teams.
					<Button kind="primary" onClick={() => navigate("/teams")}>
						{t("continue")}
					</Button>
				)}
			</div>
			{locating && (
				<LocateDialog
					row={locating}
					onClose={() => setLocating(undefined)}
					onLocated={() => {
						setLocating(undefined);
						again();
					}}
				/>
			)}
			{adding && (
				<AddProjectDialog
					onClose={() => setAdding(false)}
					onAdded={() => {
						setAdding(false);
						again();
					}}
				/>
			)}
		</Wizard>
	);
}

/** Where a project whose folder is gone is now: the owner picks the folder. */
function LocateDialog(props: {
	row: FoundRow;
	onClose: () => void;
	onLocated: () => void;
}) {
	const { client } = useHub();
	const [busy, setBusy] = useState(false);
	const [said, setSaid] = useState<string>();
	const use = async (path: string) => {
		setBusy(true);
		setSaid(undefined);
		try {
			await client?.call("finding.locate", { key: props.row.key, path });
			props.onLocated();
		} catch (e) {
			setSaid(findingSaid(e, "foundLocateFailed"));
		} finally {
			setBusy(false);
		}
	};
	return (
		<Dialog
			open
			title={t("foundLocateTitle", { name: props.row.name })}
			onClose={props.onClose}
		>
			<FolderBrowser mode="parent" busy={busy} onUse={use} />
			{said && <p role="alert">{said}</p>}
		</Dialog>
	);
}
