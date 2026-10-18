import { t } from "../../strings/t.ts";

/** Whether a host has a part written in another alphabet, which Catervas keeps in its `xn--` form. */
export const isScript = (host: string) =>
	host.split(".").some((part) => part.startsWith("xn--"));

/** The warning for a name written in another alphabet: what it is, and what to check. */
export function ScriptWarning({
	className,
}: {
	className?: string | undefined;
}) {
	return (
		<p className={className} role="note">
			<strong>{t("scriptWarningWhat")}</strong> {t("scriptWarningCheck")}
		</p>
	);
}
