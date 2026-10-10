import { Choice, InfoTip, Switch } from "@catervas/ui";
import { useAdvanced } from "../../app/advanced.ts";
import type { ThemeChoice } from "../../app/theme.ts";
import { Bullets } from "../../components/Bullets.tsx";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";

/** General: the theme, the Advanced switch that changes every page, and the language. */
export function General({
	theme,
	onTheme,
}: {
	theme: ThemeChoice;
	onTheme: (c: ThemeChoice) => void;
}) {
	const [advanced, toggle] = useAdvanced();
	return (
		<div className={styles.page}>
			<h2 className={styles.pageHeading}>{t("sectionGeneral")}</h2>
			<Choice<ThemeChoice>
				name="theme"
				legend={t("theme")}
				value={theme}
				onChange={onTheme}
				options={[
					{
						value: "light",
						label: t("themeLight"),
						description: t("themeLightNote"),
					},
					{
						value: "dark",
						label: t("themeDark"),
						description: t("themeDarkNote"),
					},
					{
						value: "system",
						label: t("themeSystem"),
						description: t("themeSystemNote"),
					},
				]}
			/>
			<section className={styles.section} aria-labelledby="advanced-heading">
				<div className={styles.titled}>
					<h3 id="advanced-heading">{t("advanced")}</h3>
					<InfoTip id="advancedMore-info">
						<Bullets text={t("advancedMore")} />
					</InfoTip>
				</div>
				<Switch
					id="advanced"
					label={t("advancedSwitch")}
					description={t("advancedBody")}
					checked={advanced}
					onChange={toggle}
				/>
			</section>
			<section className={styles.section} aria-labelledby="language-heading">
				<h3 id="language-heading">{t("language")}</h3>
				<p>{t("english")}</p>
				<p className={styles.muted}>{t("englishOnly")}</p>
			</section>
		</div>
	);
}
