/// <reference types="vitest" />
import { createHash } from "node:crypto";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const packageRoot = fileURLToPath(new URL(".", import.meta.url));

/**
 * A stamp identifying the frontend sources in this build.
 *
 * The application stores this stamp when the webview first reaches the Rust
 * command layer, so the stored marker identifies *which* bundle made the call.
 * Without it, "the interface talked to the core" would be a claim about
 * whenever the check happened to run rather than about a specific artifact.
 *
 * Built from the package version plus a digest over every source file, so
 * editing any of them changes the stamp.
 */
function buildStamp(): string {
  const { version } = JSON.parse(
    readFileSync(join(packageRoot, "package.json"), "utf8"),
  ) as { version: string };

  const files: string[] = [];
  const collect = (dir: string): void => {
    for (const entry of readdirSync(dir)) {
      const full = join(dir, entry);
      if (statSync(full).isDirectory()) {
        if (entry === "node_modules" || entry === "dist") continue;
        collect(full);
        continue;
      }
      if (/\.(ts|tsx|css|html)$/.test(entry)) {
        files.push(relative(packageRoot, full).split(sep).join("/"));
      }
    }
  };
  collect(join(packageRoot, "src"));
  collect(join(packageRoot, "e2e"));
  collect(join(packageRoot, "integration"));
  files.push("index.html", "package.json");
  files.sort();

  const hash = createHash("sha256");
  for (const file of files) {
    hash.update(file);
    hash.update("\0");
    hash.update(readFileSync(join(packageRoot, file)));
    hash.update("\0");
  }

  return `${version}+${hash.digest("hex").slice(0, 12)}`;
}

export default defineConfig({
  plugins: [react()],
  define: {
    __VECTOR_BUILD_STAMP__: JSON.stringify(buildStamp()),
  },
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
    include: [
      "src/**/*.{test,spec}.{ts,tsx}",
      "integration/**/*.test.{ts,tsx}",
    ],
  },
});
