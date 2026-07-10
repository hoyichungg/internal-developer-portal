ALTER TABLE connector_configs
  DROP CONSTRAINT connector_configs_target_check;

ALTER TABLE connector_configs
  ADD CONSTRAINT connector_configs_target_check
  CHECK (target IN ('service_health', 'work_cards', 'notifications'));
