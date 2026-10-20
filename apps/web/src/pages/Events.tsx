import type { Event } from "@catervas/protocol-client";
import { Table } from "@catervas/ui";
import { useEvents, useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";

const SHOWN = 100;

/**
 * The raw event log, a testing aid: the last 100, newest first. The log is read for them, since
 * the page's subscription starts from now; events streamed after are added at the top.
 */
export function Events() {
	const streamed = useEvents();
	const { data } = useQuery<{ events: Event[] }>("events.since", {
		newest_first: true,
		limit: SHOWN,
	});
	const bySeq = new Map<number, Event>();
	for (const e of data?.events ?? []) bySeq.set(e.seq, e);
	for (const e of streamed) bySeq.set(e.seq, e);
	const rows = [...bySeq.values()]
		.sort((a, b) => b.seq - a.seq)
		.slice(0, SHOWN);
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
