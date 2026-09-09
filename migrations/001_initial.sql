-- 001_initial.sql
-- Initial database schema for Project VECTOR desktop foundation

CREATE TABLE IF NOT EXISTS app_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS learner_profile (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    target_score INTEGER NOT NULL DEFAULT 31,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS health_diagnostics (
    id TEXT PRIMARY KEY,
    component TEXT NOT NULL,
    status TEXT NOT NULL,
    details_json TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT OR IGNORE INTO app_config (key, value) VALUES ('version', '0.1.0');
INSERT OR IGNORE INTO app_config (key, value) VALUES ('environment', 'production');
