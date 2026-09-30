import { expectNoAxeViolations } from "@farik/ui/test";
import { act, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
	COMPLETION,
	openedGate,
	REVIEW,
	sentCommand,
	TASK,
} from "../test/gate.ts";

const GATE = [
	"team.get",
	"contract.get",
	"task.history",
	"task.checks",
	"task.diff",
	"task.tries",
	"waiting.list",
];
const opened = (contract?: object, waiting?: object[]) =>
	openedGate("/tasks/FRK-1/accept", GATE, contract, waiting);

describe("acceptance gate", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("leads_with_the_two_summaries_then_the_checks", async () => {
		const { container } = await opened();
		expect(
			await screen.findByRole("heading", { name: "Accept Gift cards" }),
		).toBeTruthy();

		// The two signed summaries, the builder's first, each its note's first paragraph.
		const letters = screen.getAllByRole("region", {
			name: /, your .*(wrote this for you|reviewed it)$/,
		});
		expect(letters).toHaveLength(2);
		const [built, reviewed] = letters as [HTMLElement, HTMLElement];
		expect(
			within(built).getByText(
				"Theo, your Software Developer, wrote this for you",
			),
		).toBeTruthy();
		expect(within(built).getByText(COMPLETION)).toBeTruthy();
		expect(within(built).queryByText(/earlier try/)).toBeNull();
		expect(within(built).queryByText(/prices are worked out/)).toBeNull();
		expect(
			within(reviewed).getByText("Ada, your Architect, reviewed it"),
		).toBeTruthy();
		expect(within(reviewed).getByText(REVIEW)).toBeTruthy();

		// Then Farik's checks, after both summaries.
		const checked = screen.getByRole("region", { name: "What Farik checked" });
		expect(
			reviewed.compareDocumentPosition(checked) &
				Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
		expect(
			within(checked).getByText("A test purchase of each amount goes through."),
		).toBeTruthy();
		expect(within(checked).getByText("Passed")).toBeTruthy();

		// The code changes stay folded away until pressed.
		const toggle = screen.getByRole("button", {
			name: "See the code changes · 3 files, +142 −18",
		});
		expect(toggle.getAttribute("aria-expanded")).toBe("false");
		expect(
			screen.queryByRole("region", { name: "The code changes" }),
		).toBeNull();
		fireEvent.click(toggle);
		expect(toggle.getAttribute("aria-expanded")).toBe("true");
		expect(
			within(
				screen.getByRole("region", { name: "The code changes" }),
			).getByText("src/gift.ts"),
		).toBeTruthy();

		const about = screen.getByRole("region", { name: "About this task" });
		for (const text of [
			"Wednesday 23 September",
			"Thursday 24 September",
			"1 of 4",
			"$1.82",
		])
			expect(within(about).getByText(text)).toBeTruthy();
		expect(within(about).getByText("See the whole history")).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("says_one_changed_file_in_the_singular", async () => {
		await openedGate("/tasks/FRK-1/accept", GATE, TASK, [], {
			"task.diff": { diff: "", files: ["done.txt"], added: 1, removed: 0 },
		});
		expect(
			await screen.findByRole("button", {
				name: "See the code changes · 1 file, +1 −0",
			}),
		).toBeTruthy();
	});

	it("accepts_the_work", async () => {
		const { container, s } = await opened();
		fireEvent.click(
			await screen.findByRole("button", { name: "Accept the work" }),
		);
		const accept = await sentCommand(s);
		expect(accept.params).toEqual({
			command: {
				command: "human_accept",
				body: { task_id: "FRK-1", subject: "result" },
			},
		});
		act(() =>
			s.reply(accept, {
				error: {
					kind: "refused",
					detail:
						"not_waiting_for_the_human: FRK-1 is done, and its result does not wait for the human",
				},
			}),
		);
		expect((await screen.findByRole("alert")).textContent).toBe(
			"FRK-1 is done, and its result does not wait for the human",
		);
		await expectNoAxeViolations(container);
	});

	it("accepts_an_epic_with_what_you_checked", async () => {
		const { container, s } = await opened({ ...TASK, kind: "epic" });
		const accept = await screen.findByRole("button", {
			name: "Accept the work",
		});
		expect((accept as HTMLButtonElement).disabled).toBe(true);
		fireEvent.change(screen.getByLabelText(/What you checked/), {
			target: { value: "I bought a $25 card and used it." },
		});
		fireEvent.click(accept);
		expect((await sentCommand(s)).params).toEqual({
			command: {
				command: "human_accept",
				body: {
					task_id: "FRK-1",
					subject: "result",
					message: "I bought a $25 card and used it.",
				},
			},
		});
		await expectNoAxeViolations(container);
	});

	it("adds_accepted_work_to_the_project", async () => {
		const { container, s } = await opened({ ...TASK, status: "accepted" }, [
			{
				task_id: "FRK-1",
				kind: "integration",
				agent_id: null,
				title: "Gift cards",
				line: "Farik could not add it to your project",
			},
		]);
		const add = await screen.findByRole("button", { name: "Add to project" });
		expect(
			screen.queryByRole("button", { name: "Accept the work" }),
		).toBeNull();
		expect(
			screen.queryByRole("button", { name: "Send back with a note" }),
		).toBeNull();
		fireEvent.click(add);
		expect((await sentCommand(s)).params).toEqual({
			command: { command: "task_integrate", body: { task_id: "FRK-1" } },
		});
		await expectNoAxeViolations(container);
	});
});
