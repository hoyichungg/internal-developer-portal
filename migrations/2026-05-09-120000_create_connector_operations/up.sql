CREATE TABLE connector_workers (
    id SERIAL PRIMARY KEY,
    worker_id VARCHAR(128) NOT NULL UNIQUE,
    status VARCHAR(32) NOT NULL,
    scheduler_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    retention_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    current_run_id INTEGER REFERENCES connector_runs(id) ON DELETE SET NULL,
    last_error TEXT,
    started_at TIMESTAMP NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX connector_workers_last_seen_at_idx ON connector_workers(last_seen_at);
CREATE INDEX connector_workers_status_idx ON connector_workers(status);
CREATE INDEX connector_workers_current_run_id_idx ON connector_workers(current_run_id);

CREATE TABLE maintenance_runs (
    id SERIAL PRIMARY KEY,
    task VARCHAR(64) NOT NULL,
    status VARCHAR(32) NOT NULL,
    worker_id VARCHAR(128),
    started_at TIMESTAMP NOT NULL,
    finished_at TIMESTAMP NOT NULL,
    duration_ms BIGINT NOT NULL DEFAULT 0,
    health_checks_deleted INTEGER NOT NULL DEFAULT 0,
    connector_runs_deleted INTEGER NOT NULL DEFAULT 0,
    audit_logs_deleted INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX maintenance_runs_task_created_at_idx ON maintenance_runs(task, created_at DESC);
CREATE INDEX maintenance_runs_status_idx ON maintenance_runs(status);
CREATE INDEX maintenance_runs_worker_id_idx ON maintenance_runs(worker_id);
