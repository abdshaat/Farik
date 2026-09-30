// Farik's page check (docs/SPEC.md 5.4, step 12): opens one page of the preview at one width and in
// one theme, saves a screenshot, runs axe-core over it for the given tags, and prints the violations
// as one JSON line. Farik runs it with `node --input-type=module -` in the pinned Playwright MCP
// image, in the preview's network namespace, as the user; `AXE_SOURCE` is defined before this text.
// The agent never gets it.
import { createRequire } from "node:module";
import { parseArgs } from "node:util";

// A page that never settles still ends the check, and its container.
setTimeout(() => {
	process.stderr.write("the page check ran past its 90 seconds\n");
	process.exit(2);
}, 90_000).unref();

const text = { type: "string" };
const { values } = parseArgs({
	args: process.argv.slice(2),
	options: {
		url: text,
		width: text,
		theme: text,
		screenshot: text,
		"module-root": text,
		"proxy-server": text,
		"proxy-bypass": text,
		tags: text,
	},
});

const { chromium } = createRequire(`${values["module-root"]}/`)("playwright");
// The image ships the full Chromium the connector runs (`--browser chromium`), not the headless shell.
const browser = await chromium.launch({
	channel: "chromium",
	proxy: { server: values["proxy-server"], bypass: values["proxy-bypass"] },
});
try {
	const context = await browser.newContext({
		viewport: { width: Number(values.width), height: 800 },
		colorScheme: values.theme,
		bypassCSP: true,
	});
	const page = await context.newPage();
	const response = await page.goto(values.url, { waitUntil: "load", timeout: 30_000 });
	if (!response?.ok()) {
		throw new Error(`${values.url} answered ${response ? response.status() : "nothing"}`);
	}
	await page.screenshot({ path: values.screenshot });
	// axe runs in a world of its own, which the page's scripts cannot reach: a page that fakes
	// `window.axe` or the DOM's prototypes still has its violations found.
	const cdp = await context.newCDPSession(page);
	const { frameTree } = await cdp.send("Page.getFrameTree");
	const { executionContextId } = await cdp.send("Page.createIsolatedWorld", {
		frameId: frameTree.frame.id,
		worldName: "farik-axe",
	});
	const options = { runOnly: { type: "tag", values: values.tags.split(",") }, resultTypes: ["violations"] };
	const evaluated = await cdp.send("Runtime.evaluate", {
		expression: `${AXE_SOURCE};\naxe.run(document, ${JSON.stringify(options)})`,
		contextId: executionContextId,
		awaitPromise: true,
		returnByValue: true,
	});
	if (evaluated.exceptionDetails) {
		throw new Error(`axe failed: ${evaluated.exceptionDetails.exception?.description ?? evaluated.exceptionDetails.text}`);
	}
	const results = evaluated.result.value;
	const violations = results.violations.flatMap((rule) =>
		rule.nodes.map((node) => ({
			rule: rule.id,
			impact: rule.impact ?? "unknown",
			target: node.target.flat().join(" "),
			help: rule.help,
		})),
	);
	process.stdout.write(`${JSON.stringify({ violations })}\n`);
} finally {
	await browser.close();
}
