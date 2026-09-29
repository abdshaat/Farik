import mark from "@farik/brand/assets/logo-mark-1254.png";
import wordmark from "@farik/brand/assets/wordmark-1024.png";
import { Stepper } from "@farik/ui";
import type { ReactNode } from "react";
import { t } from "../../strings/t.ts";
import styles from "./setup.module.css";

const STEPS = [
	t("wizardComputer"),
	t("wizardAccount"),
	t("wizardProject"),
	t("wizardFound"),
	t("wizardTeam"),
	t("wizardMay"),
	t("wizardSpending"),
	t("wizardFinishing"),
];
const LOOP = [t("loopPlan"), t("loopBuild"), t("loopIterate"), t("loopShip")];
const NO_SANDBOX = "farik.noSandbox";

/** The first run's two panels: the brand on the dark side, the step's question beside it. */
export function Wizard(props: {
	step: number;
	title: string;
	lead: string;
	children: ReactNode;
}) {
	return (
		<div className={styles.wizard}>
			<aside className={styles.side}>
				<img className={styles.mark} src={mark} alt="" />
				<img className={styles.wordmark} src={wordmark} alt={t("brand")} />
				<p className={styles.tag}>{t("setupTag")}</p>
				<p className={styles.loop}>
					{LOOP.map((line) => (
						<span key={line}>
							<span className={styles.prompt} aria-hidden="true">
								&gt;{" "}
							</span>
							{line}
						</span>
					))}
				</p>
			</aside>
			<main className={styles.main}>
				<nav aria-label={t("setupSteps")}>
					<Stepper steps={STEPS} current={props.step} />
				</nav>
				<h1 className={styles.title}>{props.title}</h1>
				<p className={styles.lead}>{props.lead}</p>
				{props.children}
			</main>
		</div>
	);
}

// The choice to go on without Docker, kept for this tab until a project is taken on.
// Storage can refuse (a private window): the choice then reads as "with Docker".
export function keepNoSandbox(on: boolean): void {
	try {
		sessionStorage.setItem(NO_SANDBOX, String(on));
	} catch {}
}

export function noSandbox(): boolean {
	try {
		return sessionStorage.getItem(NO_SANDBOX) === "true";
	} catch {
		return false;
	}
}
