ALTER TABLE connector_runs
  ADD COLUMN snapshot_complete boolean,
  ADD COLUMN archived_count integer NOT NULL DEFAULT 0,
  ADD CONSTRAINT connector_runs_archived_count_check
    CHECK (archived_count >= 0);

CREATE INDEX connector_runs_snapshot_incomplete_idx
  ON connector_runs(started_at DESC)
  WHERE snapshot_complete = false;
