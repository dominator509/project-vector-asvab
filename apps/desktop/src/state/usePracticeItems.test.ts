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

  it("asks the backend for the objective the plan named", async () => {
    const fake = createFakeClient();
    await fake.client.contentGenerate("AR", 12, 5);

    // Every objective the generated corpus actually holds, read back from the
    // fake's own store rather than assumed.
    const objectives = [
      ...new Set(
        [...fake.state.items.values()].map((item) => item.objective_id),
      ),
    ];
    expect(objectives.length).toBeGreaterThan(1);

    const target = objectives[objectives.length - 1];
    fake.calls.length = 0;

    const { result } = renderHook(() =>
      usePracticeItems(fake.client, "AR", 4, false, target),
    );
    await waitFor(() => expect(result.current.state.status).toBe("ready"));

    // The objective must reach the command boundary, not merely be accepted by
    // the hook: a hook that dropped it would serve another objective's items and
    // still look correct from the outside.
    const serves = fake.calls.filter((call) => call.command === "content_next");
    expect(serves.length).toBeGreaterThan(0);
    for (const call of serves) {
      expect(call.args?.objectiveId).toBe(target);
    }

    if (result.current.state.status !== "ready") throw new Error("not ready");
    for (const item of result.current.state.items) {
      expect(item.objectiveId).toBe(target);
    }
  });

  it("falls back to the whole subtest when the objective holds nothing", async () => {
    const fake = createFakeClient();
    await fake.client.contentGenerate("AR", 6, 3);

    const { result } = renderHook(() =>
      usePracticeItems(fake.client, "AR", 3, false, "OBJ-AR-NOT-BUILT-01"),
    );
    await waitFor(() => expect(result.current.state.status).toBe("ready"));

    // An unbuilt objective is a content gap, not a reason to serve an empty
    // screen: the plan can name an objective this installation has no items for.
    if (result.current.state.status !== "ready") throw new Error("not ready");
    expect(result.current.state.items).toHaveLength(3);
    expect(result.current.state.items[0].subtest).toBe("AR");
  });
});
