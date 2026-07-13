ALTER TABLE work_cards
  ADD COLUMN project varchar(128),
  ADD COLUMN work_item_type varchar(128),
  ADD COLUMN assignee_source_id varchar(512),
  ADD COLUMN assignee_user_id integer REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX work_cards_assignee_active_due_idx
  ON work_cards (assignee_user_id, due_at ASC, id ASC)
  WHERE archived_at IS NULL AND assignee_user_id IS NOT NULL;

CREATE INDEX work_cards_assignee_active_source_updated_idx
  ON work_cards (assignee_user_id, source_updated_at DESC, id DESC)
  WHERE archived_at IS NULL AND assignee_user_id IS NOT NULL;

CREATE INDEX work_cards_assignee_active_project_idx
  ON work_cards (assignee_user_id, project)
  WHERE archived_at IS NULL AND assignee_user_id IS NOT NULL;

CREATE INDEX work_cards_assignee_active_type_idx
  ON work_cards (assignee_user_id, work_item_type)
  WHERE archived_at IS NULL AND assignee_user_id IS NOT NULL;
