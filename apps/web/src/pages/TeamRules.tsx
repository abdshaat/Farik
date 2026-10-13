import { Button, InfoTip, Switch, TextField } from "@catervas/ui";
import { type ReactNode, useState } from "react";
import { useConnection } from "../app/connection.tsx";
import { type Refusal, said, saidAll } from "../app/refusals.ts";
import { useQuery } from "../app/store.ts";
import { Bullets } from "../components/Bullets.tsx";
import { t } from "../strings/t.ts";
import type { FilesTree } from "./files.ts";
import styles from "./pages.module.css";
import {
	formOf,
	PreviewFields,
	type PreviewForm,
	previewOf,
} from "./setup/PreviewFields.tsx";
import { PlanCheck, SprintsSwitch } from "./setup/SetupAdvanced.tsx";
import { IntegrationChoice } from "./setup/SetupFinish.tsx";
import { PermissionChoices } from "./setup/SetupPermissions.tsx";
import { useDailyLimit } from "./setup/SetupSpending.tsx";
import setup from "./setup/setup.module.css";
import {
	type Defaults,
	PutBack,
	type Team,
	useDefaults,
} from "./setup/TeamSetup.tsx";
import type { Judges } from "./Team.tsx";

type Checked = { errors: Refusal[]; effects: string[]; judges?: Judges };
type Part = {
	saved: Team;
	defaults: Defaults | undefined;
	/** The saved team's own check, which a part shows until it has a draft. */
	base: Checked | undefined;
	done: () => void;
};

/**
 * The team's rules after setup: setup's four questions, each part saved on its own through
 * `team.save`, its effect shown first (SPEC 4.4, 10). The hook is what the Settings pages share;
 * each page draws its own parts with `key(name)`, so saving one starts only that one again.
 */
export function useRuleParts() {
	const { data, again } = useQuery<{ team: Team }>("team.get", {});
	const defaults = useDefaults();
	const { data: base } = useQuery<Checked>(
		"team.validate",
		{ team: data?.team },
		!data,
	);
	// Each part's round: saving or cancelling one starts that one again, and no other, so the
	// others keep their drafts over the team as the daemon reads it again.
	const [rounds, setRounds] = useState<Record<string, number>>({});
	const part = (name: string): Part => ({
		saved: data?.team as Team,
		defaults,
		base,
		done: () => {
			setRounds((r) => ({ ...r, [name]: (r[name] ?? 0) + 1 }));
			again();
		},
	});
	// The spending fields keep their own state, so a newly saved limit starts them again.
	const key = (name: string) =>
		name === "spend"
			? `spend-${rounds.spend ?? 0}-${data?.team.budgets.dailyUsd}`
			: `${name}-${rounds[name] ?? 0}`;
	return { ready: Boolean(data), part, key };
}

/** A rules page's heading, with the note that a change starts with each agent's next work. */
export function RulesTitle({ name }: { name: string }) {
	return (
		<div className={styles.titled}>
			<h2 className={styles.pageHeading}>{name}</h2>
			<InfoTip id="rulesNextSession-info">
				<Bullets text={t("rulesNextSession")} />
			</InfoTip>
		</div>
	);
}

/** A change: checked by the daemon, its effects listed, then Save or Cancel. */
export function useChange(
	{ saved, base, done }: Omit<Part, "defaults">,
	next: Team,
	options: { wrong?: boolean; shownAbove?: string } = {},
) {
	const { client } = useConnection();
	const changed = JSON.stringify(next) !== JSON.stringify(saved);
	const { data: drafted } = useQuery<Checked>(
		"team.validate",
		{ team: next },
		!changed,
	);
	const checked = changed ? drafted : base;
	const errors = changed ? (checked?.errors ?? []) : [];
	const effects = changed ? (checked?.effects ?? []) : [];
	const [busy, setBusy] = useState(false);
	const [refused, setRefused] = useState<string>();
	const save = async () => {
		if (!client) return;
		setBusy(true);
		setRefused(undefined);
		try {
			await client.call("team.save", { team: next });
			done();
		} catch (e) {
			setRefused(saidAll(e));
			setBusy(false);
		}
	};
	const { shownAbove } = options;
	const below = errors.filter(
		(e) => !shownAbove || !e.path.startsWith(shownAbove),
	);
	const blocked = !changed || errors.length > 0 || Boolean(options.wrong);
	const preview = (
		<>
			{effects.length > 0 && (
				<div aria-live="polite">
					<p>
						<strong>{t("agentEffects")}</strong>
					</p>
					<ul>
						{effects.map((e) => (
							<li key={e}>{e}</li>
						))}
					</ul>
				</div>
			)}
			{(below.length > 0 || refused) && (
				<p role="alert" className={styles.alert}>
					{[...below.map((e) => said(e.code)), refused]
						.filter(Boolean)
						.join(" ")}
				</p>
			)}
		</>
	);
	const foot = (
		<>
			{preview}
			<div className={styles.actions}>
				<Button kind="primary" busy={busy} disabled={blocked} onClick={save}>
					{t("agentSave")}
				</Button>
				<Button disabled={!changed} onClick={done}>
					{t("agentCancel")}
				</Button>
			</div>
		</>
	);
	return { checked, errors, foot, preview, save, busy, blocked };
}

/** One part of a page, under its heading. */
function Part({
	id,
	title,
	children,
}: {
	id: string;
	title: string;
	children: ReactNode;
}) {
	return (
		<section className={styles.section} aria-labelledby={id}>
			<h3 id={id}>{title}</h3>
			{children}
		</section>
	);
}

export function May(part: Part) {
	const { saved, defaults } = part;
	const [mine, setMine] = useState<Team["policy"]["permissions"]>();
	// A team file without the answers runs on the defaults (SPEC 5.6).
	const now = mine ?? saved.policy.permissions ?? defaults?.policy.permissions;
	const next = mine
		? { ...saved, policy: { ...saved.policy, permissions: mine } }
		: saved;
	const { foot } = useChange(part, next);
	const developers = saved.agents.filter(
		(a) => a.role === "software_developer" && a.status !== "retired",
	);
	return (
		<Part id="rules-may" title={t("rulesMay")}>
			<PermissionChoices
				commands={now?.runCommands}
				push={now?.push}
				developers={developers}
				onAnswer={({ commands, push }) =>
					setMine({
						...now,
						...(commands !== undefined && { runCommands: commands }),
						...(push !== undefined && { push }),
					})
				}
				putBack={defaults && (() => setMine(defaults.policy.permissions))}
			/>
			{foot}
		</Part>
	);
}

/** The daily limit: the Costs page's own control, here beside the other rules. */
export function Spend(part: Part) {
	const { saved } = part;
	const { wrong, budgets, fields } = useDailyLimit(saved.budgets.dailyUsd);
	const usd = budgets(saved.budgets).dailyUsd;
	const next =
		usd === saved.budgets.dailyUsd
			? saved
			: { ...saved, budgets: budgets(saved.budgets) };
	const { foot } = useChange(part, next, { wrong });
	return (
		<Part id="rules-spend" title={t("rulesSpendTitle")}>
			{fields}
			{foot}
		</Part>
	);
}

export function Finish(part: Part) {
	const { saved, defaults } = part;
	const [mine, setMine] = useState<string>();
	const next = mine
		? { ...saved, policy: { ...saved.policy, integration: mine } }
		: saved;
	const { foot } = useChange(part, next);
	return (
		<Part id="rules-finish" title={t("rulesFinish")}>
			<IntegrationChoice
				value={mine ?? saved.policy.integration}
				onChange={setMine}
				putBack={defaults && (() => setMine(defaults.policy.integration))}
			/>
			{foot}
		</Part>
	);
}

export function Planning(part: Part) {
	const { saved, defaults } = part;
	const [mine, setMine] = useState<boolean>();
	// An old team file without the key does not plan in sprints.
	const was = saved.policy.planInSprints === true;
	const now = mine ?? was;
	const next =
		now === was
			? saved
			: { ...saved, policy: { ...saved.policy, planInSprints: now } };
	const { foot } = useChange(part, next);
	return (
		<Part id="rules-planning" title={t("rulesPlanning")}>
			<SprintsSwitch on={now} onChange={setMine} />
			<PutBack
				onClick={
					defaults && (() => setMine(defaults.policy.planInSprints === true))
				}
			/>
			{foot}
		</Part>
	);
}

/** A Markdown document the rewrite would take: a marketing plan stays as it was approved. */
const rewritable = (path: string) =>
	path.endsWith(".md") &&
	!/\/marketing\/plans\/MP-[1-9]\d*(\.agent)?\.md$/.test(path);

/** "Documents": the format new ones are written in, and the rewrite of the Markdown that remains. */
export function Documents(part: Part) {
	const { saved, defaults } = part;
	const { client } = useConnection();
	const { data: tree } = useQuery<FilesTree>("files.tree", {});
	const [mine, setMine] = useState<boolean>();
	// A team file without the key writes HTML.
	const was = saved.policy.documentFormat !== "markdown";
	const now = mine ?? was;
	const next =
		now === was
			? saved
			: {
					...saved,
					policy: {
						...saved.policy,
						documentFormat: now ? ("html" as const) : ("markdown" as const),
					},
				};
	const { foot } = useChange(part, next);
	const [pending, setPending] = useState(false);
	const [state, setState] = useState<
		{ started: boolean } | { refused: string }
	>();
	const markdown = tree?.folders.some((f) =>
		f.documents.some((d) => [d.path, d.twin].some((p) => p && rewritable(p))),
	);
	const rewrite = async () => {
		if (!client) return;
		setPending(true);
		try {
			await client.call("files.rewrite", {});
			setState({ started: true });
		} catch (e) {
			setState({ refused: saidAll(e) });
		}
		setPending(false);
	};
	return (
		<Part id="rules-documents" title={t("rulesDocuments")}>
			<Switch
				id="document-format"
				label={t("documentsHtml")}
				info={<Bullets text={t("documentsHtmlNote")} />}
				checked={now}
				onChange={setMine}
			/>
			<PutBack
				onClick={
					defaults &&
					(() => setMine(defaults.policy.documentFormat !== "markdown"))
				}
			/>
			{foot}
			{was && markdown && (
				<div>
					<Button busy={pending} onClick={rewrite}>
						{t("documentsRewrite")}
					</Button>
					<InfoTip id="documents-rewrite-info">
						<Bullets text={t("documentsRewriteNote")} />
					</InfoTip>
					{state && "started" in state && (
						<p role="status">{t("documentsRewriteStarted")}</p>
					)}
					{state && "refused" in state && (
						<p role="alert" className={styles.alert}>
							{state.refused}
						</p>
					)}
				</div>
			)}
		</Part>
	);
}

export function Plans(part: Part) {
	const { saved, defaults } = part;
	const [mine, setMine] = useState<Team["policy"]["judgment"]>();
	const next = mine
		? { ...saved, policy: { ...saved.policy, judgment: mine } }
		: saved;
	const { checked, errors, foot } = useChange(part, next, {
		shownAbove: "/policy/judgment",
	});
	return (
		<PlanCheck
			judgment={mine ?? saved.policy.judgment ?? defaults?.policy.judgment}
			onChange={setMine}
			judges={checked?.judges}
			errors={errors}
			putBack={defaults && (() => setMine(defaults.policy.judgment ?? {}))}
			heading="h3"
		>
			{foot}
		</PlanCheck>
	);
}

/**
 * "How to open your app" (step 12, D2): the preview's two commands, its port and first page,
 * asked only of a team with a UI/UX Designer, and saved on their own.
 */
export function HowToOpen({ none }: { none?: ReactNode }) {
	const { data, again } = useQuery<{ team: Team }>("team.get", {});
	const [round, setRound] = useState(0);
	const designer = data?.team.agents.find(
		(a) => a.role === "ui_ux_designer" && a.status !== "retired",
	);
	if (!data) return null;
	if (!designer) return none ?? null;
	return (
		<Preview
			key={round}
			saved={data.team}
			designer={designer.displayName}
			done={() => {
				setRound((n) => n + 1);
				again();
			}}
		/>
	);
}

function Preview({
	saved,
	designer,
	done,
}: {
	saved: Team;
	designer: string;
	done: () => void;
}) {
	const [mine, setMine] = useState<PreviewForm>();
	const form = mine ?? formOf(saved.preview);
	const next = mine ? withPreview(saved, previewOf(mine)) : saved;
	const { errors, foot } = useChange({ saved, base: undefined, done }, next, {
		shownAbove: "/preview",
	});
	return (
		<section
			id="preview"
			className={styles.section}
			aria-labelledby="preview-heading"
		>
			<div className={styles.titled}>
				<h2 id="preview-heading">{t("previewTitle")}</h2>
				<InfoTip id="previewMore-info">
					<Bullets text={t("previewMore")} />
				</InfoTip>
			</div>
			<Bullets text={t("previewLead", { designer })} />
			<PreviewFields
				id="preview"
				form={form}
				designer={designer}
				errors={errors}
				onChange={setMine}
			/>
			{foot}
		</section>
	);
}

/** The team with `preview`, or without one when the fields are all empty. */
function withPreview(team: Team, preview: Team["preview"]): Team {
	const { preview: _, ...rest } = team;
	return preview ? { ...rest, preview } : rest;
}

/** Advanced: the globs that make a Developer's change one the Designer checks (spec 5.12). */
export function UiPaths(part: Part) {
	const { saved, defaults } = part;
	const [mine, setMine] = useState<string[]>();
	const [adding, setAdding] = useState("");
	const paths = mine ?? saved.rules.uiPaths ?? defaults?.uiPaths ?? [];
	const next = mine
		? { ...saved, rules: { ...saved.rules, uiPaths: mine } }
		: saved;
	const { foot } = useChange(part, next);
	const add = () => {
		const glob = adding.trim();
		if (glob && !paths.includes(glob)) setMine([...paths, glob]);
		setAdding("");
	};
	return (
		<Part id="rules-paths" title={t("uiPathsTitle")}>
			<p className={styles.muted}>{t("uiPathsLead")}</p>
			<ul className={styles.globs}>
				{paths.map((glob) => (
					<li key={glob}>
						<code>{glob}</code>
						<Button
							kind="quiet"
							onClick={() => setMine(paths.filter((p) => p !== glob))}
						>
							{t("uiPathsRemove")} <span className={setup.hidden}>{glob}</span>
						</Button>
					</li>
				))}
			</ul>
			<TextField
				id="ui-path"
				label={t("uiPathsAdd")}
				value={adding}
				onChange={setAdding}
			/>
			<div className={styles.actions}>
				<Button onClick={add} disabled={adding.trim() === ""}>
					{t("uiPathsAddButton")}
				</Button>
				<PutBack onClick={defaults && (() => setMine(defaults.uiPaths))} />
			</div>
			{foot}
		</Part>
	);
}
