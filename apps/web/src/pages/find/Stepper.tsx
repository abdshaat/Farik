import { Stepper } from "@catervas/ui";
import { t } from "../../strings/t.ts";

/** The three steps of finding projects: the owner is on `current` (0 is the first). */
export function FindStepper(props: { current: number }) {
	return (
		<nav aria-label={t("findSteps")}>
			<Stepper
				steps={[t("findStepWork"), t("findStepTogether"), t("findStepTeams")]}
				current={props.current}
			/>
		</nav>
	);
}
