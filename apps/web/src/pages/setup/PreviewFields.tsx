import { TextField } from "@farik/ui";
import type { Refusal } from "../../app/refusals.ts";
import { t } from "../../strings/t.ts";

/** `team.yaml`'s `preview` (step 12, D2): how Farik opens the app for the UI/UX Designer. */
export type Preview = {
	prepare?: string;
	start: string;
	port: number | string;
	path: string;
};
/** The four fields as typed. */
export type PreviewForm = {
	prepare: string;
	start: string;
	port: string;
	path: string;
};

export const formOf = (p?: Preview): PreviewForm => ({
	prepare: p?.prepare ?? "",
	start: p?.start ?? "",
	port: p?.port === undefined ? "" : String(p.port),
	path: p?.path ?? "/",
});

/** The preview the fields stand for, or none while the commands and the port are all empty. */
export function previewOf(form: PreviewForm): Preview | undefined {
	const prepare = form.prepare.trim();
	const start = form.start.trim();
	const port = form.port.trim();
	if (!prepare && !start && !port) return undefined;
	return {
		...(prepare && { prepare }),
		start,
		// A port that is not a number goes as typed, and the daemon refuses it at its field.
		port: /^\d+$/.test(port) ? Number(port) : port,
		path: form.path.trim() || "/",
	};
}

/** "Get it ready", "Start it", "Port" and "First page", each refusal at its own field. */
export function PreviewFields({
	id,
	form,
	onChange,
	designer,
	errors,
}: {
	id: string;
	form: PreviewForm;
	onChange: (next: PreviewForm) => void;
	designer: string;
	/** The daemon's refusals of the team; those under `/preview` are shown here. */
	errors: Refusal[];
}) {
	const at = (field: string) =>
		errors.some((e) => e.path === `/preview/${field}`);
	const port = at("port") ? t("refusePreviewPort") : undefined;
	const other = errors.some(
		(e) => e.path.startsWith("/preview") && e.path !== "/preview/port",
	);
	const field = (
		name: keyof PreviewForm,
		label: Parameters<typeof t>[0],
		hint: Parameters<typeof t>[0],
		error?: string,
	) => (
		<TextField
			id={`${id}-${name}`}
			label={t(label)}
			hint={t(hint, { designer })}
			value={form[name]}
			{...(error && { error })}
			onChange={(value) => onChange({ ...form, [name]: value })}
		/>
	);
	return (
		<>
			{field("prepare", "previewPrepare", "previewPrepareHint")}
			{field("start", "previewStart", "previewStartHint")}
			{field("port", "previewPort", "previewPortHint", port)}
			{field("path", "previewPath", "previewPathHint")}
			{other && <p role="alert">{t("refusePreview")}</p>}
		</>
	);
}
