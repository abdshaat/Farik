import { Button } from "@farik/ui";
import { type ReactNode, useState } from "react";
import { useConnection } from "../app/connection.tsx";
import { type Refusal, said, saidAll } from "../app/refusals.ts";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";
import { PlanCheck } from "./setup/SetupAdvanced.tsx";
import { IntegrationChoice } from "./setup/SetupFinish.tsx";
import { PermissionChoices } from "./setup/SetupPermissions.tsx";
import { useDailyLimit } from "./setup/SetupSpending.tsx";
import { type Defaults, type Team, useDefaults } from "./setup/TeamSetup.tsx";
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
export function TeamRules() {
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
			<Plans key={`plans-${rounds.plans ?? 0}`} {...part("plans")} />
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
