import { Button, StatusWord } from "@farik/ui";
import { useState } from "react";
import { useNavigate } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { useQuery } from "../../app/store.ts";
import type { en } from "../../strings/en.ts";
import { t } from "../../strings/t.ts";
import styles from "./setup.module.css";
import { keepNoSandbox, Wizard } from "./Wizard.tsx";

type Item = {
	state: "ready" | "missing" | "too_old" | "not_running";
	version?: string;
};
type Check = { claude: Item; git: Item; docker: Item; sandboxImage: Item };
type Key = keyof typeof en;

const WORD: Record<Item["state"], Key> = {
	ready: "ready",
	missing: "notFound",
	too_old: "tooOld",
	not_running: "notRunning",
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
					tone={item.state === "ready" ? "done" : "waiting"}
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

/** Setup's first step: the programs Farik needs, and what to do about each one missing. */
export function SetupComputer() {
	const { client } = useConnection();
	const navigate = useNavigate();
	const { data, again } = useQuery<Check>("computer.check", {});
	const [building, setBuilding] = useState(false);
	const [buildError, setBuildError] = useState<string>();

	const basics = data?.claude.state === "ready" && data.git.state === "ready";
	const docker = data?.docker.state === "ready";
	const boxed = docker && data?.sandboxImage.state === "ready";
	const onward = (without: boolean) => {
		keepNoSandbox(without);
		navigate("/setup/account");
	};
	const prepare = async () => {
		if (!client) return;
		setBuilding(true);
		setBuildError(undefined);
		try {
			await client.call("sandbox.build", {});
		} catch (e) {
			setBuildError((e as Error).message);
		} finally {
			setBuilding(false);
			again();
		}
	};

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
						{data?.sandboxImage.state !== "ready" && (
							<Button busy={building} onClick={prepare}>
								{t("prepare")}
							</Button>
						)}
						{buildError && <p role="alert">{buildError}</p>}
					</Row>
				)}
			</ul>
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
