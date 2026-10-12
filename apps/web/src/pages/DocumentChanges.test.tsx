import { expectNoAxeViolations } from "@catervas/ui/test";
import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import { CARD, docs, todayWithDocs } from "../test/folderDocs.ts";
import { sentCommand } from "../test/gate.ts";
import { showsWhatItHides } from "../test/hidden.ts";
import { bodyOf, refusedBy } from "../test/schema.ts";
import styles from "./DocumentChanges.module.css";

/** What a list item reads: its words, without the marks that are hidden from a reader. */
const reads = (item: HTMLElement) => {
	const copy = item.cloneNode(true) as HTMLElement;
	for (const hidden of copy.querySelectorAll("[aria-hidden]")) hidden.remove();
	return copy.textContent;
};

const card = () => screen.findByRole("region", { name: en.docChangesTitle });

describe("the product plan's changes on Today", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("shows_the_changes_line_by_line", async () => {
		const { container } = await todayWithDocs({
			docs: docs([{ ...CARD, summary: `${CARD.summary} ‮ Stop.` }]),
		});
		const region = await card();
		expect(
			within(region).getByText("From Mira, after the sprint 4 review"),
		).toBeTruthy();
		expect(within(region).getByText(en.docChangesLegend)).toBeTruthy();
		const roadmap = within(region).getByRole("list", { name: "Roadmap" });
		const spec = within(region).getByRole("list", {
			name: "Product description",
		});
		expect(
			roadmap.compareDocumentPosition(spec) & Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
		const words = within(roadmap)
			.getAllByRole("listitem")
			.map((item) => reads(item));
		expect(words).toEqual([
			"Done",
			"Added: Pie pre-orders. 41 pies ordered in the first week.",
			"Now",
			"Removed: Pie pre-orders. Customers order Thanksgiving pies ahead and collect them on 25 or 26 November.",
			"Added: Gift cards. Buy a card online, spend it in the shop.",
			"Next",
			"Removed: Gift cards. Buy a card online, spend it in the shop.",
			"Added: Christmas orders. Stollen, yule logs and mince pies, collected from 19 to 24 December.",
			"Opening hours on every page.",
		]);
		for (const item of within(region).getAllByRole("listitem")) {
			expect(item.textContent).not.toContain("## ");
			expect(reads(item)).not.toMatch(/^(Added: |Removed: )?- /);
		}
		showsWhatItHides(region);
		await expectNoAxeViolations(container);
	});

	it("html_changes_read_as_words", async () => {
		const html = {
			...CARD,
			documents: [
				{
					proposal: 9,
					path: "docs/catervas/product/spec.html",
					lines: [
						{ change: "same", text: "<h2>Risks</h2>" },
						{ change: "added", text: "<li>Ship <b>v2</b> &amp; more</li>" },
					],
				},
			],
		};
		await todayWithDocs({ docs: docs([html]) });
		const region = await card();
		const list = within(region).getByRole("list", {
			name: "Product description",
		});
		const [risks, ship] = within(list).getAllByRole("listitem");
		expect(reads(ship as HTMLElement)).toBe("Added: Ship v2 & more");
		expect(reads(risks as HTMLElement)).toBe("Risks");
		expect((risks as HTMLElement).className).toContain(styles.heading);
		expect((ship as HTMLElement).className).not.toContain(styles.heading);
		expect(region.querySelector("b")).toBeNull();
	});

	it("approve_sends_every_proposal_of_the_card", async () => {
		const { s } = await todayWithDocs({});
		const region = await card();
		fireEvent.click(within(region).getByRole("button", { name: "Approve" }));
		const first = await sentCommand(s);
		expect(first.params).toEqual({
			command: {
				command: "folder_doc_decide",
				body: { proposals: [7, 8], decision: "approve" },
			},
		});
		expect(refusedBy("folderDocDecideBody", bodyOf(first))).toEqual([]);
		const asked = () =>
			s
				.calls("query")
				.filter((q) => q.params.name === "folder_doc_proposals.list").length;
		const before = asked();
		await s.reply(first, { said: "approved", events: [9] });
		// The list is asked again, so that the card leaves it.
		await waitFor(() => expect(asked()).toBeGreaterThan(before));

		fireEvent.change(within(region).getByRole("textbox"), {
			target: { value: "  Thanks  " },
		});
		fireEvent.click(within(region).getByRole("button", { name: "Approve" }));
		const second = await sentCommand(s, 2);
		expect(bodyOf(second)).toEqual({
			proposals: [7, 8],
			decision: "approve",
			note: "Thanks",
		});
		expect(refusedBy("folderDocDecideBody", bodyOf(second))).toEqual([]);
	});

	it("send_back_needs_notes", async () => {
		const { s } = await todayWithDocs({});
		const region = await card();
		fireEvent.click(within(region).getByRole("button", { name: "Send back" }));
		expect(
			await within(region).findByText("Write what Mira should change first."),
		).toBeTruthy();
		expect(s.calls("command")).toHaveLength(0);

		fireEvent.change(within(region).getByRole("textbox"), {
			target: { value: "Keep gift cards in Now" },
		});
		fireEvent.click(within(region).getByRole("button", { name: "Send back" }));
		const sent = await sentCommand(s);
		expect(bodyOf(sent)).toEqual({
			proposals: [7, 8],
			decision: "return",
			note: "Keep gift cards in Now",
		});
		expect(refusedBy("folderDocDecideBody", bodyOf(sent))).toEqual([]);
	});

	it("says_what_approving_does_under_the_policy", async () => {
		for (const [integration, words] of [
			["auto_merge", en.docChangesNothingYet],
			["pull_request", en.docChangesPullRequest],
			["manual", en.docChangesManual],
		] as const) {
			await todayWithDocs({ docs: docs([CARD], integration) });
			const region = await card();
			const note = within(region).getByText(words);
			expect(note.textContent).not.toContain("at once");
			expect(region.textContent).not.toContain("at once");
			cleanup();
		}
		expect(en.docChangesNothingYet).toBe("Nothing changes until you approve.");
		expect(en.docChangesPullRequest).toBe(
			"Nothing changes until you approve. Then Catervas opens a pull request, and the change is in your project once it is merged.",
		);
		expect(en.docChangesManual).toBe(
			"Nothing changes until you approve. Then it waits under Waiting on you for you to add it to your project.",
		);
	});

	it("shows_a_document_whose_twin_alone_changes", async () => {
		const [roadmap, spec] = CARD.documents;
		await todayWithDocs({
			docs: docs([{ ...CARD, documents: [roadmap, { ...spec, lines: [] }] }]),
		});
		const region = await card();
		expect(
			within(region).getByText("Only the version for the team changes."),
		).toBeTruthy();
	});

	it("a_refusal_shows_its_words", async () => {
		const { s } = await todayWithDocs({});
		const region = await card();
		fireEvent.click(within(region).getByRole("button", { name: "Approve" }));
		const sent = await sentCommand(s);
		await s.reply(sent, {
			error: {
				kind: "refused",
				detail: "folder_doc_busy: docs/catervas/product/roadmap.md",
			},
		});
		expect((await within(region).findByRole("alert")).textContent).toBe(
			"One of these files has changes of yours that are not committed. Commit or undo them, then approve.",
		);
	});

	it("opens a changed document in files", async () => {
		await todayWithDocs({});
		const region = await card();
		const link = (list: string) =>
			within(
				within(region).getByRole("list", { name: list })
					.parentElement as HTMLElement,
			).getByRole("link", { name: en.filesOpen });
		expect(link("Roadmap").getAttribute("href")).toBe(
			"/files/product/roadmap.md",
		);
		expect(link("Product description").getAttribute("href")).toBe(
			"/files/product/spec.md",
		);
	});
});
