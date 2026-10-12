import { expectNoAxeViolations } from "@catervas/ui/test";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import { Feed } from "./Feed.tsx";

/** The box's size, which jsdom does not lay out: the tests set what the browser would say. */
const geometry = { scrollHeight: 2000, clientHeight: 500 };

type Row = { seq: number; mine: boolean };
type Props = {
	rows: Row[];
	more: boolean;
	paging: "busy" | "failed" | undefined;
	onEarlier: () => void;
};

const rowsOf = (count: number, from = 1): Row[] =>
	Array.from({ length: count }, (_, i) => ({ seq: from + i, mine: false }));

/** A chat's heading, and its feed of `rows`, oldest first. */
function Harness({ rows, more, paging, onEarlier }: Props) {
	const newest = rows.at(-1);
	return (
		<>
			<h1 id="chat-title">Chat</h1>
			<Feed
				labelledBy="chat-title"
				newest={newest}
				oldest={rows[0]?.seq}
				more={more}
				paging={paging}
				onEarlier={onEarlier}
			>
				<ol aria-label="Messages">
					{rows.map((r) => (
						<li key={r.seq}>Line {r.seq}</li>
					))}
				</ol>
			</Feed>
		</>
	);
}

const defaults = (): Props => ({
	rows: rowsOf(10),
	more: false,
	paging: undefined,
	onEarlier: () => {},
});

/** Renders the harness, and returns a way to change its props and the box. */
function open(given: Partial<Props> = {}) {
	let props = { ...defaults(), ...given };
	const view = render(<Harness {...props} />);
	return {
		container: view.container,
		box: () => screen.getByRole("region", { name: "Chat" }),
		change(next: Partial<Props>, size?: Partial<typeof geometry>) {
			Object.assign(geometry, size);
			props = { ...props, ...next };
			view.rerender(<Harness {...props} />);
		},
	};
}

/** The user scrolls the box to `top`. */
function scrollTo(box: HTMLElement, top: number) {
	box.scrollTop = top;
	fireEvent.scroll(box);
}

const offer = () => screen.queryByRole("button", { name: en.chatsNewMessages });

describe("Feed", () => {
	beforeEach(() => {
		Object.assign(geometry, { scrollHeight: 2000, clientHeight: 500 });
		for (const key of ["scrollHeight", "clientHeight"] as const)
			Object.defineProperty(HTMLElement.prototype, key, {
				configurable: true,
				get: () => geometry[key],
			});
	});
	afterEach(() => {
		for (const key of ["scrollHeight", "clientHeight"])
			Reflect.deleteProperty(HTMLElement.prototype, key);
	});

	it("opens_at_the_newest", () => {
		const feed = open();
		expect(feed.box().scrollTop).toBe(2000);
	});

	it("follows_a_new_message_at_the_bottom", () => {
		const feed = open();
		feed.change({ rows: rowsOf(11) }, { scrollHeight: 2100 });
		expect(feed.box().scrollTop).toBe(2100);
		expect(offer()).toBeNull();
	});

	it("stays_put_and_offers_the_newest_when_scrolled_up", () => {
		const feed = open();
		scrollTo(feed.box(), 300);
		feed.change({ rows: rowsOf(11) }, { scrollHeight: 2100 });
		expect(feed.box().scrollTop).toBe(300);
		fireEvent.click(screen.getByRole("button", { name: en.chatsNewMessages }));
		expect(feed.box().scrollTop).toBe(2100);
		expect(offer()).toBeNull();
		expect(document.activeElement).toBe(feed.box());
	});

	it("follows_your_own_message_even_when_scrolled_up", () => {
		const feed = open();
		scrollTo(feed.box(), 300);
		feed.change(
			{ rows: [...rowsOf(10), { seq: 11, mine: true }] },
			{ scrollHeight: 2100 },
		);
		expect(feed.box().scrollTop).toBe(2100);
		expect(offer()).toBeNull();
	});

	it("clears_the_offer_at_the_bottom_by_hand", () => {
		const feed = open();
		scrollTo(feed.box(), 300);
		feed.change({ rows: rowsOf(11) }, { scrollHeight: 2100 });
		expect(offer()).not.toBeNull();

		// 2100 - 1551 - 500 is 49: one pixel more than counts as the bottom.
		scrollTo(feed.box(), 1551);
		expect(offer()).not.toBeNull();
		scrollTo(feed.box(), 1552);
		expect(offer()).toBeNull();

		// Back up by that pixel, the user has left the bottom again.
		scrollTo(feed.box(), 1551);
		expect(offer()).toBeNull();
		feed.change({ rows: rowsOf(12) }, { scrollHeight: 2200 });
		expect(offer()).not.toBeNull();
	});

	it("stays_at_the_bottom_when_the_box_changes", () => {
		const feed = open();
		feed.change({}, { clientHeight: 440, scrollHeight: 2060 });
		expect(feed.box().scrollTop).toBe(2060);

		scrollTo(feed.box(), 300);
		feed.change({}, { scrollHeight: 2100 });
		expect(feed.box().scrollTop).toBe(300);
	});

	it("keeps_a_move_the_page_made", () => {
		const feed = open();
		// The page moved the view itself (a meeting link's scrollIntoView), and no scroll event came.
		feed.box().scrollTop = 300;
		feed.change({}, { scrollHeight: 2100 });
		expect(feed.box().scrollTop).toBe(300);
	});

	it("asks_for_earlier_at_the_top", () => {
		const onEarlier = vi.fn();
		const feed = open({ more: true, onEarlier });
		scrollTo(feed.box(), 49);
		expect(onEarlier).not.toHaveBeenCalled();
		scrollTo(feed.box(), 48);
		expect(onEarlier).toHaveBeenCalledTimes(1);

		onEarlier.mockClear();
		for (const given of [
			{ paging: "busy" as const },
			{ paging: "failed" as const },
			{ paging: undefined, more: false },
		]) {
			feed.change(given);
			scrollTo(feed.box(), 0);
		}
		expect(onEarlier).not.toHaveBeenCalled();
	});

	it("keeps_the_place_when_earlier_messages_arrive", () => {
		const feed = open({ rows: rowsOf(10, 101) });
		scrollTo(feed.box(), 20);
		feed.change({ rows: rowsOf(20, 91) }, { scrollHeight: 3000 });
		expect(feed.box().scrollTop).toBe(1020);
		expect(offer()).toBeNull();
	});

	it("offers_earlier_by_button_and_says_when_it_failed", () => {
		const onEarlier = vi.fn();
		const feed = open({ more: true, onEarlier });
		const button = screen.getByRole("button", { name: en.channelEarlier });
		const list = screen.getByRole("list", { name: "Messages" });
		expect(feed.box().contains(button)).toBe(true);
		expect(
			button.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
		fireEvent.click(button);
		expect(onEarlier).toHaveBeenCalledTimes(1);

		feed.change({ paging: "busy" });
		expect(
			(
				screen.getByRole("button", {
					name: new RegExp(en.channelEarlier),
				}) as HTMLButtonElement
			).disabled,
		).toBe(true);

		feed.change({ paging: "failed" });
		expect(screen.getByRole("alert").textContent).toBe(en.channelEarlierFailed);

		feed.change({ more: false, paging: undefined });
		expect(
			screen.queryByRole("button", { name: en.channelEarlier }),
		).toBeNull();
	});

	it("passes_axe", async () => {
		const feed = open({ more: true });
		scrollTo(feed.box(), 300);
		feed.change({ more: true, rows: rowsOf(11) }, { scrollHeight: 2100 });
		expect(offer()).not.toBeNull();
		expect(
			screen.getByRole("button", { name: en.channelEarlier }),
		).toBeTruthy();
		await expectNoAxeViolations(feed.container);
	});
});
