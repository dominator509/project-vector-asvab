/**
 * Typed client for the Tauri command boundary (SPEC-003).
 *
 * ## Why responses are validated rather than cast
 *
 * The webview and the Rust core are separate programs joined by a JSON channel.
 * A TypeScript cast (`as ProfileDto`) is a promise the compiler cannot keep
 * across that channel: if Rust renames a field or returns a null, the UI would
 * render `undefined` and the failure would surface as a blank screen rather than
 * an error. Every response is therefore checked against its expected shape, and
 * a mismatch becomes a `MalformedResponseError` naming the command.
 *
 * ## Why errors are typed
 *
 * Rust commands return `Err(String)` for refusals, which arrives as a rejected
 * promise. Collapsing that into a generic `Error` would lose the distinction
 * between "the learner typed something invalid" and "storage failed", and the
 * UI needs to show those differently.
 */

import type {
  AnalyticsDto,
  AppPathsDto,
  BackupDto,
  BackupEntryDto,
  EvidenceDto,
  HealthDto,
  Invoke,
  LatencyDto,
  MasteryDto,
  NewEvidenceDto,
  PlanDto,
  ProfileDto,
  ReadinessDto,
  ResetDto,
  RestoreDto,
} from "./types";

/** Every command this client will call. Kept explicit so a typo is a test failure. */
export const COMMAND_NAMES = [
  "app_paths",
  "health",
  "create_profile",
  "get_profile",
  "list_profiles",
  "record_attempt",
  "analytics",
  "analytics_all",
  "set_mastery",
  "mastery",
  "study_plan",
  "readiness",
  "evidence_list",
  "evidence_get",
  "evidence_put",
  "backup_create",
  "backup_restore",
  "backup_list",
  "latency_probe",
  "reset_local_data",
  "ui_ready",
  "recompute_mastery",
] as const;

export type CommandName = (typeof COMMAND_NAMES)[number];

// Re-exported so callers have one import site for the boundary contract.
export {
  BackendError,
  BackendUnavailableError,
  MalformedResponseError,
} from "./errors";

import { BackendError, MalformedResponseError } from "./errors";

// ---------------------------------------------------------------------------
// Shape checks
// ---------------------------------------------------------------------------

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

class Reader {
  constructor(
    private readonly command: string,
    private readonly value: unknown,
  ) {}

  private expect(ok: boolean, what: string): void {
    if (!ok) {
      throw new MalformedResponseError(
        this.command,
        `expected ${what}, got ${describe(this.value)}`,
      );
    }
  }

  object(): Record<string, unknown> {
    this.expect(isRecord(this.value), "an object");
    return this.value as Record<string, unknown>;
  }

  array(): unknown[] {
    this.expect(Array.isArray(this.value), "an array");
    return this.value as unknown[];
  }

  string(source: Record<string, unknown>, key: string): string {
    const field = source[key];
    if (typeof field !== "string") {
      throw new MalformedResponseError(
        this.command,
        `field "${key}" must be a string, got ${describe(field)}`,
      );
    }
    return field;
  }

  number(source: Record<string, unknown>, key: string): number {
    const field = source[key];
    if (typeof field !== "number" || !Number.isFinite(field)) {
      throw new MalformedResponseError(
        this.command,
        `field "${key}" must be a finite number, got ${describe(field)}`,
      );
    }
    return field;
  }

  boolean(source: Record<string, unknown>, key: string): boolean {
    const field = source[key];
    if (typeof field !== "boolean") {
      throw new MalformedResponseError(
        this.command,
        `field "${key}" must be a boolean, got ${describe(field)}`,
      );
    }
    return field;
  }
}

function describe(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return "an array";
  return typeof value;
}

function readHealth(command: string, value: unknown): HealthDto {
  const r = new Reader(command, value);
  const o = r.object();
  return {
    component: r.string(o, "component"),
    healthy: r.boolean(o, "healthy"),
    basis: r.string(o, "basis"),
    profiles: r.number(o, "profiles"),
  };
}

function readProfile(command: string, value: unknown): ProfileDto {
  const r = new Reader(command, value);
  const o = r.object();
  return {
    id: r.string(o, "id"),
    name: r.string(o, "name"),
    target_score: r.number(o, "target_score"),
  };
}

function readProfileList(command: string, value: unknown): ProfileDto[] {
  const r = new Reader(command, value);
  return r.array().map((entry) => readProfile(command, entry));
}

function readAnalytics(command: string, value: unknown): AnalyticsDto {
  const r = new Reader(command, value);
  const o = r.object();
  return {
    total: r.number(o, "total"),
    correct: r.number(o, "correct"),
    accuracy: r.number(o, "accuracy"),
    mean_latency_ms: r.number(o, "mean_latency_ms"),
  };
}

function readMasteryList(command: string, value: unknown): MasteryDto[] {
  const r = new Reader(command, value);
  return r.array().map((entry) => {
    const row = new Reader(command, entry).object();
    return {
      subtest: r.string(row, "subtest"),
      score: r.number(row, "score"),
      uncertainty: r.number(row, "uncertainty"),
    };
  });
}

function readPlan(command: string, value: unknown): PlanDto {
  const r = new Reader(command, value);
  const o = r.object();
  const drills = new Reader(command, o.drills).array().map((entry) => {
    const row = new Reader(command, entry).object();
    return {
      subtest: r.string(row, "subtest"),
      minutes: r.number(row, "minutes"),
      reason: r.string(row, "reason"),
    };
  });
  return { drills, total_minutes: r.number(o, "total_minutes") };
}

function readReadiness(command: string, value: unknown): ReadinessDto {
  const r = new Reader(command, value);
  const o = r.object();
  const low = r.number(o, "low");
  const high = r.number(o, "high");
  const confidence = r.number(o, "confidence");
  const claim = r.boolean(o, "official_score_claim");

  // ADR-010 is enforced at the boundary, not only in the view. A backend that
  // ever returned a claim, or an inverted band, must not reach the renderer.
  if (claim) {
    throw new MalformedResponseError(
      command,
      "official_score_claim was true, which ADR-010 forbids",
    );
  }
  if (low > high) {
    throw new MalformedResponseError(
      command,
      `readiness band is inverted (low ${low} > high ${high})`,
    );
  }
  return { low, high, confidence, official_score_claim: claim };
}

function readEvidenceList(command: string, value: unknown): EvidenceDto[] {
  const r = new Reader(command, value);
  return r.array().map((entry) => readEvidence(command, entry));
}

function readEvidence(command: string, value: unknown): EvidenceDto {
  const r = new Reader(command, value);
  const o = r.object();
  return {
    id: r.string(o, "id"),
    content_hash: r.string(o, "content_hash"),
    url: r.string(o, "url"),
    title: r.string(o, "title"),
    license: r.string(o, "license"),
    effective_date: r.string(o, "effective_date"),
    trust: r.number(o, "trust"),
    retrieval_status: r.string(o, "retrieval_status"),
    created_at: r.string(o, "created_at"),
  };
}

function readBackup(command: string, value: unknown): BackupDto {
  const r = new Reader(command, value);
  const o = r.object();
  return {
    path: r.string(o, "path"),
    checksum: r.string(o, "checksum"),
    bytes: r.number(o, "bytes"),
    integrity: r.string(o, "integrity"),
    encrypted: r.boolean(o, "encrypted"),
    attempt_rows: r.number(o, "attempt_rows"),
  };
}

function readRestore(command: string, value: unknown): RestoreDto {
  const r = new Reader(command, value);
  return { rows: r.number(r.object(), "rows") };
}

function readLatency(command: string, value: unknown): LatencyDto {
  const r = new Reader(command, value);
  const o = r.object();
  return {
    operation: r.string(o, "operation"),
    iterations: r.number(o, "iterations"),
    mean_micros: r.number(o, "mean_micros"),
    max_micros: r.number(o, "max_micros"),
  };
}

function readAppPaths(command: string, value: unknown): AppPathsDto {
  const r = new Reader(command, value);
  const o = r.object();
  return {
    data_dir: r.string(o, "data_dir"),
    db_path: r.string(o, "db_path"),
    backup_dir: r.string(o, "backup_dir"),
  };
}

function readBackupEntryList(
  command: string,
  value: unknown,
): BackupEntryDto[] {
  const r = new Reader(command, value);
  return r.array().map((entry) => {
    const o = new Reader(command, entry).object();
    return {
      path: r.string(o, "path"),
      checksum: r.string(o, "checksum"),
      bytes: r.number(o, "bytes"),
      modified: r.string(o, "modified"),
    };
  });
}

function readReset(command: string, value: unknown): ResetDto {
  const r = new Reader(command, value);
  const o = r.object();
  return {
    profiles_removed: r.number(o, "profiles_removed"),
    attempts_removed: r.number(o, "attempts_removed"),
    evidence_removed: r.number(o, "evidence_removed"),
    profiles_remaining: r.number(o, "profiles_remaining"),
    attempts_remaining: r.number(o, "attempts_remaining"),
    evidence_remaining: r.number(o, "evidence_remaining"),
  };
}

function readAnalyticsPairs(
  command: string,
  value: unknown,
): Array<[string, AnalyticsDto]> {
  const r = new Reader(command, value);
  return r.array().map((entry) => {
    const pair = new Reader(command, entry).array();
    if (pair.length !== 2) {
      throw new MalformedResponseError(
        command,
        `expected a [subtest, analytics] pair, got ${pair.length} element(s)`,
      );
    }
    const code = pair[0];
    if (typeof code !== "string") {
      throw new MalformedResponseError(
        command,
        `expected a subtest code string, got ${describe(code)}`,
      );
    }
    return [code, readAnalytics(command, pair[1])] as [string, AnalyticsDto];
  });
}

function readBoolean(command: string, value: unknown): boolean {
  if (typeof value !== "boolean") {
    throw new MalformedResponseError(
      command,
      `expected a boolean, got ${describe(value)}`,
    );
  }
  return value;
}

function readIdString(command: string, value: unknown): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new MalformedResponseError(
      command,
      `expected a non-empty id string, got ${describe(value)}`,
    );
  }
  return value;
}

function readRowCount(command: string, value: unknown): number {
  if (typeof value !== "number" || !Number.isInteger(value) || value < 0) {
    throw new MalformedResponseError(
      command,
      `expected a non-negative integer row count, got ${describe(value)}`,
    );
  }
  return value;
}

// ---------------------------------------------------------------------------
// The client
// ---------------------------------------------------------------------------

export interface VectorClient {
  health(): Promise<HealthDto>;
  createProfile(name: string, targetScore: number): Promise<ProfileDto>;
  getProfile(id: string): Promise<ProfileDto>;
  listProfiles(): Promise<ProfileDto[]>;
  recordAttempt(input: {
    attemptId: string;
    learnerId: string;
    subtest: string;
    questionId: string;
    correct: boolean;
    latencyMs: number;
  }): Promise<boolean>;
  analytics(learnerId: string, subtest: string): Promise<AnalyticsDto>;
  analyticsAll(learnerId: string): Promise<Array<[string, AnalyticsDto]>>;
  setMastery(
    learnerId: string,
    subtest: string,
    score: number,
    uncertainty: number,
  ): Promise<void>;
  mastery(learnerId: string): Promise<MasteryDto[]>;
  studyPlan(
    learnerId: string,
    targetScore: number,
    availableMinutes: number,
  ): Promise<PlanDto>;
  readiness(learnerId: string): Promise<ReadinessDto>;
  evidenceList(): Promise<EvidenceDto[]>;
  evidenceGet(id: string): Promise<EvidenceDto>;
  evidencePut(input: NewEvidenceDto): Promise<string>;
  backupCreate(destDir: string): Promise<BackupDto>;
  backupRestore(source: string, expectedChecksum: string): Promise<RestoreDto>;
  backupList(dir: string): Promise<BackupEntryDto[]>;
  latencyProbe(iterations: number): Promise<LatencyDto>;
  appPaths(): Promise<AppPathsDto>;
  resetLocalData(confirmation: string): Promise<ResetDto>;
  /**
   * Record that this bundle reached the Rust command layer.
   *
   * Resolves to the id of the stored marker, or rejects if the boundary is not
   * answering. Callers must not treat a rejection as harmless.
   */
  uiReady(bundle: string): Promise<string>;
  /**
   * Recompute a learner's mastery estimates from their stored attempts.
   *
   * Resolves to the number of mastery rows written.
   */
  recomputeMastery(learnerId: string): Promise<number>;
}

/**
 * Wrap an `invoke` implementation in the typed client.
 *
 * The raw transport is injected rather than imported so that the client can be
 * tested without a Tauri runtime, which is what makes the command names and
 * argument names verifiable.
 */
export function createVectorClient(invoke: Invoke): VectorClient {
  async function call<T>(
    command: CommandName,
    args: Record<string, unknown> | undefined,
    read: (command: string, value: unknown) => T,
  ): Promise<T> {
    let raw: unknown;
    try {
      raw = await invoke(command, args);
    } catch (error) {
      // Tauri rejects with the command's `Err(String)` payload.
      const message =
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : String(error);
      throw new BackendError(command, message);
    }
    return read(command, raw);
  }

  return {
    health: () => call("health", undefined, readHealth),

    createProfile: (name, targetScore) =>
      call("create_profile", { name, target_score: targetScore }, readProfile),

    getProfile: (id) => call("get_profile", { id }, readProfile),

    listProfiles: () => call("list_profiles", undefined, readProfileList),

    recordAttempt: (input) =>
      call(
        "record_attempt",
        {
          attempt_id: input.attemptId,
          learner_id: input.learnerId,
          subtest: input.subtest,
          question_id: input.questionId,
          correct: input.correct,
          latency_ms: input.latencyMs,
        },
        readBoolean,
      ),

    analytics: (learnerId, subtest) =>
      call("analytics", { learner_id: learnerId, subtest }, readAnalytics),

    analyticsAll: (learnerId) =>
      call("analytics_all", { learner_id: learnerId }, readAnalyticsPairs),

    setMastery: (learnerId, subtest, score, uncertainty) =>
      call(
        "set_mastery",
        { learner_id: learnerId, subtest, score, uncertainty },
        () => undefined as void,
      ),

    mastery: (learnerId) =>
      call("mastery", { learner_id: learnerId }, readMasteryList),

    studyPlan: (learnerId, targetScore, availableMinutes) =>
      call(
        "study_plan",
        {
          learner_id: learnerId,
          target_score: targetScore,
          available_minutes: availableMinutes,
        },
        readPlan,
      ),

    readiness: (learnerId) =>
      call("readiness", { learner_id: learnerId }, readReadiness),

    evidenceList: () => call("evidence_list", undefined, readEvidenceList),

    evidenceGet: (id) => call("evidence_get", { id }, readEvidence),

    evidencePut: (input) => call("evidence_put", { input }, readIdString),

    backupCreate: (destDir) =>
      call("backup_create", { dest_dir: destDir }, readBackup),

    backupRestore: (source, expectedChecksum) =>
      call(
        "backup_restore",
        { source, expected_checksum: expectedChecksum },
        readRestore,
      ),

    latencyProbe: (iterations) =>
      call("latency_probe", { iterations }, readLatency),

    appPaths: () => call("app_paths", undefined, readAppPaths),

    backupList: (dir) => call("backup_list", { dir }, readBackupEntryList),

    resetLocalData: (confirmation) =>
      call("reset_local_data", { confirmation }, readReset),

    uiReady: (bundle) => call("ui_ready", { bundle }, readIdString),

    recomputeMastery: (learnerId) =>
      call("recompute_mastery", { learner_id: learnerId }, readRowCount),
  };
}
