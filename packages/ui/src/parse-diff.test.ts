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
		expect(files[1]?.lines).toEqual([
			{ kind: "hunk", text: "@@ -1 +0,0 @@" },
			{ kind: "removed", text: "bye" },
		]);
	});

	it("reads hunk lines that look like headers by the hunk's counts", () => {
		const files = parseDiff(
			[
				"diff --git a/q.sql b/q.sql",
				"--- a/q.sql",
				"+++ b/q.sql",
				"@@ -1,2 +1,2 @@",
				"--- x",
				"+++ y",
				" select 1;",
			].join("\n"),
		);
		expect(files).toEqual([
			{
				path: "q.sql",
				lines: [
					{ kind: "hunk", text: "@@ -1,2 +1,2 @@" },
					{ kind: "removed", text: "-- x" },
					{ kind: "added", text: "++ y" },
					{ kind: "context", text: "select 1;" },
				],
			},
		]);
	});

	it("stops a hunk whose counts run past the next file", () => {
		const files = parseDiff(
			[
				"diff --git a/a.txt b/a.txt",
				"--- a/a.txt",
				"+++ b/a.txt",
				"@@ -1,5 +1,5 @@",
				" keep",
				"-old",
				"+new",
				"diff --git a/b b/b",
				"--- a/b",
				"+++ b/b",
				"@@ -1 +1 @@",
				"-x",
				"+y",
			].join("\n"),
		);
		expect(files.map((f) => f.path)).toEqual(["a.txt", "b"]);
		expect(files[0]?.lines).toHaveLength(4);
		expect(files[1]?.lines).toEqual([
			{ kind: "hunk", text: "@@ -1 +1 @@" },
			{ kind: "removed", text: "x" },
			{ kind: "added", text: "y" },
		]);
	});

	it("starts a new hunk when a hunk's counts run past the next @@ row", () => {
		const [file] = parseDiff(
			[
				"--- a/f.txt",
				"+++ b/f.txt",
				"@@ -1,5 +1,5 @@",
				" a",
				"@@ -9 +9 @@",
				"-x",
				"+y",
			].join("\n"),
		);
		expect(file?.lines).toEqual([
			{ kind: "hunk", text: "@@ -1,5 +1,5 @@" },
			{ kind: "context", text: "a" },
			{ kind: "hunk", text: "@@ -9 +9 @@" },
			{ kind: "removed", text: "x" },
			{ kind: "added", text: "y" },
		]);
	});

	it("counts a blank line inside a hunk as context", () => {
		const [file] = parseDiff(
			[
				"--- a/f.txt",
				"+++ b/f.txt",
				"@@ -1,3 +1,3 @@",
				" a",
				"",
				"-b",
				"\\ No newline at end of file",
				"+c",
			].join("\n"),
		);
		expect(file?.lines.slice(1)).toEqual([
			{ kind: "context", text: "a" },
			{ kind: "context", text: "" },
			{ kind: "removed", text: "b" },
			{ kind: "added", text: "c" },
		]);
	});

	it("reads a diff with CRLF line ends", () => {
		const files = parseDiff(
			"--- a/f.txt\r\n+++ b/f.txt\r\n@@ -1 +1 @@\r\n-x\r\n+y\r\n",
		);
		expect(files).toEqual([
			{
				path: "f.txt",
				lines: [
					{ kind: "hunk", text: "@@ -1 +1 @@" },
					{ kind: "removed", text: "x" },
					{ kind: "added", text: "y" },
				],
			},
		]);
	});

	it("names a rename with no other change and a binary file", () => {
		const files = parseDiff(
			[
				"diff --git a/old.txt b/new.txt",
				"similarity index 100%",
				"rename from old.txt",
				"rename to new.txt",
				"diff --git a/a.txt b/b.txt",
				"similarity index 90%",
				"rename from a.txt",
				"rename to b.txt",
				"--- a/a.txt",
				"+++ b/b.txt",
				"@@ -1 +1 @@",
				"-x",
				"+y",
				"diff --git a/logo.png b/logo.png",
				"index 1..2 100644",
				"Binary files a/logo.png and b/logo.png differ",
			].join("\n"),
		);
		expect(files.map((f) => [f.path, f.note])).toEqual([
			["new.txt", "renamed"],
			["b.txt", undefined],
			["logo.png", "binary"],
		]);
	});

	it("gives nothing for text that is not a diff", () => {
		expect(parseDiff("hello\nworld")).toEqual([]);
		expect(parseDiff("")).toEqual([]);
	});
});
