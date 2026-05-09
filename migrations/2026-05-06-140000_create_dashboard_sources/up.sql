CREATE TABLE services (
  id SERIAL PRIMARY KEY,
  maintainer_id integer NOT NULL REFERENCES maintainers(id),
  slug varchar(64) NOT NULL,
  name varchar(128) NOT NULL,
  lifecycle_status varchar(32) NOT NULL DEFAULT 'active',
  health_status varchar(32) NOT NULL DEFAULT 'unknown',
  description text,
  repository_url varchar(2048),
  dashboard_url varchar(2048),
  runbook_url varchar(2048),
  last_checked_at TIMESTAMP,
  created_at TIMESTAMP DEFAULT NOW() NOT NULL,
  updated_at TIMESTAMP DEFAULT NOW() NOT NULL,
  CONSTRAINT services_lifecycle_status_check
    CHECK (lifecycle_status IN ('active', 'deprecated', 'archived')),
  CONSTRAINT services_health_status_check
    CHECK (health_status IN ('healthy', 'degraded', 'down', 'unknown'))
);

CREATE TABLE work_cards (
  id SERIAL PRIMARY KEY,
  source varchar(64) NOT NULL,
  external_id varchar(128),
  title varchar(255) NOT NULL,
  status varchar(32) NOT NULL DEFAULT 'todo',
  priority varchar(32) NOT NULL DEFAULT 'medium',
  assignee varchar(128),
  due_at TIMESTAMP,
  url varchar(2048),
  created_at TIMESTAMP DEFAULT NOW() NOT NULL,
  updated_at TIMESTAMP DEFAULT NOW() NOT NULL,
  CONSTRAINT work_cards_status_check
    CHECK (status IN ('todo', 'in_progress', 'blocked', 'done')),
  CONSTRAINT work_cards_priority_check
    CHECK (priority IN ('low', 'medium', 'high', 'urgent'))
);

CREATE TABLE notifications (
  id SERIAL PRIMARY KEY,
  source varchar(64) NOT NULL,
  title varchar(255) NOT NULL,
  body text,
  severity varchar(32) NOT NULL DEFAULT 'info',
  is_read boolean NOT NULL DEFAULT false,
  url varchar(2048),
  created_at TIMESTAMP DEFAULT NOW() NOT NULL,
  updated_at TIMESTAMP DEFAULT NOW() NOT NULL,
  CONSTRAINT notifications_severity_check
    CHECK (severity IN ('info', 'warning', 'critical'))
);
