import { Button, Segmented, TextField } from "@catervas/ui";
import { useState } from "react";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";
import setup from "./setup/setup.module.css";
import type { Team } from "./setup/TeamSetup.tsx";
import { Part, useChange } from "./TeamRules.tsx";

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
