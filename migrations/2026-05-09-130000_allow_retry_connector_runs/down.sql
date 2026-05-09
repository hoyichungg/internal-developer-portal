ALTER TABLE connector_runs
  DROP CONSTRAINT connector_runs_trigger_check,
  ADD CONSTRAINT connector_runs_trigger_check
    CHECK (trigger IN ('manual', 'scheduled', 'import'));
