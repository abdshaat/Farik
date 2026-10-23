import { Button, TextField } from "@catervas/ui";
import { useState } from "react";
import { useNavigate } from "react-router";
import { useHub } from "../../app/connection.tsx";
import type { en } from "../../strings/en.ts";
import { t } from "../../strings/t.ts";
import { Wizard } from "../setup/Wizard.tsx";
import styles from "./find.module.css";
import { findingSaid, useCatalog } from "./finding.ts";
import { FindStepper } from "./Stepper.tsx";

const KINDS: Record<string, keyof typeof en> = {
	bookkeeping: "serviceKindBookkeeping",
	spreadsheets: "serviceKindSpreadsheets",
	payments: "serviceKindPayments",
	selling: "serviceKindSelling",
	marketing: "serviceKindMarketing",
	code: "serviceKindCode",
	other: "serviceKindOther",
};

/** "Which of these do you use with Claude?": claude.ai's connectors can't be listed, so the owner taps them. */
export function Services() {
	const { client } = useHub();
	const navigate = useNavigate();
	const services = useCatalog();
	const [tapped, setTapped] = useState<string[]>([]);
	const [else_, setElse] = useState(false);
	const [other, setOther] = useState("");
	const [busy, setBusy] = useState(false);
	const [said, setSaid] = useState<string>();
	const toggle = (id: string) =>
		setTapped((all) =>
			all.includes(id) ? all.filter((one) => one !== id) : [...all, id],
		);
	const next = async () => {
		if (!client) return;
		setBusy(true);
		setSaid(undefined);
		try {
			await client.call("finding.declare_services", {
				// In the catalog's order, whatever order they were tapped in.
				ids: (services ?? [])
					.filter((s) => tapped.includes(s.id))
					.map((s) => s.id),
				...(else_ && other.trim() !== "" && { other: other.trim() }),
			});
			navigate("/find/found");
		} catch (e) {
			setSaid(findingSaid(e, "servicesFailed"));
		} finally {
			setBusy(false);
		}
	};
	return (
		<Wizard
			step={0}
			title={t("servicesTitle")}
			lead={t("servicesKicker")}
			nav={<FindStepper current={0} />}
		>
			<p>{t("servicesLead")}</p>
			<ul className={styles.tiles}>
				{services?.map((service) => (
					<li key={service.id}>
						<button
							type="button"
							className={styles.tile}
							aria-pressed={tapped.includes(service.id)}
							onClick={() => toggle(service.id)}
						>
							<strong>{service.name}</strong>
							<span>{t(KINDS[service.kind] ?? "serviceKindOther")}</span>
						</button>
					</li>
				))}
				<li>
					<button
						type="button"
						className={styles.tile}
						aria-pressed={else_}
						onClick={() => setElse((on) => !on)}
					>
						<strong>{t("servicesSomethingElse")}</strong>
						<span>{t("servicesTypeName")}</span>
					</button>
				</li>
			</ul>
			{else_ && (
				<TextField
					id="service-other"
					label={t("servicesOtherName")}
					value={other}
					onChange={setOther}
				/>
			)}
			{said && <p role="alert">{said}</p>}
			<div className={styles.foot}>
				<span />
				<Button kind="primary" busy={busy} onClick={next}>
					{t("continue")}
				</Button>
			</div>
		</Wizard>
	);
}
