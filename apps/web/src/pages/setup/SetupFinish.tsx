import { Button, Choice, Switch } from "@farik/ui";
import { useNavigate } from "react-router";
import { t } from "../../strings/t.ts";
import styles from "./setup.module.css";
import { PutBack, useDefaults, useSetup, useStart } from "./TeamSetup.tsx";
import { Wizard } from "./Wizard.tsx";

type Integration = "auto_merge" | "pull_request" | "manual";

/** How accepted work is added, and "Put back the default": setup's and Settings'. */
export function IntegrationChoice({
	value,
	onChange,
	putBack,
}: {
	value: string;
	onChange: (integration: string) => void;
	putBack: (() => void) | undefined;
}) {
	return (
		<>
			<div className={styles.card}>
				<Choice<Integration>
					name="integration"
					legend={t("finishChoice")}
					value={value as Integration}
					onChange={onChange}
					options={[
						{
							value: "auto_merge",
							label: t("finishAuto"),
							description: t("finishAutoNote"),
						},
						{
							value: "pull_request",
							label: t("finishPullRequest"),
							description: t("finishPullRequestNote"),
						},
						{
							value: "manual",
							label: t("finishManual"),
							description: t("finishManualNote"),
						},
					]}
				/>
			</div>
			<PutBack onClick={putBack} />
		</>
	);
}

/** Setup's last step: what happens to accepted work, then "Start the team". */
export function SetupFinish() {
	const navigate = useNavigate();
	const { draft, change } = useSetup();
	const { start, busy, refused } = useStart();
	const defaults = useDefaults();
	const integrate = (integration: string) =>
		change({
			...draft,
			team: { ...draft.team, policy: { ...draft.team.policy, integration } },
		});

	return (
		<Wizard step={7} title={t("finishTitle")} lead={t("finishLead")}>
			<IntegrationChoice
				value={draft.team.policy.integration}
				onChange={integrate}
				putBack={defaults && (() => integrate(defaults.policy.integration))}
			/>
			<p className={styles.note}>{t("finishSafe")}</p>
			<Switch
				id="advanced"
				label={t("advancedSwitch")}
				checked={false}
				onChange={() => navigate("/setup/advanced")}
			/>
			{refused && (
				<p role="alert" className={styles.alert}>
					{refused}
				</p>
			)}
			<div className={styles.foot}>
				<Button onClick={() => navigate("/setup/spending")}>{t("back")}</Button>
				<Button kind="primary" busy={busy} onClick={start}>
					{t("startTeam")}
				</Button>
			</div>
		</Wizard>
	);
}
