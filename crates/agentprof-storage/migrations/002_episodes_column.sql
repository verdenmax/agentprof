-- M2.1.1: episodes blob column for aggregate's per-call/per-turn data.
--
-- Default '{}' so existing M2.1-schema rows remain queryable —
-- load_episodes(id) on an un-reingested row returns Episodes::default()
-- (all empty maps + empty vecs), which aggregate callers gracefully
-- skip (zero contribution to the percentile pool). Cache-mode users
-- get full coverage on the next ingest; store-mode users keep their
-- existing report data and can choose when to backfill.
ALTER TABLE sessions ADD COLUMN episodes_json TEXT NOT NULL DEFAULT '{}';
