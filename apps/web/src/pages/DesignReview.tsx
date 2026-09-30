import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import gate from "./Gate.module.css";
import type { HistoryEvent } from "./Gate.tsx";
import own from "./PlanPage.module.css";
import styles from "./pages.module.css";

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
		| "passed"
		| "failed";
	reasons?: string;
	checks: Check[];
};

/** The Designer's letter and the four checks, as the gate's and the task page's mockups show them. */
export function DesignReview({
	taskId,
	review,
	events,
	designer,
	reviewer,
	builder,
}: {
	taskId: string;
	review: Review;
	events: HistoryEvent[];
	designer: string;
	reviewer: string;
	builder: string;
}) {
	const decided = review.state === "passed" || review.state === "failed";
	return (
		<>
			{decided && (
				<section className={own.letter} aria-labelledby="design-signed">
					<p id="design-signed" className={styles.muted}>
						{t("designChecked", { name: designer })}
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
