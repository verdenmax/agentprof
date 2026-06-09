-- agentprof M2.1 initial schema (schema_version=1)
--
-- Normative mirror of `docs/architecture.md` §9. Keep both in sync.
--
-- Tables:
--   * sessions      — one row per ingested session (analysis report blob)
--   * tools_loaded  — per-session tool inventory + call stats
--   * turn_buckets  — per-turn token accounting

CREATE TABLE sessions (
    id                    TEXT    PRIMARY KEY,
    agent                 TEXT    NOT NULL,
    dominant_model        TEXT,
    started_at            INTEGER,
    duration_ms           INTEGER,
    raw_path              TEXT NOT NULL,
    raw_mtime             INTEGER NOT NULL,
    total_input_tokens    INTEGER,
    total_output_tokens   INTEGER,
    total_cache_read      INTEGER,
    total_cache_creation  INTEGER,
    schema_version        INTEGER NOT NULL DEFAULT 1,
    ingested_at           INTEGER NOT NULL,
    analysis_report_json  TEXT NOT NULL
);
CREATE INDEX idx_sessions_started       ON sessions(started_at DESC);
CREATE INDEX idx_sessions_agent_started ON sessions(agent, started_at DESC);

CREATE TABLE tools_loaded (
    session_id        TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tool_name         TEXT NOT NULL,
    source            TEXT NOT NULL,
    call_count        INTEGER NOT NULL,
    total_duration_ms INTEGER NOT NULL,
    tokens            INTEGER,
    token_source      TEXT,
    PRIMARY KEY (session_id, tool_name)
);
CREATE INDEX idx_tools_call_count ON tools_loaded(session_id, call_count DESC);

CREATE TABLE turn_buckets (
    session_id      TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    turn_index      INTEGER NOT NULL,
    input_tokens    INTEGER,
    output_tokens   INTEGER,
    cache_read      INTEGER,
    cache_creation  INTEGER,
    model           TEXT,
    PRIMARY KEY (session_id, turn_index)
);
