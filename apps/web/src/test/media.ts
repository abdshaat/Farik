type Listener = (e: { matches: boolean }) => void;

const matches = new Map<string, boolean>();
const listeners = new Map<string, Set<Listener>>();

/** The stubbed `matchMedia`: tests set what each query matches, and listeners hear the change. */
export const media = {
	set(query: string, value: boolean): void {
		matches.set(query, value);
		for (const fn of listeners.get(query) ?? []) fn({ matches: value });
	},
	reset(): void {
		matches.clear();
		listeners.clear();
	},
	list(query: string): MediaQueryList {
		const own = listeners.get(query) ?? new Set<Listener>();
		listeners.set(query, own);
		return {
			media: query,
			get matches() {
				return matches.get(query) ?? false;
			},
			addEventListener: (_: string, fn: Listener) => own.add(fn),
			removeEventListener: (_: string, fn: Listener) => own.delete(fn),
		} as unknown as MediaQueryList;
	},
};
