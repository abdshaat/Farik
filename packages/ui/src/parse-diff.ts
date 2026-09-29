export type DiffFile = {
	path: string;
	lines: { kind: "hunk" | "context" | "added" | "removed"; text: string }[];
};

const strip = (p: string) => p.replace(/^[ab]\//, "");

export function parseDiff(diff: string): DiffFile[] {
	const rows = diff.split("\n");
	const files: DiffFile[] = [];
	let file: DiffFile | undefined;
	let inHunk = false;
	let oldPath = "";
	rows.forEach((row, i) => {
		if (row.startsWith("diff --git ")) {
			file = { path: strip(row.slice(row.lastIndexOf(" b/") + 1)), lines: [] };
			files.push(file);
			inHunk = false;
			return;
		}
		// A `---`/`+++` pair starts a file when there is no `diff --git` line.
		if (row.startsWith("--- ") && rows[i + 1]?.startsWith("+++ ")) {
			if (inHunk || !file) {
				file = { path: "", lines: [] };
				files.push(file);
				inHunk = false;
			}
			oldPath = row.slice(4);
			return;
		}
		if (!file) return;
		if (!inHunk && row.startsWith("+++ ")) {
			const p = row.slice(4);
			file.path = strip(p === "/dev/null" ? oldPath : p);
			return;
		}
		if (row.startsWith("@@")) {
			inHunk = true;
			file.lines.push({ kind: "hunk", text: row });
		} else if (!inHunk) {
			return; // index, mode, similarity, rename lines
		} else if (row.startsWith("+")) {
			file.lines.push({ kind: "added", text: row.slice(1) });
		} else if (row.startsWith("-")) {
			file.lines.push({ kind: "removed", text: row.slice(1) });
		} else if (row.startsWith(" ")) {
			file.lines.push({ kind: "context", text: row.slice(1) });
		}
		// `\ No newline at end of file` and blank trailing rows are dropped.
	});
	return files;
}
