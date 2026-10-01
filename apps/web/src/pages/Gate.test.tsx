import { expectNoAxeViolations } from "@farik/ui/test";
import { fireEvent, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import {
	ACCEPTING,
	COMPLETION,
	event,
	HISTORY,
	openedGate,
	REVIEW,
	sentCommand,
	TASK,
} from "../test/gate.ts";
import { TEAM } from "../test/plan.ts";

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

const designer = (id: string, name: string) => ({
	id,
	display_name: name,
	role: "ui_ux_designer",
	avatar: "extra-1",
	status: "active",
});
const FAINT = "The prices were too faint to read on a phone in the dark.";
const PASSED = "The prices read well now, in both themes.";
/** A gate whose UI change Iris sent back on Tuesday and Kai, the team's second Designer, passed on Friday before Ada reviewed the code. */
const reviewedTwice = (
	earlier: object[] = [],
	designReview: object = { state: "passed", reasons: PASSED, checks: [] },
	lastPasses = true,
) =>
	openedGate("/tasks/FRK-1/accept", [...GATE, "task.get"], TASK, ACCEPTING, {
		"team.get": {
			team: {
				...TEAM,
				agents: [
					...TEAM.agents,
					designer("iris", "Iris"),
					designer("kai", "Kai"),
				],
			},
		},
		"task.history": {
			events: [
				...HISTORY,
				...earlier,
				event(
					11,
					"review.recorded",
					{ reviewer: "ada", criteria_run: 1, passed: true },
					"2026-09-25T08:45:00Z",
					"ada",
				),
			],
		},
		"task.get": {
			task: {},
			design_plan: null,
			ui_change: true,
			design_review: designReview,
			design_reviews: [
				{
					agent_id: "iris",
					pass: false,
					reasons: FAINT,
					recorded_at: "2026-09-22T15:00:00Z",
				},
				{
					agent_id: "kai",
					pass: lastPasses,
					reasons: PASSED,
					recorded_at: "2026-09-25T08:30:00Z",
				},
			],
		},
	});

describe("acceptance gate", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("leads_with_the_two_summaries_then_the_checks", async () => {
		const { container } = await opened();
		expect(
			await screen.findByRole("heading", { name: "Accept Gift cards" }),
		).toBeTruthy();
		expect(
			screen.getByText("FRK-1. The work waits for you to accept it."),
		).toBeTruthy();

		// The two signed summaries, the builder's first, each its note's first paragraph.
		const letters = screen.getAllByRole("region", {
			name: /, your .*(wrote this for you|reviewed it)$/,
		});
		expect(letters).toHaveLength(2);
		const [built, reviewed] = letters as [HTMLElement, HTMLElement];
		expect(
			within(built).getByText("Theo, your Developer, wrote this for you"),
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
		// The whole history is the task page's, not a list of raw kinds here.
		expect(
			within(about)
				.getByRole("link", { name: "See the whole history" })
				.getAttribute("href"),
		).toBe("/tasks/FRK-1");
		await expectNoAxeViolations(container);
	});

	it("says_one_changed_file_in_the_singular", async () => {
		await openedGate("/tasks/FRK-1/accept", GATE, TASK, undefined, {
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
		await s.reply(accept, {
			error: {
				kind: "refused",
				detail:
					"not_waiting_for_the_human: FRK-1 is done, and its result does not wait for the human",
			},
		});
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refuseNotWaiting,
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

	it("offers_no_answer_when_nothing_waits_on_you", async () => {
		// Verifying, but the review is not in: the result does not wait on the human yet.
		const { container } = await opened(TASK, []);
		expect(
			await screen.findByText(
				"This does not wait on you now. Where it is: Review.",
			),
		).toBeTruthy();
		for (const name of ["Accept the work", "Send back with a note"])
			expect(screen.queryByRole("button", { name })).toBeNull();
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

	it("puts_the_designers_letter_first_on_the_gate", async () => {
		const LOOKED =
			"I opened the page on a phone and on a computer, in the light and dark themes. All four pass.";
		const { container } = await openedGate(
			"/tasks/FRK-1/accept",
			[...GATE, "task.get"],
			TASK,
			ACCEPTING,
			{
				"team.get": {
					team: {
						...TEAM,
						agents: [
							...TEAM.agents,
							{
								id: "iris",
								display_name: "Iris",
								role: "ui_ux_designer",
								avatar: "extra-1",
								status: "active",
							},
						],
					},
				},
				"task.get": {
					task: {},
					design_plan: null,
					ui_change: true,
					design_review: {
						state: "passed",
						reasons: LOOKED,
						checks: [],
					},
				},
			},
		);
		const iris = await screen.findByRole("region", {
			name: "Iris, your UI/UX Designer, checked the screens first",
		});
		expect(within(iris).getByText(LOOKED)).toBeTruthy();
		expect(within(iris).getByText("Passed, and sent on to Ada")).toBeTruthy();
		const theo = screen.getByRole("region", {
			name: "Theo, your Developer, wrote this for you",
		});
		const ada = screen.getByRole("region", {
			name: "Ada, your Architect, reviewed the code after Iris passed the screens",
		});
		// The builder's letter, then the Designer's, then the Architect's review.
		expect(
			theo.compareDocumentPosition(iris) & Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
		expect(
			iris.compareDocumentPosition(ada) & Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("signs_the_letter_by_the_designer_who_reviewed", async () => {
		const { container } = await reviewedTwice();
		// Kai recorded the review, though Iris is the team's first Designer.
		const kai = await screen.findByRole("region", {
			name: "Kai, your UI/UX Designer, checked the screens first",
		});
		expect(within(kai).getByText(PASSED)).toBeTruthy();
		expect(
			screen.queryByRole("region", {
				name: "Iris, your UI/UX Designer, checked the screens first",
			}),
		).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("tells_how_often_the_designer_sent_it_back", async () => {
		const { container } = await reviewedTwice();
		const back = await screen.findByText(
			"Iris sent it back once, on Tuesday 22 September",
		);
		// Folded away until opened, with the Designer's reasons inside.
		const details = back.closest("details");
		expect(details).toBeTruthy();
		expect(details?.open).toBe(false);
		expect(within(details as HTMLElement).getByText(FAINT)).toBeTruthy();
		// The passing review is the letter, not a send-back.
		expect(within(details as HTMLElement).queryByText(PASSED)).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("says_who_looked_at_it_in_order", async () => {
		const { container } = await reviewedTwice();
		const about = await screen.findByRole("region", {
			name: "About this task",
		});
		await within(about).findByText("Screens checked");
		expect(within(about).getByText("Friday 25 September, by Kai")).toBeTruthy();
		const order = within(about).getByRole("list", {
			name: "Who looked at it, in order",
		});
		expect(
			within(order)
				.getAllByRole("listitem")
				.map((li) => li.textContent),
		).toEqual([
			"Farik ran its checks",
			"Iris sent the screens back",
			"Kai checked the screens",
			"Ada reviewed the code",
			"Now you decide",
		]);
		await expectNoAxeViolations(container);
	});

	it("keeps_the_latest_send_back_out_of_the_earlier_ones", async () => {
		// Kai's latest review sent it back too: it is the letter, not an earlier send-back.
		const { container } = await reviewedTwice(
			[],
			{ state: "failed", reasons: PASSED, checks: [] },
			false,
		);
		const back = await screen.findByText(
			"Iris sent it back once, on Tuesday 22 September",
		);
		const details = back.closest("details") as HTMLElement;
		expect(within(details).queryByText(PASSED)).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("puts_an_earlier_code_review_in_its_place_in_time", async () => {
		// Ada sent the code back on Wednesday, between Iris's send-back and Kai's pass.
		const { container } = await reviewedTwice([
			event(
				10,
				"review.recorded",
				{ reviewer: "ada", criteria_run: 1, passed: false },
				"2026-09-23T12:00:00Z",
				"ada",
			),
		]);
		const order = await screen.findByRole("list", {
			name: "Who looked at it, in order",
		});
		expect(
			within(order)
				.getAllByRole("listitem")
				.map((li) => li.textContent),
		).toEqual([
			"Farik ran its checks",
			"Iris sent the screens back",
			"Ada sent the code back",
			"Kai checked the screens",
			"Ada reviewed the code",
			"Now you decide",
		]);
		await expectNoAxeViolations(container);
	});

	it("says_the_architect_reviewed_the_code_after_the_screens_passed", async () => {
		const { container } = await reviewedTwice();
		// The approved mockup: the reviewer's letter says it came after the Designer's pass.
		const ada = await screen.findByRole("region", {
			name: "Ada, your Architect, reviewed the code after Kai passed the screens",
		});
		expect(within(ada).getByText(REVIEW)).toBeTruthy();
		const about = screen.getByRole("region", { name: "About this task" });
		await within(about).findByText("Code reviewed");
		expect(within(about).getByText("Friday 25 September, by Ada")).toBeTruthy();
		// The send-back ends with what came of it.
		const back = screen.getByText(
			"Iris sent it back once, on Tuesday 22 September",
		);
		expect(
			within(back.closest("details") as HTMLElement).getByText(
				"Theo changed it, and the next look passed.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});
});
