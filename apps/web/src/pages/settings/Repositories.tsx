import { Button, Choice, Dialog, Table } from "@catervas/ui";
import { useState } from "react";
import { useLocation } from "react-router";
import { useConnection, useHub } from "../../app/connection.tsx";
import { useNavigate } from "../../app/links.tsx";
import { type ServeStatus, useQuery } from "../../app/store.ts";
import { t } from "../../strings/t.ts";
import { type FoundList, findingSaid, tilde } from "../find/finding.ts";
import styles from "../pages.module.css";
import { AddSecondRepository } from "./AddSecondRepository.tsx";

/** A member repository, as `repos.list` gives it. */
type Member = {
	name: string;
	path: string;
	integrationBranch: string | null;
	missing: boolean;
};

/** What becomes of a repository taken out of a team. */
type Then = "watch" | "own";

/**
 * Settings, Repositories: the repositories a project folder works across, each with the one
 * action that fixes a mistake, This doesn't belong here (journey F; ADR 0059).
 */
export function Repositories() {
	const team = useConnection();
	const hub = useHub();
	const navigate = useNavigate();
	const { data: named } = useQuery<{ team: { name: string } }>("team.get", {});
	const { data, again } = useQuery<{ repos: Member[] | null }>(
		"repos.list",
		{},
	);
	const { data: serve } = useQuery<ServeStatus>("serve.status", {});
	// A "Your teams" card that adds a project to this team sends the owner here with it chosen.
	const wanted = (
		useLocation().state as {
			addRepository?: { path: string; name?: string };
		} | null
	)?.addRepository;
	const [adding, setAdding] = useState(wanted !== undefined);
	// Where the team's notes are moving to, once the owner asked for the move.
	const [moving, setMoving] = useState<string>();
	const [leaving, setLeaving] = useState<Member>();
	const [then, setThen] = useState<Then>("watch");
	const [busy, setBusy] = useState(false);
	const [said, setSaid] = useState<string>();

	const takeOut = async (member: Member) => {
		setBusy(true);
		setSaid(undefined);
		try {
			await team.client?.call("repo.remove", { name: member.name });
			// The folders found, to name the repositories by the keys finding knows them by.
			const found = (await hub.client?.call("finding.list", {})) as
				| FoundList
				| undefined;
			const rows = [
				...(found?.main ?? []),
				...(found?.explored ?? []),
				...(found?.ignored ?? []),
			];
			const keyOf = (path: string) =>
				rows.find((row) => row.folders.includes(path))?.key;
			const key = keyOf(member.path);
			if (key !== undefined) {
				const others = (data?.repos ?? [])
					.filter((each) => each.name !== member.name)
					.map((each) => keyOf(each.path))
					.filter((each): each is string => each !== undefined);
				if (others.length > 0)
					await hub.client?.call("linking.reject", { key, others });
				await hub.client?.call("finding.choose", {
					key,
					choice: then === "watch" ? "watch" : "team",
				});
				if (then === "own") {
					await hub.client?.call("proposal.create", {
						confirmed: true,
						only: [key],
					});
					navigate("/teams", { state: { created: 1 } });
					return;
				}
			}
			setLeaving(undefined);
			again();
		} catch (e) {
			setSaid(findingSaid(e, "reposFailed"));
			again();
		} finally {
			setBusy(false);
		}
	};

	const members = data?.repos ?? undefined;
	if (moving !== undefined)
		return (
			<div className={styles.page}>
				<h2>{t("movingTitle")}</h2>
				<p>{t("movingBody", { folder: moving })}</p>
				<p role="status">{t("movingNotice")}</p>
			</div>
		);
	return (
		<div className={styles.page}>
			<h2>{t("sectionRepositories")}</h2>
			{serve?.takeOnError && (
				<p role="alert">{t("reposAddFailed", { said: serve.takeOnError })}</p>
			)}
			{data && members === undefined && (
				<>
					<p>{t("reposOneAt", { path: tilde(serve?.projectRoot ?? "") })}</p>
					<div>
						<Button onClick={() => setAdding(true)}>{t("reposAdd")}</Button>
					</div>
				</>
			)}
			{adding && serve?.projectRoot && (
				<AddSecondRepository
					root={serve.projectRoot}
					{...(wanted && { second: wanted })}
					onClose={() => setAdding(false)}
					onMoved={(folder) => {
						setAdding(false);
						setMoving(folder);
					}}
				/>
			)}
			{members && (
				<Table
					caption={t("sectionRepositories")}
					rows={members}
					getKey={(member) => member.name}
					columns={[
						{
							key: "name",
							header: t("reposName"),
							render: (member) => member.name,
						},
						{
							key: "folder",
							header: t("reposFolder"),
							render: (member) => (
								<>
									{tilde(member.path)}
									{member.missing && <> · {t("reposMissing")}</>}
								</>
							),
						},
						{
							key: "action",
							header: "",
							render: (member) =>
								members.length > 1 ? (
									<Button
										kind="quiet"
										onClick={() => {
											setThen("watch");
											setSaid(undefined);
											setLeaving(member);
										}}
									>
										{t("reposNotHere")}
									</Button>
								) : null,
						},
					]}
				/>
			)}
			{leaving && (
				<Dialog
					open
					title={t("reposTakeOut", {
						repo: leaving.name,
						team: named?.team.name ?? "",
					})}
					onClose={() => setLeaving(undefined)}
					actions={
						<>
							<Button onClick={() => setLeaving(undefined)}>
								{t("agentCancel")}
							</Button>
							<Button
								kind="primary"
								busy={busy}
								onClick={() => takeOut(leaving)}
							>
								{t("reposTakeItOut")}
							</Button>
						</>
					}
				>
					<p>{t("reposStops")}</p>
					<p>{t("reposNever")}</p>
					<Choice<Then>
						name="repo-then"
						legend={t("reposWhat")}
						value={then}
						onChange={setThen}
						options={[
							{
								value: "watch",
								label: t("reposWatch"),
								description: t("reposWatchNote"),
							},
							{
								value: "own",
								label: t("reposOwn"),
								description: t("reposOwnNote"),
							},
						]}
					/>
					{said && <p role="alert">{said}</p>}
				</Dialog>
			)}
		</div>
	);
}
