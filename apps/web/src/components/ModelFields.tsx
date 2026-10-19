import { InfoTip, Segmented } from "@catervas/ui";
import { t } from "../strings/t.ts";
import { Bullets } from "./Bullets.tsx";
import styles from "./ModelFields.module.css";

export type Effort = "low" | "medium" | "high" | "xhigh" | "max";
/** A model the price table knows, with its name ("Opus 5.5"). */
export type ModelOption = { id: string; label: string };
/** An agent's own model and effort; either left out is its role's. */
export type ModelChoice = { id?: string; effort?: Effort };

export const EFFORTS: Effort[] = ["low", "medium", "high", "xhigh", "max"];

const EFFORT_WORDS = {
	low: "effortLow",
	medium: "effortMedium",
	high: "effortHigh",
	xhigh: "effortXhigh",
	max: "effortMax",
} as const;
const EFFORT_NOTES = {
	low: "effortLowNote",
	medium: "effortMediumNote",
	high: "effortHighNote",
	xhigh: "effortXhighNote",
	max: "effortMaxNote",
} as const;
/** What each family is for, by the start of its ids. */
const FAMILY_NOTES = [
	["claude-fable-", "modelFableNote"],
	["claude-opus-", "modelOpusNote"],
	["claude-sonnet-", "modelSonnetNote"],
	["claude-haiku-", "modelHaikuNote"],
] as const;

/** "High", "Extra high": an effort's name. */
export function effortWord(effort: Effort | undefined): string {
	return t(EFFORT_WORDS[effort ?? "medium"]);
}

/** "High effort": an effort as a row's second line says it. */
export function effortLine(effort: Effort | undefined): string {
	return t("effortLine", { effort: effortWord(effort) });
}

/** The name a model goes by: the price table's, else its id. */
export function modelName(
	id: string | undefined,
	models: ModelOption[],
): string {
	return models.find((m) => m.id === id)?.label ?? id ?? "";
}

function familyNote(id: string | undefined): string | undefined {
	const found = FAMILY_NOTES.find(([prefix]) => id?.startsWith(prefix));
	return found && t(found[1]);
}

/**
 * An agent's model, effort and temperature, each in its own row: the model and the effort as
 * segmented choices, the temperature as the fact that the model sets it.
 */
export function ModelFields({
	id,
	model,
	role,
	models,
	onChange,
}: {
	/** Prefixes the groups' names, so two sheets never share a radio group. */
	id: string;
	/** The agent's own choice. */
	model: ModelChoice;
	/** The role's model and effort, which stand in for what the agent leaves out. */
	role: { id?: string; effort?: Effort };
	models: ModelOption[];
	onChange: (next: ModelChoice) => void;
}) {
	const modelId = model.id ?? role.id;
	const effort = model.effort ?? role.effort ?? "medium";
	// An agent on a model the price table no longer lists keeps it as a choice of its own.
	const options =
		modelId && !models.some((m) => m.id === modelId)
			? [...models, { id: modelId, label: modelId }]
			: models;
	return (
		<div className={styles.fields}>
			<Segmented
				name={`${id}-model`}
				legend={t("modelLegend")}
				info={<Bullets text={t("modelInfo")} />}
				options={options.map((m) => ({ value: m.id, label: m.label }))}
				value={modelId}
				hint={familyNote(modelId)}
				onChange={(next) => onChange({ ...model, id: next })}
			/>
			<Segmented<Effort>
				name={`${id}-effort`}
				legend={t("effortLegend")}
				info={<Bullets text={t("effortInfo")} />}
				options={EFFORTS.map((e) => ({ value: e, label: effortWord(e) }))}
				value={effort}
				hint={t(EFFORT_NOTES[effort])}
				onChange={(next) => onChange({ ...model, effort: next })}
			/>
			<div className={styles.fixed}>
				<span className={styles.fixedLabel}>
					{t("temperatureLegend")}
					<InfoTip
						id={`${id}-temperature-info`}
						label={t("temperatureInfoLabel")}
					>
						<Bullets text={t("temperatureInfo")} />
					</InfoTip>
				</span>
				<span className={styles.fixedValue}>{t("temperatureValue")}</span>
			</div>
		</div>
	);
}
