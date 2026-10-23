import { Button, InfoTip, StatusWord } from "@catervas/ui";
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { daemonSaid } from "../../app/refusals.ts";
import { useQuery } from "../../app/store.ts";
import type { en } from "../../strings/en.ts";
import { t } from "../../strings/t.ts";
import { FindOffer } from "../find/FindOffer.tsx";
import { useFindingStatus, useOffer } from "../find/finding.ts";
import styles from "./setup.module.css";
import { keepNoSandbox, Wizard } from "./Wizard.tsx";

type Item = {
	state: "ready" | "missing" | "too_old" | "not_running" | "preparing";
	version?: string;
	/** The build step reached, and the steps in all, while preparing. */
	done?: number;
	of?: number;
	/** Docker's last words, when the last build failed. */
	failed?: string;
};
type Check = {
	claude: Item;
	git: Item;
	docker: Item;
	sandboxImage: Item;
	/** Only for a team with a UI/UX Designer. */
	designerBrowser?: Item;
	/** A project that stopped on the safe box is being taken on: Catervas restarts on it. */
	takingOn?: boolean;
};
type Key = keyof typeof en;

const WORD: Record<Item["state"], Key> = {
	ready: "ready",
	missing: "notFound",
	too_old: "tooOld",
	not_running: "notRunning",
	preparing: "preparingWord",
};

function Row(props: {
	what: Key;
	item: Item | undefined;
	fix: Partial<Record<Item["state"], Key>>;
	children?: React.ReactNode;
}) {
	const { item } = props;
	const fix = item && props.fix[item.state];
	return (
		<li className={styles.row}>
			<span>{t(props.what)}</span>
			{item ? (
				<StatusWord
					tone={
						item.state === "ready"
							? "done"
							: item.state === "preparing"
								? "working"
								: "waiting"
					}
					pill={item.state === "ready"}
				>
					{t(WORD[item.state])}
				</StatusWord>
			) : (
				<span>{t("checking")}</span>
			)}
			{item && (
				<div className={styles.how}>
					{item.version && (
						<p>{t("versionFound").replace("{version}", item.version)}</p>
					)}
					{fix && <p>{t(fix)}</p>}
					{props.children}
				</div>
			)}
		</li>
	);
}

/** Setup's first step: the programs Catervas needs, and what to do about each one missing. */
export function SetupComputer() {
	const { client, status, reopen } = useConnection();
	const navigate = useNavigate();
	const { data, again } = useQuery<Check>("computer.check", {});
	const [opening, setOpening] = useState(false);
	const takingOn = !!data?.takingOn;
	const reopenOnce = useRef(reopen);
	reopenOnce.current = reopen;
	useEffect(() => {
		if (!takingOn) return;
		reopenOnce.current();
		setOpening(true);
	}, [takingOn]);
	// Once the page is connected to the restarted Catervas, it goes home.
	useEffect(() => {
		if (opening && status === "open") navigate("/", { replace: true });
	}, [opening, status, navigate]);
	const [building, setBuilding] = useState(false);
	const askAgain = useRef(again);
	askAgain.current = again;
	const [buildError, setBuildError] = useState<string>();
	const [fetching, setFetching] = useState(false);
	const [fetchError, setFetchError] = useState<string>();

	// While the safe box is being prepared the page asks again by itself (ADR 0070).
	const preparing = data?.sandboxImage.state === "preparing";
	// biome-ignore lint/correctness/useExhaustiveDependencies: each answer is a new `data`, and the next ask is timed from it
	useEffect(() => {
		if (!preparing) return;
		const timer = setTimeout(() => askAgain.current(), 1000);
		return () => clearTimeout(timer);
	}, [data, preparing]);

	// Claude Code history on this computer is offered once: not while a scan has run or is running.
	const offer = useOffer();
	const { status: finding } = useFindingStatus();
	const [skipped, setSkipped] = useState(false);
	const offered =
		offer === "claude_code" &&
		finding?.phase === "idle" &&
		!finding.indexExists &&
		!skipped;

	const basics = data?.claude.state === "ready" && data.git.state === "ready";
	const docker = data?.docker.state === "ready";
	const boxed = docker && data?.sandboxImage.state === "ready";
	const onward = (without: boolean) => {
		keepNoSandbox(without);
		// Without Claude Code history, the owner says where Claude is used first (journey C).
		navigate(offer === "none" ? "/setup/where" : "/setup/account");
	};
	const prepare = async () => {
		if (!client) return;
		setBuilding(true);
		setBuildError(undefined);
		try {
			await client.call("sandbox.build", {});
		} catch (e) {
			setBuildError(daemonSaid(e, "setupBuildFailed"));
		} finally {
			setBuilding(false);
			again();
		}
	};

	const fetchBrowser = async () => {
		if (!client) return;
		setFetching(true);
		setFetchError(undefined);
		try {
			await client.call("browser.pull", {});
		} catch (e) {
			setFetchError(daemonSaid(e, "setupPullFailed"));
		} finally {
			setFetching(false);
			again();
		}
	};

	if (opening)
		return (
			<Wizard step={0} title={t("computerTitle")} lead={t("computerLead")}>
				<p role="status">{t("opening")}</p>
			</Wizard>
		);

	return (
		<Wizard step={0} title={t("computerTitle")} lead={t("computerLead")}>
			<ul className={styles.rows}>
				<Row
					what="computerClaude"
					item={data?.claude}
					fix={{ missing: "claudeMissing", too_old: "claudeTooOld" }}
				/>
				<Row
					what="computerGit"
					item={data?.git}
					fix={{ missing: "gitMissing" }}
				/>
				<Row
					what="computerDocker"
					item={data?.docker}
					fix={{ missing: "dockerMissing", not_running: "dockerNotRunning" }}
				>
					{data && !docker && <p>{t("dockerOrWithout")}</p>}
				</Row>
				{docker && (
					<Row
						what="computerImage"
						item={data?.sandboxImage}
						fix={{ missing: "imageMissing" }}
					>
						{preparing && data && (
							<>
								<progress
									className={styles.progress}
									aria-label={t("imagePreparingLabel")}
									{...(data.sandboxImage.of
										? {
												value: data.sandboxImage.done ?? 0,
												max: data.sandboxImage.of,
											}
										: {})}
								/>
								<p>
									{data.sandboxImage.of
										? t("imageStep", {
												done: String(data.sandboxImage.done ?? 0),
												of: String(data.sandboxImage.of),
											})
										: t("imagePreparing")}
								</p>
							</>
						)}
						{data && !preparing && data.sandboxImage.state !== "ready" && (
							<Button busy={building} onClick={prepare}>
								{t("prepare")}
							</Button>
						)}
						{buildError && <p role="alert">{buildError}</p>}
						{!buildError && data?.sandboxImage.failed && (
							<p role="alert">
								{t("setupBuildFailed")}{" "}
								<InfoTip id="image-failed">{data.sandboxImage.failed}</InfoTip>
							</p>
						)}
					</Row>
				)}
				{docker && data?.designerBrowser && (
					<Row
						what="computerBrowser"
						item={data.designerBrowser}
						fix={{ missing: "browserMissing" }}
					>
						{data.designerBrowser.state !== "ready" && (
							<Button busy={fetching} onClick={fetchBrowser}>
								{t("fetchIt")}
							</Button>
						)}
						{fetchError && <p role="alert">{fetchError}</p>}
					</Row>
				)}
			</ul>
			{offered && <FindOffer onSkip={() => setSkipped(true)} />}
			{data && !boxed && <p className={styles.warn}>{t("noDockerWarning")}</p>}
			<div className={styles.foot}>
				<Button onClick={again}>{t("checkAgain")}</Button>
				<span>
					{data && !boxed && (
						<Button disabled={!basics} onClick={() => onward(true)}>
							{t("continueWithoutDocker")}
						</Button>
					)}
					<Button
						kind="primary"
						disabled={!(basics && boxed)}
						onClick={() => onward(false)}
					>
						{t("continue")}
					</Button>
				</span>
			</div>
		</Wizard>
	);
}
