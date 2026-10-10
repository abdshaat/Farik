import MarkdownIt from "markdown-it";
import { useEffect, useMemo, useRef } from "react";
import { t } from "../strings/t.ts";
import styles from "./Markdown.module.css";

// Raw HTML off: every `<`, `>`, `&` and `"` of the source comes out escaped, and a link or image
// whose address is `javascript:`, `vbscript:`, `file:` or a `data:` other than an image is left
// as text. A diagram is the fence rule below: its source escaped in a `<pre>`, drawn afterwards.
const md = new MarkdownIt({ html: false, linkify: false, typographer: false });
const fence = md.renderer.rules.fence;
md.renderer.rules.fence = (tokens, idx, options, env, self) => {
	const token = tokens[idx];
	if (token?.info.trim().split(/\s+/)[0] === "mermaid")
		return `<figure class="${styles.diagram}" data-diagram><pre>${md.utils.escapeHtml(token.content)}</pre></figure>\n`;
	return fence
		? fence(tokens, idx, options, env, self)
		: self.renderToken(tokens, idx, options);
};

const started = new WeakSet<object>();
let drawn = 0;

/** Draws each figure's source as an image: an SVG image runs no script and loads nothing. */
async function draw(figures: HTMLElement[], live: () => boolean) {
	const { default: mermaid } = await import("mermaid");
	if (!started.has(mermaid)) {
		started.add(mermaid);
		mermaid.initialize({
			startOnLoad: false,
			securityLevel: "strict",
			theme: "neutral",
			fontFamily: "system-ui, sans-serif",
			htmlLabels: false,
			suppressErrorRendering: true,
		});
	}
	for (const figure of figures) {
		const source = figure.querySelector("pre")?.textContent?.trimEnd() ?? "";
		// mermaid empties the element it draws into, so it gets a scratch one, not the figure.
		const scratch = document.createElement("div");
		figure.append(scratch);
		try {
			const { svg } = await mermaid.render(
				`catervas-diagram-${++drawn}`,
				source,
				scratch,
			);
			if (!live()) return;
			const img = document.createElement("img");
			img.alt = t("diagram");
			img.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
			figure.replaceChildren(img);
		} catch {
			scratch.remove();
			if (!live()) return;
			const note = document.createElement("p");
			note.textContent = t("diagramFailed");
			figure.prepend(note);
		}
	}
}

/** Markdown as the owner reads it. The text is the team's or the owner's, so it is never trusted. */
export function Markdown({ text }: { text: string }) {
	const html = useMemo(() => md.render(text), [text]);
	const ref = useRef<HTMLDivElement>(null);
	// biome-ignore lint/correctness/useExhaustiveDependencies: a new html is a new set of figures
	useEffect(() => {
		const figures = [
			...(ref.current?.querySelectorAll<HTMLElement>("figure[data-diagram]") ??
				[]),
		];
		if (figures.length === 0) return;
		let live = true;
		void draw(figures, () => live);
		return () => {
			live = false;
		};
	}, [html]);
	return (
		<div
			ref={ref}
			className={styles.text}
			// biome-ignore lint/security/noDangerouslySetInnerHtml: markdown-it with html: false escapes all HTML and drops unsafe links
			dangerouslySetInnerHTML={{ __html: html }}
		/>
	);
}
