import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { DocumentFrame } from "./DocumentFrame.tsx";

const PATH = "docs/catervas/architecture/overview.html";

describe("DocumentFrame", () => {
	afterEach(cleanup);

	it("frames_the_document_sandboxed", () => {
		const { container } = render(
			<DocumentFrame
				path={PATH}
				task="CTV-4&x=1"
				version="abc123"
				title="Overview"
			/>,
		);
		const frame = container.querySelector("iframe") as HTMLIFrameElement;
		const src = `/documents/raw?path=${encodeURIComponent(PATH)}&task=${encodeURIComponent("CTV-4&x=1")}&v=abc123`;
		expect(frame.getAttribute("src")).toBe(src);
		expect(frame.hasAttribute("sandbox")).toBe(true);
		expect(frame.getAttribute("sandbox")).toBe("");
		expect(frame.getAttribute("title")).toBe("Overview");
		const open = screen.getByRole("link", { name: "Open full page" });
		expect(open.getAttribute("href")).toBe(src);
		expect(open.getAttribute("target")).toBe("_blank");
		expect(open.getAttribute("rel")).toContain("noopener");
	});

	it("adds_a_class_to_the_frame", () => {
		const { container } = render(
			<DocumentFrame path={PATH} title="Overview" className="mine" />,
		);
		expect(container.querySelector("iframe")?.className).toContain("mine");
	});

	it("leaves_out_the_task_and_version_when_not_given", () => {
		const { container } = render(
			<DocumentFrame path={PATH} title="Overview" />,
		);
		expect(container.querySelector("iframe")?.getAttribute("src")).toBe(
			`/documents/raw?path=${encodeURIComponent(PATH)}`,
		);
	});
});
