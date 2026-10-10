import { Button, TextField } from "@catervas/ui";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useLocation } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { type Refusal, said, saidAll } from "../app/refusals.ts";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
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
 * Settings' "Your team's rules": setup's four questions after setup, each saved on its own
 * through `team.save`, its effect shown first (SPEC 4.4, 10).
 */
export function TeamRules({ advanced = false }: { advanced?: boolean }) {
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
	if (!data) return null;
	const part = (name: string) => ({
		saved: data.team,
		defaults,
		base,
		done: () => {
			setRounds((r) => ({ ...r, [name]: (r[name] ?? 0) + 1 }));
			again();
		},
	});
	return (
		<section className={styles.section} aria-labelledby="rules-area">
			<h2 id="rules-area">{t("rulesArea")}</h2>
			<p className={styles.muted}>{t("rulesNextSession")}</p>
			<May key={`may-${rounds.may ?? 0}`} {...part("may")} />
			{/* Its fields keep their own state, so a newly saved limit starts it again. */}
			<Spend
				key={`spend-${rounds.spend ?? 0}-${data.team.budgets.dailyUsd}`}
				{...part("spend")}
			/>
			<Finish key={`finish-${rounds.finish ?? 0}`} {...part("finish")} />
			<Planning
				key={`planning-${rounds.planning ?? 0}`}
				{...part("planning")}
			/>
			<Plans key={`plans-${rounds.plans ?? 0}`} {...part("plans")} />
			{advanced && (
				<UiPaths key={`paths-${rounds.paths ?? 0}`} {...part("paths")} />
			)}
		</section>
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

/** One part of the area, under its heading. */
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
			<h2 id={id}>{title}</h2>
			{children}
		</section>
	);
}

function May(part: Part) {
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
function Spend(part: Part) {
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

function Finish(part: Part) {
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

function Planning(part: Part) {
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

function Plans(part: Part) {
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
		>
			{foot}
		</PlanCheck>
	);
}

/**
 * "How to open your app" (step 12, D2): the preview's two commands, its port and first page,
 * asked only of a team with a UI/UX Designer, and saved on their own.
 */
export function HowToOpen() {
	const { data, again } = useQuery<{ team: Team }>("team.get", {});
	const [round, setRound] = useState(0);
	const designer = data?.team.agents.find(
		(a) => a.role === "ui_ux_designer" && a.status !== "retired",
	);
	if (!data || !designer) return null;
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
	// Today's "Open Settings" links here: the page goes to the section once it is drawn.
	const { hash } = useLocation();
	const here = useRef<HTMLElement>(null);
	useEffect(() => {
		if (hash === "#preview") here.current?.scrollIntoView();
	}, [hash]);
	return (
		<section
			ref={here}
			id="preview"
			className={styles.section}
			aria-labelledby="preview-heading"
		>
			<h2 id="preview-heading">{t("previewTitle")}</h2>
			<p>{t("previewLead", { designer })}</p>
			<PreviewFields
				id="preview"
				form={form}
				designer={designer}
				errors={errors}
				onChange={setMine}
			/>
			{foot}
			<p className={styles.muted}>{t("previewNotSure")}</p>
		</section>
	);
}

/** The team with `preview`, or without one when the fields are all empty. */
function withPreview(team: Team, preview: Team["preview"]): Team {
	const { preview: _, ...rest } = team;
	return preview ? { ...rest, preview } : rest;
}

/** Advanced: the globs that make a Developer's change one the Designer checks (spec 5.12). */
function UiPaths(part: Part) {
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
