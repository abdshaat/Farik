import { Choice } from "@catervas/ui";
import type { ThemeChoice } from "../../app/theme.ts";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";

/** General: the theme and the language. */
export function General({
	theme,
	onTheme,
}: {
	theme: ThemeChoice;
	onTheme: (c: ThemeChoice) => void;
}) {
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
			<section className={styles.section} aria-labelledby="language-heading">
				<h3 id="language-heading">{t("language")}</h3>
				<p>{t("english")}</p>
				<p className={styles.muted}>{t("englishOnly")}</p>
			</section>
		</div>
	);
}
