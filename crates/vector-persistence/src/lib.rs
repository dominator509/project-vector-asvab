//! Project VECTOR persistence layer (EP-003).
//!
//! SQLite is the canonical local operational store (ADR-002). This crate owns
//! connection management, monotonic migrations, the durable repositories, and
//! atomic integrity-checked backup/restore.

pub mod backup;
pub mod content;
pub mod db;
pub mod repo;

pub use content::{ContentItemRepo, NewContentItem, ReviewEntry, StoredItem};
pub use db::{Database, Migration, MigrationManager};

#[cfg(test)]
mod db_test;
