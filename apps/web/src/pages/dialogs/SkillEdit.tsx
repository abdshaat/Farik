import { Button, Choice, Dialog, TextArea, TextField } from "@farik/ui";
import { useRef, useState } from "react";
import { useConnection } from "../../app/connection.tsx";
import { said } from "../../app/refusals.ts";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import { kb, type SkillAt, type SkillFolder, sendSkill } from "../skills.ts";
import { FileFrame, inOrder } from "./FileFrame.tsx";

/** The skill being edited, as its folder read. */
export type Editing = SkillAt & { folder: SkillFolder };

/** What `SKILL.md` says in the three fields this dialog writes, or nothing when it says more. */
function fieldsOf(file: string) {
	const found =
		/^---\nname: ([^\n]*)\ndescription: ("(?:[^"\\\n]|\\.)*")\n---\n([\s\S]*)$/.exec(
			file,
		);
	if (!found) return undefined;
	try {
		return {
			name: found[1] ?? "",
			description: JSON.parse(found[2] ?? "") as string,
			body: found[3] ?? "",
		};
	} catch {
		return undefined;
	}
}

/** The frontmatter keys of `file`, other than `name` and `description`, which Farik drops. */
function ignoredIn(file: string): string[] {
	const head = /^---\n([\s\S]*?)\n---(\n|$)/.exec(file)?.[1] ?? "";
	return [...head.matchAll(/^([A-Za-z][\w-]*):/gm)]
		.map((m) => m[1] ?? "")
		.filter((key) => key !== "name" && key !== "description");
}

/** Which field a refusal belongs under. */
const FIELD: Record<string, "name" | "description" | "instructions"> = {
	skill_name_invalid: "name",
	skill_name_mismatch: "name",
	skill_name_taken: "name",
	skill_description_invalid: "description",
	skill_runs_commands: "instructions",
	skill_attaches_files: "instructions",
	skill_too_large: "instructions",
	skill_file_not_text: "instructions",
};

/**
 * "Add a skill" and "Edit <skill>" (spec 6.7): who it is for, a name, when to use it and the
 * instructions, or a whole SKILL.md; then the whole text to read before anything is added.
 */
export function SkillEdit({
	agent,
	name: who,
	role,
	shipped,
	editing,
	onClose,
}: {
	agent: string;
	/** The agent's name. */
	name: string;
	/** The role's name, in words. */
	role: string;
	/** The names of the skills the role comes with. */
	shipped: string[];
	editing?: Editing | undefined;
	/** `changed` once a skill was saved. */
	onClose: (changed: boolean) => void;
}) {
	const { client } = useConnection();
	const original = editing?.folder.files["SKILL.md"] ?? "";
	const parsed = editing?.folder.ignoredFields.length
		? undefined
		: fieldsOf(original);
	// A skill with more in its file than three fields opens as its file, so nothing unseen is dropped.
	const [asFile, setAsFile] = useState(editing !== undefined && !parsed);
	const [level, setLevel] = useState<"team" | "agent">(
		editing?.level ?? "agent",
	);
	const [skill, setSkill] = useState(parsed?.name ?? "");
	const [description, setDescription] = useState(parsed?.description ?? "");
	const [body, setBody] = useState(parsed?.body ?? "");
	const [text, setText] = useState(original);
	const [reading, setReading] = useState(false);
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	// A name another role ships is learned from the daemon's refusal; the person then chooses.
	const [taken, setTaken] = useState<string>();
	const upload = useRef<HTMLInputElement>(null);

	const file = asFile
		? text
		: `---\nname: ${skill.trim()}\ndescription: ${JSON.stringify(description.trim())}\n---\n${body}`;
	const named = asFile
		? (/^name: *([^\n]*)$/m.exec(text.split("\n---")[0] ?? "")?.[1] ?? "")
				.trim()
				.replace(/^(["'])(.*)\1$/, "$2")
		: skill.trim();
	const others = Object.entries(editing?.folder.files ?? {}).filter(
		([path]) => path !== "SKILL.md",
	);
	const ours = shipped.includes(named);
	const replaces = ours || taken === named;
	const whom = level === "agent" ? who : t("skillWholeTeam");
	const ready = asFile
		? text.trim() !== ""
		: named && description.trim() && body.trim();
	const ignored = ignoredIn(file);
	const field = refusal ? FIELD[refusal] : undefined;
	const refused =
		refusal === undefined ? undefined : said(refusal, {}, "skillOtherRefusal");
	/** The refusal as an `error` prop for the field it belongs under. */
	const err = (at: string, whole = false) =>
		refused && (whole ? field : field === at) ? { error: refused } : {};
	const renamed =
		editing && ((named && named !== editing.name) || level !== editing.level);

	const add = async () => {
		if (!client) return;
		setBusy(true);
		setRefusal(undefined);
		const files = { "SKILL.md": file, ...Object.fromEntries(others) };
		const code = await sendSkill(client, {
			command: "skill_save",
			body: {
				level,
				...(level === "agent" && { agent }),
				files,
				...(replaces && { replaceShipped: true }),
			},
		});
		setBusy(false);
		if (code === undefined) return onClose(true);
		if (code === "skill_name_taken") return setTaken(named);
		setRefusal(code);
		setReading(false);
	};
	const picked = async (f: File | undefined) => {
		if (!f) return;
		setText(await f.text());
		setAsFile(true);
	};

	return (
		<Dialog
			open
			fillsPhone
			title={
				editing
					? t("skillEditTitle", { skill: editing.name })
					: t("skillAddTitle", { name: who })
			}
			onClose={() => onClose(false)}
			actions={
				reading ? (
					<>
						<Button onClick={() => setReading(false)}>{t("skillBack")}</Button>
						<Button kind="primary" busy={busy} onClick={add}>
							{t(replaces ? "skillReplaceNow" : "skillAddNow")}
						</Button>
					</>
				) : (
					<>
						<Button onClick={() => onClose(false)}>{t("skillCancel")}</Button>
						<Button
							kind="primary"
							disabled={!ready}
							onClick={() => {
								setRefusal(undefined);
								setReading(true);
							}}
						>
							{t("skillNext")}
						</Button>
					</>
				)
			}
		>
			{reading ? (
				<div className={styles.toolApproval}>
					<p>{t("skillRead")}</p>
					{inOrder({ "SKILL.md": file, ...Object.fromEntries(others) }).map(
						([path, text]) => (
							<FileFrame key={path} path={path} text={text} />
						),
					)}
					{ignored.length > 0 && (
						<p className={styles.muted}>
							{t("skillIgnores", { fields: ignored.join(", ") })}
						</p>
					)}
					{replaces && (
						<p>
							<strong>
								{t(ours ? "skillReplaces" : "skillReplacesFarik", {
									role,
									skill: named,
									whom,
								})}
							</strong>
						</p>
					)}
				</div>
			) : (
				<div className={styles.toolApproval}>
					<Choice<"team" | "agent">
						name="skill-level"
						legend={t("skillFor")}
						value={level}
						onChange={setLevel}
						options={[
							{
								value: "agent",
								label: t("skillForOwn", { name: who }),
								description: t("skillForOwnNote", { name: who }),
							},
							{
								value: "team",
								label: t("skillForTeam"),
								description: t("skillForTeamNote"),
							},
						]}
					/>
					{asFile ? (
						<div className={styles.code}>
							<TextArea
								id="skill-file"
								label={t("skillWholeFile")}
								rows={16}
								value={text}
								onChange={setText}
								{...err("instructions", true)}
							/>
						</div>
					) : (
						<>
							<TextField
								id="skill-name"
								label={t("skillName")}
								hint={t("skillNameHint")}
								value={skill}
								onChange={setSkill}
								{...err("name")}
							/>
							<TextArea
								id="skill-when"
								label={t("skillWhen", { name: who })}
								hint={t("skillWhenHint")}
								rows={3}
								value={description}
								onChange={setDescription}
								{...err("description")}
							/>
							<div className={styles.code}>
								<TextArea
									id="skill-instructions"
									label={t("skillInstructions")}
									rows={12}
									value={body}
									onChange={setBody}
									{...err("instructions")}
								/>
							</div>
						</>
					)}
					{renamed && <p>{t("skillRenamed", { old: editing?.name ?? "" })}</p>}
					<input
						ref={upload}
						type="file"
						accept=".md,text/markdown,text/plain"
						hidden
						onChange={(e) => picked(e.target.files?.[0])}
					/>
					{!asFile && (
						<span>
							<Button kind="quiet" onClick={() => upload.current?.click()}>
								{t("skillUpload")}
							</Button>
						</span>
					)}
					{others.length > 0 && (
						<p className={styles.muted}>
							{t("skillOtherFiles", {
								files: others
									.map(([path, text]) => `${path}, ${kb(text)} KB`)
									.join("; "),
							})}
						</p>
					)}
					{refused && !field && (
						<p role="alert" className={styles.alert}>
							{refused}
						</p>
					)}
				</div>
			)}
		</Dialog>
	);
}
