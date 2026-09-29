import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { media } from "../test/media.ts";
import { useTheme } from "./theme.ts";

const shown = () => document.documentElement.dataset.theme;

describe("theme", () => {
	afterEach(() => {
		vi.restoreAllMocks();
		localStorage.clear();
	});

	it("remembers_the_theme_and_follows_the_computer", () => {
		const { result } = renderHook(() => useTheme());
		act(() => result.current[1]("dark"));
		expect(shown()).toBe("dark");
		expect(localStorage.getItem("farik.theme")).toBe("dark");

		act(() => result.current[1]("system"));
		expect(result.current[0]).toBe("system");
		expect(shown()).toBe("light");
		act(() => media.set("(prefers-color-scheme: dark)", true));
		expect(shown()).toBe("dark");
		act(() => media.set("(prefers-color-scheme: dark)", false));
		expect(shown()).toBe("light");
	});

	it("works_without_local_storage", () => {
		const refuse = () => {
			throw new DOMException("denied", "SecurityError");
		};
		vi.spyOn(Storage.prototype, "getItem").mockImplementation(refuse);
		vi.spyOn(Storage.prototype, "setItem").mockImplementation(refuse);
		document.documentElement.dataset.theme = "dark";
		const { result } = renderHook(() => useTheme());
		expect(result.current[0]).toBe("light");
		expect(shown()).toBe("light");
		act(() => result.current[1]("dark"));
		expect(shown()).toBe("dark");
	});
});
