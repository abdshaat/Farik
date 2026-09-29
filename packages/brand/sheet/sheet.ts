/// <reference types="vite/client" />
import {
	AVATAR_KEYS,
	contrastRatio,
	ICON_SIZES,
	TEXT_PAIRS,
	type ThemeName,
	tokens,
} from "../src/index.ts";

const THEMES: readonly ThemeName[] = ["light", "dark"];
const url = (glob: Record<string, unknown>, suffix: string) =>
	Object.entries(glob).find(([path]) => path.endsWith(suffix))?.[1] as string;
const png = import.meta.glob("../assets/**/*.png", {
	eager: true,
	query: "?url",
	import: "default",
});
const asset = (name: string) => url(png, `/${name}`);

const kebab = (s: string) => s.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
const camel = (s: string) => s.replace(/-(.)/g, (_, c) => c.toUpperCase());
const words = (key: string) =>
	key.replace(/-/g, " ").replace(/^./, (c) => c.toUpperCase());
const colour = (theme: ThemeName, name: string) =>
	(tokens.color[theme] as Record<string, string>)[camel(name)] as string;

function el<K extends keyof HTMLElementTagNameMap>(
	tag: K,
	attrs: Record<string, string> = {},
	...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
	const node = document.createElement(tag);
	for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
	node.append(...children);
	return node;
}

const img = (src: string, alt: string, attrs: Record<string, string> = {}) =>
	el("img", { src, alt, ...attrs });

function themeColumn(theme: ThemeName): HTMLElement {
	const c = tokens.color[theme];
	const scale = el("div", { class: "scale" });
	for (const [name, t] of Object.entries(tokens.type)) {
		scale.append(
			el(
				"p",
				{
					"data-type": name,
					style: `color:${c.ink};font:${t.weight} ${t.size}px/${t.lineHeight}px var(--farik-type-${name}-family)`,
				},
				`${name}, ${t.size}/${t.lineHeight}, ${t.weight}: Farik runs your team`,
			),
		);
	}

	const col = el(
		"section",
		{
			"data-theme": theme,
			"aria-labelledby": `h-${theme}`,
			style: `background:${c.page};color:${c.ink};border:1px solid ${c.rule}`,
		},
		el("h2", { id: `h-${theme}` }, `${words(theme)} theme`),
	);

	const swatches = el("div", { class: "swatches" });
	for (const [key, hex] of Object.entries(c)) {
		const name = kebab(key);
		swatches.append(
			el(
				"div",
				{ "data-swatch": `${theme}:${name}`, class: "swatch" },
				el("span", {
					class: "chip",
					style: `background:${hex};border-color:${c.controlBorder}`,
				}),
				el("span", {}, name),
				el("code", {}, hex.toUpperCase()),
			),
		);
	}

	const pairs = el("div", { class: "pairs" });
	for (const p of TEXT_PAIRS) {
		const fg = colour(theme, p.foreground);
		const bg = colour(theme, p.background);
		pairs.append(
			el(
				"div",
				{
					"data-pair": `${theme}:${p.foreground}/${p.background}`,
					class: "pair",
				},
				el(
					"span",
					{
						class: "sample",
						style: `color:${fg};background:${bg};border-color:${c.controlBorder}`,
					},
					"Aa",
				),
				el("span", {}, `${p.foreground} on ${p.background}`),
				el(
					"code",
					{},
					`${contrastRatio(fg, bg).toFixed(2)}:1, minimum ${p.minimum}`,
				),
			),
		);
	}

	col.append(
		el("h3", {}, "Colours"),
		swatches,
		el("h3", {}, "Checked contrast pairs"),
		pairs,
		el("h3", {}, "Logo and wordmark"),
		// brand.md: on a light surface the wordmark sits on a Midnight Terminal tile.
		el(
			"div",
			{
				class: "lockup",
				style:
					theme === "light"
						? `background:${c.band};padding:${tokens.space["4"]}px;border-radius:${tokens.radius.raised}px`
						: "",
			},
			img(asset("logo-mark-1254.png"), "Farik logo mark", {
				class: "mark",
			}),
			img(asset("wordmark-1024.png"), "Farik wordmark", { class: "wordmark" }),
		),
		el("h3", {}, "Type scale"),
		scale,
	);
	return col;
}

export function renderSheet(root: HTMLElement): void {
	const icons = el("div", { class: "row" });
	for (const n of ICON_SIZES) {
		icons.append(
			el(
				"figure",
				{},
				img(asset(`icon-${n}.png`), `Icon, ${n} px`, {
					width: String(Math.min(n, 96)),
				}),
				el("figcaption", {}, `${n} px`),
			),
		);
	}

	const avatars = el("div", { class: "row" });
	for (const key of AVATAR_KEYS) {
		avatars.append(
			el(
				"figure",
				{},
				img(asset(`${key}-256.png`), `${words(key)} avatar`, {
					"data-avatar": key,
					width: "96",
				}),
				el("figcaption", {}, key),
			),
		);
	}

	root.replaceChildren(
		el("h1", {}, "Farik brand sheet"),
		el("div", { class: "themes" }, ...THEMES.map(themeColumn)),
		el(
			"section",
			{ "aria-labelledby": "h-icons" },
			el("h2", { id: "h-icons" }, "Icons"),
			icons,
		),
		el(
			"section",
			{ "aria-labelledby": "h-avatars" },
			el("h2", { id: "h-avatars" }, "Avatars"),
			avatars,
		),
	);
}
