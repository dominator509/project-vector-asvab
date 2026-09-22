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
  ContentItemSummaryDto,
  ContentManagerDto,
  ContentStatsDto,
  EvidenceDto,
  GenerationReportDto,
  HealthDto,
  Invoke,
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
  SourceDto,
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
  "content_generate",
  "content_next",
  "content_stats",
  "content_manager",
  "content_quarantine",
  "content_reinstate",
  "content_history",
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

  /**
   * A field that is either a string or an explicit null.
   *
   * Absence is rejected. The backend always sends the field, so a missing one means
   * the response is not the shape the reader was written for, and quietly treating
   * that as "no value" would hide a contract break behind a plausible-looking
   * result.
   */
  nullableString(source: Record<string, unknown>, key: string): string | null {
    const field = source[key];
    if (field === null) {
      return null;
    }
    if (typeof field !== "string") {
      throw new MalformedResponseError(
        this.command,
        `field "${key}" must be a string or null, got ${describe(field)}`,
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
// Content
// ---------------------------------------------------------------------------

/**
 * Read one served item, and refuse one that cannot be answered.
 *
 * `correct_index` addressing an existing option is checked here rather than in
 * the view. A view that reads `options[correct_index]` on an out-of-range index
 * renders a question whose right answer is `undefined`, which looks like a
 * content problem and is really a boundary problem. This mirrors the schema's
 * own `CHECK (correct_index < json_array_length(options_json))`, so a backend
 * that somehow bypassed it still cannot reach the renderer.
 */
function readItem(command: string, value: unknown): ItemDto {
  const r = new Reader(command, value);
  const o = r.object();
  const options = new Reader(command, o.options).array().map((entry) => {
    if (typeof entry !== "string") {
      throw new MalformedResponseError(
        command,
        `option must be a string, got ${describe(entry)}`,
      );
    }
    return entry;
  });
  const correctIndex = r.number(o, "correct_index");

  if (options.length < 2) {
    throw new MalformedResponseError(
      command,
      `an item needs at least 2 options, got ${options.length}`,
    );
  }
  if (
    !Number.isInteger(correctIndex) ||
    correctIndex < 0 ||
    correctIndex >= options.length
  ) {
    throw new MalformedResponseError(
      command,
      `correct_index ${correctIndex} does not address any of the ${options.length} options`,
    );
  }

  const rawRationales = new Reader(command, o.distractor_rationales).object();
  const distractorRationales: Record<string, string> = {};
  for (const [key, entry] of Object.entries(rawRationales)) {
    if (typeof entry !== "string") {
      throw new MalformedResponseError(
        command,
        `distractor rationale ${key} must be a string, got ${describe(entry)}`,
      );
    }
    distractorRationales[key] = entry;
  }

  return {
    id: r.string(o, "id"),
    subtest: r.string(o, "subtest"),
    objective_id: r.string(o, "objective_id"),
    stem: r.string(o, "stem"),
    passage: r.nullableString(o, "passage"),
    options,
    correct_index: correctIndex,
    explanation: r.string(o, "explanation"),
    distractor_rationales: distractorRationales,
    difficulty: r.number(o, "difficulty"),
  };
}

/**
 * Read the next item, which the backend represents as `null` when the corpus
 * has nothing to serve. `undefined` would mean the field was missing, and those
 * are different facts, so only an explicit null is accepted.
 */
function readOptionalItem(command: string, value: unknown): ItemDto | null {
  if (value === null) {
    return null;
  }
  if (value === undefined) {
    throw new MalformedResponseError(
      command,
      "expected an item or null; the field was absent entirely",
    );
  }
  return readItem(command, value);
}

function readContentStats(command: string, value: unknown): ContentStatsDto {
  const r = new Reader(command, value);
  const o = r.object();
  const byState = new Reader(command, o.by_state).array().map((entry) => {
    const row = new Reader(command, entry).object();
    return { state: r.string(row, "state"), count: r.number(row, "count") };
  });
  const bySubtest = new Reader(command, o.by_subtest).array().map((entry) => {
    const row = new Reader(command, entry).object();
    return { subtest: r.string(row, "subtest"), count: r.number(row, "count") };
  });
  return {
    total: r.number(o, "total"),
    servable: r.number(o, "servable"),
    sources: r.number(o, "sources"),
    by_state: byState,
    by_subtest: bySubtest,
  };
}

function readGenerationReport(
  command: string,
  value: unknown,
): GenerationReportDto {
  const r = new Reader(command, value);
  const o = r.object();
  const rejected = new Reader(command, o.rejected).array().map((entry) => {
    if (typeof entry !== "string") {
      throw new MalformedResponseError(
        command,
        `rejection must be a string, got ${describe(entry)}`,
      );
    }
    return entry;
  });
  const activated = r.number(o, "activated");
  const verified = r.number(o, "verified");

  // Verification happens before storage, so an item cannot be activated without
  // having verified. A report claiming otherwise would mean the pipeline order
  // was reversed, and the content manager would then show a reassuring number
  // over content that was never checked.
  if (activated > verified) {
    throw new MalformedResponseError(
      command,
      `activated ${activated} exceeds verified ${verified}, which the pipeline cannot produce`,
    );
  }

  return {
    subtest: r.string(o, "subtest"),
    generated: r.number(o, "generated"),
    verified,
    activated,
    already_present: r.number(o, "already_present"),
    rejected,
  };
}

// ---------------------------------------------------------------------------
// Content manager
// ---------------------------------------------------------------------------

/**
 * A command that reports success by resolving and carries no payload.
 *
 * The Rust side returns `Result<(), String>`, which serialises to `null`. Anything
 * else means the command answered with something this client does not understand,
 * and accepting it silently would let a changed response go unnoticed.
 */
function readVoid(command: string, value: unknown): void {
  if (value !== null && value !== undefined) {
    throw new MalformedResponseError(
      command,
      `expected no payload, got ${describe(value)}`,
    );
  }
}

function readSource(command: string, value: unknown): SourceDto {
  const r = new Reader(command, value);
  const o = r.object();
  const licence = r.string(o, "licence");
  // A source with no licence recorded is not something a reviewer can act on, and
  // the backend deliberately records licence strings rather than inferring them.
  // An empty one means the provenance record is incomplete, not that the source is
  // unencumbered.
  if (licence.trim() === "") {
    throw new MalformedResponseError(
      command,
      "a corpus source has no licence recorded, so its terms are unknown",
    );
  }
  return {
    id: r.string(o, "id"),
    title: r.string(o, "title"),
    url: r.string(o, "url"),
    licence,
    trust: r.number(o, "trust"),
    item_count: r.number(o, "item_count"),
  };
}

function readContentItemSummary(
  command: string,
  value: unknown,
): ContentItemSummaryDto {
  const r = new Reader(command, value);
  const o = r.object();
  const sources = new Reader(command, o.sources).array().map((entry) => {
    if (typeof entry !== "string") {
      throw new MalformedResponseError(
        command,
        `a cited source must be a string, got ${describe(entry)}`,
      );
    }
    return entry;
  });
  return {
    id: r.string(o, "id"),
    subtest: r.string(o, "subtest"),
    state: r.string(o, "state"),
    objective_id: r.string(o, "objective_id"),
    preview: r.string(o, "preview"),
    correct_answer: r.string(o, "correct_answer"),
    reviewer: r.string(o, "reviewer"),
    content_hash: r.string(o, "content_hash"),
    sources,
  };
}

function readContentManager(
  command: string,
  value: unknown,
): ContentManagerDto {
  const r = new Reader(command, value);
  const o = r.object();
  const items = new Reader(command, o.items)
    .array()
    .map((entry) => readContentItemSummary(command, entry));
  const sources = new Reader(command, o.sources)
    .array()
    .map((entry) => readSource(command, entry));

  // Every listed item must cite a source the payload also carries: an item whose
  // citation is not in the source list is provenance the manager cannot show, and
  // the view would render an empty cell that looks like a layout bug.
  const known = new Set(sources.map((source) => source.id));
  for (const item of items) {
    for (const cited of item.sources) {
      if (!known.has(cited)) {
        throw new MalformedResponseError(
          command,
          `item ${item.id} cites ${cited}, which is not among the listed sources`,
        );
      }
    }
  }

  return {
    stats: readContentStats(command, o.stats),
    sources,
    items,
  };
}

function readReviewHistory(command: string, value: unknown): ReviewEntryDto[] {
  return new Reader(command, value).array().map((entry) => {
    const r = new Reader(command, entry);
    const o = r.object();
    return {
      from_state: r.string(o, "from_state"),
      to_state: r.string(o, "to_state"),
      actor: r.string(o, "actor"),
      rationale: r.string(o, "rationale"),
      created_at: r.string(o, "created_at"),
    };
  });
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
  /**
   * Generate original items for a subtest and activate the ones that prove out.
   *
   * Idempotent for a given seed: repeating the call reports the items as
   * `already_present` and writes nothing.
   */
  contentGenerate(
    subtest: string,
    count: number,
    seed: number,
  ): Promise<GenerationReportDto>;
  /** The next item to practise, or `null` when the corpus has nothing to serve. */
  contentNext(subtest: string, seen: string[]): Promise<ItemDto | null>;
  /** How much content this installation holds. */
  contentStats(): Promise<ContentStatsDto>;
  /** Everything the content manager draws, in one payload. */
  contentManager(limit: number): Promise<ContentManagerDto>;
  /** Withdraw an item from service, recording who did it and why. */
  contentQuarantine(
    itemId: string,
    actor: string,
    reason: string,
  ): Promise<void>;
  /** Return a quarantined item to service along the documented pipeline. */
  contentReinstate(
    itemId: string,
    actor: string,
    reason: string,
  ): Promise<void>;
  /** One item's audit trail. */
  contentHistory(itemId: string): Promise<ReviewEntryDto[]>;
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

    contentGenerate: (subtest, count, seed) =>
      call("content_generate", { subtest, count, seed }, readGenerationReport),

    contentNext: (subtest, seen) =>
      call("content_next", { subtest, seen }, readOptionalItem),

    contentStats: () => call("content_stats", undefined, readContentStats),

    contentManager: (limit) =>
      call("content_manager", { limit }, readContentManager),

    contentQuarantine: (itemId, actor, reason) =>
      call("content_quarantine", { item_id: itemId, actor, reason }, readVoid),

    contentReinstate: (itemId, actor, reason) =>
      call("content_reinstate", { item_id: itemId, actor, reason }, readVoid),

    contentHistory: (itemId) =>
      call("content_history", { item_id: itemId }, readReviewHistory),
  };
}
