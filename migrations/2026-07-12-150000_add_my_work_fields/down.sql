DROP INDEX work_cards_assignee_active_type_idx;
DROP INDEX work_cards_assignee_active_project_idx;
DROP INDEX work_cards_assignee_active_source_updated_idx;
DROP INDEX work_cards_assignee_active_due_idx;

ALTER TABLE work_cards
  DROP COLUMN assignee_user_id,
  DROP COLUMN assignee_source_id,
  DROP COLUMN work_item_type,
  DROP COLUMN project;
