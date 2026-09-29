import { en } from "./en.ts";

export function t(key: keyof typeof en): string {
	return en[key];
}
