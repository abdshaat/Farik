// A new project's interview and product plan, as `chat.messages` and `product_plan.state` answer
// them (spec 4.3), with the words of the approved mockups: Mira's summary of the talk about
// Little Oak Cakes.

/** What Mira heard, as the chat's `heard` has it on the wire. */
export const HEARD = {
	customers:
		"People nearby ordering a cake for a birthday, a wedding or an office party.",
	problem:
		"Orders arrive by Instagram and phone, some get lost, and you can’t see the week ahead.",
	must: "Let customers choose a size, a flavour and a collection day and pay a deposit; show you the week’s orders in one list.",
	not_now: "Delivery and a phone app.",
	works_when:
		"No order is lost, and half your orders come through the website by March.",
};

/** The state while Mira interviews: a new project, no plan task filed yet. */
export const INTERVIEWING = {
	new_project: { name: "little-oak-cakes" },
	approved: false,
	task: null,
	documents: [],
};

/** The state once the plan task is accepted. */
export const APPROVED = { ...INTERVIEWING, approved: true };
