import { render } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { en } from "../strings/en.ts";
import { renderMessageText } from "./MessageText.tsx";

const show = (text: string, waiting: string[] = []) =>
	render(
		<MemoryRouter>
			<p>
				{renderMessageText(
					text,
					[],
					waiting.map((taskId) => ({ taskId })),
				)}
			</p>
		</MemoryRouter>,
	).container;

describe("message text", () => {
	it("marks_a_waiting_task_once", () => {
		const box = show("CTV-22 is stuck. CTV-22 again, and CTV-22.", ["CTV-22"]);
		expect(box.textContent?.split(en.channelWaitingOnYou)).toHaveLength(2);
		expect(box.querySelectorAll("a")).toHaveLength(3);
	});

	it("leaves_a_file_name_alone", () => {
		expect(show("See plans/CTV-1.html.").querySelectorAll("a")).toHaveLength(0);
		expect(show("CTV-1 is done.").querySelectorAll("a")).toHaveLength(1);
		// A full stop that ends a sentence still links.
		expect(show("It is CTV-1.").querySelectorAll("a")).toHaveLength(1);
	});
});
