import { describe, expect, it } from "vitest";
import { parseDiff } from "./parse-diff.ts";

const two = `diff --git a/a.txt b/a.txt
index 1..2 100644
--- a/a.txt
+++ b/a.txt
@@ -1,2 +1,2 @@
 keep
-old
+new
diff --git a/dir/b.txt b/dir/b.txt
index 3..4 100644
--- a/dir/b.txt
+++ b/dir/b.txt
@@ -1 +1 @@
-x
+y`;

describe("parseDiff", () => {
	it("reads files, hunks and lines", () => {
		expect(parseDiff(two)).toEqual([
			{
				path: "a.txt",
				lines: [
					{ kind: "hunk", text: "@@ -1,2 +1,2 @@" },
					{ kind: "context", text: "keep" },
					{ kind: "removed", text: "old" },
					{ kind: "added", text: "new" },
				],
			},
			{
				path: "dir/b.txt",
				lines: [
					{ kind: "hunk", text: "@@ -1 +1 @@" },
					{ kind: "removed", text: "x" },
					{ kind: "added", text: "y" },
				],
			},
		]);
	});

	it("ignores the header lines", () => {
		const [file] = parseDiff(two);
		expect(file?.lines.map((l) => l.text)).not.toContain("a/a.txt");
		expect(file?.lines).toHaveLength(4);
	});

	it("reads a new and a deleted file", () => {
		const files = parseDiff(
			[
				"--- /dev/null",
				"+++ b/new.txt",
				"@@ -0,0 +1 @@",
				"+hi",
				"\\ No newline at end of file",
				"--- a/old.txt",
				"+++ /dev/null",
				"@@ -1 +0,0 @@",
				"-bye",
			].join("\n"),
		);
		expect(files.map((f) => f.path)).toEqual(["new.txt", "old.txt"]);
		expect(files[0]?.lines).toEqual([
			{ kind: "hunk", text: "@@ -0,0 +1 @@" },
			{ kind: "added", text: "hi" },
		]);
	});

	it("gives nothing for text that is not a diff", () => {
		expect(parseDiff("hello\nworld")).toEqual([]);
		expect(parseDiff("")).toEqual([]);
	});
});
