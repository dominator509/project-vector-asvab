// Vitest setup: jsdom environment plus jest-dom matchers so accessibility
// assertions (toHaveAttribute, toBeVisible, ...) are available in component
// tests.
import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// Unmount between tests so query results cannot leak across cases.
afterEach(() => {
  cleanup();
});
