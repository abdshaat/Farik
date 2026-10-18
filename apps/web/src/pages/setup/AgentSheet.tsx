import {
	AVATAR_URLS,
	type AvatarKey,
	Button,
	Dialog,
	type Role,
	Switch,
	TextArea,
	TextField,
} from "@catervas/ui";
import type { ReactNode } from "react";
import { Bullets } from "../../components/Bullets.tsx";
import {
	type ModelChoice,
	ModelFields,
	type ModelOption,
} from "../../components/ModelFields.tsx";
import { t } from "../../strings/t.ts";
import styles from "./TeamPicker.module.css";
import { type Agent, ringOf, roleName } from "./TeamSetup.tsx";

/** The roles one agent alone, or an agent added in setup, may take, in the Team page's order. */
export const JOB_ROLES: Role[] = [
	"product_manager",
	"architect",
	"software_developer",
	"ui_ux_designer",
	"marketing_specialist",
	"finance_specialist",
	"procurement_specialist",
	"scrum_master",
];

/**
 * One agent's sheet: its picture and name, its job, its model, effort and temperature, what it
 * focuses on, and whether it is on the team. Every change goes into the draft at once; "Done"
 * only closes it.
 */
export function AgentSheet({
	agent,
	index,
	on,
	models,
	roleModel,
	jobs,
	solo,
	required,
	error,
	children,
	onChange,
	onRole,
	onToggle,
	onRemove,
	onClose,
}: {
	agent: Agent;
	index: number;
	on: boolean;
	models: ModelOption[];
	/** The role's own model and effort, which stand in for what the agent leaves out. */
	roleModel: ModelChoice;
	/** The jobs it may take; none when its job is fixed. */
	jobs: Role[];
	solo: boolean;
	/** The Product Manager every team needs: it cannot leave. */
	required: boolean;
	/** Why the team was refused, for this agent. */
	error?: string | undefined;
	/** More for this agent alone, such as how the Designer opens the app. */
	children?: ReactNode;
	onChange: (next: Agent) => void;
	onRole: (role: Role) => void;
	onToggle: (on: boolean) => void;
	onRemove?: (() => void) | undefined;
	onClose: () => void;
}) {
	const avatar = AVATAR_URLS[agent.avatar as AvatarKey];
	const role = roleName(agent.role);
	const id = `sheet-${index}`;
	return (
		<Dialog
			open
			title={agent.displayName.trim() || role}
			onClose={onClose}
			fillsPhone
			actions={
				<>
					{onRemove && (
						<Button kind="quiet" onClick={onRemove}>
							{t("sheetRemove")}
						</Button>
					)}
					<Button kind="primary" onClick={onClose}>
						{t("sheetDone")}
					</Button>
				</>
			}
		>
			<div className={styles.sheet}>
				<div className={styles.sheetHead}>
					{avatar && (
						<img
							className={styles.sheetFace}
							style={ringOf(agent.role)}
							src={avatar}
							alt=""
						/>
					)}
					<div className={styles.sheetWho}>
						<TextField
							id={`${id}-name`}
							label={t("sheetName")}
							value={agent.displayName}
							onChange={(displayName) => onChange({ ...agent, displayName })}
							{...(error && { error })}
						/>
						{jobs.length > 0 ? (
							<label className={styles.job}>
								<span>{t("sheetJob")}</span>
								<select
									value={agent.role}
									onChange={(e) => onRole(e.target.value as Role)}
								>
									{jobs.map((r) => (
										<option key={r} value={r}>
											{roleName(r)}
										</option>
									))}
								</select>
							</label>
						) : (
							<p className={styles.jobFixed}>
								<span>{t("sheetJob")}</span>
								<span>{role}</span>
							</p>
						)}
					</div>
				</div>
				{solo && <Bullets text={t("sheetSoloNote")} />}
				<TextArea
					id={`${id}-focus`}
					label={t("sheetFocus")}
					rows={2}
					hint={t("sheetFocusHint")}
					value={agent.persona ?? ""}
					onChange={(persona) => onChange({ ...agent, persona })}
				/>
				<ModelFields
					id={id}
					model={agent.model ?? {}}
					role={roleModel}
					models={models}
					onChange={(model) => onChange({ ...agent, model })}
				/>
				{children}
				{!solo && !required && (
					<Switch
						id={`${id}-on`}
						label={t("sheetOnTeam")}
						checked={on}
						onChange={onToggle}
					/>
				)}
			</div>
		</Dialog>
	);
}
