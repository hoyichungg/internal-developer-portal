CREATE TABLE service_health_checks (
    id SERIAL PRIMARY KEY,
    service_id INTEGER NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    connector_run_id INTEGER REFERENCES connector_runs(id) ON DELETE SET NULL,
    source VARCHAR(64) NOT NULL,
    external_id VARCHAR(128),
    health_status VARCHAR(32) NOT NULL,
    previous_health_status VARCHAR(32),
    checked_at TIMESTAMP NOT NULL,
    response_time_ms INTEGER,
    message TEXT,
    raw_payload TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    CONSTRAINT service_health_checks_health_status_check
        CHECK (health_status IN ('healthy', 'degraded', 'down', 'unknown')),
    CONSTRAINT service_health_checks_previous_health_status_check
        CHECK (
            previous_health_status IS NULL
            OR previous_health_status IN ('healthy', 'degraded', 'down', 'unknown')
        )
);

CREATE INDEX service_health_checks_service_checked_at_idx
    ON service_health_checks(service_id, checked_at DESC);

CREATE INDEX service_health_checks_source_checked_at_idx
    ON service_health_checks(source, checked_at DESC);

CREATE INDEX service_health_checks_status_checked_at_idx
    ON service_health_checks(health_status, checked_at DESC);

CREATE INDEX service_health_checks_run_idx
    ON service_health_checks(connector_run_id);
