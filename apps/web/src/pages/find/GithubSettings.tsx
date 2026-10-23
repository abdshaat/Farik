import { Button } from "@catervas/ui";
import { useState } from "react";
import { useNavigate } from "react-router";
import { useHub } from "../../app/connection.tsx";
import { t } from "../../strings/t.ts";
import styles from "./find.module.css";
import { findingSaid, useHubCall } from "./finding.ts";

/** This computer's GitHub: whose it is and Disconnect, or Connect GitHub (SPEC 4.4; ADR 0059). */
export function GithubSettings() {
	const { client } = useHub();
	const navigate = useNavigate();
	const github = useHubCall<{ connected: boolean; login?: string }>(
		"github.status",
	);
	const [busy, setBusy] = useState(false);
	const [said, setSaid] = useState<string>();
	if (!github.data) return null;
	const disconnect = async () => {
		setBusy(true);
		setSaid(undefined);
		try {
			await client?.call("github.disconnect", {});
			github.again();
		} catch (e) {
			setSaid(findingSaid(e, "githubFailed"));
		} finally {
			setBusy(false);
		}
	};
	return (
		<section className={styles.card} aria-labelledby="computer-github">
			<h2 id="computer-github">{t("computerGithub")}</h2>
			{github.data.connected ? (
				<>
					<p>
						{github.data.login
							? t("computerGithubConnected", { login: github.data.login })
							: t("computerGithubConnectedNoName")}
					</p>
					<div>
						<Button busy={busy} onClick={disconnect}>
							{t("computerGithubDisconnect")}
						</Button>
					</div>
				</>
			) : (
				<>
					<p>{t("computerGithubNot")}</p>
					<div>
						<Button onClick={() => navigate("/find/github")}>
							{t("githubConnect")}
						</Button>
					</div>
				</>
			)}
			{said && <p role="alert">{said}</p>}
		</section>
	);
}
