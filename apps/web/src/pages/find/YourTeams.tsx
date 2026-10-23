import { Button, InfoTip, TextField } from "@catervas/ui";
import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { useHub } from "../../app/connection.tsx";
import { said } from "../../app/refusals.ts";
import { t } from "../../strings/t.ts";
import { repoNameOf } from "../settings/AddSecondRepository.tsx";
import { Wizard } from "../setup/Wizard.tsx";
import styles from "./find.module.css";
import { type FoundList, findingSaid, tilde } from "./finding.ts";
import { FindStepper } from "./Stepper.tsx";

/** A project of a card. */
type CardProject = {
	key: string;
	name: string;
	folder?: string;
	services: string[];
};

/** One team to make, or one to add projects to, as `proposal.get` gives it. */
type Card = {
	id: string;
	kind: "new_team" | "add_to_team";
	projects: CardProject[];
	jobs: string[];
	why: string[];
	name: string;
	purpose?: string;
	template: string;
	folder?: string;
	fromClaude: boolean;
	team?: string;
	teamName?: string;
};

/** `proposal.get`'s answer. */
type Proposal = {
	cards: Card[];
	templates: { slug: string; name: string }[];
};

/** `proposal.create`'s answer. */
type Created = {
	made: { id: string; slug: string; name: string; root: string }[];
	added: { id: string; team: string; names: string[] }[];
	failed: { id: string; name: string; reason: string; team?: string }[];
	split: { id: string; projects: string[] }[];
};

/** `amount` dollars, to the cent. */
function cents(amount: number): string {
	return `$${amount.toFixed(2)}`;
}

/** A refusal's words from the code its sentence begins with. */
function reasonSaid(reason: string): string {
	const code = /^([a-z_]+): /.exec(reason)?.[1];
	return said(
		code,
		{ why: reason.replace(/^[a-z_]+: /, "") },
		"yourTeamsFailed",
	);
}

/** A team of one repository that adding a card's projects would make a team of several. */
type Conversion = {
	id: string;
	team: string;
	teamName: string;
	/** The projects' names, for the sentence. */
	names: string;
	/** The first project's folder, relative to home. */
	path: string;
};

/** The code a failed card begins its reason with when its team must be converted first. */
const CONVERSION_NEEDED = "conversion_needed";

/** One team of the proposal: what it is made of, why, and what the owner can change. */
function TeamCard(props: {
	card: Card;
	templates: Proposal["templates"];
	busy: boolean;
	onEdit: (fields: { name?: string; template?: string }) => void;
	onMoveOut: (key: string) => void;
	onKeep: () => void;
}) {
	const { card } = props;
	const [renaming, setRenaming] = useState(false);
	const [draft, setDraft] = useState(card.name);
	const [moving, setMoving] = useState(false);
	const [why, setWhy] = useState(false);
	const adding = card.kind === "add_to_team";
	const services = [...new Set(card.projects.flatMap((p) => p.services))];
	return (
		<article className={styles.tcard}>
			<header className={styles.thead}>
				<h2>
					{adding
						? t("yourTeamAddTo", { team: card.teamName ?? card.team ?? "" })
						: card.name}
				</h2>
				{!adding && !renaming && (
					<Button
						kind="quiet"
						onClick={() => {
							setDraft(card.name);
							setRenaming(true);
						}}
					>
						{t("yourTeamRename")}
					</Button>
				)}
			</header>
			{renaming && (
				<div className={styles.actions}>
					<TextField
						id={`name-${card.id}`}
						label={t("yourTeamName")}
						value={draft}
						onChange={setDraft}
					/>
					<Button
						kind="primary"
						disabled={draft.trim() === ""}
						onClick={() => {
							props.onEdit({ name: draft.trim() });
							setRenaming(false);
						}}
					>
						{t("yourTeamSave")}
					</Button>
				</div>
			)}
			{card.purpose && !adding && <p>{card.purpose}</p>}
			<ul className={styles.tags}>
				{card.projects.flatMap((project) =>
					project.services.length > 0 && !project.folder
						? project.services.map((service) => (
								<li key={`${project.key}|${service}`} className={styles.ptag}>
									{service}
								</li>
							))
						: [
								<li key={project.key} className={styles.ptag}>
									{project.name}
								</li>,
							],
				)}
			</ul>
			{adding && (
				<p className={styles.muted}>
					{t("yourTeamAddedAs", { team: card.teamName ?? card.team ?? "" })}
				</p>
			)}
			{card.why.length > 0 ? (
				<>
					<p className={styles.proof}>
						<span>{card.why.join(" · ")}</span>{" "}
						<button
							type="button"
							className={styles.more}
							aria-label={t("yourTeamWhy")}
							aria-expanded={why}
							aria-controls={`why-${card.id}`}
							onClick={() => setWhy((on) => !on)}
						>
							ⓘ
						</button>
					</p>
					{why && (
						<div id={`why-${card.id}`} className={styles.panel}>
							<p>{t("togetherChecked")}</p>
						</div>
					)}
				</>
			) : (
				card.projects.length === 1 &&
				!card.folder && <p className={styles.muted}>{t("yourTeamOwn")}</p>
			)}
			{card.jobs.length > 0 && (
				<p>{t("yourTeamJobs", { jobs: card.jobs.join(", ") })}</p>
			)}
			{card.folder && (
				<>
					<p>
						{t("yourTeamNewFolder")} <code>{card.folder}</code>
					</p>
					<ul className={styles.notes}>
						<li>{t("yourTeamNoFolder")}</li>
						<li>{t("yourTeamMakesFolder")}</li>
						{services.length > 0 && (
							<li>
								{t("yourTeamServices", { services: services.join(", ") })}
							</li>
						)}
					</ul>
				</>
			)}
			{!adding && (
				<div className={styles.actions}>
					<label className={styles.field}>
						<span>{t("yourTeamTemplate")}</span>
						<select
							value={card.template}
							disabled={props.busy}
							onChange={(e) => props.onEdit({ template: e.target.value })}
						>
							{props.templates.map((each) => (
								<option key={each.slug} value={each.slug}>
									{each.name}
								</option>
							))}
						</select>
					</label>
					{card.projects.length > 1 && (
						<Button expanded={moving} onClick={() => setMoving((on) => !on)}>
							{t("yourTeamMoveOut")}
						</Button>
					)}
					{card.fromClaude && (
						<Button onClick={props.onKeep}>{t("namesKeep")}</Button>
					)}
				</div>
			)}
			{moving && (
				<div className={styles.actions}>
					{card.projects.map((project) => (
						<Button
							key={project.key}
							onClick={() => {
								setMoving(false);
								props.onMoveOut(project.key);
							}}
						>
							{t("yourTeamMoveOutOf", { name: project.name })}
						</Button>
					))}
				</div>
			)}
		</article>
	);
}

/** "Your teams": a card for each team to make, the names from Claude, and Create (journey A, step 5). */
export function YourTeams() {
	const { client } = useHub();
	const navigate = useNavigate();
	const [proposal, setProposal] = useState<Proposal>();
	const [estimate, setEstimate] = useState<number>();
	const [watched, setWatched] = useState<string[]>([]);
	const [cost, setCost] = useState<number>();
	const [busy, setBusy] = useState<"names" | "create">();
	const [problems, setProblems] = useState<string[]>([]);
	const [conversions, setConversions] = useState<Conversion[]>([]);

	const load = useCallback(async () => {
		if (!client) return;
		try {
			setProposal((await client.call("proposal.get", {})) as Proposal);
		} catch (e) {
			setProblems([findingSaid(e, "yourTeamsFailed")]);
		}
		client.call("proposal.names_estimate", {}).then(
			(answer) => setEstimate((answer as { usd: number }).usd),
			() => {},
		);
		client.call("finding.list", {}).then(
			(answer) =>
				setWatched(
					(answer as FoundList).main
						.filter(
							(row) =>
								(row.choice ?? row.suggested) === "watch" && !row.alreadyTeam,
						)
						.map((row) => row.name),
				),
			() => {},
		);
	}, [client]);
	useEffect(() => {
		void load();
	}, [load]);

	const change = async (
		method: "proposal.edit" | "proposal.move_out" | "proposal.revert",
		params: Record<string, unknown>,
	) => {
		setProblems([]);
		try {
			await client?.call(method, params);
		} catch (e) {
			setProblems([findingSaid(e, "yourTeamsFailed")]);
		}
		await load();
	};
	const suggest = async () => {
		setBusy("names");
		setProblems([]);
		try {
			const answer = (await client?.call("proposal.names", {
				confirmed: true,
			})) as { costUsd: number } | undefined;
			setCost(answer?.costUsd);
		} catch (e) {
			setProblems([findingSaid(e, "yourTeamsFailed")]);
		} finally {
			setBusy(undefined);
			await load();
		}
	};
	const create = async () => {
		setBusy("create");
		setProblems([]);
		try {
			const made = (await client?.call("proposal.create", {
				confirmed: true,
			})) as Created;
			// A team of one repository takes a second only once the owner agrees to the move.
			const convert = made.failed.filter(
				(each) => each.reason.startsWith(CONVERSION_NEEDED) && each.team,
			);
			setConversions(
				convert.map((each) => {
					const one = (proposal?.cards ?? []).find(
						(card) => card.id === each.id,
					);
					const folder = one?.projects[0]?.folder;
					const home = folder === undefined ? undefined : tilde(folder);
					return {
						id: each.id,
						team: each.team ?? "",
						teamName: one?.teamName ?? each.team ?? "",
						names:
							one?.projects.map((project) => project.name).join(", ") ?? "",
						path: (home?.startsWith("~/") ? home.slice(2) : folder) ?? "",
					};
				}),
			);
			const wrong = [
				...made.failed
					.filter((each) => !convert.includes(each))
					.map((each) =>
						t("yourTeamsFailedAt", {
							name: each.name,
							why: reasonSaid(each.reason),
						}),
					),
				...made.split.map(() => t("yourTeamsSplit")),
			];
			if (wrong.length === 0 && convert.length === 0) {
				navigate("/teams", {
					state: { created: made.made.length + made.added.length },
				});
				return;
			}
			setProblems(wrong);
			await load();
		} catch (e) {
			setProblems([findingSaid(e, "yourTeamsFailed")]);
		} finally {
			setBusy(undefined);
		}
	};

	const cards = proposal?.cards ?? [];
	const making = cards.filter((card) => card.kind === "new_team").length;
	return (
		<Wizard
			step={0}
			title={t("yourTeamsTitle")}
			lead={t("yourTeamsLead")}
			nav={<FindStepper current={2} />}
		>
			<section className={styles.card} aria-labelledby="names-title">
				<h2 id="names-title">{t("namesTitle")}</h2>
				<p>
					{t("namesAsk")}{" "}
					<InfoTip id="names-what" label={t("namesWhat")}>
						<ul>
							<li>{t("namesSees")}</li>
							<li>{t("namesKeys")}</li>
							<li>{t("namesPaid")}</li>
							<li>{t("namesNever")}</li>
						</ul>
					</InfoTip>
				</p>
				<div className={styles.actions}>
					<Button
						busy={busy === "names"}
						disabled={cards.length === 0 || estimate === undefined}
						onClick={suggest}
					>
						{t("namesSuggest", { amount: cents(estimate ?? 0) })}
					</Button>
					{cost !== undefined && (
						<span role="status">{t("namesCost", { amount: cents(cost) })}</span>
					)}
				</div>
			</section>
			{proposal && cards.length === 0 && <p>{t("yourTeamsNone")}</p>}
			<div className={styles.tcards}>
				{cards.map((card) => (
					<TeamCard
						key={card.id}
						card={card}
						templates={proposal?.templates ?? []}
						busy={busy !== undefined}
						onEdit={(fields) =>
							change("proposal.edit", { id: card.id, ...fields })
						}
						onMoveOut={(key) =>
							change("proposal.move_out", { id: card.id, key })
						}
						onKeep={() => change("proposal.revert", { id: card.id })}
					/>
				))}
			</div>
			{watched.length > 0 && (
				<p>{t("yourTeamsWatched", { names: watched.join(", ") })}</p>
			)}
			{problems.map((problem) => (
				<p key={problem} role="alert">
					{problem}
				</p>
			))}
			{conversions.map((one) => (
				<div key={one.id} className={styles.actions}>
					<p>
						{t("yourTeamsConvert", { project: one.names, team: one.teamName })}
					</p>
					<Button
						onClick={() =>
							navigate(`/t/${one.team}/settings/repositories`, {
								state: {
									addRepository: {
										path: one.path,
										name: repoNameOf(one.path.split("/").at(-1) ?? ""),
									},
								},
							})
						}
					>
						{t("reposContinue")}
					</Button>
				</div>
			))}
			<div className={styles.foot}>
				<Button onClick={() => navigate("/find/together")}>
					{t("yourTeamsBack")}
				</Button>
				<span className={styles.muted}>{t("yourTeamsFoot")}</span>
				<Button
					kind="primary"
					busy={busy === "create"}
					disabled={cards.length === 0}
					onClick={create}
				>
					{making === 0
						? t("yourTeamsAdd")
						: making === 1
							? t("yourTeamsCreateOne")
							: t("yourTeamsCreate", { count: making })}
				</Button>
			</div>
		</Wizard>
	);
}
