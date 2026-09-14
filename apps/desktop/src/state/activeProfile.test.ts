/**
 * Tests for the remembered learner selection.
 *
 * These cover the failures that actually happen: storage that throws, a value
 * that is present but unusable, and a remembered profile that no longer exists
 * because it was deleted.
 */

import { describe, expect, it } from "vitest";

import {
  ACTIVE_PROFILE_KEY,
  clearActiveProfileId,
  readActiveProfileId,
  resolveActiveProfileId,
  writeActiveProfileId,
  type StorageLike,
} from "./activeProfile";

function memoryStorage(initial: Record<string, string> = {}): StorageLike {
  const map = new Map(Object.entries(initial));
  return {
    getItem: (key) => map.get(key) ?? null,
    setItem: (key, value) => void map.set(key, value),
    removeItem: (key) => void map.delete(key),
  };
}

const throwingStorage: StorageLike = {
  getItem: () => {
    throw new Error("storage is disabled");
  },
  setItem: () => {
    throw new Error("storage is disabled");
  },
  removeItem: () => {
    throw new Error("storage is disabled");
  },
};

describe("reading the remembered profile", () => {
  it("returns null when nothing is stored", () => {
    expect(readActiveProfileId(memoryStorage())).toBeNull();
  });

  it("returns a stored id", () => {
    const storage = memoryStorage({ [ACTIVE_PROFILE_KEY]: "learner-7" });
    expect(readActiveProfileId(storage)).toBe("learner-7");
  });

  it("trims surrounding whitespace", () => {
    const storage = memoryStorage({ [ACTIVE_PROFILE_KEY]: "  learner-7 " });
    expect(readActiveProfileId(storage)).toBe("learner-7");
  });

  it("treats a blank value as absent rather than sending it to the backend", () => {
    // An empty id would otherwise produce a confusing "not found" for a
    // lexically valid request.
    expect(
      readActiveProfileId(memoryStorage({ [ACTIVE_PROFILE_KEY]: "" })),
    ).toBeNull();
    expect(
      readActiveProfileId(memoryStorage({ [ACTIVE_PROFILE_KEY]: "   " })),
    ).toBeNull();
  });

  it("survives storage that throws", () => {
    expect(readActiveProfileId(throwingStorage)).toBeNull();
  });
});

describe("writing the remembered profile", () => {
  it("round-trips a value", () => {
    const storage = memoryStorage();
    writeActiveProfileId(storage, "learner-3");
    expect(readActiveProfileId(storage)).toBe("learner-3");
  });

  it("clears a value", () => {
    const storage = memoryStorage({ [ACTIVE_PROFILE_KEY]: "learner-3" });
    clearActiveProfileId(storage);
    expect(readActiveProfileId(storage)).toBeNull();
  });

  it("does not throw when storage is unavailable", () => {
    expect(() =>
      writeActiveProfileId(throwingStorage, "learner-3"),
    ).not.toThrow();
    expect(() => clearActiveProfileId(throwingStorage)).not.toThrow();
  });
});

describe("resolving the remembered id against what exists", () => {
  it("keeps a remembered id that still exists", () => {
    expect(
      resolveActiveProfileId("learner-2", ["learner-1", "learner-2"]),
    ).toBe("learner-2");
  });

  it("falls back to the only profile when the remembered one is gone", () => {
    // A deleted profile must not stay selected: every request would fail.
    expect(resolveActiveProfileId("learner-9", ["learner-1"])).toBe(
      "learner-1",
    );
  });

  it("selects nothing when several profiles exist and none is remembered", () => {
    // Guessing would silently attribute one learner's practice to another.
    expect(resolveActiveProfileId(null, ["learner-1", "learner-2"])).toBeNull();
  });

  it("selects nothing when there are no profiles", () => {
    expect(resolveActiveProfileId(null, [])).toBeNull();
    expect(resolveActiveProfileId("learner-1", [])).toBeNull();
  });
});
