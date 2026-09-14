/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "es2021",
    outDir: "dist",
  },
  test: {
    // The UI is a browser application: components must be tested against a real
    // DOM, not a hand-rolled mock, or keyboard and ARIA behaviour cannot be
    // verified at all.
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test-setup.ts"],
    // Unit tests live beside their source; integration tests live in
    // `integration/` and exercise cross-layer data shapes rather than a single
    // module. Keeping them separate lets `test:unit` and `test:integration`
    // report distinct counts.
    include: ["src/**/*.{test,spec}.{ts,tsx}", "integration/**/*.test.{ts,tsx}"],
  },
});
