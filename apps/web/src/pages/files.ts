import type { Role } from "@catervas/ui";
import { t } from "../strings/t.ts";
import type { Integration } from "./DocumentChanges.tsx";

export type { Integration };

/** `files.tree`, as the client maps the wire (spec 5.17). */
export type FilesTree = {
	integration: Integration;
	folders: {
		folder: string;
		name: string;
		role: Role;
		agentId: string;
		documents: { path: string; name: string; twin: string | null }[];
	}[];
};

/** One file as a branch holds it: its text, its blob (what a save is based on) and its last change. */
export type FileVersion = {
	path: string;
	text: string;
	blob: string;
	lastChange: {
		at: string;
		taskId: string | null;
		agentId: string | null;
		author: string;
		byYou: boolean;
	} | null;
};

/** `files.read`: a document and its twin, as the integration branch holds them. */
export type FileRead = {
	document: FileVersion;
	twin: FileVersion | null;
	stale: boolean;
	rederiving: boolean;
	editable: boolean;
	askable: boolean;
	waiting: { change: number; branch: string } | null;
};

/** Where the team's folders are in the project, which a document's route leaves out. */
const ROOT = "docs/catervas/";

/** A document's page: `docs/catervas/product/roadmap.md` is `/files/product/roadmap.md`. */
export function fileRoute(path: string): string {
	return `/files/${path.startsWith(ROOT) ? path.slice(ROOT.length) : path}`;
}

/** The project path a route's splat names: `product/roadmap.md` is `docs/catervas/product/roadmap.md`. */
export function filePath(splat: string): string {
	return `${ROOT}${splat}`;
}

const words = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

/** How long before `now` it was, in the largest whole unit: "just now", "5 days ago", "yesterday". */
export function timeAgo(at: string, now: Date): string {
	const minutes = Math.floor((now.getTime() - Date.parse(at)) / 60_000);
	if (minutes < 1) return t("filesJustNow");
	if (minutes < 60) return words.format(-minutes, "minute");
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return words.format(-hours, "hour");
	const days = Math.floor(hours / 24);
	if (days < 30) return words.format(-days, "day");
	const months = Math.floor(days / 30);
	if (months < 12) return words.format(-months, "month");
	return words.format(-Math.max(1, Math.floor(days / 365)), "year");
}
