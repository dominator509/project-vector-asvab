/**
 * Wire types for the desktop command boundary.
 *
 * These mirror the DTOs in `crates/vector-application/src/service.rs` and the
 * command signatures in `apps/desktop/src-tauri/src/commands.rs`. Field names
 * are snake_case because that is what `serde` emits and what Tauri's argument
 * resolver looks up; renaming them here would silently produce `undefined`
 * rather than a compile error, so the names are kept identical on purpose.
 *
 * `apps/desktop/src-tauri/tests/command_boundary.rs` is the executable half of
 * this contract: change a Rust signature without changing this file and the
 * Rust tests still pass, but `ipc/client.test.ts` fails against the recorded
 * command surface.
 */

/** The `invoke` shape, so the client can be driven by a fake in tests. */
export type Invoke = (
  command: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

export interface HealthDto {
  component: string;
  healthy: boolean;
  basis: string;
  profiles: number;
}

export interface ProfileDto {
  id: string;
  name: string;
  target_score: number;
}

export interface AnalyticsDto {
  total: number;
  correct: number;
  accuracy: number;
  mean_latency_ms: number;
}

export interface MasteryDto {
  subtest: string;
  score: number;
  uncertainty: number;
}

export interface DrillDto {
  subtest: string;
  minutes: number;
  reason: string;
  /**
   * The objective inside that subtest to work on, when the installed pack declares one whose
   * prerequisites are met. Null on a device with no pack: it has no curriculum to name one from.
   */
  objective_id: string | null;
}

export interface PlanDto {
  drills: DrillDto[];
  total_minutes: number;
}

export interface ReadinessDto {
  low: number;
  high: number;
  confidence: number;
  /** Always false in this version; ADR-010 forbids a precise score claim. */
  official_score_claim: boolean;
}

export interface EvidenceDto {
  id: string;
  content_hash: string;
  url: string;
  title: string;
  license: string;
  effective_date: string;
  trust: number;
  retrieval_status: string;
  created_at: string;
}

export interface NewEvidenceDto {
  url: string;
  title: string;
  content_hash: string;
  license: string;
  effective_date: string;
  trust: number;
  retrieval_status: string;
}

export interface BackupDto {
  path: string;
  checksum: string;
  bytes: number;
  integrity: string;
  encrypted: boolean;
  attempt_rows: number;
}

export interface RestoreDto {
  rows: number;
}

/** An archive on disk, offered for restore. */
export interface BackupEntryDto {
  path: string;
  checksum: string;
  bytes: number;
  modified: string;
}

/** The outcome of erasing all local learner data. */
export interface ResetDto {
  profiles_removed: number;
  attempts_removed: number;
  evidence_removed: number;
  profiles_remaining: number;
  attempts_remaining: number;
  evidence_remaining: number;
}

/** Where this installation keeps its files. */
export interface AppPathsDto {
  data_dir: string;
  db_path: string;
  backup_dir: string;
}

export interface LatencyDto {
  operation: string;
  iterations: number;
  mean_micros: number;
  max_micros: number;
}

/**
 * A generated practice item, as served.
 *
 * `correct_index` is present because grading happens locally. It is not a
 * secret: this is a study tool on the learner's own machine.
 */
export interface ItemDto {
  id: string;
  subtest: string;
  /** The versioned learning objective this item serves (REQ-056). */
  objective_id: string;
  stem: string;
  /**
   * The passage the item is about, for Paragraph Comprehension; `null` for every
   * other subtest.
   *
   * Explicitly nullable rather than optional: the backend always sends the field,
   * and "this subtest has no passage" is a fact the interface can state, whereas a
   * missing field would be indistinguishable from a malformed response.
   */
  passage: string | null;
  options: string[];
  correct_index: number;
  /** The worked answer. For a generated item this is its proof expression. */
  explanation: string;
  /**
   * Why each wrong option is wrong, keyed by option index. A generated item
   * always names a misconception for every distractor it offers.
   *
   * JSON object keys are strings on the wire, so the runtime keys are strings
   * even though the indices are numeric.
   */
  distractor_rationales: Record<string, string>;
  difficulty: number;
}

export interface StateCountDto {
  state: string;
  count: number;
}

export interface SubtestCountDto {
  subtest: string;
  count: number;
}

/** How much content this installation holds. */
export interface ContentStatsDto {
  total: number;
  servable: number;
  sources: number;
  by_state: StateCountDto[];
  by_subtest: SubtestCountDto[];
}

/** What one generation run did. */
export interface GenerationReportDto {
  subtest: string;
  generated: number;
  verified: number;
  activated: number;
  already_present: number;
  /** One line per refused item; empty on a clean run. */
  rejected: string[];
}

/** A source the corpus rests on. */
export interface SourceDto {
  id: string;
  title: string;
  url: string;
  /**
   * The terms the source may be used on. Shown because it is the question a
   * reviewer actually has: not "which file" but "on what terms may this be here".
   */
  licence: string;
  trust: number;
  /** How many items cite this source, active or not. */
  item_count: number;
}

/** An item as the content manager lists it. */
export interface ContentItemSummaryDto {
  id: string;
  subtest: string;
  state: string;
  objective_id: string;
  preview: string;
  correct_answer: string;
  reviewer: string;
  content_hash: string;
  sources: string[];
}

/** One entry in an item's audit trail. */
export interface ReviewEntryDto {
  from_state: string;
  to_state: string;
  actor: string;
  rationale: string;
  created_at: string;
}

/** Everything the content manager draws, in one payload. */
export interface ContentManagerDto {
  stats: ContentStatsDto;
  sources: SourceDto[];
  items: ContentItemSummaryDto[];
}

/**
 * A signed content pack as the registry holds it.
 *
 * `signature_valid` is the field a reader has to see: a pack whose stored identity no
 * longer verifies against the key that signed it is one nobody should act on, and
 * hiding that behind a status of "active" would present a broken attestation as a good
 * one.
 */
export interface InstalledPackDto {
  id: string;
  name: string;
  version: number;
  status: string;
  signer: string;
  content_hash: string;
  schema_version: number;
  item_count: number;
  created_at: string;
  signature_valid: boolean;
  /**
   * What the pack teaches, read out of the manifest it was installed with.
   *
   * Empty for a pack whose manifest predates the curriculum, which is why the view says
   * "declares no curriculum" rather than rendering an empty list as if it were one.
   */
  objectives: PackObjectiveDto[];
}

/** One objective a pack teaches, and what it claims about its difficulty. */
export interface PackObjectiveDto {
  objective_id: string;
  subtest: string;
  title: string;
  prerequisites: string[];
  /**
   * The declared share expected to answer correctly.
   *
   * `responses` is what says whether that figure was measured: zero means the pack declared it
   * from the material rather than observing learners, and the view says so.
   */
  expected_correct: number | null;
  responses: number | null;
  basis: string | null;
}

/** What installing a pack delivered. */
export interface InstallReportDto {
  name: string;
  version: number;
  content_hash: string;
  items: number;
  installed: number;
  already_present: number;
  sources_added: number;
  sources_reused: number;
}
