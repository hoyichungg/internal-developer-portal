DROP INDEX connector_runs_snapshot_incomplete_idx;

ALTER TABLE connector_runs
  DROP CONSTRAINT connector_runs_archived_count_check,
  DROP COLUMN archived_count,
  DROP COLUMN snapshot_complete;
