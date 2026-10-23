import mark from "@catervas/brand/assets/logo-mark-1254.png";
import wordmark from "@catervas/brand/assets/wordmark-1024.png";
import { Stepper } from "@catervas/ui";
import type { ReactNode } from "react";
import { useNavigate } from "react-router";
import { t } from "../../strings/t.ts";
import { StillLooking } from "../find/StillLooking.tsx";
import styles from "./setup.module.css";

const MAY = t("wizardMay");
const STEPS = [
	t("wizardComputer"),
	t("wizardAccount"),
	t("wizardProject"),
	t("wizardFound"),
	t("wizardTeam"),
	MAY,
	t("wizardSpending"),
	t("wizardFinishing"),
];
/** Each step's address, in `STEPS`' order. */
const PATHS = [
	"/setup/computer",
	"/setup/account",
	"/setup/project",
	"/setup/scan",
	"/setup/team",
	"/setup/permissions",
	"/setup/spending",
	"/setup/finish",
];
const LOOP = [t("loopPlan"), t("loopBuild"), t("loopIterate"), t("loopShip")];
const NO_SANDBOX = "catervas.noSandbox";

/** The first run's two panels: the brand on the dark side, the step's question beside it. */
export function Wizard(props: {
	step: number;
	title: string;
	/** The line under the title; none when empty. */
	lead: string;
	children: ReactNode;
	/** Beside the title in place of the setup's steps (the pages that find projects have their own), or none when false. */
	nav?: ReactNode | false;
	/** A saved team's permission answers carried over: "What they may do" is not a step. */
	carried?: boolean;
	/** Called before a step-list link leaves the screen, to keep what was typed. */
	onLeave?: () => void;
	/** Whether earlier steps are links (default); off while moving a team or opening a project. */
	links?: boolean;
}) {
	const navigate = useNavigate();
	const steps = props.carried ? STEPS.filter((s) => s !== MAY) : STEPS;
	const paths = props.carried
		? PATHS.filter((_, i) => STEPS[i] !== MAY)
		: PATHS;
	const current = props.carried && props.step > 5 ? props.step - 1 : props.step;
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
				{props.nav === undefined ? (
					<nav aria-label={t("setupSteps")}>
						<Stepper
							steps={steps}
							current={current}
							{...(props.links !== false && {
								onPick: (i: number) => {
									props.onLeave?.();
									navigate(paths[i] ?? "/");
								},
							})}
						/>
					</nav>
				) : (
					props.nav
				)}
				{props.nav === undefined && <StillLooking />}
				<h1 className={styles.title}>{props.title}</h1>
				{props.lead && <p className={styles.lead}>{props.lead}</p>}
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
