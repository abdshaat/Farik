import { AVATAR_URLS, type AvatarKey } from "@catervas/ui";
import { expect } from "vitest";

/** Every picture of one of `agents` in `container` opens that agent's page, `/team/<id>`; at least one is drawn. */
export function picturesOpenTheirAgents(
	container: HTMLElement,
	agents: { id: string; avatar: string }[],
) {
	let seen = 0;
	for (const { id, avatar } of agents)
		for (const img of container.querySelectorAll(
			`img[src="${AVATAR_URLS[avatar as AvatarKey]}"]`,
		)) {
			seen++;
			expect(img.closest("a")?.getAttribute("href")).toBe(`/team/${id}`);
		}
	expect(seen).toBeGreaterThan(0);
}
