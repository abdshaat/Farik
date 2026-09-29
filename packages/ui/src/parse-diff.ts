export type DiffFile = {
	path: string;
	note?: "renamed" | "binary";
	lines: { kind: "hunk" | "context" | "added" | "removed"; text: string }[];
};

const strip = (p: string) => p.replace(/^[ab]\//, "");
const hunkHeader = /^@@ -\d+(?:,(\d+))? \+\d+(?:,(\d+))? @@/;

export function parseDiff(diff: string): DiffFile[] {
	const files: DiffFile[] = [];
	let file: DiffFile | undefined;
	let oldPath = "";
	// Lines still to read in the current hunk, per side; while either is
	// above zero a row is hunk content, whatever it starts with.
	let oldLeft = 0;
	let newLeft = 0;
	const start = (path: string) => {
		file = { path, lines: [] };
		files.push(file);
	};
	for (const row of diff.split(/\r?\n/)) {
		if (file && (oldLeft > 0 || newLeft > 0)) {
			if (row.startsWith("+")) {
				newLeft--;
				file.lines.push({ kind: "added", text: row.slice(1) });
			} else if (row.startsWith("-")) {
				oldLeft--;
				file.lines.push({ kind: "removed", text: row.slice(1) });
			} else if (!row.startsWith("\\")) {
				// A space, or a blank row whose space was trimmed, is context.
				oldLeft--;
				newLeft--;
				file.lines.push({ kind: "context", text: row.slice(1) });
			}
			continue;
		}
		const hunk = hunkHeader.exec(row);
		if (file && hunk) {
			oldLeft = Number(hunk[1] ?? 1);
			newLeft = Number(hunk[2] ?? 1);
			delete file.note; // a rename with changes needs no note
			file.lines.push({ kind: "hunk", text: row });
		} else if (row.startsWith("diff --git ")) {
			start(strip(row.slice(row.lastIndexOf(" b/") + 1)));
		} else if (row.startsWith("--- ")) {
			// Without a `diff --git` line, `---` starts the next file.
			if (!file || file.lines.length > 0) start("");
			oldPath = row.slice(4);
		} else if (file && row.startsWith("+++ ")) {
			const p = row.slice(4);
			file.path = strip(p === "/dev/null" ? oldPath : p);
		} else if (file && row.startsWith("rename to ")) {
			file.note = "renamed";
		} else if (file && /^Binary files .* differ$/.test(row)) {
			file.note = "binary";
		}
		// index, mode, similarity lines and `\ No newline` are dropped.
	}
	return files;
}
