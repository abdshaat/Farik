import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { FakeSocket } from "../../test/fake-socket.ts";
import { answerCalls, Refused } from "../../test/finding.ts";
import { renderApp, where } from "../../test/render-app.tsx";

const ago = (days: number) =>
	new Date(Date.now() - days * 86_400_000).toISOString();

const WEB = {
	key: "github.com/me/tidepool-web",
	name: "tidepool-web",
	last_at: ago(0),
	folder: "~/code/tidepool-web",
};
const API = {
	key: "github.com/me/tidepool-api",
	name: "tidepool-api",
	last_at: ago(1),
	folder: "~/code/tidepool-api",
};
const INFRA = {
	key: "github.com/me/tidepool-infra",
	name: "tidepool-infra",
	last_at: ago(9),
	folder: "~/code/tidepool-infra",
};
const WATCH = {
	key: "github.com/me/price-watch",
	name: "price-watch",
	last_at: ago(47),
	folder: "~/code/price-watch",
};

const PAIRS = [
	{
		a: WEB,
		b: API,
		proof: {
			kind: "path_dependency",
			a: WEB.key,
			b: API.key,
			sentence: "tidepool-web uses tidepool-api's code.",
			ends: [
				{
					project: WEB.key,
					name: "tidepool-web",
					file: "package.json",
					line: 14,
					text: '"@tidepool/api-client": "file:../tidepool-api/client"',
				},
				{
					project: API.key,
					name: "tidepool-api",
					file: "client/package.json",
					line: 2,
					text: '"name": "@tidepool/api-client"',
				},
			],
		},
		also: [],
		hints: ["same_days"],
	},
	{
		a: INFRA,
		b: API,
		proof: {
			kind: "deploys",
			a: INFRA.key,
			b: API.key,
			sentence: "tidepool-infra deploys tidepool-api.",
			ends: [
				{
					project: INFRA.key,
					name: "tidepool-infra",
					file: "compose.yaml",
					line: 15,
					text: "build: ../tidepool-api",
				},
			],
		},
		also: ["tidepool-infra deploys tidepool-web."],
		hints: [],
	},
];

describe("do these work together", () => {
	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
		localStorage.clear();
	});

	it("asks_one_pair_at_a_time_with_its_proof", async () => {
		const { hub } = await renderApp("/find/together");
		const h = hub as FakeSocket;
		const answered: Record<string, unknown>[] = [];
		answerCalls(h, {
			// The hub asks again after each answer: what is answered is not asked.
			"linking.pairs": () => ({ pairs: PAIRS.slice(answered.length) }),
			"linking.answer": (params: Record<string, unknown>) => {
				answered.push(params);
				return {};
			},
			"linking.groups": () => ({
				groups: answered.some((one) => one.answer === "yes")
					? [
							{
								projects: [WEB.key, API.key],
								names: ["tidepool-web", "tidepool-api"],
								why: ["tidepool-web uses tidepool-api's code."],
							},
						]
					: [],
			}),
			"linking.maybe": () => ({
				maybe: [{ a: WATCH, b: API, hints: ["same_owner", "same_days"] }],
			}),
		});

		// One pair: who, the proof in a sentence, and what else is known.
		expect(await screen.findByText("Question 1 of 2")).toBeTruthy();
		expect(
			screen.getByRole("heading", { name: "Do these work together?" }),
		).toBeTruthy();
		expect(screen.getByText("Today · ~/code/tidepool-web")).toBeTruthy();
		expect(screen.getByText("Yesterday · ~/code/tidepool-api")).toBeTruthy();
		expect(
			screen.getByText("tidepool-web uses tidepool-api's code."),
		).toBeTruthy();
		expect(
			screen.getByText("Also: you worked on both on the same days."),
		).toBeTruthy();
		expect(
			screen.getByText(
				"Not sure? Keep them separate. You can change it later.",
			),
		).toBeTruthy();

		// The files and lines are behind the info button.
		expect(screen.queryByText("Checked again just now.")).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "More about this" }));
		expect(
			screen.getByText("tidepool-web/package.json, line 14:", {
				exact: false,
			}),
		).toBeTruthy();
		expect(
			screen.getByText('"@tidepool/api-client": "file:../tidepool-api/client"'),
		).toBeTruthy();
		expect(
			screen.getByText("tidepool-api/client/package.json, line 2:", {
				exact: false,
			}),
		).toBeTruthy();
		expect(screen.getByText("Checked again just now.")).toBeTruthy();

		// Yes sends the answer and shows the next pair, with the second proof as "also".
		fireEvent.click(screen.getByRole("button", { name: "Yes, one team" }));
		expect(await screen.findByText("Question 2 of 2")).toBeTruthy();
		expect(answered).toEqual([{ a: WEB.key, b: API.key, answer: "yes" }]);
		expect(screen.getByText("Also: it deploys tidepool-web.")).toBeTruthy();

		// No keeps them separate.
		fireEvent.click(screen.getByRole("button", { name: "No, keep separate" }));
		expect(await screen.findByText("That's all we could prove")).toBeTruthy();
		expect(answered[1]).toEqual({ a: INFRA.key, b: API.key, answer: "no" });

		// What the Yes answers made, and everything else separate.
		expect(
			screen.getByText("tidepool-web and tidepool-api: one team."),
		).toBeTruthy();
		expect(screen.getByText("Everything else stays separate.")).toBeTruthy();

		// Maybe related is collapsed until opened, and answered Separate unless it is.
		expect(screen.queryByRole("radio", { name: "One team" })).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Maybe related (1)" }));
		const maybe = within(screen.getByRole("radiogroup"));
		expect(screen.getByText("price-watch and tidepool-api")).toBeTruthy();
		expect(
			screen.getByText(
				"No proof. Only hints: the same GitHub account, worked on the same days.",
			),
		).toBeTruthy();
		expect(
			(maybe.getByRole("radio", { name: "Separate" }) as HTMLInputElement)
				.checked,
		).toBe(true);
		fireEvent.click(maybe.getByRole("radio", { name: "One team" }));
		await waitFor(() => expect(answered).toHaveLength(3));
		expect(answered[2]).toEqual({
			a: WATCH.key,
			b: API.key,
			answer: "yes",
			from_maybe: true,
		});
		fireEvent.click(maybe.getByRole("radio", { name: "Separate" }));
		await waitFor(() => expect(answered).toHaveLength(4));
		expect(answered[3]).toEqual({ a: WATCH.key, b: API.key, answer: "no" });

		// Continue leads on, to the proposal of teams.
		fireEvent.click(screen.getByRole("button", { name: "Continue" }));
		await waitFor(() => expect(where()).toBe("/find/teams"));
	});

	it("says_so_when_the_files_no_longer_prove_it", async () => {
		const { hub } = await renderApp("/find/together");
		const h = hub as FakeSocket;
		answerCalls(h, {
			"linking.pairs": { pairs: PAIRS },
			"linking.answer": new Refused(
				"proof_no_longer_holds",
				"the files that proved it no longer say so",
			),
			"linking.groups": { groups: [] },
			"linking.maybe": { maybe: [] },
		});
		await screen.findByText("Question 1 of 2");
		fireEvent.click(screen.getByRole("button", { name: "Yes, one team" }));
		expect(
			await screen.findByText(
				"The files that showed these work together have changed. Nothing was linked.",
			),
		).toBeTruthy();
		expect(screen.getByText("Question 1 of 2")).toBeTruthy();
	});
});
