import { expectNoAxeViolations } from "@catervas/ui/test";
import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import { Markdown } from "./Markdown.tsx";

// A fresh mermaid for every test, so that "initialized once" is counted from zero.
const mermaid = vi.hoisted(() => ({
	current: {} as {
		initialize: ReturnType<typeof vi.fn>;
		render: ReturnType<typeof vi.fn>;
	},
}));
vi.mock("mermaid", () => ({
	get default() {
		return mermaid.current;
	},
}));

const SVG = '<svg xmlns="http://www.w3.org/2000/svg"><text>A</text></svg>';

beforeEach(() => {
	mermaid.current = {
		initialize: vi.fn(),
		// Like mermaid: it empties the element it is given before it draws.
		render: vi.fn(async (_id: string, _src: string, el: HTMLElement) => {
			el.innerHTML = "";
			return { svg: SVG };
		}),
	};
});

describe("Markdown", () => {
	it("renders headings lists emphasis and code", () => {
		render(<Markdown text={"# A\n\n- **b**\n\n`c`"} />);
		expect(screen.getByRole("heading", { name: "A" })).toBeTruthy();
		expect(
			screen.getByRole("listitem").querySelector("strong")?.textContent,
		).toBe("b");
		expect(document.querySelector("code")?.textContent).toBe("c");
	});

	it("shows raw html as text", async () => {
		const { container, unmount } = render(
			<Markdown
				text={"<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>"}
			/>,
		);
		expect(container.querySelector("script")).toBeNull();
		expect(container.querySelector("img")).toBeNull();
		expect(container.textContent).toContain("<script>alert(1)</script>");
		expect(container.textContent).toContain("<img src=x onerror=alert(1)>");
		unmount();

		// A diagram's source is text too, until mermaid has drawn it.
		mermaid.current.render.mockReturnValue(new Promise(() => {}));
		const fence = render(
			<Markdown text={"```mermaid\n<img src=x onerror=alert(1)>\n```"} />,
		);
		await waitFor(() => expect(mermaid.current.render).toHaveBeenCalled());
		expect(fence.container.querySelector("img")).toBeNull();
		expect(fence.container.querySelector("pre")?.textContent).toContain(
			"<img src=x onerror=alert(1)>",
		);
	});

	it("leaves unsafe links as text", () => {
		const { container } = render(
			<Markdown
				text={
					"[a](javascript:alert(1))\n\n[b](JavaScript:alert(1))\n\n[c](vbscript:x)\n\n[d](data:text/html,x)\n\n[e](https://example.com/)"
				}
			/>,
		);
		const links = [...container.querySelectorAll("a")];
		expect(links.map((a) => a.getAttribute("href"))).toEqual([
			"https://example.com/",
		]);
	});

	it("loads mermaid only for a document with a diagram", async () => {
		const { rerender, container } = render(<Markdown text={"# No diagram"} />);
		await Promise.resolve();
		expect(mermaid.current.initialize).not.toHaveBeenCalled();
		rerender(<Markdown text={"```mermaid\nflowchart LR\n  A --> B\n```"} />);
		const img = await screen.findByRole("img", { name: en.diagram });
		expect(mermaid.current.initialize).toHaveBeenCalledTimes(1);
		expect(mermaid.current.initialize).toHaveBeenCalledWith(
			expect.objectContaining({
				securityLevel: "strict",
				startOnLoad: false,
				htmlLabels: false,
				suppressErrorRendering: true,
			}),
		);
		expect(mermaid.current.render).toHaveBeenCalledWith(
			expect.any(String),
			"flowchart LR\n  A --> B",
			expect.any(HTMLElement),
		);
		expect(img.getAttribute("src")).toMatch(/^data:image\/svg\+xml/);
		expect(container.querySelector("pre")).toBeNull();
	});

	it("keeps a diagram mermaid refuses as its source", async () => {
		// Like mermaid: it empties the element, draws its own error there, and throws.
		mermaid.current.render.mockImplementation(
			async (_id: string, _src: string, el: HTMLElement) => {
				el.innerHTML = "";
				el.append(document.createElement("svg"));
				throw new Error("Parse error");
			},
		);
		const { container } = render(
			<Markdown text={"```mermaid\nnot a diagram\n```"} />,
		);
		await screen.findByText(en.diagramFailed);
		expect(container.querySelector("pre")?.textContent).toContain(
			"not a diagram",
		);
		expect(container.querySelector("img")).toBeNull();
		expect(container.querySelector("svg")).toBeNull();
	});

	it("has no accessibility violations", async () => {
		const { container } = render(
			<Markdown
				text={"# A\n\n- b\n\n```mermaid\nflowchart LR\n  A --> B\n```"}
			/>,
		);
		await screen.findByRole("img", { name: en.diagram });
		await expectNoAxeViolations(container);
	});
});
