import mark from "@farik/brand/assets/logo-mark-1254.png";
import wordmark from "@farik/brand/assets/wordmark-1024.png";
import { Button } from "@farik/ui";
import { useConnection } from "../app/connection.tsx";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";

const COMMAND = "farik serve";

/** Why the page cannot work yet: no session, a used link, or Farik not answering. */
export function Connect() {
	const { status, linkUsed } = useConnection();
	return (
		<main className={styles.connect}>
			<div className={styles.brand}>
				<img className={styles.mark} src={mark} alt="" />
				<img className={styles.wordmark} src={wordmark} alt={t("brand")} />
			</div>
			{linkUsed && <p>{t("linkUsed")}</p>}
			{status === "no_session" && (
				<>
					<h1 className={styles.title}>{t("noSessionTitle")}</h1>
					<p>{t("noSessionBody")}</p>
					<ol className={styles.steps}>
						<li>{t("stepTerminal")}</li>
						<li>
							{t("stepType")}
							<div className={styles.command}>
								<code>{COMMAND}</code>
								<Button onClick={() => navigator.clipboard?.writeText(COMMAND)}>
									{t("copy")}
								</Button>
							</div>
						</li>
						<li>{t("stepOpen")}</li>
					</ol>
					<p className={styles.muted}>{t("desktopLater")}</p>
				</>
			)}
			{status === "lost" && (
				<>
					<h1 className={styles.title}>{t("lostTitle")}</h1>
					<p>{t("lostBody")}</p>
					<p role="status" className={styles.retry}>
						<span aria-hidden="true">&gt; </span>
						{t("lostRetry")}
					</p>
				</>
			)}
		</main>
	);
}
