import { Avatar, type AvatarKey, Button, RoleTag } from "@catervas/ui";
import { Navigate, useNavigate } from "react-router";
import { useQuery } from "../../app/store.ts";
import type { en } from "../../strings/en.ts";
import { t } from "../../strings/t.ts";
import type { Catervafication } from "../CatervaficationCard.tsx";
import { useCommand } from "../dialogs/StartSprint.tsx";
import styles from "./setup.module.css";
import type { Team } from "./TeamSetup.tsx";
import { Wizard } from "./Wizard.tsx";

/** What each role that writes in a Catervafication says of itself, in the order the daemon lists them. */
const LINES: Record<string, keyof typeof en> = {
	product_manager: "readyPm",
	architect: "readyArchitect",
	software_developer: "readyDeveloper",
	ui_ux_designer: "readyDesigner",
	marketing_specialist: "readyMarketing",
};

/** "A", "A and B", "A, B and C". */
export function joinedNames(names: string[]): string {
	return names.length < 2
		? (names[0] ?? "")
		: `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/** The end of setup for a project that already exists: the offer to document it, Start or Later. */
export function SetupReady() {
	const navigate = useNavigate();
	const { data: offer, error } = useQuery<Catervafication>(
		"catervafication.get",
		{},
	);
	const { data: team } = useQuery<{ team: Team }>("team.get", {});
	const { busy, refusal, send } = useCommand(() =>
		navigate("/", { replace: true }),
	);
	// A new project, or one already started, has nothing to offer: it lands where it did.
	// An offer that cannot be read is no offer.
	if (error || (offer && !offer.offered)) return <Navigate to="/" replace />;
	if (!offer || !team) return null;
	const active = team.team.agents.filter((a) => a.status === "active");
	return (
		<Wizard
			step={8}
			title={t("readyTitle")}
			lead={t("readyLead", {
				names: joinedNames(active.map((a) => a.displayName)),
				team: team.team.name,
			})}
		>
			<section className={styles.offer} aria-labelledby="offer-title">
				<p className={styles.note}>{t("readyRecommended")}</p>
				<h2 id="offer-title" className={styles.heading}>
					{t("readyOfferTitle")}
				</h2>
				<p>{t("readyOfferLead")}</p>
				<ul className={styles.writers}>
					{offer.roles.flatMap((role) => {
						const who = active.find((a) => a.role === role);
						const line = LINES[role];
						if (!who || !line) return [];
						return (
							<li key={role}>
								{who.avatar && (
									<Avatar
										avatarKey={who.avatar as AvatarKey}
										name={who.displayName}
										size={48}
									/>
								)}
								<span>
									<strong>{who.displayName}</strong> <RoleTag role={who.role} />{" "}
									{t(line)}
								</span>
							</li>
						);
					})}
				</ul>
				<p className={styles.note}>{t("readyNote")}</p>
				<div className={styles.actions}>
					<Button
						kind="primary"
						busy={busy}
						onClick={() => send({ command: "catervafication_start", body: {} })}
					>
						{t("readyStart")}
					</Button>
					<Button onClick={() => navigate("/", { replace: true })}>
						{t("readyLater")}
					</Button>
					<span className={styles.note}>{t("readyLaterNote")}</span>
				</div>
				{refusal && (
					<p role="alert" className={styles.alert}>
						{refusal}
					</p>
				)}
			</section>
		</Wizard>
	);
}
