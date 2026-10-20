import { Button, Segmented, TextField } from "@catervas/ui";
import { useState } from "react";
import { Link } from "react-router";
import { ShowAdvanced } from "../components/ShowAdvanced.tsx";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";
import setup from "./setup/setup.module.css";
import type { Team } from "./setup/TeamSetup.tsx";
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
const SHIPPED = [
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
	const [adding, setAdding] = useState("");
	const [addKind, setAddKind] = useState<"off" | "read">("off");
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

	const own = [
		...(protectedPaths ?? []).map((glob) => ({ glob, kind: "off" as const })),
		...(readOnlyPaths ?? []).map((glob) => ({ glob, kind: "read" as const })),
	].filter((o) => !SHIPPED_GLOBS.includes(o.glob));
	const setOwn = (glob: string, kind: "off" | "read" | "none") =>
		setMine(
			withRules(team, {
				protectedPaths:
					kind === "off"
						? plus(protectedPaths, glob)
						: without(protectedPaths, glob),
				readOnlyPaths:
					kind === "read"
						? plus(readOnlyPaths, glob)
						: without(readOnlyPaths, glob),
			}),
		);
	const add = () => {
		const glob = adding.trim();
		if (glob && !SHIPPED_GLOBS.includes(glob)) setOwn(glob, addKind);
		setAdding("");
	};

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
			{own.length > 0 && (
				<ul className={styles.globs}>
					{own.map(({ glob, kind }) => (
						<li key={glob}>
							<Segmented
								name={`own-${glob}`}
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
				id="files-add"
				label={t("filesAdd")}
				value={adding}
				onChange={setAdding}
			/>
			<Segmented
				name="files-add-kind"
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
			{foot}
		</Part>
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
export function Commands(part: RulePart) {
	const { saved } = part;
	const [mine, setMine] = useState<string[]>();
	const [adding, setAdding] = useState("");
	const [raw, setRaw] = useState("");
	const [advanced, setAdvanced] = useState(false);
	const patterns = mine ?? saved.rules.forbiddenCommands ?? [];
	const next = mine ? withRules(saved, { forbiddenCommands: mine }) : saved;
	const { foot } = useChange(part, next);
	const push = (pattern: string) => {
		if (!patterns.includes(pattern)) setMine([...patterns, pattern]);
	};
	return (
		<Part id="rules-commands" title={t("rulesCommandsTitle")}>
			<p className={styles.muted}>{t("commandsLead")}</p>
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
									onClick={() => setMine(patterns.filter((p) => p !== pattern))}
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
				id="command-add"
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
			<ShowAdvanced
				open={advanced}
				onToggle={() => setAdvanced(!advanced)}
				controls="command-raw"
			/>
			{advanced && (
				<div id="command-raw">
					<TextField
						id="command-raw-add"
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

/** "Git in agents' commands": the eight that read, always allowed, and the owner's own. */
export function Git(part: RulePart) {
	const { saved } = part;
	const [mine, setMine] = useState<string[]>();
	const [adding, setAdding] = useState("");
	const listed = mine ?? saved.rules.gitSubcommands ?? [];
	const next = mine ? withRules(saved, { gitSubcommands: mine }) : saved;
	const { foot } = useChange(part, next);
	const word = adding.trim();
	const wrong = word !== "" && !GIT_WORD.test(word);
	return (
		<Part id="rules-git" title={t("rulesGitTitle")}>
			<p className={styles.muted}>{t("gitLead")}</p>
			<ul className={styles.globs}>
				{READ_ONLY_GIT.map((sub) => (
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
							onClick={() => setMine(listed.filter((g) => g !== sub))}
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
				id="git-add"
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
							setMine([...listed, word]);
						setAdding("");
					}}
				>
					{t("gitAddButton")}
				</Button>
			</div>
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
