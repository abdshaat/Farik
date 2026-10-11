import { Button } from "@catervas/ui";
import { t } from "../strings/t.ts";

/** The button under a section that opens that section's advanced settings. */
export function ShowAdvanced({
	open,
	onToggle,
	controls,
}: {
	open: boolean;
	onToggle: () => void;
	controls?: string;
}) {
	return (
		<Button
			kind="quiet"
			expanded={open}
			{...(controls ? { controls } : {})}
			onClick={onToggle}
		>
			{t(open ? "advancedHide" : "advancedSwitch")}
		</Button>
	);
}
