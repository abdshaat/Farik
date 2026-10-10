import type { Event } from "@catervas/protocol-client";
import { Table } from "@catervas/ui";
import { useEvents } from "../app/store.ts";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";

/** The raw event log, a testing aid: the last 100, newest first. */
export function Events() {
	const rows = useEvents().slice(-100).reverse();
	return (
		<div className={styles.page}>
			<h1 className={styles.title}>{t("events")}</h1>
			<Table<Event>
				caption={t("eventsCaption")}
				rows={rows}
				getKey={(e) => String(e.seq)}
				columns={[
					{
						key: "seq",
						header: t("eventSeq"),
						align: "end",
						render: (e) => e.seq,
					},
					{
						key: "time",
						header: t("eventTime"),
						// Local HH:MM:SS fits a phone; the full value is on hover.
						render: (e) => (
							<time dateTime={e.recordedAt} title={e.recordedAt}>
								{new Date(e.recordedAt).toTimeString().slice(0, 8)}
							</time>
						),
					},
					{ key: "kind", header: t("eventKind"), render: (e) => e.kind },
					{ key: "task", header: t("eventTask"), render: (e) => e.taskId },
				]}
			/>
		</div>
	);
}
