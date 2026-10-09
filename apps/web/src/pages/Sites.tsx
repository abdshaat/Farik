import { Button, Dialog, Switch, TextField } from "@farik/ui";
import { type FormEvent, useState } from "react";
import { useQuery } from "../app/store.ts";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";
import { useCommand } from "./dialogs/StartSprint.tsx";
import { listed } from "./marketing.ts";
import styles from "./pages.module.css";

/** One of Farik's own approved sites, and whether the team may read it now. */
type FarikSite = {
	host: string;
	shop: string;
	category: string;
	on: boolean;
	/** When the owner last turned it off or back on, when they ever did. */
	at?: string;
};
/** A site the owner allowed. */
type OwnerSite = {
	host: string;
	at: string;
	/** The request it answered, when the agent asked. */
	request?: number;
};
type SiteList = { farik: FarikSite[]; owner: OwnerSite[] };

/** What a kind of shop is called, by the daemon's word for it. */
const CATEGORIES: Record<string, keyof typeof en> = {
	general_marketplace: "siteCategoryGeneralMarketplace",
	office_supplies: "siteCategoryOfficeSupplies",
	industrial_supplies: "siteCategoryIndustrialSupplies",
	packaging_and_shipping: "siteCategoryPackagingAndShipping",
	electronic_components: "siteCategoryElectronicComponents",
	computers_and_it: "siteCategoryComputersAndIt",
	furniture: "siteCategoryFurniture",
	food_service: "siteCategoryFoodService",
	printing: "siteCategoryPrinting",
	software: "siteCategorySoftware",
};

/** How many shops a closed category names before it says how many more there are. */
const NAMED = 3;

/** The site words name, as the daemon keeps it, for the line that says it was added. */
function siteOf(words: string): string {
	const address = words.includes("://") ? words : `https://${words}`;
	let host: string;
	try {
		host = new URL(address).hostname.replace(/\.$/, "");
	} catch {
		return words;
	}
	const bare = host.slice("www.".length);
	return host.startsWith("www.") && bare.includes(".") ? bare : host;
}

/** "today", "yesterday" or "on 2 October": the day of `time` in the browser's time zone. */
function dayOf(time: string, now: Date): string {
	const date = new Date(time);
	const days = (d: Date) =>
		Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()) / 86_400_000;
	const ago = days(now) - days(date);
	if (ago === 0) return t("siteDayToday");
	if (ago === 1) return t("siteDayYesterday");
	return t("siteDayOn", {
		day: new Intl.DateTimeFormat("en-GB", {
			day: "numeric",
			month: "long",
		}).format(date),
	});
}

/** "Amazon, eBay and Walmart", or "Amazon, eBay, Walmart and 3 more". */
function named(shops: FarikSite[]): string {
	const names = shops.map((shop) => shop.shop);
	return names.length <= NAMED
		? listed(names)
		: t("sitesMore", {
				names: names.slice(0, NAMED).join(", "),
				n: names.length - NAMED,
			});
}

/** "5 shops, 1 turned off". */
function counted(shops: FarikSite[]): string {
	const off = shops.filter((shop) => !shop.on).length;
	return (
		(shops.length === 1
			? t("sitesCountOne")
			: t("sitesCount", { n: shops.length })) +
		(off > 0 ? t("sitesCountOff", { n: off }) : "")
	);
}

/**
 * "Sites it may read" on the Procurement Specialist's page: Farik's approved sites by kind of
 * shop, each with a switch, and the sites the owner allowed, each with a Remove, and "Add a
 * site". Every one acts at once, not through "Save changes" (spec 6.10).
 */
export function SitesSection({ name }: { name: string }) {
	const { data, again } = useQuery<SiteList>("sites.list", {});
	const [open, setOpen] = useState<Record<string, boolean>>({});
	const [removing, setRemoving] = useState<string>();
	const [text, setText] = useState("");
	const [added, setAdded] = useState<string>();
	const toggle = useCommand(again);
	const removal = useCommand(() => {
		setRemoving(undefined);
		again();
	});
	const adding = useCommand(() => {
		setAdded(siteOf(text.trim()));
		setText("");
		again();
	});
	if (!data) return null;
	const now = new Date();
	const categories = [...new Set(data.farik.map((shop) => shop.category))];
	const flip = (shop: FarikSite, on: boolean) =>
		toggle.send(
			on
				? { command: "site_add", body: { site: shop.host } }
				: { command: "site_remove", body: { host: shop.host } },
		);
	const add = (event: FormEvent) => {
		event.preventDefault();
		setAdded(undefined);
		adding.send({ command: "site_add", body: { site: text.trim() } });
	};
	return (
		<section className={styles.section} aria-labelledby="sites-heading">
			<h2 id="sites-heading">{t("sitesTitle")}</h2>
			<p className={styles.muted}>{t("sitesLead", { name })}</p>
			<h3 className={styles.subheading}>{t("sitesFarikTitle")}</h3>
			<p className={styles.muted}>{t("sitesFarikNote", { name })}</p>
			<ul className={styles.siteCategories}>
				{categories.map((category) => {
					const shops = data.farik.filter((shop) => shop.category === category);
					// One with a shop turned off is open, so that what was changed is seen.
					const isOpen = open[category] ?? shops.some((shop) => !shop.on);
					const listId = `sites-${category}`;
					const key = CATEGORIES[category];
					return (
						<li key={category}>
							<h4 className={styles.siteCategory}>
								<button
									type="button"
									className={styles.siteToggle}
									aria-expanded={isOpen}
									aria-controls={listId}
									onClick={() => setOpen({ ...open, [category]: !isOpen })}
								>
									<span className={styles.siteKind}>
										{key ? t(key) : category}
									</span>
									<span className={styles.muted}>
										{isOpen ? counted(shops) : named(shops)}
									</span>
								</button>
							</h4>
							{isOpen && (
								<ul id={listId} className={styles.siteShops}>
									{shops.map((shop) => (
										<li key={shop.host}>
											<Switch
												id={`site-${shop.host}`}
												label={shop.shop}
												description={shop.host}
												checked={shop.on}
												onChange={(on) => flip(shop, on)}
											/>
											<span className={styles.siteState} aria-hidden="true">
												{t(shop.on ? "sitesOn" : "sitesOff")}
											</span>
											{!shop.on && shop.at && (
												<span className={styles.muted}>
													{t("sitesTurnedOff", { day: dayOf(shop.at, now) })}
												</span>
											)}
										</li>
									))}
								</ul>
							)}
						</li>
					);
				})}
			</ul>
			{toggle.refusal && (
				<p role="alert" className={styles.alert}>
					{toggle.refusal}
				</p>
			)}
			<h3 className={styles.subheading}>{t("sitesOwnTitle")}</h3>
			{data.owner.length === 0 ? (
				<p className={styles.muted}>{t("sitesNoneOwn")}</p>
			) : (
				<ul className={styles.siteOwn} aria-label={t("sitesOwnTitle")}>
					{data.owner.map((site) => (
						<li
							key={site.host}
							className={site.host === added ? styles.siteNew : undefined}
						>
							<span className={styles.siteText}>
								<code className={styles.siteHost}>{site.host}</code>
								<span className={styles.muted}>
									{site.request
										? t("sitesOwnAsked", { day: dayOf(site.at, now), name })
										: t("sitesOwnAdded", { day: dayOf(site.at, now) })}
								</span>
							</span>
							<button
								type="button"
								className={styles.siteRemove}
								aria-label={t("sitesRemoveLabel", { host: site.host })}
								onClick={() => setRemoving(site.host)}
							>
								{t("sitesRemove")}
							</button>
						</li>
					))}
				</ul>
			)}
			<form className={styles.siteAdd} onSubmit={add}>
				<TextField
					id="site-add"
					label={t("sitesAdd")}
					hint={t("sitesAddHint")}
					value={text}
					onChange={setText}
				/>
				<Button type="submit" busy={adding.busy} disabled={!text.trim()}>
					{t("sitesAddButton")}
				</Button>
			</form>
			{adding.refusal && (
				<p role="alert" className={styles.alert}>
					{adding.refusal}
				</p>
			)}
			{added && (
				<p role="status" className={styles.siteAdded}>
					{t("sitesAdded", { name, host: added })}
				</p>
			)}
			{removing && (
				<Dialog
					open
					title={t("sitesRemoveTitle", { host: removing })}
					onClose={() => setRemoving(undefined)}
					actions={
						<>
							<Button onClick={() => setRemoving(undefined)}>
								{t("connectorKeep")}
							</Button>
							<Button
								kind="primary"
								busy={removal.busy}
								onClick={() =>
									removal.send({
										command: "site_remove",
										body: { host: removing },
									})
								}
							>
								{t("sitesRemove")}
							</Button>
						</>
					}
				>
					<p>{t("sitesRemoveConfirm", { name, host: removing })}</p>
					<p>{t("sitesRemoveAgain", { name })}</p>
					{removal.refusal && <p role="alert">{removal.refusal}</p>}
				</Dialog>
			)}
		</section>
	);
}
