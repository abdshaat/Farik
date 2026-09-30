import { AVATAR_URLS, type AvatarKey, Button } from "@farik/ui";
import { useState } from "react";
import { useNavigate } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { type Refusal, refusalsOf, said } from "../../app/refusals.ts";
import { t } from "../../strings/t.ts";
import styles from "./setup.module.css";
import { roleName, someone, teamOf, useSetup } from "./TeamSetup.tsx";
import { Wizard } from "./Wizard.tsx";

type Checked = { errors: Refusal[] };

/** Setup's fifth step: the five suggested agents, named, and any second Developer. */
export function SetupTeam() {
	const { client } = useConnection();
	const navigate = useNavigate();
	const { draft, change } = useSetup();
	const [errors, setErrors] = useState<Checked["errors"]>([]);
	const [busy, setBusy] = useState(false);
	const on = draft.members.filter((m) => m.on);
	// An error's path names the agent by its place on the team, which leaves out the unticked.
	const at = (spot: number) =>
		errors.filter(
			(e) => e.path.split("/").slice(1, 3).join("/") === `agents/${spot}`,
		);
	const loose = errors.filter((e) => !/^\/agents\/\d+/.test(e.path));

	const set = (index: number, next: Partial<{ on: boolean; name: string }>) => {
		setErrors([]);
		change({
			...draft,
			members: draft.members.map((m, i) =>
				i !== index
					? m
					: {
							key: m.key,
							on: next.on ?? m.on,
							agent: {
								...m.agent,
								displayName: next.name ?? m.agent.displayName,
							},
						},
			),
		});
	};
	const add = () => {
		setErrors([]);
		const developer = draft.members.find(
			(m) => m.agent.role === "software_developer",
		)?.agent;
		if (!developer) return;
		change({
			...draft,
			members: [
				...draft.members,
				{
					agent: someone(
						draft.members.map((m) => m.agent),
						developer,
					),
					on: true,
					// An id is handed out again once its name changes; a row's key is its own.
					key: draft.members.length,
				},
			],
		});
	};
	const onward = async () => {
		if (!client) return;
		setBusy(true);
		try {
			const checked = (await client.query("team.validate", {
				team: teamOf(draft),
			})) as Checked;
			setErrors(checked.errors);
			if (checked.errors.length === 0) navigate("/setup/permissions");
		} catch (e) {
			setErrors(refusalsOf(e));
		}
		setBusy(false);
	};

	return (
		<Wizard step={4} title={t("teamTitle")} lead={t("teamLead")}>
			<ul className={styles.members} aria-label={t("teamMembers")}>
				{draft.members.map((member, index) => {
					const { agent, on: included } = member;
					const role = roleName(agent.role);
					const avatar = AVATAR_URLS[agent.avatar as AvatarKey];
					const nameId = `name-${index}`;
					const why = included ? at(on.indexOf(member)) : [];
					const whyId = `why-${index}`;
					return (
						<li key={member.key} className={styles.member}>
							<input
								type="checkbox"
								checked={included}
								aria-label={t("teamInclude").replace("{role}", role)}
								onChange={(e) => set(index, { on: e.target.checked })}
							/>
							{avatar && <img className={styles.face} src={avatar} alt="" />}
							<span className={styles.who}>
								<label htmlFor={nameId}>{role}</label>
								<input
									id={nameId}
									className={styles.input}
									value={agent.displayName}
									aria-label={t("teamName").replace("{role}", role)}
									aria-invalid={why.length > 0 || undefined}
									aria-describedby={why.length > 0 ? whyId : undefined}
									onChange={(e) => set(index, { name: e.target.value })}
								/>
							</span>
							<span className={styles.persona}>{agent.persona}</span>
							{why.length > 0 && (
								<span id={whyId} className={styles.error}>
									{why.map((e) => said(e.code)).join(" ")}
								</span>
							)}
						</li>
					);
				})}
			</ul>
			<span>
				<Button kind="quiet" onClick={add}>
					{t("teamAdd")}
				</Button>
			</span>
			{loose.length > 0 && (
				<p role="alert" className={styles.alert}>
					{loose.map((e) => said(e.code)).join(" ")}
				</p>
			)}
			<div className={styles.foot}>
				<Button onClick={() => navigate("/setup/scan")}>{t("back")}</Button>
				<Button
					kind="primary"
					busy={busy}
					disabled={on.length === 0}
					onClick={onward}
				>
					{t(on.length === 5 ? "teamContinueFive" : "teamContinue")}
				</Button>
			</div>
		</Wizard>
	);
}
