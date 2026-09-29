import { Navigate, Route, Routes } from "react-router";
import { Connect } from "../pages/Connect.tsx";
import { Events } from "../pages/Events.tsx";
import { NotFound } from "../pages/NotFound.tsx";
import { Settings } from "../pages/Settings.tsx";
import { Shell } from "../shell/Shell.tsx";
import { useConnection } from "./connection.tsx";
import { useTheme } from "./theme.ts";

export function App() {
	const { status } = useConnection();
	// The one theme state: Settings changes it, and it applies on every page.
	const [theme, setTheme] = useTheme();
	// Without a session, or while Farik is not answering, every path shows why.
	if (status === "no_session" || status === "lost") return <Connect />;
	return (
		<Routes>
			{/* With a live session, the connect page has nothing to say: a used link goes home. */}
			<Route
				path="/connect"
				element={status === "open" ? <Navigate to="/" replace /> : <Connect />}
			/>
			<Route element={<Shell />}>
				<Route path="/" element={<Navigate to="/events" replace />} />
				<Route path="/events" element={<Events />} />
				<Route
					path="/settings"
					element={<Settings theme={theme} onTheme={setTheme} />}
				/>
				<Route path="*" element={<NotFound />} />
			</Route>
		</Routes>
	);
}
