/**
 * The practice content loader.
 *
 * The behaviour worth pinning down is what happens when the corpus is empty.
 * The previous wiring rendered `sampleQuestions` unconditionally, so an empty
 * corpus was indistinguishable from a full one. Here an empty corpus must
 * generate a starter batch and then serve from it, and must never silently
 * produce demonstration content.
 */

import { renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { createFakeClient } from "../ipc/testing/fakeClient";
import { usePracticeItems } from "./usePracticeItems";

describe("usePracticeItems", () => {
  it("generates a starter batch when the corpus is empty, then serves items", async () => {
    const fake = createFakeClient();
    expect(fake.state.items.size).toBe(0);

    const { result } = renderHook(() => usePracticeItems(fake.client, "AR", 5));

    await waitFor(() => expect(result.current.state.status).toBe("ready"));
    expect(fake.commands()).toContain("content_generate");
    expect(fake.commands()).toContain("content_next");

    if (result.current.state.status !== "ready") throw new Error("not ready");
    expect(result.current.state.items).toHaveLength(5);
    expect(result.current.state.items[0].subtest).toBe("AR");
    expect(result.current.state.stats.total).toBeGreaterThan(0);
  });

  it("serves real items rather than the demonstration set", async () => {
    const fake = createFakeClient();
    const { result } = renderHook(() => usePracticeItems(fake.client, "AR", 3));
    await waitFor(() => expect(result.current.state.status).toBe("ready"));

    if (result.current.state.status !== "ready") throw new Error("not ready");
    for (const item of result.current.state.items) {
      // Demonstration ids are hand-written slugs like "q-ar-1"; generated ids
      // carry the subtest and seed the factory used.
      expect(item.id).toMatch(/^q-AR-/);
      expect(item.objectiveId).toBeDefined();
      expect(item.sourceId).toBeUndefined();
    }
  });

  it("does not generate when the corpus already holds items", async () => {
    const fake = createFakeClient();
    await fake.client.contentGenerate("AR", 4, 99);
    fake.calls.length = 0;

    const { result } = renderHook(() => usePracticeItems(fake.client, "AR", 2));
    await waitFor(() => expect(result.current.state.status).toBe("ready"));
    expect(fake.commands()).not.toContain("content_generate");
  });

  it("reports an empty corpus for a subtest with no items", async () => {
    const fake = createFakeClient();
    // Seed AR so the loader does not generate, then ask for MK.
    await fake.client.contentGenerate("AR", 3, 0);
    const { result } = renderHook(() => usePracticeItems(fake.client, "MK", 5));

    await waitFor(() => expect(result.current.state.status).toBe("empty"));
    if (result.current.state.status !== "empty") throw new Error("not empty");
    expect(result.current.state.stats.total).toBeGreaterThan(0);
  });

  it("reports an error rather than rendering nothing when the backend fails", async () => {
    const fake = createFakeClient({ unavailable: true });
    const { result } = renderHook(() => usePracticeItems(fake.client, "AR", 5));

    await waitFor(() => expect(result.current.state.status).toBe("error"));
    if (result.current.state.status !== "error") throw new Error("not error");
    expect(result.current.state.message).not.toBe("");
  });

  it("generates more without repeating what is already stored", async () => {
    const fake = createFakeClient();
    const { result } = renderHook(() => usePracticeItems(fake.client, "AR", 2));
    await waitFor(() => expect(result.current.state.status).toBe("ready"));
    const before = fake.state.items.size;

    await result.current.generate("AR", 6);
    await waitFor(() => expect(fake.state.items.size).toBeGreaterThan(before));
  });
});
