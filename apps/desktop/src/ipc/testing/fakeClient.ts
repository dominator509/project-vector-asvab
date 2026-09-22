/**
 * An in-memory implementation of the command boundary, for frontend tests.
 *
 * ## What this is and is not
 *
 * This stands in for the Rust core so that view behaviour — validation,
 * persistence readback, error surfacing, the unavailable state — can be driven
 * from a test without launching the desktop application. It is **not** evidence
 * about the product. Nothing here proves the planner, the database or the
 * backup path works: those are proven by `cargo test --workspace` and by
 * `vector-desktop --self-check` running the real command layer inside the
 * shipped binary.
 *
 * To keep that boundary visible, every method starts from the same state model
 * and the state is inspectable (`snapshot()`), so a test can assert on the
 * durable effect of a click rather than only on what was rendered.
 *
 * ## Strictness
 *
 * Calling a method that a test did not configure is not silently tolerated: the
 * fake throws for unknown learner ids and duplicate profile names, and
 * `calls()` records everything so a test can assert which commands ran and with
 * what arguments.
 */

import { BackendUnavailableError, type VectorClient } from "../client";
import type {
  AnalyticsDto,
  AppPathsDto,
  BackupDto,
  BackupEntryDto,
  ContentItemSummaryDto,
  ContentManagerDto,
  ContentStatsDto,
  EvidenceDto,
  GenerationReportDto,
  HealthDto,
  ItemDto,
  LatencyDto,
  MasteryDto,
  NewEvidenceDto,
  PlanDto,
  ProfileDto,
  ReadinessDto,
  ResetDto,
  RestoreDto,
  ReviewEntryDto,
} from "../types";
export interface FakeCall {
  command: string;
  args?: Record<string, unknown>;
}

export interface FakeState {
  profiles: Map<string, ProfileDto>;
  attempts: Map<
    string,
    { learnerId: string; subtest: string; correct: boolean; latencyMs: number }
  >;
  mastery: Map<string, MasteryDto>;
  /** Generated practice items, keyed by id. */
  items: Map<string, ItemDto>;
  /** Ids withdrawn from service. */
  quarantined: Set<string>;
  /** Review entries appended by quarantine and reinstatement. */
  reviews: ReviewEntryDto[];
  evidence: Map<string, EvidenceDto>;
  backups: Array<{
    path: string;
    checksum: string;
    bytes: number;
    body: string;
  }>;
  /**
   * Markers written by `ui_ready`.
   *
   * The real implementation writes these to `health_diagnostics`; the fake keeps
   * them here so a test can assert that the boundary handshake happened without
   * standing up a database.
   */
  uiReadyMarkers: Array<{ id: string; bundle: string }>;
}

export interface FakeClientOptions {
  /** Simulate an unreachable backend. */
  unavailable?: boolean;
  /** Data directory reported by `app_paths`. */
  dataDir?: string;
  /** Feed a specific readiness band; defaults to one derived from mastery. */
  readiness?: ReadinessDto;
  /** Force the next `backup_restore` to verify against the archive digest. */
  verifyRestoreDigest?: boolean;
}

export interface FakeClient {
  client: VectorClient;
  state: FakeState;
  calls: FakeCall[];
  /** Commands invoked so far, in order. */
  commands(): string[];
}

/** A deterministic 64-hex-character digest, so tests can assert on it. */
function digest(seed: string): string {
  let h1 = 0x811c9dc5;
  let h2 = 0x01000193;
  for (let i = 0; i < seed.length; i += 1) {
    h1 = (h1 ^ seed.charCodeAt(i)) * 16777619;
    h2 = (h2 + seed.charCodeAt(i) * (i + 1)) * 2654435761;
  }
  const part = (n: number) => (n >>> 0).toString(16).padStart(8, "0");
  return (
    part(h1) +
    part(h2) +
    part(h1 ^ h2) +
    part(h1 + h2) +
    part(h1 * 3) +
    part(h2 * 7) +
    part(h1 ^ 0x5bf03635) +
    part(h2 ^ 0x27d4eb2f)
  ).slice(0, 64);
}

export function createFakeClient(options: FakeClientOptions = {}): FakeClient {
  const state: FakeState = {
    profiles: new Map(),
    attempts: new Map(),
    mastery: new Map(),
    items: new Map(),
    quarantined: new Set(),
    reviews: [],
    evidence: new Map(),
    backups: [],
    uiReadyMarkers: [],
  };
  const calls: FakeCall[] = [];
  let nextId = 1;

  const dataDir = options.dataDir ?? "C:/vector-test-data";
  const paths: AppPathsDto = {
    data_dir: dataDir,
    db_path: `${dataDir}/vector.db`,
    backup_dir: `${dataDir}/backups`,
  };

  function record(command: string, args?: Record<string, unknown>) {
    calls.push(args === undefined ? { command } : { command, args });
    if (options.unavailable) {
      throw new BackendUnavailableError(`no backend for ${command}`);
    }
  }

  function stats(learnerId: string, subtest: string): AnalyticsDto {
    const rows = [...state.attempts.values()].filter(
      (a) => a.learnerId === learnerId && a.subtest === subtest,
    );
    const correct = rows.filter((a) => a.correct).length;
    const total = rows.length;
    const mean =
      total === 0
        ? 0
        : Math.round(rows.reduce((s, a) => s + a.latencyMs, 0) / total);
    return {
      total,
      correct,
      accuracy: total === 0 ? 0 : correct / total,
      mean_latency_ms: mean,
    };
  }

  const client: VectorClient = {
    async health(): Promise<HealthDto> {
      record("health");
      return {
        component: "database",
        healthy: true,
        basis: "operation_succeeded",
        profiles: state.profiles.size,
      };
    },

    async appPaths(): Promise<AppPathsDto> {
      record("app_paths");
      return paths;
    },

    async createProfile(name, targetScore): Promise<ProfileDto> {
      record("create_profile", { name, target_score: targetScore });
      const trimmed = name.trim();
      if (trimmed.length === 0)
        throw new Error("invalid request: a learner name is required");
      if (
        !Number.isInteger(targetScore) ||
        targetScore < 1 ||
        targetScore > 99
      ) {
        throw new Error(
          `invalid request: target score ${targetScore} is outside the 1..99 reporting scale`,
        );
      }
      const id = `learner-${nextId++}`;
      const profile = { id, name: trimmed, target_score: targetScore };
      state.profiles.set(id, profile);
      return profile;
    },

    async getProfile(id): Promise<ProfileDto> {
      record("get_profile", { id });
      const found = state.profiles.get(id);
      if (!found) throw new Error(`not found: profile ${id}`);
      return found;
    },

    async listProfiles(): Promise<ProfileDto[]> {
      record("list_profiles");
      return [...state.profiles.values()].sort((a, b) =>
        a.name.localeCompare(b.name),
      );
    },

    async recordAttempt(input): Promise<boolean> {
      record("record_attempt", {
        attempt_id: input.attemptId,
        learner_id: input.learnerId,
        subtest: input.subtest,
        question_id: input.questionId,
        correct: input.correct,
        latency_ms: input.latencyMs,
      });
      if (input.latencyMs < 0)
        throw new Error("invalid request: latency cannot be negative");
      if (!state.profiles.has(input.learnerId)) {
        throw new Error(`not found: profile ${input.learnerId}`);
      }
      if (state.attempts.has(input.attemptId)) return false;
      state.attempts.set(input.attemptId, {
        learnerId: input.learnerId,
        subtest: input.subtest,
        correct: input.correct,
        latencyMs: input.latencyMs,
      });
      return true;
    },

    async analytics(learnerId, subtest): Promise<AnalyticsDto> {
      record("analytics", { learner_id: learnerId, subtest });
      return stats(learnerId, subtest);
    },

    async analyticsAll(learnerId): Promise<Array<[string, AnalyticsDto]>> {
      record("analytics_all", { learner_id: learnerId });
      const codes = [
        ...new Set(
          [...state.attempts.values()]
            .filter((a) => a.learnerId === learnerId)
            .map((a) => a.subtest),
        ),
      ].sort();
      return codes.map((code) => [code, stats(learnerId, code)]);
    },

    async setMastery(learnerId, subtest, score, uncertainty): Promise<void> {
      record("set_mastery", {
        learner_id: learnerId,
        subtest,
        score,
        uncertainty,
      });
      state.mastery.set(`${learnerId}:${subtest}`, {
        subtest,
        score,
        uncertainty,
      });
    },

    async mastery(learnerId): Promise<MasteryDto[]> {
      record("mastery", { learner_id: learnerId });
      return [...state.mastery.entries()]
        .filter(([key]) => key.startsWith(`${learnerId}:`))
        .map(([, value]) => value);
    },

    async studyPlan(
      learnerId,
      targetScore,
      availableMinutes,
    ): Promise<PlanDto> {
      record("study_plan", {
        learner_id: learnerId,
        target_score: targetScore,
        available_minutes: availableMinutes,
      });
      if (availableMinutes <= 0) {
        throw new Error(
          "invalid request: available time must be greater than zero",
        );
      }
      if (!state.profiles.has(learnerId)) {
        throw new Error(`not found: profile ${learnerId}`);
      }
      // A fixture plan whose allocation is exact, so the view can be checked
      // against a plan that obeys the contract. The planner's own ranking rules
      // are verified in Rust, not here.
      const codes = ["AR", "WK"];
      const each = Math.floor(availableMinutes / codes.length);
      const drills = codes.map((subtest, index) => ({
        subtest,
        minutes:
          index === codes.length - 1
            ? availableMinutes - each * (codes.length - 1)
            : each,
        reason: index === 0 ? "weakness" : "due review",
      }));
      return { drills, total_minutes: availableMinutes };
    },

    async readiness(learnerId): Promise<ReadinessDto> {
      record("readiness", { learner_id: learnerId });
      if (options.readiness) return options.readiness;
      const rows = [...state.mastery.entries()].filter(([key]) =>
        key.startsWith(`${learnerId}:`),
      );
      if (rows.length === 0) {
        return { low: 0, high: 1, confidence: 0, official_score_claim: false };
      }
      const mastery = rows.reduce((s, [, m]) => s + m.score, 0) / rows.length;
      const uncertainty =
        rows.reduce((s, [, m]) => s + m.uncertainty, 0) / rows.length;
      const half = Math.min(1, uncertainty);
      return {
        low: Math.max(0, mastery - half),
        high: Math.min(1, mastery + half),
        confidence: Math.max(0, Math.min(0.95, 1 - half)),
        official_score_claim: false,
      };
    },

    async evidenceList(): Promise<EvidenceDto[]> {
      record("evidence_list");
      return [...state.evidence.values()].sort((a, b) =>
        b.created_at.localeCompare(a.created_at),
      );
    },

    async evidenceGet(id): Promise<EvidenceDto> {
      record("evidence_get", { id });
      const found = state.evidence.get(id);
      if (!found) throw new Error(`not found: evidence ${id}`);
      return found;
    },

    async evidencePut(input: NewEvidenceDto): Promise<string> {
      record("evidence_put", { input });
      if (!input.content_hash.trim()) {
        throw new Error("invalid request: a content hash is required");
      }
      const existing = [...state.evidence.values()].find(
        (row) => row.content_hash === input.content_hash,
      );
      if (existing) return existing.id;
      const id = `ev-${nextId++}`;
      state.evidence.set(id, {
        id,
        url: input.url,
        title: input.title,
        content_hash: input.content_hash,
        license: input.license,
        effective_date: input.effective_date,
        trust: input.trust,
        retrieval_status: input.retrieval_status,
        created_at: "2026-01-01T00:00:00Z",
      });
      return id;
    },

    async backupCreate(destDir): Promise<BackupDto> {
      record("backup_create", { dest_dir: destDir });
      const path = `${destDir}/vector-test-${state.backups.length + 1}.db`;
      const body = JSON.stringify({
        profiles: [...state.profiles.values()],
        attempts: [...state.attempts.entries()],
        mastery: [...state.mastery.entries()],
        evidence: [...state.evidence.values()],
      });
      const checksum = digest(body);
      state.backups.push({ path, checksum, bytes: body.length, body });
      return {
        path,
        checksum,
        bytes: body.length,
        integrity: "ok",
        encrypted: false,
        attempt_rows: state.attempts.size,
      };
    },

    async backupRestore(source, expectedChecksum): Promise<RestoreDto> {
      record("backup_restore", {
        source,
        expected_checksum: expectedChecksum,
      });
      const archive = state.backups.find((b) => b.path === source);
      if (!archive) throw new Error(`not found: backup ${source}`);
      if (
        options.verifyRestoreDigest !== false &&
        archive.checksum !== expectedChecksum
      ) {
        throw new Error(
          `backup at ${source} does not match the recorded digest (expected ${expectedChecksum}, got ${archive.checksum})`,
        );
      }
      const parsed = JSON.parse(archive.body) as {
        profiles: ProfileDto[];
        attempts: Array<
          [
            string,
            {
              learnerId: string;
              subtest: string;
              correct: boolean;
              latencyMs: number;
            },
          ]
        >;
        mastery: Array<[string, MasteryDto]>;
        evidence: EvidenceDto[];
      };
      state.profiles = new Map(parsed.profiles.map((p) => [p.id, p]));
      state.attempts = new Map(parsed.attempts);
      state.mastery = new Map(parsed.mastery);
      state.evidence = new Map(parsed.evidence.map((e) => [e.id, e]));
      return { rows: state.attempts.size };
    },

    async backupList(dir): Promise<BackupEntryDto[]> {
      record("backup_list", { dir });
      return state.backups
        .filter((b) => b.path.startsWith(dir))
        .map((b) => ({
          path: b.path,
          checksum: b.checksum,
          bytes: b.bytes,
          modified: "2026-01-01T00:00:00Z",
        }));
    },

    async latencyProbe(iterations): Promise<LatencyDto> {
      record("latency_probe", { iterations });
      return {
        operation: "select_count_learner_profile",
        iterations,
        mean_micros: 12,
        max_micros: 80,
      };
    },

    async resetLocalData(confirmation): Promise<ResetDto> {
      record("reset_local_data", { confirmation });
      if (confirmation !== "DELETE") {
        throw new Error(
          'invalid request: erasing local data requires the exact confirmation phrase "DELETE"',
        );
      }
      const result: ResetDto = {
        profiles_removed: state.profiles.size,
        attempts_removed: state.attempts.size,
        evidence_removed: state.evidence.size,
        profiles_remaining: 0,
        attempts_remaining: 0,
        evidence_remaining: 0,
      };
      state.profiles.clear();
      state.attempts.clear();
      state.mastery.clear();
      state.evidence.clear();
      return result;
    },

    async uiReady(bundle): Promise<string> {
      record("ui_ready", { bundle });
      const id = `ui-ready-${nextId++}`;
      state.uiReadyMarkers.push({ id, bundle });
      return id;
    },

    async recomputeMastery(learnerId): Promise<number> {
      record("recompute_mastery", { learner_id: learnerId });
      if (!state.profiles.has(learnerId)) {
        throw new Error(`not found: profile ${learnerId}`);
      }
      // The same Laplace estimate the service layer applies, so a test that
      // depends on the derived value checks the rule rather than a fixed number.
      let written = 0;
      const subtests = new Set(
        [...state.attempts.values()]
          .filter((a) => a.learnerId === learnerId)
          .map((a) => a.subtest),
      );
      for (const subtest of subtests) {
        const row = stats(learnerId, subtest);
        state.mastery.set(`${learnerId}:${subtest}`, {
          subtest,
          score: (row.correct + 1) / (row.total + 2),
          uncertainty: 1 / Math.sqrt(row.total + 1),
        });
        written += 1;
      }
      return written;
    },

    async contentGenerate(
      subtest: string,
      count: number,
      seed: number,
    ): Promise<GenerationReportDto> {
      record("content_generate", { subtest, count, seed });
      if (count < 1) {
        throw new Error("invalid request: count must be at least 1");
      }
      let activated = 0;
      let alreadyPresent = 0;
      for (let i = 0; i < count; i += 1) {
        // Deterministic from the seed, like the real factory, so a test that
        // repeats a call sees the same ids and the same `already_present`.
        const id = `q-${subtest}-${seed}-${i}`;
        if (state.items.has(id)) {
          alreadyPresent += 1;
          continue;
        }
        const answer = ((seed + i) % 9) + 1;
        state.items.set(id, {
          id,
          subtest,
          objective_id: `OBJ-${subtest}-TEST-01`,
          stem: `${subtest} generated item ${i} for seed ${seed}`,
          // A Paragraph Comprehension item always has a passage, because the store
          // refuses one that does not: the question refers to a text, so an item
          // without one is unanswerable. Every other subtest has none.
          passage:
            subtest === "PC"
              ? `The tower stood on the ridge for ${100 + i} years before the surveyors ` +
                "arrived. They measured its base and recorded the result in a log. " +
                "The log is kept in the county office to this day, and anyone may read it."
              : null,
          options: [
            String(answer),
            String(answer + 1),
            String(answer + 2),
            String(answer + 3),
          ],
          correct_index: 0,
          explanation: `${answer} + 0 = ${answer}`,
          distractor_rationales: {
            "1": "Added one too many.",
            "2": "Added two too many.",
            "3": "Added three too many.",
          },
          difficulty: 0,
        });
        activated += 1;
      }
      return {
        subtest,
        generated: count,
        verified: count,
        activated,
        already_present: alreadyPresent,
        rejected: [],
      };
    },

    async contentNext(
      subtest: string,
      seen: string[],
    ): Promise<ItemDto | null> {
      record("content_next", { subtest, seen });
      // Quarantined items are excluded, mirroring the backend's `servable`, which
      // reads `state = 'active'`. A fake that served them would let the practice
      // view pass its tests while handing a learner a withdrawn question.
      const candidates = [...state.items.values()].filter(
        (item) => item.subtest === subtest && !state.quarantined.has(item.id),
      );
      if (candidates.length === 0) {
        return null;
      }
      return (
        candidates.find((item) => !seen.includes(item.id)) ?? candidates[0]
      );
    },

    async contentManager(limit: number): Promise<ContentManagerDto> {
      record("content_manager", { limit });
      const stats = await this.contentStats();
      const items: ContentItemSummaryDto[] = [...state.items.values()]
        .slice(0, limit)
        .map((item) => ({
          id: item.id,
          subtest: item.subtest,
          state: state.quarantined.has(item.id) ? "quarantined" : "active",
          objective_id: item.objective_id,
          preview: item.stem.slice(0, 120),
          correct_answer: item.options[item.correct_index] ?? "",
          reviewer: "content-reviewer",
          content_hash: `sha256:${item.id}`,
          sources: ["ev-test-source"],
        }));
      return {
        stats,
        sources:
          state.items.size === 0
            ? []
            : [
                {
                  id: "ev-test-source",
                  title: "Test source",
                  url: "https://example.invalid/source",
                  licence: "Public domain in the USA",
                  trust: 0.9,
                  item_count: state.items.size,
                },
              ],
        items,
      };
    },

    async contentQuarantine(itemId, actor, reason): Promise<void> {
      record("content_quarantine", { item_id: itemId, actor, reason });
      if (!state.items.has(itemId)) {
        throw new Error(`not found: content item ${itemId}`);
      }
      if (state.quarantined.has(itemId)) {
        throw new Error(`${itemId} is already quarantined`);
      }
      state.quarantined.add(itemId);
      state.reviews.push({
        from_state: "active",
        to_state: "quarantined",
        actor,
        rationale: reason,
        created_at: "2026-09-22T00:00:00Z",
      });
    },

    async contentReinstate(itemId, actor, reason): Promise<void> {
      record("content_reinstate", { item_id: itemId, actor, reason });
      if (!state.quarantined.has(itemId)) {
        throw new Error(`${itemId} is not quarantined`);
      }
      state.quarantined.delete(itemId);
      state.reviews.push({
        from_state: "quarantined",
        to_state: "active",
        actor,
        rationale: reason,
        created_at: "2026-09-22T00:00:01Z",
      });
    },

    async contentHistory(itemId: string): Promise<ReviewEntryDto[]> {
      record("content_history", { item_id: itemId });
      return [...state.reviews];
    },

    async contentStats(): Promise<ContentStatsDto> {
      record("content_stats");
      const items = [...state.items.values()];
      // Mirror the backend, which counts `state = 'active'`. A fake that reported
      // every stored item as servable would let a view pass its tests while
      // showing a withdrawn question as available.
      const quarantined = [...state.quarantined].filter((id) =>
        state.items.has(id),
      ).length;
      const active = items.length - quarantined;
      const byState =
        items.length === 0
          ? []
          : [
              { state: "active", count: active },
              ...(quarantined > 0
                ? [{ state: "quarantined", count: quarantined }]
                : []),
            ];
      const subtests = [...new Set(items.map((item) => item.subtest))].sort();
      return {
        total: items.length,
        servable: active,
        sources: items.length === 0 ? 0 : 1,
        by_state: byState,
        by_subtest: subtests.map((subtest) => ({
          subtest,
          count: items.filter((item) => item.subtest === subtest).length,
        })),
      };
    },
  };

  return {
    client,
    state,
    calls,
    commands: () => calls.map((c) => c.command),
  };
}
