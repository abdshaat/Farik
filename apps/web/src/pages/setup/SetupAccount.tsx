import { Button } from "@catervas/ui";
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { useQuery } from "../../app/store.ts";
import { t } from "../../strings/t.ts";
import {
	type CheckAnswer,
	ClaudeSignIn,
	outcomeWords,
} from "../ClaudeSignIn.tsx";
import styles from "./setup.module.css";
import { Wizard } from "./Wizard.tsx";

type AccountStatus = {
	signedIn: boolean;
	kind: "subscription" | "console" | "environment" | "other" | null;
	environmentVariable?: string;
};

/** The words for what Claude Code is signed in with. */
function signedInWords(account: AccountStatus): string {
	switch (account.kind) {
		case "subscription":
			return t("signedInSubscription");
		case "console":
			return t("signedInConsole");
		case "environment":
			return t("signedInEnvironment", {
				variable: account.environmentVariable ?? "",
			});
		default:
			return t("signedInOther");
	}
}

/**
 * Setup's second step: the AI account, which Claude Code holds (ADR 0084). Signed in already, it
 * only checks the connection; otherwise it signs in through Claude Code.
 */
export function SetupAccount() {
	const { client, status, reopen } = useConnection();
	const navigate = useNavigate();
	const { data: account } = useQuery<AccountStatus>("account.status", {});
	const [checked, setChecked] = useState<CheckAnswer>();
	const [checking, setChecking] = useState(false);
	const [opening, setOpening] = useState(false);
	const asked = useRef(false);

	// The outcome of a check that worked: a waiting project is taken on, else the next step.
	const worked = (answer: CheckAnswer) => {
		if (answer.takingOn) {
			reopen();
			setOpening(true);
			return;
		}
		navigate("/setup/project");
	};

	const check = async () => {
		if (!client) return;
		setChecking(true);
		try {
			const answer = (await client.call("account.check", {})) as CheckAnswer;
			setChecked(answer);
			if (answer.takingOn) worked(answer);
		} catch {
			setChecked({ outcome: "failed", detail: "" });
		}
		setChecking(false);
	};

	// Signed in already: the connection is checked once, with no press.
	const signedIn = account?.signedIn === true;
	// biome-ignore lint/correctness/useExhaustiveDependencies: asked once, when the account is known
	useEffect(() => {
		if (!signedIn || asked.current) return;
		asked.current = true;
		void check();
	}, [signedIn, client]);

	// Once the page is connected to the restarted Catervas, it goes home.
	useEffect(() => {
		if (opening && status === "open") navigate("/", { replace: true });
	}, [opening, status, navigate]);

	if (opening)
		return (
			<Wizard step={1} title={t("accountTitle")} lead={t("accountLead")}>
				<p role="status">{t("opening")}</p>
			</Wizard>
		);

	const works = checked?.outcome === "working";
	return (
		<Wizard step={1} title={t("accountTitle")} lead={t("accountLead")}>
			<div className={styles.card}>
				{account && signedIn && <p>{signedInWords(account)}</p>}
				{signedIn && checking && <p role="status">{t("signInChecking")}</p>}
				{signedIn && checked && <p role="status">{outcomeWords(checked)}</p>}
				{account && (!signedIn || (checked && !works)) && (
					<ClaudeSignIn onWorking={worked} />
				)}
			</div>
			<div className={styles.foot}>
				<Button onClick={() => navigate("/setup/computer")}>{t("back")}</Button>
				<Button
					kind="primary"
					disabled={!works}
					onClick={() => navigate("/setup/project")}
				>
					{t("continue")}
				</Button>
			</div>
		</Wizard>
	);
}
