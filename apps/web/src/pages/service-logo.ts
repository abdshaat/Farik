// Bundled, never fetched: a file is named for the kit connector it marks.
const files = import.meta.glob<string>("../assets/services/*.{svg,png}", {
	eager: true,
	query: "?url",
	import: "default",
});

const logos = new Map(
	Object.entries(files).map(([path, url]) => [
		(path.split("/").pop() ?? "").replace(/\.(svg|png)$/, ""),
		url,
	]),
);

const plug = logos.get("plug") ?? "";

/** The logo's URL; the plug for a name with none, such as a connector you added. */
export function serviceLogo(name: string): string {
	return logos.get(name) ?? plug;
}
