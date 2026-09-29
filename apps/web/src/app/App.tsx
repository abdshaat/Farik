import { Link, Navigate, Route, Routes } from "react-router";
import { t } from "../strings/t.ts";
import { useConnection } from "./connection.tsx";

// Bare pages until the shell step replaces them with the real ones.
function ConnectPage() {
	const { status, linkUsed } = useConnection();
	return (
		<main>
			{linkUsed && <p>{t("linkUsed")}</p>}
			{status === "no_session" && <h1>{t("noSessionTitle")}</h1>}
			{status === "lost" && <h1>{t("lostTitle")}</h1>}
		</main>
	);
}

function NoPage() {
	return (
		<main>
			<h1>{t("noPage")}</h1>
			<Link to="/">{t("noPageHome")}</Link>
		</main>
	);
}

export function App() {
	return (
		<Routes>
			<Route path="/" element={<Navigate to="/events" replace />} />
			<Route path="/connect" element={<ConnectPage />} />
			<Route path="/events" element={<h1>{t("events")}</h1>} />
			<Route path="/settings" element={<h1>{t("settings")}</h1>} />
			<Route path="*" element={<NoPage />} />
		</Routes>
	);
}
