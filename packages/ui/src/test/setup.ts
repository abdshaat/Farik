import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";
import "./dialog-shim.ts";

// Vitest has no globals, so Testing Library's automatic cleanup does not register.
afterEach(cleanup);
