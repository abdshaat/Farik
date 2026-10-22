import { Button } from "@catervas/ui";
import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { useHub } from "../../app/connection.tsx";
import { t } from "../../strings/t.ts";
import { Wizard } from "../setup/Wizard.tsx";
import styles from "./find.module.css";
import { findingSaid, whenWorked } from "./finding.ts";
import { FindStepper } from "./Stepper.tsx";

type HintKind = "same_owner" | "same_days" | "similar_names" | "wrote_both";

/** A project of a pair, as `linking.pairs` gives it. */
type LinkProject = {
	key: string;
	name: string;
	lastAt: string | null;
	folder: string | null;
};

/** One line a proof was read from. */
type ProofEnd = {
	project: string;
	name: string;
	file: string;
	line: number;
	text: string;
};

/** A pair with a proof, to be asked about. */
type AskedPair = {
	a: LinkProject;
	b: LinkProject;
	proof: { sentence: string; ends: ProofEnd[] };
	also: string[];
	hints: HintKind[];
};

/** A pair with hints only. */
type MaybePair = { a: LinkProject; b: LinkProject; hints: HintKind[] };

/** Projects the owner's Yes answers made one. */
type Group = { projects: string[]; names: string[]; why: string[] };

const HINT_ALSO = {
	same_owner: "togetherHintAlsoOwner",
	same_days: "togetherHintAlsoDays",
	similar_names: "togetherHintAlsoNames",
	wrote_both: "togetherHintAlsoWrote",
} as const;

const HINT_WORDS = {
	same_owner: "togetherHintOwner",
	same_days: "togetherHintDays",
	similar_names: "togetherHintNames",
	wrote_both: "togetherHintWrote",
} as const;

/** "a", "a and b", "a, b and c". */
function listed(names: string[]): string {
	if (names.length < 2) return names.join("");
	return t("togetherList", {
		first: names.slice(0, -1).join(", "),
		last: names.at(-1) ?? "",
	});
}

/** A sentence about `name` as "it ...", the way a second line about the same project reads. */
function asIt(sentence: string, name: string): string {
	return sentence.startsWith(`${name} `)
		? `it ${sentence.slice(name.length + 1)}`
		: sentence;
}

/** One project of a pair: its name, and when it was last worked on and where it is. */
function Card(props: { project: LinkProject }) {
	const { project } = props;
	const meta = [
		project.lastAt ? whenWorked(project.lastAt) : "",
		project.folder ?? "",
	]
		.filter(Boolean)
		.join(" · ");
	return (
		<div className={styles.pcard}>
			<strong>{project.name}</strong>
			{meta && <span className={styles.pcardMeta}>{meta}</span>}
		</div>
	);
}

/** Where a proof was read: "tidepool-web/package.json, line 14:" and the line. */
function End(props: { end: ProofEnd }) {
	const { end } = props;
	return (
		<li>
			{end.file === "description"
				? t("togetherEndDescription", { name: end.name })
				: t("togetherEnd", {
						name: end.name,
						file: end.file,
						line: end.line,
					})}{" "}
			<code>{end.text}</code>
		</li>
	);
}

/** The question for one pair (journey A, step 4). */
function Question(props: {
	pair: AskedPair;
	busy: boolean;
	onAnswer: (answer: "yes" | "no") => void;
}) {
	const { pair } = props;
	const [open, setOpen] = useState(false);
	const also = [
		...pair.also.map((one) =>
			asIt(one, pair.proof.ends[0]?.name ?? pair.a.name),
		),
		...pair.hints.map((hint) => t(HINT_ALSO[hint])),
	];
	return (
		<>
			<div className={styles.pair}>
				<Card project={pair.a} />
				<span aria-hidden="true">↔</span>
				<Card project={pair.b} />
			</div>
			<p className={styles.proof}>
				<span>{pair.proof.sentence}</span>{" "}
				<button
					type="button"
					className={styles.more}
					aria-label={t("togetherMore")}
					aria-expanded={open}
					aria-controls="proof-lines"
					onClick={() => setOpen((on) => !on)}
				>
					ⓘ
				</button>
			</p>
			{open && (
				<div id="proof-lines" className={styles.panel}>
					<ul>
						{pair.proof.ends.map((end) => (
							<End key={`${end.project}${end.file}${end.line}`} end={end} />
						))}
						<li>{t("togetherChecked")}</li>
					</ul>
				</div>
			)}
			{also.map((line) => (
				<p key={line}>{t("togetherAlso", { what: line })}</p>
			))}
			<div className={styles.actions}>
				<Button
					kind="primary"
					disabled={props.busy}
					onClick={() => props.onAnswer("yes")}
				>
					{t("togetherYes")}
				</Button>
				<Button disabled={props.busy} onClick={() => props.onAnswer("no")}>
					{t("togetherNo")}
				</Button>
				<span className={styles.muted}>{t("togetherNotSure")}</span>
			</div>
		</>
	);
}

/** "Maybe related": pairs with hints only, Separate unless the owner opens one and says otherwise. */
function Maybe(props: {
	pairs: MaybePair[];
	onAnswer: (pair: MaybePair, answer: "yes" | "no") => void;
}) {
	const [open, setOpen] = useState(false);
	const [yes, setYes] = useState<Set<string>>(new Set());
	return (
		<section className={styles.section}>
			<Button expanded={open} onClick={() => setOpen((on) => !on)}>
				{t("togetherMaybe", { count: props.pairs.length })}
			</Button>
			{open &&
				props.pairs.map((pair) => {
					const id = `${pair.a.key}|${pair.b.key}`;
					const names = t("togetherPair", {
						a: pair.a.name,
						b: pair.b.name,
					});
					const choose = (answer: "yes" | "no") => {
						setYes((all) => {
							const next = new Set(all);
							if (answer === "yes") next.add(id);
							else next.delete(id);
							return next;
						});
						props.onAnswer(pair, answer);
					};
					return (
						<div key={id} className={styles.maybe}>
							<strong>{names}</strong>
							<span className={styles.muted}>
								{t("togetherNoProof", {
									hints: pair.hints
										.map((hint) => t(HINT_WORDS[hint]))
										.join(", "),
								})}
							</span>
							<div role="radiogroup" aria-label={names} className={styles.pick}>
								<label>
									<input
										type="radio"
										name={`maybe-${id}`}
										checked={!yes.has(id)}
										onChange={() => choose("no")}
									/>
									{t("togetherSeparate")}
								</label>
								<label>
									<input
										type="radio"
										name={`maybe-${id}`}
										checked={yes.has(id)}
										onChange={() => choose("yes")}
									/>
									{t("togetherOneTeam")}
								</label>
							</div>
						</div>
					);
				})}
		</section>
	);
}

/** "Do these work together?": one pair at a time with its proof, then what was made of the answers (journey A, step 4). */
export function Together() {
	const { client } = useHub();
	const navigate = useNavigate();
	const [pairs, setPairs] = useState<AskedPair[]>();
	const [done, setDone] = useState(0);
	const [busy, setBusy] = useState(false);
	const [groups, setGroups] = useState<Group[]>([]);
	const [maybe, setMaybe] = useState<MaybePair[]>([]);
	const [said, setSaid] = useState<string>();

	const ask = useCallback(async () => {
		try {
			const answer = (await client?.call("linking.pairs", {})) as
				| { pairs: AskedPair[] }
				| undefined;
			setPairs(answer?.pairs ?? []);
		} catch (e) {
			setPairs([]);
			setSaid(findingSaid(e, "togetherFailed"));
		}
	}, [client]);
	const settle = useCallback(async () => {
		try {
			const made = (await client?.call("linking.groups", {})) as
				| { groups: Group[] }
				| undefined;
			setGroups(made?.groups ?? []);
		} catch {}
	}, [client]);
	useEffect(() => {
		void ask();
	}, [ask]);
	const finished = pairs !== undefined && pairs.length === 0;
	useEffect(() => {
		if (!finished) return;
		void settle();
		client?.call("linking.maybe", {}).then(
			(answer) => setMaybe((answer as { maybe: MaybePair[] }).maybe),
			() => {},
		);
	}, [finished, settle, client]);

	const answer = async (pair: AskedPair, choice: "yes" | "no") => {
		setBusy(true);
		setSaid(undefined);
		try {
			await client?.call("linking.answer", {
				a: pair.a.key,
				b: pair.b.key,
				answer: choice,
			});
			setDone((n) => n + 1);
			await ask();
		} catch (e) {
			setSaid(findingSaid(e, "togetherFailed"));
		} finally {
			setBusy(false);
		}
	};
	const answerMaybe = async (pair: MaybePair, choice: "yes" | "no") => {
		setSaid(undefined);
		try {
			await client?.call("linking.answer", {
				a: pair.a.key,
				b: pair.b.key,
				answer: choice,
				...(choice === "yes" && { fromMaybe: true }),
			});
			await settle();
		} catch (e) {
			setSaid(findingSaid(e, "togetherFailed"));
		}
	};

	const first = pairs?.[0];
	return (
		<Wizard
			step={0}
			title={finished ? t("togetherDone") : t("togetherTitle")}
			lead={
				first && pairs
					? t("togetherQuestion", { n: done + 1, total: done + pairs.length })
					: ""
			}
			nav={<FindStepper current={1} />}
		>
			{first && (
				<Question
					key={`${first.a.key}|${first.b.key}`}
					pair={first}
					busy={busy}
					onAnswer={(choice) => answer(first, choice)}
				/>
			)}
			{finished && (
				<>
					<ul className={styles.legend}>
						{groups.map((group) => (
							<li key={group.projects.join("|")}>
								{t("togetherGroup", { names: listed(group.names) })}
							</li>
						))}
						<li>{t("togetherRest")}</li>
					</ul>
					{maybe.length > 0 && <Maybe pairs={maybe} onAnswer={answerMaybe} />}
				</>
			)}
			{said && <p role="alert">{said}</p>}
			{finished && (
				<div className={styles.foot}>
					<Button onClick={() => navigate("/find/found")}>{t("back")}</Button>
					{/* The proposal of teams (step 06e) comes next; until it lands, All teams. */}
					<Button kind="primary" onClick={() => navigate("/teams")}>
						{t("continue")}
					</Button>
				</div>
			)}
		</Wizard>
	);
}
