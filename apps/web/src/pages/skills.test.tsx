import { expectNoAxeViolations } from "@farik/ui/test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";

type Frame = ReturnType<FakeSocket["calls"]>[number];

import { sentCommand } from "../test/gate.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";

const TEAM = {
	name: "Corner Bakery",
	agents: [
		{
			id: "theo",
			display_name: "Theo",
			role: "software_developer",
			avatar: "developer",
			persona: "Theo persona",
			status: "active",
		},
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};
const EFFECTIVE = [
	{
		id: "theo",
		model: { id: "claude-opus-5-5", label: "Strongest model", effort: "high" },
		tiers: ["read"],
		base_tiers: ["read"],
	},
];
const row = (
	level: string,
	name: string,
	state: string,
	description = `About ${name}.`,
) => ({ level, name, description, state, bytes: 900 });
const ROWS = [
	row("role", "implementing-a-contract", "in_use"),
	row("role", "writing-tests", "replaced"),
	row("role", "api-style", "replaced"),
	row("team", "release-notes", "in_use", "Write release notes in our voice"),
	row("team", "api-style", "replaced"),
	row("team", "deploy-checklist", "review", "a\u202eb"),
	row("agent", "api-style", "in_use"),
	row("agent", "old-skill", "missing"),
];
const NOTES = `---
name: release-notes
description: "Write release notes in our voice"
---
# Release notes
Write for the people who use the product.
`;
const HASH = "a".repeat(64);

/** Theo's page with his skills answered. */
async function opened(skills: object[] = ROWS) {
	const { container, socket } = await renderApp("/team/theo");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", {
		team: TEAM,
		agents: EFFECTIVE,
		judges: { auto: null, architect: null, scrum_master: null },
		max_agents: 7,
		connectors: [],
		sandboxed: true,
	});
	await answerQuery(s, "models.list", { models: [] });
	await answerQuery(s, "skills.list", { skills });
	await screen.findByRole("heading", { name: "Theo, your Developer" });
	return { container, s };
}

/** The body of a sent command. */
const bodyOf = (frame: Frame) =>
	(frame.params as { command: { body: object } }).command.body;
const teamAsked = (s: FakeSocket) =>
	s.calls("query").filter((q) => q.params.name === "team.get").length;
const group = (name: string) => screen.getByRole("list", { name });
const inRow = (list: string, skill: string) =>
	within(group(list))
		.getAllByText(skill, { selector: "strong" })[0]
		?.closest("li") as HTMLElement;

/** Edit on a skill's row, its folder answered. */
async function editing(
	s: FakeSocket,
	list: string,
	skill: string,
	got: { files: Record<string, string>; ignored_fields?: string[] },
) {
	fireEvent.click(
		within(inRow(list, skill)).getByRole("button", { name: `Edit ${skill}` }),
	);
	await answerQuery(s, "skill.get", {
		sha256: HASH,
		ignored_fields: [],
		...got,
	});
	return screen.findByRole("dialog", { name: `Edit ${skill}` });
}

const NOTES_FILES = {
	"SKILL.md": NOTES,
	"references/checklist.md": "x".repeat(2048),
	"templates/note.md": "y".repeat(1024),
};

describe("skills on the agent page", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("agent_edit_lists_skills_by_level", async () => {
		const { container } = await opened();
		await expectNoAxeViolations(container);
		const role = group("Comes with Developer");
		expect(
			within(role).getByText("implementing-a-contract", { selector: "strong" }),
		).toBeTruthy();
		expect(role.textContent).toContain(
			"Your team’s writing-tests replaces this one",
		);
		expect(role.textContent).toContain(
			"Theo’s own api-style replaces this one",
		);
		const team = group("For the whole team");
		expect(team.textContent).toContain("Write release notes in our voice");
		expect(team.textContent).toContain(
			"Theo’s own api-style replaces this one for Theo",
		);
		expect(
			inRow("For the whole team", "deploy-checklist").textContent,
		).toContain("Changed in the project. Review before Theo uses it");
		expect(
			within(team).getByRole("button", { name: "Review deploy-checklist" }),
		).toBeTruthy();
		// A description comes from the project's file: its control characters are written out.
		expect(
			inRow("For the whole team", "deploy-checklist").textContent,
		).toContain("a\\u{202e}b");
		// A row to review is neither edited nor removed before it is read.
		expect(
			within(inRow("For the whole team", "deploy-checklist")).queryByRole(
				"button",
				{ name: /^(Edit|Remove)/ },
			),
		).toBeNull();
		const own = group("Just for Theo");
		expect(inRow("Just for Theo", "old-skill").textContent).toContain(
			"old-skill is in the team file but its folder is gone",
		);
		expect(
			within(own).getByRole("button", { name: "Remove old-skill" }),
		).toBeTruthy();
		expect(screen.getByRole("button", { name: "Add a skill" })).toBeTruthy();
	});

	it("agent_edit_removes_a_skill_after_asking", async () => {
		const { s } = await opened();
		fireEvent.click(
			within(group("Just for Theo")).getByRole("button", {
				name: "Remove old-skill",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Remove old-skill?",
		});
		expect(dialog.textContent).toContain(
			"Theo stops using it, and its folder is deleted from the project.",
		);
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Remove old-skill" }),
		);
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: {
				command: "skill_remove",
				body: { level: "agent", agent: "theo", name: "old-skill" },
			},
		});
		await s.reply(sent, { said: "Removed", events: [5] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		// The team file's pins are read again, so a later Save cannot write the old ones back.
		await waitFor(() => expect(teamAsked(s)).toBe(2));
	});

	it("skill_edit_adds_a_skill_for_one_agent", async () => {
		const { s } = await opened();
		fireEvent.click(screen.getByRole("button", { name: "Add a skill" }));
		const dialog = await screen.findByRole("dialog", {
			name: "Add a skill for Theo",
		});
		expect(dialog.hasAttribute("data-fills-phone")).toBe(true);
		fireEvent.change(within(dialog).getByLabelText("Name"), {
			target: { value: "release-notes" },
		});
		fireEvent.change(
			within(dialog).getByLabelText("When should Theo use it?"),
			{
				target: { value: 'Say "what changed"' },
			},
		);
		fireEvent.change(within(dialog).getByLabelText("Instructions"), {
			target: { value: "# Notes\nBe short.\n" },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Next" }));
		fireEvent.click(
			await within(dialog).findByRole("button", { name: "Add skill" }),
		);
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: {
				command: "skill_save",
				body: {
					level: "agent",
					agent: "theo",
					files: {
						"SKILL.md":
							'---\nname: release-notes\ndescription: "Say \\"what changed\\""\n---\n# Notes\nBe short.\n',
					},
				},
			},
		});
	});

	it("skill_edit_shows_the_whole_text_before_adding", async () => {
		const { s } = await opened();
		const dialog = await editing(s, "For the whole team", "release-notes", {
			files: NOTES_FILES,
		});
		expect(
			within(dialog).queryByRole("button", { name: "Add skill" }),
		).toBeNull();
		expect(dialog.textContent).toContain(
			"Also in this skill: references/checklist.md, 2 KB; templates/note.md, 1 KB. Farik keeps them as they are.",
		);
		expect(
			(within(dialog).getByLabelText("Instructions") as HTMLTextAreaElement)
				.value,
		).toBe("# Release notes\nWrite for the people who use the product.\n");
		fireEvent.click(within(dialog).getByRole("button", { name: "Next" }));
		const reading = await within(dialog).findByText(
			"Farik will follow these instructions and use these files. Read them before adding.",
		);
		expect(reading).toBeTruthy();
		expect(dialog.querySelector('[data-trust="untrusted"]')).toBeTruthy();
		expect(dialog.textContent).toContain(
			'name: release-notes\ndescription: "Write release notes in our voice"',
		);
		fireEvent.click(within(dialog).getByRole("button", { name: "Add skill" }));
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: {
				command: "skill_save",
				body: { level: "team", files: NOTES_FILES },
			},
		});
	});

	it("skill_edit_shows_every_file_before_adding", async () => {
		const { s } = await opened();
		const dialog = await editing(s, "For the whole team", "release-notes", {
			files: {
				...NOTES_FILES,
				"templates/note.md": `${"y".repeat(1024)}\u202e`,
			},
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Next" }));
		await within(dialog).findByRole("button", { name: "Add skill" });
		expect(dialog.querySelectorAll('[data-trust="untrusted"]')).toHaveLength(3);
		expect(dialog.textContent).toContain("x".repeat(2048));
		expect(dialog.textContent).toContain(`${"y".repeat(1024)}\\u{202e}`);
		expect(within(dialog).getByText("references/checklist.md")).toBeTruthy();
	});

	it("skill_edit_offers_replace_when_the_name_is_shipped_elsewhere", async () => {
		const { s } = await opened();
		fireEvent.click(screen.getByRole("button", { name: "Add a skill" }));
		const dialog = await screen.findByRole("dialog", {
			name: "Add a skill for Theo",
		});
		fireEvent.change(within(dialog).getByLabelText("Name"), {
			target: { value: "design-review" },
		});
		fireEvent.change(
			within(dialog).getByLabelText("When should Theo use it?"),
			{ target: { value: "When designing." } },
		);
		fireEvent.change(within(dialog).getByLabelText("Instructions"), {
			target: { value: "# Ours\n" },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Next" }));
		fireEvent.click(
			await within(dialog).findByRole("button", { name: "Add skill" }),
		);
		const first = await sentCommand(s);
		await s.reply(first, {
			error: { kind: "refused", detail: "skill_name_taken: x" },
		});
		expect(
			await within(dialog).findByText(
				"This replaces the skill Farik comes with of that name, design-review, for Theo.",
			),
		).toBeTruthy();
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Replace and add skill" }),
		);
		await waitFor(() => expect(s.calls("command")).toHaveLength(2));
		const [one, two] = s.calls("command").map(bodyOf);
		expect(one).not.toHaveProperty("replace_shipped");
		expect(two).toMatchObject({ replace_shipped: true });
	});

	it("skill_edit_strips_quotes_from_a_name_in_an_uploaded_file", async () => {
		const { s, container } = await opened();
		fireEvent.click(screen.getByRole("button", { name: "Add a skill" }));
		const dialog = await screen.findByRole("dialog", {
			name: "Add a skill for Theo",
		});
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Upload a SKILL.md instead" }),
		);
		fireEvent.change(
			container.ownerDocument.querySelector("input[type=file]") as Element,
			{
				target: {
					files: [
						new File(
							[
								'---\nname: "implementing-a-contract"\ndescription: "d"\n---\nb',
							],
							"SKILL.md",
						),
					],
				},
			},
		);
		await within(dialog).findByLabelText("The whole SKILL.md");
		fireEvent.click(within(dialog).getByRole("button", { name: "Next" }));
		expect(
			await within(dialog).findByRole("button", {
				name: "Replace and add skill",
			}),
		).toBeTruthy();
		expect(s).toBeTruthy();
	});

	it("skill_edit_uploads_a_skill_md", async () => {
		const { container } = await opened();
		fireEvent.click(screen.getByRole("button", { name: "Add a skill" }));
		const dialog = await screen.findByRole("dialog", {
			name: "Add a skill for Theo",
		});
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Upload a SKILL.md instead" }),
		);
		fireEvent.change(
			container.ownerDocument.querySelector("input[type=file]") as Element,
			{ target: { files: [new File([NOTES], "SKILL.md")] } },
		);
		const field = (await within(dialog).findByLabelText(
			"The whole SKILL.md",
		)) as HTMLTextAreaElement;
		expect(field.value).toBe(NOTES);
		expect(within(dialog).queryByLabelText("Name")).toBeNull();
	});

	it("skill_edit_says_a_new_level_adds_a_new_skill", async () => {
		const { s } = await opened();
		const dialog = await editing(s, "For the whole team", "release-notes", {
			files: { "SKILL.md": NOTES },
		});
		expect(dialog.textContent).not.toContain("This adds a new skill");
		fireEvent.click(within(dialog).getByRole("radio", { name: /Just Theo/ }));
		expect(dialog.textContent).toContain(
			"This adds a new skill. Remove release-notes yourself if you no longer want it.",
		);
	});

	it("agent_edit_reads_a_shipped_skill_as_trusted_text", async () => {
		const { s } = await opened();
		expect(
			within(
				inRow("Comes with Developer", "implementing-a-contract"),
			).getByRole("button", { name: "Read implementing-a-contract" }),
		).toBeTruthy();
		fireEvent.click(
			screen.getByRole("button", { name: "Read implementing-a-contract" }),
		);
		const asked = await waitFor(() => {
			const f = s.calls("query").find((q) => q.params.name === "skill.get");
			if (!f) throw new Error("skill.get was not asked");
			return f;
		});
		expect(asked.params).toMatchObject({
			params: {
				level: "role",
				role: "software_developer",
				name: "implementing-a-contract",
			},
		});
		await s.reply(asked, {
			files: {
				"SKILL.md": "---\nname: implementing-a-contract\n---\n# Shipped\n",
			},
			ignored_fields: [],
		});
		const dialog = await screen.findByRole("dialog", {
			name: "Read implementing-a-contract",
		});
		expect(dialog.textContent).toContain("# Shipped");
		expect(dialog.textContent).toContain("Farik comes with this skill");
		expect(dialog.querySelector('[data-trust="untrusted"]')).toBeNull();
		expect(
			within(dialog).queryByRole("button", { name: "Use this skill" }),
		).toBeNull();
	});

	it("skill_edit_opens_a_skill_with_ignored_fields_as_its_file", async () => {
		const { s } = await opened();
		const file = `---\nname: release-notes\ndescription: "x"\nhooks:\n  Stop: []\n---\n# Body\n`;
		const dialog = await editing(s, "For the whole team", "release-notes", {
			files: { "SKILL.md": file },
			ignored_fields: ["hooks"],
		});
		expect(
			(
				within(dialog).getByLabelText(
					"The whole SKILL.md",
				) as HTMLTextAreaElement
			).value,
		).toBe(file);
		expect(within(dialog).queryByLabelText("Name")).toBeNull();
		fireEvent.click(within(dialog).getByRole("button", { name: "Next" }));
		expect(
			await within(dialog).findByText("Farik ignores: hooks"),
		).toBeTruthy();
		fireEvent.click(within(dialog).getByRole("button", { name: "Add skill" }));
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: {
				command: "skill_save",
				body: { level: "team", files: { "SKILL.md": file } },
			},
		});
	});

	it("skill_edit_says_a_rename_adds_a_new_skill", async () => {
		const { s } = await opened();
		const dialog = await editing(s, "Just for Theo", "api-style", {
			files: {
				"SKILL.md":
					'---\nname: api-style\ndescription: "Endpoints."\n---\n# API\n',
				"references/checklist.md": "z",
			},
		});
		expect(dialog.textContent).not.toContain("This adds a new skill");
		fireEvent.change(within(dialog).getByLabelText("Name"), {
			target: { value: "payments-style" },
		});
		expect(dialog.textContent).toContain(
			"This adds a new skill. Remove api-style yourself if you no longer want it.",
		);
		fireEvent.click(within(dialog).getByRole("button", { name: "Next" }));
		fireEvent.click(
			await within(dialog).findByRole("button", { name: "Add skill" }),
		);
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: {
				command: "skill_save",
				body: {
					level: "agent",
					agent: "theo",
					files: {
						"SKILL.md":
							'---\nname: payments-style\ndescription: "Endpoints."\n---\n# API\n',
						"references/checklist.md": "z",
					},
				},
			},
		});
		expect(s.calls("command")).toHaveLength(1);
	});

	it("skill_edit_replaces_a_shipped_skill_only_on_its_button", async () => {
		const { s } = await opened();
		fireEvent.click(screen.getByRole("button", { name: "Add a skill" }));
		const dialog = await screen.findByRole("dialog", {
			name: "Add a skill for Theo",
		});
		fireEvent.change(within(dialog).getByLabelText("Name"), {
			target: { value: "implementing-a-contract" },
		});
		fireEvent.change(
			within(dialog).getByLabelText("When should Theo use it?"),
			{
				target: { value: "When building." },
			},
		);
		fireEvent.change(within(dialog).getByLabelText("Instructions"), {
			target: { value: "# Ours\n" },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Next" }));
		expect(
			await within(dialog).findByText(
				"This replaces Developer’s own implementing-a-contract for Theo.",
			),
		).toBeTruthy();
		expect(
			within(dialog).queryByRole("button", { name: "Add skill" }),
		).toBeNull();
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Replace and add skill" }),
		);
		const sent = await sentCommand(s);
		expect(
			(sent.params as { command: { body: object } }).command.body,
		).toMatchObject({ replace_shipped: true });
	});

	it.each([
		[
			"skill_runs_commands",
			"This skill runs commands when it loads, which Farik doesn’t allow.",
			"Instructions",
			"When should Theo use it?",
		],
		[
			"skill_attaches_files",
			"This skill pulls in files when it loads, which Farik doesn’t allow. Name a file without the @.",
			"Instructions",
			"When should Theo use it?",
		],
		[
			"skill_too_large",
			"Instructions are limited to 32 KB.",
			"Instructions",
			"When should Theo use it?",
		],
		[
			"skill_file_not_text",
			"A file in this skill is not plain text, which Farik cannot use.",
			"Instructions",
			"When should Theo use it?",
		],
		[
			"skill_description_invalid",
			"Say when to use it, in 1 to 1024 characters.",
			"When should Theo use it?",
			"Instructions",
		],
	])(
		"skill_edit_says_refusals_at_their_field (%s)",
		async (code, words, under, not) => {
			const { s } = await opened();
			fireEvent.click(screen.getByRole("button", { name: "Add a skill" }));
			const dialog = await screen.findByRole("dialog", {
				name: "Add a skill for Theo",
			});
			fireEvent.change(within(dialog).getByLabelText("Name"), {
				target: { value: "deploy-notes" },
			});
			fireEvent.change(
				within(dialog).getByLabelText("When should Theo use it?"),
				{
					target: { value: "Always." },
				},
			);
			fireEvent.change(within(dialog).getByLabelText("Instructions"), {
				target: { value: "# Deploy\n" },
			});
			fireEvent.click(within(dialog).getByRole("button", { name: "Next" }));
			fireEvent.click(
				await within(dialog).findByRole("button", { name: "Add skill" }),
			);
			const sent = await sentCommand(s);
			await s.reply(sent, {
				error: { kind: "refused", detail: `${code}: the daemon's words` },
			});
			const field = await within(dialog).findByLabelText(under);
			expect(field.getAttribute("aria-describedby")).toBeTruthy();
			expect(field.getAttribute("aria-invalid")).toBe("true");
			expect(
				within(dialog).getByLabelText(not).getAttribute("aria-invalid"),
			).not.toBe("true");
			expect(dialog.textContent).toContain(words);
			expect(dialog.textContent).not.toContain("the daemon's words");
		},
	);

	/** `skill`'s Review (deploy-checklist's), its folder answered with `got`. */
	async function reviewing(
		s: FakeSocket,
		got: object | [number, string],
		skill = "deploy-checklist",
	) {
		fireEvent.click(screen.getByRole("button", { name: `Review ${skill}` }));
		const frame = await waitFor(() => {
			const f = s.calls("query").find((q) => q.params.name === "skill.get");
			if (!f) throw new Error("skill.get was not asked");
			return f;
		});
		if (Array.isArray(got)) await s.fail(frame, -32005, got[1]);
		else await s.reply(frame, got);
		return screen.findByRole("dialog", { name: `Review ${skill}` });
	}

	it("skill_review_confirms_with_the_hash_it_showed", async () => {
		const { s } = await opened();
		const dialog = await reviewing(s, {
			files: {
				"SKILL.md": "---\nname: deploy-checklist\n---\n# Deploy\n",
				"references/rollback.md": "Undo it.",
			},
			sha256: HASH,
			ignored_fields: [],
		});
		expect(dialog.textContent).toContain(
			"It came with the project, by a clone, a pull or an edit, and Theo won’t use it until you’ve read it.",
		);
		// One caption above its frame and one in the list of files.
		expect(within(dialog).getAllByText("references/rollback.md")).toHaveLength(
			2,
		);
		expect(dialog.textContent).toContain("Undo it.");
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Use this skill" }),
		);
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: {
				command: "skill_confirm",
				body: {
					level: "team",
					name: "deploy-checklist",
					sha256: HASH,
				},
			},
		});
	});

	it("skill_review_shows_a_refused_folder_with_remove_alone", async () => {
		const { s } = await opened();
		const dialog = await reviewing(s, [
			-32005,
			"skill_runs_commands: SKILL.md runs a command",
		]);
		expect(dialog.textContent).toContain(
			"This skill can’t be used: it runs commands when it loads.",
		);
		expect(
			within(dialog).queryByRole("button", { name: "Use this skill" }),
		).toBeNull();
		expect(within(dialog).getByRole("button", { name: "Remove" })).toBeTruthy();
	});

	it("skill_review_replaces_a_shipped_name_only_after_saying_so", async () => {
		const { s } = await opened([
			...ROWS,
			row("team", "implementing-a-contract", "review"),
		]);
		const dialog = await reviewing(
			s,
			{
				files: { "SKILL.md": "---\nname: implementing-a-contract\n---\nb\n" },
				sha256: HASH,
				ignored_fields: [],
			},
			"implementing-a-contract",
		);
		expect(dialog.textContent).toContain(
			"This replaces Developer’s own implementing-a-contract for your whole team.",
		);
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Use this skill" }),
		);
		const sent = await sentCommand(s);
		expect(
			(sent.params as { command: { body: object } }).command.body,
		).toMatchObject({ replace_shipped: true, sha256: HASH });
	});

	it("skill_review_offers_replacing_a_name_shipped_elsewhere", async () => {
		const { s } = await opened([
			...ROWS,
			row("team", "design-review", "review"),
		]);
		const dialog = await reviewing(
			s,
			{
				files: { "SKILL.md": "---\nname: design-review\n---\nb\n" },
				sha256: HASH,
				ignored_fields: [],
			},
			"design-review",
		);
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Use this skill" }),
		);
		const first = await sentCommand(s);
		await s.reply(first, {
			error: { kind: "refused", detail: "skill_name_taken: x" },
		});
		expect(
			await within(dialog).findByText(
				"This replaces the skill Farik comes with of that name, design-review, for your whole team.",
			),
		).toBeTruthy();
		expect(dialog.textContent).not.toContain("Farik cannot add");
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Use this skill" }),
		);
		await waitFor(() => expect(s.calls("command")).toHaveLength(2));
		expect(bodyOf(s.calls("command")[1] as Frame)).toMatchObject({
			replace_shipped: true,
		});
	});

	it("skill_review_says_a_file_that_is_not_text_cannot_be_used", async () => {
		const { s } = await opened();
		const dialog = await reviewing(s, [
			-32005,
			"skill_file_not_text: references/a.md is not UTF-8 text, or holds a NUL.",
		]);
		expect(dialog.textContent).toContain(
			"This skill can’t be used: a file in it is not plain text.",
		);
	});

	it("skill_review_renders_markup_as_text", async () => {
		const { s } = await opened();
		const markup = "<img src=x onerror=alert(1)><b>bold</b>\u202e";
		const dialog = await reviewing(s, {
			files: { "SKILL.md": `---\nname: deploy-checklist\n---\n${markup}\n` },
			sha256: HASH,
			ignored_fields: [],
		});
		expect(dialog.querySelector("img, b")).toBeNull();
		expect(dialog.textContent).toContain(markup.replace("\u202e", "\\u{202e}"));
		expect(dialog.querySelector('[data-trust="untrusted"]')).toBeTruthy();
		expect(en.skillReviewUse).toBe("Use this skill");
	});
});
