import { readFileSync } from "node:fs";
import { join } from "node:path";
import { screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import { media } from "../test/media.ts";
import { answerStatus, renderApp } from "../test/render-app.tsx";
import styles from "./Shell.module.css";

// The rail unmounts when the socket closes, so the connection's status is set by hand here.
const state = vi.hoisted(() => ({ status: "" }));
vi.mock("../app/connection.tsx", async (original) => {
	const real = await original<typeof import("../app/connection.tsx")>();
	return {
		...real,
		useConnection: () => {
			const c = real.useConnection();
			return state.status ? { ...c, status: state.status } : c;
		},
	};
});

afterEach(() => {
	state.status = "";
	media.reset();
});

describe("the rail's Connected dot outside 'open'", () => {
	it.each(["connecting", "closed"])("stays_still_while_%s", async (status) => {
		media.set("(min-width: 1024px)", true);
		state.status = status;
		const { socket } = await renderApp("/team");
		if (!socket) throw new Error("no socket");
		await answerStatus(socket, false);
		const dot = (await screen.findByText(en.connecting)).querySelector("span");
		expect(dot?.classList).toContain(styles.dot);
		expect(dot?.classList).not.toContain(styles.live);
		// Only .live carries the animation; the plain dot has none.
		const css = readFileSync(
			join(import.meta.dirname, "Shell.module.css"),
			"utf8",
		);
		const plain = /\.dot \{[^}]*\}/.exec(css)?.[0] ?? "";
		expect(plain).not.toMatch(/animation/);
	});
});
