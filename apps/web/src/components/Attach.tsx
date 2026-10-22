import { Button, Dialog } from "@catervas/ui";
import { type ReactNode, useCallback, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useConnection } from "../app/connection.tsx";
import { Link } from "../app/links.tsx";
import { daemonSaid } from "../app/refusals.ts";
import { codeOf } from "../app/words.ts";
import { fileRoute } from "../pages/files.ts";
import { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";
import styles from "./Attach.module.css";

/** What a document is about, which picks the folder it goes to (SPEC 5.17). */
export type AttachKind = "product" | "architecture";

/** A document on a message: sent and kept in the team's folder, or still being sent. */
export type Attached =
	| { path: string; name: string }
	| { pending: string; name: string };

/** A message holds at most five documents. */
const MAX_DOCUMENTS = 5;

/** The files the setup's project screen has picked, sent once the team exists. */
export const pendingSetupFiles: { file: File; kind: AttachKind }[] = [];

const fileName = (path: string) => path.slice(path.lastIndexOf("/") + 1);

/** A file's bytes as base64, with no `data:` prefix. */
export function base64Of(file: File): Promise<string> {
	return new Promise((resolve, reject) => {
		const reader = new FileReader();
		reader.onerror = () => reject(reader.error);
		reader.onload = () =>
			resolve(String(reader.result).replace(/^data:[^,]*,/, ""));
		reader.readAsDataURL(file);
	});
}

/** A refusal in the code's words (`attach_<code>`), else the failure's own plain sentence. */
export function refusalWords(error: unknown): string {
	const key = `attach_${codeOf(error)}`;
	return key in en
		? t(key as keyof typeof en)
		: daemonSaid(error, "attach_unreadable");
}

/** The documents on a message being written: each is sent to the team's folder when it is picked. */
export function useAttachments() {
	const { client } = useConnection();
	const [items, setItems] = useState<Attached[]>([]);
	const [refusal, setRefusal] = useState<string>();
	const count = useRef(0);
	// A second pick before the first state update lands would otherwise pass the limit.
	const held = useRef(0);

	const add = useCallback(
		async (file: File, kind: AttachKind) => {
			setRefusal(undefined);
			if (!client) return;
			if (held.current >= MAX_DOCUMENTS) {
				setRefusal(t("attach_too_many"));
				return;
			}
			held.current += 1;
			count.current += 1;
			const pending = `pending-${count.current}`;
			setItems((now) => [...now, { pending, name: file.name }]);
			try {
				const dataBase64 = await base64Of(file);
				const done = (await client.call("document.attach", {
					name: file.name,
					kind,
					dataBase64,
				})) as { path: string };
				setItems((now) =>
					now.map((one) =>
						"pending" in one && one.pending === pending
							? { path: done.path, name: file.name }
							: one,
					),
				);
			} catch (error) {
				held.current -= 1;
				setItems((now) =>
					now.filter((one) => !("pending" in one && one.pending === pending)),
				);
				setRefusal(refusalWords(error));
			}
		},
		[client],
	);

	const remove = useCallback((key: string) => {
		setItems((now) => {
			const next = now.filter(
				(one) => ("path" in one ? one.path : one.pending) !== key,
			);
			held.current = next.length;
			return next;
		});
	}, []);

	const clear = useCallback(() => {
		held.current = 0;
		setItems([]);
		setRefusal(undefined);
	}, []);

	return {
		items,
		paths: items.flatMap((one) => ("path" in one ? [one.path] : [])),
		add,
		remove,
		busy: items.some((one) => "pending" in one),
		refusal,
		clear,
	};
}

/** Chips for a message's documents: the paths a live event holds, or the `{ path, name }` a query answers. */
export function chipsOf(
	attached: (string | { path: string; name?: string })[] | undefined,
): { path: string; name: string }[] | undefined {
	if (!attached?.length) return undefined;
	return attached.map((one) => {
		const path = typeof one === "string" ? one : one.path;
		const name = typeof one === "string" ? undefined : one.name;
		return { path, name: name ?? fileName(path) };
	});
}

const clip = (
	<svg
		viewBox="0 0 24 24"
		width="18"
		height="18"
		fill="none"
		stroke="currentColor"
		strokeWidth="2"
		strokeLinecap="round"
		strokeLinejoin="round"
		aria-hidden="true"
	>
		<path d="m21.4 11.6-8.5 8.5a5.5 5.5 0 0 1-7.8-7.8l8.5-8.5a3.7 3.7 0 0 1 5.2 5.2l-8.5 8.5a1.8 1.8 0 0 1-2.6-2.6l7.8-7.8" />
	</svg>
);

/** The paperclip: a file picker, then the question of what the file is about. */
export function AttachButton({
	onPick,
	disabled,
	full,
}: {
	onPick: (file: File, kind: AttachKind) => void;
	disabled?: boolean;
	full?: boolean;
}) {
	const input = useRef<HTMLInputElement>(null);
	const [picked, setPicked] = useState<File>();
	const [kind, setKind] = useState<AttachKind>("product");
	const close = () => setPicked(undefined);
	const options: { value: AttachKind; label: string }[] = [
		{ value: "product", label: t("attachKindProduct") },
		{ value: "architecture", label: t("attachKindArchitecture") },
	];
	let dialog: ReactNode = null;
	if (picked)
		dialog = createPortal(
			<Dialog
				open
				title={t("attachKindQuestion")}
				onClose={close}
				actions={
					<Button
						kind="primary"
						onClick={() => {
							onPick(picked, kind);
							close();
						}}
					>
						{t("attachSend")}
					</Button>
				}
			>
				<p className={styles.file}>{picked.name}</p>
				<div role="radiogroup" className={styles.kinds}>
					{options.map((o) => (
						<label key={o.value} className={styles.kind}>
							<input
								type="radio"
								name="attach-kind"
								value={o.value}
								checked={kind === o.value}
								onChange={() => setKind(o.value)}
							/>
							{o.label}
						</label>
					))}
				</div>
			</Dialog>,
			document.body,
		);
	return (
		<>
			<button
				type="button"
				className={full ? `${styles.clip} ${styles.full}` : styles.clip}
				disabled={disabled}
				aria-label={t("attachButton")}
				title={t("attachButton")}
				onClick={() => input.current?.click()}
			>
				{clip}
			</button>
			<input
				ref={input}
				type="file"
				hidden
				accept=".md,.markdown,.txt,.pdf,.docx"
				onChange={(e) => {
					const file = e.target.files?.[0];
					// Cleared, so that picking the same file again is a change.
					e.target.value = "";
					if (!file) return;
					setKind("product");
					setPicked(file);
				}}
			/>
			{dialog}
		</>
	);
}

/** The documents on a message: links to their pages once sent, or, with `onRemove`, chips that can be taken off. */
export function AttachedChips({
	items,
	onRemove,
}: {
	items: Attached[];
	onRemove?: (key: string) => void;
}) {
	if (items.length === 0) return null;
	return (
		<ul className={styles.chips} aria-label={t("attachedList")}>
			{items.map((one) => {
				if ("pending" in one)
					return (
						<li key={one.pending} className={styles.chip}>
							<span
								className={styles.spinner}
								role="status"
								aria-label={t("attachReading", { name: one.name })}
							/>
							<span>{one.name}</span>
						</li>
					);
				return (
					<li key={one.path} className={styles.chip}>
						{onRemove ? (
							<>
								<span>{one.name}</span>
								<button
									type="button"
									className={styles.remove}
									aria-label={t("attachRemove", { name: one.name })}
									onClick={() => onRemove(one.path)}
								>
									<span aria-hidden="true">x</span>
								</button>
							</>
						) : (
							<Link to={fileRoute(one.path)}>{one.name}</Link>
						)}
					</li>
				);
			})}
		</ul>
	);
}
