import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fireEvent, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Dialog } from "./Dialog.tsx";
import { uiStrings } from "./strings.ts";
import { expectNoAxeViolations } from "./test/axe.ts";

describe("Dialog", () => {
	it("opens as a modal named by its title", async () => {
		const showModal = vi.spyOn(HTMLDialogElement.prototype, "showModal");
		const { container } = render(
			<Dialog open title="Remove the plan" onClose={() => {}}>
				<p>Sure?</p>
			</Dialog>,
		);
		expect(showModal).toHaveBeenCalledTimes(1);
		expect(
			screen.getByRole("dialog", { name: "Remove the plan" }),
		).toBeTruthy();
		await expectNoAxeViolations(container);
		showModal.mockRestore();
	});

	it("closes on Escape and on the close button", async () => {
		const onClose = vi.fn();
		const { container } = render(
			<Dialog open title="Remove the plan" onClose={onClose}>
				<p>Sure?</p>
			</Dialog>,
		);
		fireEvent(
			screen.getByRole("dialog"),
			new Event("cancel", { cancelable: true }),
		);
		expect(onClose).toHaveBeenCalledTimes(1);
		await userEvent.click(
			screen.getByRole("button", { name: uiStrings.close }),
		);
		expect(onClose).toHaveBeenCalledTimes(2);
		await expectNoAxeViolations(container);
	});

	it("tells its parent when the browser closes it while open", () => {
		const onClose = vi.fn();
		const { rerender } = render(
			<Dialog open title="Remove the plan" onClose={onClose}>
				<p>Sure?</p>
			</Dialog>,
		);
		fireEvent(screen.getByRole("dialog"), new Event("close"));
		expect(onClose).toHaveBeenCalledTimes(1);
		rerender(
			<Dialog open={false} title="Remove the plan" onClose={onClose}>
				<p>Sure?</p>
			</Dialog>,
		);
		expect(onClose).toHaveBeenCalledTimes(1);
	});

	it("fills a phone's screen only when it asks to, its actions pinned", async () => {
		const { container, rerender } = render(
			<Dialog
				open
				title="Allow"
				onClose={() => {}}
				actions={<button type="button">Go</button>}
			>
				<p>Long</p>
			</Dialog>,
		);
		const dialog = screen.getByRole("dialog");
		expect(dialog.hasAttribute("data-fills-phone")).toBe(false);
		rerender(
			<Dialog
				open
				fillsPhone
				title="Allow"
				onClose={() => {}}
				actions={<button type="button">Go</button>}
			>
				<p>Long</p>
			</Dialog>,
		);
		expect(dialog.hasAttribute("data-fills-phone")).toBe(true);
		await expectNoAxeViolations(container);
		// jsdom lays nothing out, so the rule that the attribute selects is read from the source.
		const css = readFileSync(
			join(import.meta.dirname, "Dialog.module.css"),
			"utf8",
		);
		const phone = css.slice(css.indexOf("@media (max-width: 480px)"));
		expect(phone).toContain(".dialog[data-fills-phone][open]");
		expect(phone).toContain("height: 100dvh");
		expect(phone).toContain(".dialog[data-fills-phone] .body");
	});
});
