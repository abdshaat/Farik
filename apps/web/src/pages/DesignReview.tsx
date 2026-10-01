import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import gate from "./Gate.module.css";
import type { HistoryEvent } from "./Gate.tsx";
import own from "./PlanPage.module.css";
import { day } from "./PlanPage.tsx";
import styles from "./pages.module.css";
import type { Agent } from "./setup/TeamSetup.tsx";

type Violation = { rule: string; impact: string; target: string; help: string };
type Check = {
	width: "phone" | "desktop";
	theme: "light" | "dark";
	violations: Violation[];
};
/** `task.get`'s design review (step 12): where it stands, and the latest record's words and checks. */
export type Review = {
	state:
		| "not_needed"
		| "waiting"
		| "waiting_on_designer"
		| "preview_missing"
		| "designer_needs_sandbox"
		| "designer_needs_browser"
		| "passed"
		| "failed";
	reasons?: string;
	checks: Check[];
};

/** One of `task.get`'s `design_reviews`: who recorded it, whether it passed, why, and when. */
export type Recorded = {
	agentId: string | null;
	pass: boolean;
	reasons: string;
	recordedAt: string;
};

/** The name of the agent `id`, or `fallback` for one the team does not have. */
export const nameIn = (agents: Agent[], id: string | null, fallback: string) =>
	agents.find((a) => a.id === id)?.displayName ?? fallback;

/** The Designer's letter and the four checks, as the gate's and the task page's mockups show them. */
export function DesignReview({
	taskId,
	review,
	reviews = [],
	agents,
	events,
	designer,
	reviewer,
	builder,
}: {
	taskId: string;
	review: Review;
	/** Every design review of the task, oldest first. */
	reviews?: Recorded[];
	agents: Agent[];
	events: HistoryEvent[];
	/** The team's Designer, for a review no record names. */
	designer: string;
	reviewer: string;
	builder: string;
}) {
	const decided = review.state === "passed" || review.state === "failed";
	// The latest record is the decided review; the Designer's send-backs are those before it.
	const signer = decided
		? nameIn(agents, reviews.at(-1)?.agentId ?? null, designer)
		: designer;
	const backs = (decided ? reviews.slice(0, -1) : reviews).filter(
		(r) => !r.pass,
	);
	return (
		<>
			{decided && (
				<section className={own.letter} aria-labelledby="design-signed">
					<p id="design-signed" className={styles.muted}>
						{t("designChecked", { name: signer })}
					</p>
					{review.reasons?.split(/\n\s*\n/).map((part) => (
						<p key={part}>{part}</p>
					))}
					<p>
						<strong>
							{review.state === "passed"
								? t("designPassed", { reviewer })
								: t("designFailed", { builder })}
						</strong>
					</p>
				</section>
			)}
			{review.checks.length > 0 && (
				<section className={styles.section} aria-labelledby="design-checks">
					<h2 id="design-checks">{t("designChecksTitle")}</h2>
					<p className={styles.muted}>{t("designChecksLead")}</p>
					<div className={gate.shots}>
						{review.checks.map((check) => (
							<Shot
								key={`${check.width}-${check.theme}`}
								taskId={taskId}
								check={check}
								events={events}
							/>
						))}
					</div>
				</section>
			)}
			{backs.length > 0 && (
				<details className={gate.backs}>
					<summary>
						{t(backs.length === 1 ? "designBackOnce" : "designBackMany", {
							name: [
								...new Set(
									backs.map((r) => nameIn(agents, r.agentId, designer)),
								),
							].join(" and "),
							n: backs.length,
							day: day(backs.at(-1)?.recordedAt),
						})}
					</summary>
					{backs.map((r) => (
						<p key={r.recordedAt}>{r.reasons}</p>
					))}
					{review.state === "passed" && builder && (
						<p>{t("designBackThen", { builder })}</p>
					)}
				</details>
			)}
		</>
	);
}

/** One check: its screenshot, where and how it was taken, and each problem axe found. */
function Shot({
	taskId,
	check,
	events,
}: {
	taskId: string;
	check: Check;
	events: HistoryEvent[];
}) {
	// The picture is the latest the task's own checks took at this width and theme.
	const file = (
		events.findLast((e) => {
			const body = e.body as { width?: string; theme?: string };
			return (
				e.kind === "page.checked" &&
				body.width === check.width &&
				body.theme === check.theme
			);
		})?.body as { screenshot?: string } | undefined
	)?.screenshot;
	const { data } = useQuery<{ pngBase64: string }>(
		"task.screenshot",
		{ task_id: taskId, file },
		!file,
	);
	const fill = {
		width: t(check.width === "phone" ? "shotPhone" : "shotComputer"),
		theme: t(check.theme === "light" ? "shotLight" : "shotDark"),
	};
	return (
		<figure className={gate.shot}>
			{data && (
				<img
					className={check.width === "phone" ? gate.phone : undefined}
					src={`data:image/png;base64,${data.pngBase64}`}
					alt={t("shotAlt", fill)}
				/>
			)}
			<figcaption>
				<strong>{t("shotCaption", fill)}</strong>
				<span className={styles.muted}>
					{t(check.width === "phone" ? "shotPhoneWide" : "shotComputerWide")}
				</span>
				{check.violations.length === 0 ? (
					<span>{t("shotClean")}</span>
				) : (
					<ul>
						{check.violations.map((v) => (
							<li key={`${v.rule}-${v.target}`}>
								<span>{v.help}</span>
								<span className={styles.muted}>
									{t("shotViolation", { rule: v.rule, impact: v.impact })}
								</span>
							</li>
						))}
					</ul>
				)}
			</figcaption>
		</figure>
	);
}
