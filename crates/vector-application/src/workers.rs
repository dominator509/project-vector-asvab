//! Background workers with serialized database access (REQ-054).
//!
//! `REQ-054` asks for "idempotent attempts and safe DB/background workers". The
//! idempotency half is enforced by the `attempts` primary key and covered by the
//! persistence tests. This module is the other half: work that happens away from
//! the request that triggered it, without corrupting the database or losing a
//! write.
//!
//! ## Why the workers open their own connections
//!
//! Each worker opens its own SQLite connection rather than sharing the UI's
//! handle. The shared-handle alternative — one connection behind a `Mutex` —
//! serializes *reads* against writes for no benefit, and it hides the real
//! concurrency question. Separate connections put the burden where it belongs:
//! WAL plus a busy timeout must actually let a reader make progress while a
//! writer holds the write lock, and if that fails the tests here fail.
//!
//! ## What a worker is allowed to do
//!
//! A job is a closed enum, not a closure. A generic "run this in the background"
//! API is how a background thread ends up doing something no test exercises, and
//! it would make the audit trail ("what ran?") unanswerable.
//!
//! Every outcome is returned to the caller and recorded, so a job that silently
//! did nothing is visible rather than inferred from an absence of errors.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::service::{ServiceError, Services};
use vector_persistence::Database;

/// A unit of background work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Job {
    /// Recompute one learner's mastery estimates from their stored attempts.
    ///
    /// This is what keeps mastery a *derived* fact rather than something only a
    /// manual write can change: the planner reads mastery, so a stale value
    /// silently misdirects study time.
    RecomputeMastery { learner_id: String },
    /// Recompute mastery for every learner on this installation.
    RecomputeAllMastery,
}

impl Job {
    /// A short name for logs and reports.
    pub fn name(&self) -> &'static str {
        match self {
            Job::RecomputeMastery { .. } => "recompute_mastery",
            Job::RecomputeAllMastery => "recompute_all_mastery",
        }
    }
}

/// What happened to one job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobStatus {
    /// The job ran and changed this many mastery rows.
    Completed { mastery_rows_written: usize },
    /// The job ran and there was nothing to do. Distinct from `Completed` with
    /// zero rows so "no learners" is not confused with "no attempts".
    NoWork { reason: String },
    /// The job failed. Carries the message the service returned.
    Failed { message: String },
}

/// The record of one job execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobOutcome {
    pub job: String,
    pub status: JobStatus,
    pub duration_ms: u128,
}

/// Recompute mastery for one learner and report how many rows were written.
///
/// Exposed as a free function because the desktop command calls it directly:
/// the worker is a scheduler over real behaviour, not a second implementation.
pub fn recompute_mastery(db: &Database, learner_id: &str) -> Result<usize, ServiceError> {
    Services::new(db).recompute_mastery(learner_id)
}

/// Run one job against `db`.
pub fn run_job(db: &Database, job: &Job) -> JobOutcome {
    let started = Instant::now();

    let status = match job {
        Job::RecomputeMastery { learner_id } => match recompute_mastery(db, learner_id) {
            Ok(rows) => JobStatus::Completed {
                mastery_rows_written: rows,
            },
            Err(ServiceError::NotFound(_)) => JobStatus::NoWork {
                reason: format!("no learner {learner_id}"),
            },
            Err(error) => JobStatus::Failed {
                message: error.to_string(),
            },
        },
        Job::RecomputeAllMastery => match Services::new(db).list_profiles() {
            Ok(profiles) if profiles.is_empty() => JobStatus::NoWork {
                reason: "no learners on this installation".to_string(),
            },
            Ok(profiles) => {
                let mut written = 0usize;
                for profile in &profiles {
                    match recompute_mastery(db, &profile.id) {
                        Ok(rows) => written += rows,
                        Err(error) => {
                            return JobOutcome {
                                job: job.name().to_string(),
                                status: JobStatus::Failed {
                                    message: format!("learner {}: {error}", profile.id),
                                },
                                duration_ms: started.elapsed().as_millis(),
                            }
                        }
                    }
                }
                JobStatus::Completed {
                    mastery_rows_written: written,
                }
            }
            Err(error) => JobStatus::Failed {
                message: error.to_string(),
            },
        },
    };

    JobOutcome {
        job: job.name().to_string(),
        status,
        duration_ms: started.elapsed().as_millis(),
    }
}

/// A pool of worker threads draining a bounded queue.
///
/// The pool owns the database *path*, not a handle, which is what makes the
/// concurrency real: each worker opens its own connection on first use.
pub struct WorkerPool {
    sender: Option<Sender<Job>>,
    handles: Vec<std::thread::JoinHandle<()>>,
    outcomes: Arc<Mutex<Vec<JobOutcome>>>,
}

impl WorkerPool {
    /// Start `workers` threads against the database at `db_path`.
    pub fn start(db_path: &Path, workers: usize) -> Result<Self, ServiceError> {
        if workers == 0 {
            return Err(ServiceError::Invalid(
                "a worker pool needs at least one worker".into(),
            ));
        }

        let (sender, receiver) = mpsc::channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));
        let outcomes = Arc::new(Mutex::new(Vec::new()));
        let path: PathBuf = db_path.to_path_buf();

        let mut handles = Vec::with_capacity(workers);
        for index in 0..workers {
            let receiver = Arc::clone(&receiver);
            let outcomes = Arc::clone(&outcomes);
            let path = path.clone();
            let handle = std::thread::Builder::new()
                .name(format!("vector-worker-{index}"))
                .spawn(move || {
                    // One connection per thread, opened lazily so a pool that is
                    // never used costs nothing.
                    let mut db: Option<Database> = None;
                    loop {
                        let job = {
                            let guard = match receiver.lock() {
                                Ok(guard) => guard,
                                // A poisoned queue means another worker
                                // panicked; continuing would risk duplicating
                                // its work, so this worker stops.
                                Err(_) => return,
                            };
                            match guard.recv() {
                                Ok(job) => job,
                                Err(_) => return,
                            }
                        };

                        if db.is_none() {
                            match Database::open(&path) {
                                Ok(opened) => db = Some(opened),
                                Err(error) => {
                                    outcomes
                                        .lock()
                                        .map(|mut out| {
                                            out.push(JobOutcome {
                                                job: job.name().to_string(),
                                                status: JobStatus::Failed {
                                                    message: format!(
                                                        "cannot open {}: {error}",
                                                        path.display()
                                                    ),
                                                },
                                                duration_ms: 0,
                                            })
                                        })
                                        .ok();
                                    continue;
                                }
                            }
                        }

                        let outcome = run_job(db.as_ref().expect("opened above"), &job);
                        if let Ok(mut out) = outcomes.lock() {
                            out.push(outcome);
                        }
                    }
                })
                .map_err(|e| ServiceError::Storage(format!("cannot start a worker: {e}")))?;
            handles.push(handle);
        }

        Ok(Self {
            sender: Some(sender),
            handles,
            outcomes,
        })
    }

    /// Queue a job. Fails once the pool has been shut down.
    pub fn submit(&self, job: Job) -> Result<(), ServiceError> {
        match &self.sender {
            Some(sender) => sender
                .send(job)
                .map_err(|_| ServiceError::Storage("the worker pool has stopped".into())),
            None => Err(ServiceError::Storage("the worker pool has stopped".into())),
        }
    }

    /// Queue jobs and wait until all of them have been recorded.
    ///
    /// Draining by count rather than by a fixed sleep: a sleep would either be
    /// flaky or waste time, and neither proves the work happened.
    pub fn run_to_completion(&self, jobs: Vec<Job>) -> Result<Vec<JobOutcome>, ServiceError> {
        let expected = jobs.len();
        let before = self.outcomes.lock().map(|o| o.len()).unwrap_or(0);
        for job in jobs {
            self.submit(job)?;
        }

        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let recorded = self.outcomes.lock().map(|o| o.len()).unwrap_or(0);
            if recorded >= before + expected {
                let mut out = self.outcomes.lock().expect("lock").clone();
                out.drain(0..before);
                return Ok(out);
            }
            if Instant::now() >= deadline {
                return Err(ServiceError::Storage(format!(
                    "only {} of {expected} jobs finished within the deadline",
                    recorded - before
                )));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Every outcome recorded so far.
    pub fn outcomes(&self) -> Vec<JobOutcome> {
        self.outcomes.lock().map(|o| o.clone()).unwrap_or_default()
    }

    /// Number of worker threads.
    pub fn worker_count(&self) -> usize {
        self.handles.len()
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        // Closing the queue is the shutdown signal: each worker's `recv`
        // returns Err once every sender is gone, and the threads then exit.
        self.sender = None;
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}
