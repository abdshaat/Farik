import { Button, TextField } from "@catervas/ui";
import { useState } from "react";
import { useNavigate } from "react-router";
import { useHub } from "../../app/connection.tsx";
import { t } from "../../strings/t.ts";
import { Wizard } from "../setup/Wizard.tsx";
import styles from "./find.module.css";
import { findingSaid } from "./finding.ts";
import { FindStepper } from "./Stepper.tsx";

/** GitHub's page for making a fine-grained token. */
export const NEW_TOKEN =
	"https://github.com/settings/personal-access-tokens/new";

/** "Connect GitHub": a read-only token the owner pastes, until Catervas Cloud's single click (journey C, step 2 of 3). */
export function ConnectGithub() {
	const { client } = useHub();
	const navigate = useNavigate();
	const [token, setToken] = useState("");
	const [busy, setBusy] = useState(false);
	const [login, setLogin] = useState<string>();
	const [said, setSaid] = useState<string>();
	const connect = async () => {
		if (!client || token.trim() === "") return;
		setBusy(true);
		setSaid(undefined);
		try {
			const answer = (await client.call("github.connect", {
				token: token.trim(),
			})) as { login: string };
			setLogin(answer.login);
			// The token is not kept in the page once GitHub has taken it.
			setToken("");
		} catch (e) {
			setSaid(findingSaid(e, "githubFailed"));
		} finally {
			setBusy(false);
		}
	};
	return (
		<Wizard
			step={0}
			title={t("githubTitle")}
			lead={t("githubKicker")}
			nav={<FindStepper current={0} />}
		>
			<ul className={styles.legend}>
				<li>{t("githubFind")}</li>
				<li>{t("githubSees")}</li>
				<li>{t("githubChanges")}</li>
			</ul>
			{login === undefined && (
				<>
					<p>{t("githubSetup")}</p>
					<p>
						<a href={NEW_TOKEN} target="_blank" rel="noreferrer">
							{t("githubMakeToken")}
						</a>
					</p>
					<TextField
						id="github-token"
						label={t("githubTokenLabel")}
						type="password"
						autoComplete="off"
						value={token}
						onChange={setToken}
					/>
				</>
			)}
			{login !== undefined && (
				<p role="status">{t("githubConnectedAs", { login })}</p>
			)}
			{said && <p role="alert">{said}</p>}
			<div className={styles.foot}>
				{login === undefined ? (
					<>
						<Button onClick={() => navigate("/find/services")}>
							{t("githubSkip")}
						</Button>
						<Button kind="primary" busy={busy} onClick={connect}>
							{t("githubConnect")}
						</Button>
					</>
				) : (
					<>
						<span />
						<Button kind="primary" onClick={() => navigate("/find/services")}>
							{t("githubNext")}
						</Button>
					</>
				)}
			</div>
		</Wizard>
	);
}
