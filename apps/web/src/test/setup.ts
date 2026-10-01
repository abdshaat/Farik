import "@farik/ui/test/dialog-shim";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";
import { media } from "./media.ts";

// Vitest has no globals, so Testing Library's automatic cleanup does not register.
afterEach(cleanup);
afterEach(() => media.reset());

// jsdom has no matchMedia; this stub answers what the tests set through `media`.
window.matchMedia = (query: string) => media.list(query);
