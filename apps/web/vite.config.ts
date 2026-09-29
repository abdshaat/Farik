import { defineConfig, type ProxyOptions } from "vite";

// The dev server stands in for the daemon's own origin (step 02): the daemon
// checks Origin and Host, so the proxy rewrites both to the daemon's.
const daemon = `http://127.0.0.1:${process.env.FARIK_PORT}`;
const toDaemon: ProxyOptions = {
	target: daemon,
	changeOrigin: true,
	headers: { Origin: daemon },
};

export default defineConfig({
	build: { outDir: "dist" },
	server: {
		proxy: {
			"/rpc": { ...toDaemon, ws: true },
			// GET /connect is the app's own page; only the code's POST goes on.
			"/connect": {
				...toDaemon,
				bypass: (req) => (req.method === "GET" ? req.url : undefined),
			},
			"/session": toDaemon,
			"/disconnect": toDaemon,
		},
	},
});
