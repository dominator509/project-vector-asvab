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
