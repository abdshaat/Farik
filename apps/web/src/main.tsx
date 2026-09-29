import "@farik/brand/tokens.css";
import "@farik/brand/fonts.css";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router";
import { App } from "./app/App.tsx";
import { ConnectionProvider } from "./app/connection.tsx";

const root = document.getElementById("root");
if (!root) throw new Error("index.html has no #root");
// No StrictMode: its doubled effect would post the link's one-time code twice.
createRoot(root).render(
	<ConnectionProvider>
		<BrowserRouter>
			<App />
		</BrowserRouter>
	</ConnectionProvider>,
);
