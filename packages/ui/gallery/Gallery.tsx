import { type ReactNode, useState } from "react";
import {
	AVATAR_URLS,
	Avatar,
	type AvatarKey,
	Button,
	ChatList,
	Choice,
	Dialog,
	DiffView,
	KanbanColumn,
	List,
	type Role,
	RoleTag,
	StatusWord,
	Stepper,
	Switch,
	Table,
	TextArea,
	TextField,
} from "../src/index.ts";
import styles from "./Gallery.module.css";

const avatarKeys = Object.keys(AVATAR_URLS) as AvatarKey[];
const roles: Role[] = [
	"product_manager",
	"scrum_master",
	"architect",
	"software_developer",
	"marketing_specialist",
	"finance_specialist",
	"procurement_specialist",
];
const diff = [
	"--- a/site/menu.html",
	"+++ b/site/menu.html",
	"@@ -1,3 +1,3 @@",
	" <h1>Corner Bakery</h1>",
	"-<p>Open every day.</p>",
	"+<p>Open Tuesday to Sunday, from 7 in the morning.</p>",
	' <a href="/order">Order</a>',
	"--- a/site/style.css",
	"+++ b/site/style.css",
	"@@ -1,2 +1,2 @@",
	"-h1 { color: brown; }",
	"+h1 { color: #5a3a22; }",
	" p { margin: 0; }",
].join("\n");
const messages = [
	{
		id: "m1",
		author: {
			name: "Mira",
			role: "product_manager" as Role,
			avatarKey: "product-manager" as AvatarKey,
		},
		time: "9:02",
		text: "The order page is ready for review. Theo has it on the board.",
	},
	{
		id: "m2",
		author: {
			name: "Sol",
			role: "scrum_master" as Role,
			avatarKey: "scrum-master" as AvatarKey,
		},
		time: "9:15",
		text: "Standup: Ada finished the menu layout, Theo is on the order form, nobody is blocked.",
		thread: "Standup, 3 replies",
	},
	{
		id: "m3",
		author: {
			name: "Kai",
			role: "marketing_specialist" as Role,
			avatarKey: "marketing-specialist" as AvatarKey,
		},
		time: "9:40",
		text: "I drafted the opening-week post. It is waiting for your look.",
	},
];
const moved = [
	"Theo finished the order form",
	"Ada moved the menu page to Review",
	"Kai drafted the opening-week post",
];
const costs = [
	{ agent: "Mira", cost: "$1.20" },
	{ agent: "Sol", cost: "$0.45" },
	{ agent: "Ada", cost: "$2.10" },
	{ agent: "Theo", cost: "$3.85" },
	{ agent: "Kai", cost: "$0.95" },
];

function Example({ name, children }: { name: string; children: ReactNode }) {
	return (
		<section className={styles.example} data-component={name}>
			<h2 className={styles.name}>{name}</h2>
			{children}
		</section>
	);
}

function Review({ id, title }: { id: string; title: string }) {
	return (
		<KanbanColumn id={id} title={title} count={2}>
			<div className={styles.row}>
				<Avatar avatarKey="developer" name="Theo" size={32} />
				<span>The order form</span>
				<StatusWord tone="waiting">Waiting for you</StatusWord>
			</div>
			<div className={styles.row}>
				<Avatar avatarKey="architect" name="Ada" size={32} />
				<span>The menu page</span>
				<StatusWord tone="working">Being checked</StatusWord>
			</div>
		</KanbanColumn>
	);
}

// Every id carries the column suffix so the page keeps them unique.
function Column({ theme }: { theme: "light" | "dark" }) {
	const s = `-${theme}`;
	const [name, setName] = useState("Corner Bakery");
	const [url, setUrl] = useState("bakery");
	const [note, setNote] = useState("");
	const [plan, setPlan] = useState("one");
	const [on, setOn] = useState(true);
	const [open, setOpen] = useState(false);
	return (
		<div className={styles.column} data-theme={theme}>
			<h1>{theme === "light" ? "Light" : "Dark"}</h1>
			<Example name="Button">
				<div className={styles.row}>
					<Button kind="primary">Accept the work</Button>
					<Button>Send back with a note</Button>
					<Button kind="quiet">Not now</Button>
					<Button busy>Saving</Button>
					<Button disabled>Approve the plan</Button>
				</div>
			</Example>
			<Example name="TextField">
				<TextField
					id={`name${s}`}
					label="Project name"
					hint="Shown on your Today page."
					value={name}
					onChange={setName}
				/>
				<TextField
					id={`url${s}`}
					label="Website"
					error="That does not look like a web address."
					value={url}
					onChange={setUrl}
				/>
			</Example>
			<Example name="TextArea">
				<TextArea
					id={`note${s}`}
					label="Your note"
					hint="Mira reads this first."
					value={note}
					onChange={setNote}
				/>
			</Example>
			<Example name="Choice">
				<Choice
					name={`plan${s}`}
					legend="How should the team start?"
					value={plan}
					onChange={setPlan}
					options={[
						{
							value: "one",
							label: "One small task",
							description: "Good for a first look.",
						},
						{
							value: "week",
							label: "A week of work",
							description: "Mira plans a sprint.",
						},
						{
							value: "later",
							label: "Not yet",
							description: "Start when you are ready.",
						},
					]}
				/>
			</Example>
			<Example name="Switch">
				<Switch
					id={`on${s}`}
					label="Tell me when work is ready"
					checked={on}
					onChange={setOn}
				/>
				<Switch
					id={`off${s}`}
					label="Weekly summary by email"
					checked={false}
					onChange={() => {}}
				/>
			</Example>
			<Example name="Dialog">
				<Button onClick={() => setOpen(true)}>Send back with a note</Button>
				<Dialog
					open={open}
					title="Send back with a note"
					onClose={() => setOpen(false)}
					actions={
						<Button kind="primary" onClick={() => setOpen(false)}>
							Send back
						</Button>
					}
				>
					<p>Tell Theo what to change. He will pick it up again.</p>
				</Dialog>
			</Example>
			<Example name="Stepper">
				<Stepper
					steps={["Name", "Team", "Budget", "Connect", "Plan", "Finish"]}
					current={2}
				/>
			</Example>
			<Example name="List">
				<List
					label={`What moved, ${theme}`}
					items={moved}
					getKey={(m) => m}
					render={(m) => m}
					empty={<p>Nothing yet.</p>}
				/>
			</Example>
			<Example name="Table">
				<Table
					caption={`Costs per agent, ${theme}`}
					rows={costs}
					getKey={(r) => r.agent}
					columns={[
						{ key: "agent", header: "Agent", render: (r) => r.agent },
						{
							key: "cost",
							header: "Cost",
							align: "end",
							render: (r) => r.cost,
						},
					]}
				/>
			</Example>
			<Example name="KanbanColumn">
				{/* Axe wants unique landmark names on one page, so the dark column's title differs. */}
				<Review
					id={`review${s}`}
					title={theme === "light" ? "Review" : "In review"}
				/>
			</Example>
			<Example name="ChatList">
				<ChatList label={`Channel, ${theme}`} messages={messages} />
			</Example>
			<Example name="DiffView">
				<DiffView label={`Code changes, ${theme}`} diff={diff} />
			</Example>
			<Example name="Avatar">
				<div className={styles.row}>
					{avatarKeys.map((k) => (
						<Avatar key={k} avatarKey={k} name={k} />
					))}
				</div>
			</Example>
			<Example name="RoleTag">
				<div className={styles.row}>
					{roles.map((r) => (
						<RoleTag key={r} role={r} />
					))}
				</div>
			</Example>
			<Example name="StatusWord">
				<div className={styles.row}>
					<StatusWord tone="done">Done</StatusWord>
					<StatusWord tone="working">Working</StatusWord>
					<StatusWord tone="waiting">Waiting for you</StatusWord>
				</div>
			</Example>
		</div>
	);
}

function Phone() {
	const [name, setName] = useState("Corner Bakery");
	return (
		<div className={styles.phone}>
			<h1>Phone, 360 px</h1>
			<Example name="Button">
				<Button kind="primary">Accept the work</Button>
			</Example>
			<Example name="TextField">
				<TextField
					id="name-phone"
					label="Project name"
					value={name}
					onChange={setName}
				/>
			</Example>
			<Example name="KanbanColumn">
				<KanbanColumn id="review-phone" title="Up for review" count={1}>
					<div className={styles.row}>
						<Avatar avatarKey="developer" name="Theo" size={32} />
						<span>The order form</span>
						<StatusWord tone="waiting">Waiting for you</StatusWord>
					</div>
				</KanbanColumn>
			</Example>
			<Example name="ChatList">
				<ChatList label="Channel, phone" messages={messages.slice(0, 2)} />
			</Example>
		</div>
	);
}

export function Gallery() {
	return (
		<div className={styles.page}>
			<Column theme="light" />
			<Column theme="dark" />
			<Phone />
		</div>
	);
}
