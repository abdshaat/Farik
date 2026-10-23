import { Button, Segmented, TextField } from "@catervas/ui";
import { useState } from "react";
import { Link } from "../app/links.tsx";
import { ShowAdvanced } from "../components/ShowAdvanced.tsx";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";
import setup from "./setup/setup.module.css";
import {
	PutBack,
	type SessionLimits,
	type Team,
	useDefaults,
} from "./setup/TeamSetup.tsx";
import { Part, useChange } from "./TeamRules.tsx";

type RulePart = Omit<Parameters<typeof useChange>[0], never>;

/** The team with `patch` laid over its rules; a list left empty is left out, as a team file has it. */
export function withRules(team: Team, patch: Partial<Team["rules"]>): Team {
	const rules: Team["rules"] = { ...team.rules, ...patch };
	for (const [key, value] of Object.entries(rules)) {
		if (Array.isArray(value) && value.length === 0) delete rules[key];
		if (value === undefined) delete rules[key];
	}
	return { ...team, rules };
}

type Treatment = "off" | "read" | "allowed";

/** The five globs Catervas ships as off limits, each with its plain name and what lifting it risks. */
export const SHIPPED = [
	{ glob: ".env", name: "forbiddenSecrets", warn: "forbiddenSecretsWarn" },
	{ glob: ".env.*", name: "forbiddenSecrets", warn: "forbiddenSecretsWarn" },
	{ glob: "**/*.pem", name: "forbiddenKeys", warn: "forbiddenKeysWarn" },
	{ glob: "**/*.key", name: "forbiddenKeys", warn: "forbiddenKeysWarn" },
	{
		glob: ".catervas/local/**",
		name: "forbiddenCatervas",
		warn: "forbiddenCatervasWarn",
	},
] as const;
const CATERVAS_STATE = ".catervas/local/**";
const SHIPPED_GLOBS: string[] = SHIPPED.map((s) => s.glob);

const without = (list: string[] | undefined, glob: string) =>
	(list ?? []).filter((g) => g !== glob);
const plus = (list: string[] | undefined, glob: string) =>
	(list ?? []).includes(glob) ? (list ?? []) : [...(list ?? []), glob];

/**
 * "Files agents may not touch" (ADR 0080): each shipped glob is off limits, read only or allowed;
 * the owner adds their own, off limits or read only.
 */
export function ForbiddenFiles(
	part: { saved: Team } & Parameters<typeof useChange>[0],
) {
	const { saved } = part;
	const [mine, setMine] = useState<Team>();
	const team = mine ?? saved;
	const { liftedPaths, readOnlyPaths, protectedPaths } = team.rules;
	const { foot } = useChange(part, team);

	const treatment = (glob: string): Treatment =>
		liftedPaths?.includes(glob)
			? readOnlyPaths?.includes(glob)
				? "read"
				: "allowed"
			: "off";
	const choose = (glob: string, to: Treatment) =>
		setMine(
			withRules(team, {
				liftedPaths:
					to === "off" ? without(liftedPaths, glob) : plus(liftedPaths, glob),
				readOnlyPaths:
					to === "read"
						? plus(readOnlyPaths, glob)
						: without(readOnlyPaths, glob),
			}),
		);

	return (
		<Part id="rules-files" title={t("rulesFilesTitle")}>
			<p className={styles.muted}>{t("rulesFilesLead")}</p>
			{SHIPPED.map(({ glob, name, warn }) => {
				const now = treatment(glob);
				const options = [
					{ value: "off" as const, label: t("filesOffLimits") },
					{ value: "read" as const, label: t("filesReadOnly") },
					// Catervas's own files stay unwritable whatever the owner sets (ADR 0080, decision 4).
					...(glob === CATERVAS_STATE
						? []
						: [{ value: "allowed" as const, label: t("filesAllowed") }]),
				];
				return (
					<div key={glob}>
						<Segmented
							name={`file-${glob}`}
							legend={t("forbiddenRowLegend", { name: t(name), glob })}
							options={options}
							value={now}
							onChange={(to) => choose(glob, to)}
						/>
						{now !== "off" && <p className={styles.warn}>{t(warn)}</p>}
					</div>
				);
			})}
			<h4>{t("filesOwnTitle")}</h4>
			<OwnFiles
				idPrefix="files"
				protectedPaths={protectedPaths}
				readOnlyPaths={readOnlyPaths}
				exclude={SHIPPED_GLOBS}
				onChange={(next) => setMine(withRules(team, next))}
			/>
			{foot}
		</Part>
	);
}

/**
 * The owner's own globs, off limits or read only, each removable, with a field to add one. It
 * holds no team: `onChange` gets both lists, for the Rules page and for one agent's page.
 */
export function OwnFiles({
	idPrefix,
	protectedPaths,
	readOnlyPaths,
	exclude = [],
	onChange,
}: {
	idPrefix: string;
	protectedPaths: string[] | undefined;
	readOnlyPaths: string[] | undefined;
	/** Globs listed elsewhere, such as the five shipped ones. */
	exclude?: string[];
	onChange: (next: {
		protectedPaths: string[];
		readOnlyPaths: string[];
	}) => void;
}) {
	const [adding, setAdding] = useState("");
	const [addKind, setAddKind] = useState<"off" | "read">("off");
	const own = [
		...(protectedPaths ?? []).map((glob) => ({ glob, kind: "off" as const })),
		...(readOnlyPaths ?? []).map((glob) => ({ glob, kind: "read" as const })),
	].filter((o) => !exclude.includes(o.glob));
	const setOwn = (glob: string, kind: "off" | "read" | "none") =>
		onChange({
			protectedPaths:
				kind === "off"
					? plus(protectedPaths, glob)
					: without(protectedPaths, glob),
			readOnlyPaths:
				kind === "read"
					? plus(readOnlyPaths, glob)
					: without(readOnlyPaths, glob),
		});
	const add = () => {
		const glob = adding.trim();
		if (glob && !exclude.includes(glob)) setOwn(glob, addKind);
		setAdding("");
	};
	return (
		<>
			{own.length > 0 && (
				<ul className={styles.globs}>
					{own.map(({ glob, kind }) => (
						<li key={glob}>
							<Segmented
								name={`${idPrefix}-own-${glob}`}
								legend={glob}
								options={[
									{ value: "off", label: t("filesOffLimits") },
									{ value: "read", label: t("filesReadOnly") },
								]}
								value={kind}
								onChange={(to) => setOwn(glob, to)}
							/>
							<Button kind="quiet" onClick={() => setOwn(glob, "none")}>
								{t("filesRemove")} <span className={setup.hidden}>{glob}</span>
							</Button>
						</li>
					))}
				</ul>
			)}
			<TextField
				id={`${idPrefix}-add`}
				label={t("filesAdd")}
				value={adding}
				onChange={setAdding}
			/>
			<Segmented
				name={`${idPrefix}-add-kind`}
				legend={t("filesAddKind")}
				options={[
					{ value: "off", label: t("filesOffLimits") },
					{ value: "read", label: t("filesReadOnly") },
				]}
				value={addKind}
				onChange={setAddKind}
			/>
			<div className={styles.actions}>
				<Button onClick={add} disabled={adding.trim() === ""}>
					{t("filesAddButton")}
				</Button>
			</div>
		</>
	);
}

/** Words as a pattern that matches them at the start of a command: metacharacters escaped. */
const escaped = (words: string) => words.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
/** The words a pattern is exactly `^` and, or nothing when it is any other shape. */
function plainWords(pattern: string): string | undefined {
	if (!pattern.startsWith("^")) return undefined;
	const body = pattern.slice(1);
	const words = body.replace(/\\(.)/g, "$1");
	return escaped(words) === body ? words : undefined;
}

/** "Commands agents may not run": the owner's words, or under advanced a raw pattern. */
export function CommandList({
	idPrefix,
	patterns,
	onChange,
	raw: rawAllowed = true,
}: {
	idPrefix: string;
	patterns: string[];
	onChange: (next: string[]) => void;
	/** Whether the list offers its own "Show advanced settings" for a raw pattern. */
	raw?: boolean;
}) {
	const [adding, setAdding] = useState("");
	const [raw, setRaw] = useState("");
	const [advanced, setAdvanced] = useState(false);
	const push = (pattern: string) => {
		if (!patterns.includes(pattern)) onChange([...patterns, pattern]);
	};
	return (
		<>
			{patterns.length > 0 && (
				<ul className={styles.globs}>
					{patterns.map((pattern) => {
						const words = plainWords(pattern);
						return (
							<li key={pattern}>
								{words !== undefined ? (
									<span>{words}</span>
								) : (
									<code>{pattern}</code>
								)}
								<Button
									kind="quiet"
									onClick={() =>
										onChange(patterns.filter((p) => p !== pattern))
									}
								>
									{t("commandsRemove")}{" "}
									<span className={setup.hidden}>{words ?? pattern}</span>
								</Button>
							</li>
						);
					})}
				</ul>
			)}
			<TextField
				id={`${idPrefix}-add`}
				label={t("commandsAdd")}
				value={adding}
				onChange={setAdding}
			/>
			<div className={styles.actions}>
				<Button
					disabled={adding.trim() === ""}
					onClick={() => {
						push(`^${escaped(adding.trim())}`);
						setAdding("");
					}}
				>
					{t("commandsAddButton")}
				</Button>
			</div>
			{rawAllowed && (
				<ShowAdvanced
					open={advanced}
					onToggle={() => setAdvanced(!advanced)}
					controls={`${idPrefix}-raw`}
				/>
			)}
			{rawAllowed && advanced && (
				<div id={`${idPrefix}-raw`}>
					<TextField
						id={`${idPrefix}-raw-add`}
						label={t("commandsRawAdd")}
						hint={t("commandsRawHint")}
						value={raw}
						onChange={setRaw}
					/>
					<div className={styles.actions}>
						<Button
							disabled={raw.trim() === ""}
							onClick={() => {
								push(raw.trim());
								setRaw("");
							}}
						>
							{t("commandsAddButton")}
						</Button>
					</div>
				</div>
			)}
		</>
	);
}

/** "Commands agents may not run": the owner's words, or under advanced a raw pattern. */
export function Commands(part: RulePart) {
	const { saved } = part;
	const [mine, setMine] = useState<string[]>();
	const next = mine ? withRules(saved, { forbiddenCommands: mine }) : saved;
	const { foot } = useChange(part, next);
	return (
		<Part id="rules-commands" title={t("rulesCommandsTitle")}>
			<p className={styles.muted}>{t("commandsLead")}</p>
			<CommandList
				idPrefix="command"
				patterns={mine ?? saved.rules.forbiddenCommands ?? []}
				onChange={setMine}
			/>
			{foot}
		</Part>
	);
}

/** The git that never changes anything, which every command may run (ADR 0079). */
const READ_ONLY_GIT = [
	"status",
	"diff",
	"log",
	"show",
	"ls-files",
	"grep",
	"rev-parse",
	"blame",
];
const GIT_WORD = /^[a-z][a-z0-9-]{0,39}$/;

/**
 * The git the owner lists, with a field to add one. The eight that only read are shown as always
 * allowed on the Rules page (`always`), not on an agent's.
 */
export function GitList({
	idPrefix,
	listed,
	always,
	onChange,
}: {
	idPrefix: string;
	listed: string[];
	always: boolean;
	onChange: (next: string[]) => void;
}) {
	const [adding, setAdding] = useState("");
	const word = adding.trim();
	const wrong = word !== "" && !GIT_WORD.test(word);
	return (
		<>
			<ul className={styles.globs}>
				{always &&
					READ_ONLY_GIT.map((sub) => (
						<li key={sub}>
							<code>{sub}</code>
							<span className={styles.muted}>{t("gitAlways")}</span>
						</li>
					))}
				{listed.map((sub) => (
					<li key={sub}>
						<code>{sub}</code>
						<Button
							kind="quiet"
							onClick={() => onChange(listed.filter((g) => g !== sub))}
						>
							{t("gitRemove")} <span className={setup.hidden}>{sub}</span>
						</Button>
					</li>
				))}
			</ul>
			{listed.includes("push") && (
				<p className={styles.warn}>{t("gitPushNote")}</p>
			)}
			<TextField
				id={`${idPrefix}-add`}
				label={t("gitAdd")}
				value={adding}
				onChange={setAdding}
				{...(wrong && { error: t("gitAddWrong") })}
			/>
			<div className={styles.actions}>
				<Button
					disabled={word === "" || wrong}
					onClick={() => {
						if (!listed.includes(word) && !READ_ONLY_GIT.includes(word))
							onChange([...listed, word]);
						setAdding("");
					}}
				>
					{t("gitAddButton")}
				</Button>
			</div>
		</>
	);
}

/** "Git in agents' commands": the eight that read, always allowed, and the owner's own. */
export function Git(part: RulePart) {
	const { saved } = part;
	const [mine, setMine] = useState<string[]>();
	const next = mine ? withRules(saved, { gitSubcommands: mine }) : saved;
	const { foot } = useChange(part, next);
	return (
		<Part id="rules-git" title={t("rulesGitTitle")}>
			<p className={styles.muted}>{t("gitLead")}</p>
			<GitList
				idPrefix="git"
				listed={mine ?? saved.rules.gitSubcommands ?? []}
				always
				onChange={setMine}
			/>
			{foot}
		</Part>
	);
}

const REMOTE_NAME = /^[A-Za-z0-9][A-Za-z0-9._-]{0,99}$/;

/** "Where agents may push": shown once any agent can push, otherwise a pointer to Allowed actions. */
export function Remotes(part: RulePart) {
	const { saved } = part;
	const [mine, setMine] = useState<string[]>();
	const [adding, setAdding] = useState("");
	const remotes = mine ?? saved.policy.pushRemotes ?? ["origin"];
	const next = mine
		? { ...saved, policy: { ...saved.policy, pushRemotes: mine } }
		: saved;
	const { foot } = useChange(part, next);
	const canPush =
		saved.policy.permissions?.push === true ||
		saved.agents.some((a) => a.grants?.includes("git_remote"));
	if (!canPush)
		return (
			<Part id="rules-remotes" title={t("rulesRemotesTitle")}>
				<p>
					{t("remotesNone")}{" "}
					<Link to="/settings/allowed-actions">
						{t("sectionAllowedActions")}
					</Link>
					.
				</p>
			</Part>
		);
	const name = adding.trim();
	const wrong = name !== "" && !REMOTE_NAME.test(name);
	const full = remotes.length >= 20;
	return (
		<Part id="rules-remotes" title={t("rulesRemotesTitle")}>
			<p className={styles.muted}>{t("remotesLead")}</p>
			<ul className={styles.globs}>
				{remotes.map((remote) => (
					<li key={remote}>
						<code>{remote}</code>
						<Button
							kind="quiet"
							disabled={remotes.length === 1}
							onClick={() => setMine(remotes.filter((r) => r !== remote))}
						>
							{t("remotesRemove")}{" "}
							<span className={setup.hidden}>{remote}</span>
						</Button>
					</li>
				))}
			</ul>
			{remotes.length === 1 && (
				<p className={styles.muted}>{t("remotesLast")}</p>
			)}
			<TextField
				id="remote-add"
				label={t("remotesAdd")}
				value={adding}
				onChange={setAdding}
				{...(wrong && { error: t("remotesAddWrong") })}
			/>
			<div className={styles.actions}>
				<Button
					disabled={name === "" || wrong || full}
					onClick={() => {
						if (!remotes.includes(name)) setMine([...remotes, name]);
						setAdding("");
					}}
				>
					{t("remotesAddButton")}
				</Button>
			</div>
			{foot}
		</Part>
	);
}

type LimitKey =
	| "readinessAttempts"
	| "maxIterations"
	| "blockedLimitHours"
	| "escalationAgeHours"
	| "wipLimitPerAgent"
	| "paidCallsPerHour";
/** The six limits: their words, range, and the number a team file without them runs on. */
const LIMITS: {
	key: LimitKey;
	label:
		| "limitsReadiness"
		| "limitsIterations"
		| "limitsBlocked"
		| "limitsEscalation"
		| "limitsWip"
		| "limitsPaidCalls";
	min: number;
	max: number;
	usually: number;
}[] = [
	{
		key: "readinessAttempts",
		label: "limitsReadiness",
		min: 1,
		max: 100,
		usually: 3,
	},
	{
		key: "maxIterations",
		label: "limitsIterations",
		min: 1,
		max: 100,
		usually: 3,
	},
	{
		key: "blockedLimitHours",
		label: "limitsBlocked",
		min: 1,
		max: 720,
		usually: 24,
	},
	{
		key: "escalationAgeHours",
		label: "limitsEscalation",
		min: 1,
		max: 720,
		usually: 24,
	},
	{ key: "wipLimitPerAgent", label: "limitsWip", min: 0, max: 100, usually: 1 },
	{
		key: "paidCallsPerHour",
		label: "limitsPaidCalls",
		min: 1,
		max: 1000,
		usually: 30,
	},
];

/** "When work stops and asks you": six whole numbers, each saved into `policy`. */
export function Limits(part: RulePart) {
	const { saved } = part;
	const defaults = useDefaults();
	const [typed, setTyped] = useState<Partial<Record<LimitKey, string>>>({});
	const [cleared, setCleared] = useState(false);
	const policy = { ...saved.policy };
	let wrong = false;
	for (const l of LIMITS) {
		const text = typed[l.key];
		if (text === undefined) continue;
		const n = Number(text);
		if (text.trim() === "") continue;
		if (Number.isInteger(n) && n >= l.min && n <= l.max) policy[l.key] = n;
		else wrong = true;
	}
	if (cleared)
		for (const l of LIMITS) {
			const was = defaults?.policy[l.key];
			if (typeof was === "number") policy[l.key] = was;
			else if (
				l.key === "readinessAttempts" ||
				l.key === "escalationAgeHours" ||
				l.key === "paidCallsPerHour"
			)
				delete policy[l.key];
		}
	const next = { ...saved, policy };
	const { foot } = useChange(part, next, { wrong });
	return (
		<Part id="rules-limits" title={t("rulesLimitsTitle")}>
			<p className={styles.muted}>{t("limitsLead")}</p>
			{LIMITS.map((l) => {
				const text = typed[l.key];
				const n = Number(text);
				const bad =
					text !== undefined &&
					text.trim() !== "" &&
					!(Number.isInteger(n) && n >= l.min && n <= l.max);
				const shown =
					text ??
					String(cleared ? (policy[l.key] ?? "") : (saved.policy[l.key] ?? ""));
				return (
					<TextField
						key={l.key}
						id={`limit-${l.key}`}
						label={t(l.label)}
						hint={t("limitsUsually", {
							n: Number(defaults?.policy[l.key] ?? l.usually),
						})}
						value={shown}
						onChange={(v) => setTyped({ ...typed, [l.key]: v })}
						{...(bad && {
							error: t("limitsWrong", { min: l.min, max: l.max }),
						})}
					/>
				);
			})}
			<PutBack
				onClick={() => {
					setTyped({});
					setCleared(true);
				}}
			/>
			{foot}
		</Part>
	);
}

type SessionField =
	| "maxInputTokens"
	| "maxOutputTokens"
	| "maxWallClockSeconds"
	| "maxToolCalls";
/** The four session limits: their words, and how many of the stored unit make one shown. */
const SESSION_FIELDS: {
	key: SessionField;
	label: "sessionInput" | "sessionOutput" | "sessionMinutes" | "sessionCalls";
	unit: number;
}[] = [
	{ key: "maxInputTokens", label: "sessionInput", unit: 1000 },
	{ key: "maxOutputTokens", label: "sessionOutput", unit: 1000 },
	{ key: "maxWallClockSeconds", label: "sessionMinutes", unit: 60 },
	{ key: "maxToolCalls", label: "sessionCalls", unit: 1 },
];

/** The session limits as fields: blank keeps the role's. Used for the team and for one agent. */
export function SessionFields({
	idPrefix,
	value,
	onChange,
}: {
	idPrefix: string;
	value: SessionLimits | undefined;
	onChange: (next: SessionLimits | undefined, wrong: boolean) => void;
}) {
	const [typed, setTyped] = useState<Partial<Record<SessionField, string>>>({});
	const change = (key: SessionField, unit: number, text: string) => {
		const all = { ...typed, [key]: text };
		setTyped(all);
		const next: SessionLimits = { ...value };
		let wrong = false;
		for (const f of SESSION_FIELDS) {
			const entry = all[f.key];
			if (entry === undefined) continue;
			delete next[f.key];
			if (entry.trim() === "") continue;
			const n = Math.round(Number(entry) * f.unit);
			if (Number.isFinite(n) && n >= 1) next[f.key] = n;
			else wrong = true;
		}
		onChange(Object.keys(next).length > 0 ? next : undefined, wrong);
	};
	return (
		<>
			{SESSION_FIELDS.map((f) => {
				const stored = value?.[f.key];
				const text = typed[f.key];
				const bad =
					text !== undefined &&
					text.trim() !== "" &&
					!(Math.round(Number(text) * f.unit) >= 1);
				return (
					<TextField
						key={f.key}
						id={`${idPrefix}-${f.key}`}
						label={t(f.label)}
						value={
							text ?? (stored === undefined ? "" : String(stored / f.unit))
						}
						onChange={(v) => change(f.key, f.unit, v)}
						{...(bad && { error: t("sessionWrong") })}
					/>
				);
			})}
		</>
	);
}

/** "How long one session may run": the team's session limits, in minutes and thousands of tokens. */
export function SessionBudget(part: RulePart) {
	const { saved } = part;
	const [mine, setMine] = useState<
		{ value: SessionLimits | undefined } | undefined
	>();
	const [wrong, setWrong] = useState(false);
	const [round, setRound] = useState(0);
	const value = mine ? mine.value : saved.budgets.session;
	const { session: _, ...rest } = saved.budgets;
	const next = mine
		? {
				...saved,
				budgets: { ...rest, ...(mine.value && { session: mine.value }) },
			}
		: saved;
	const { foot } = useChange(part, next, { wrong });
	return (
		<Part id="rules-session" title={t("rulesSessionTitle")}>
			<p className={styles.muted}>{t("sessionLead")}</p>
			<SessionFields
				key={round}
				idPrefix="team-session"
				value={value}
				onChange={(v, bad) => {
					setMine({ value: v });
					setWrong(bad);
				}}
			/>
			<PutBack
				onClick={() => {
					setMine({ value: undefined });
					setWrong(false);
					setRound(round + 1);
				}}
			/>
			{foot}
		</Part>
	);
}
