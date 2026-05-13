export type DateTimeString = string;

export type ApiId = number;

export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };

export type ApiErrorDetail = {
  field: string;
  message: string;
};

export type ApiErrorBody = {
  message?: string;
  details?: ApiErrorDetail[];
};

export type ApiResponse<T> = {
  data?: T;
  error?: ApiErrorBody;
};

export type MeResponse = {
  id: ApiId;
  username: string;
  roles: string[];
};

export type UserSummary = {
  id: ApiId;
  username: string;
  roles: string[];
  created_at: DateTimeString;
};

export type LoginRequest = {
  username: string;
  password: string;
};

export type LoginResponse = {
  token: string;
  token_type: string;
  expires_at: DateTimeString;
};

export type AuditLog = {
  id: ApiId;
  actor_user_id: ApiId | null;
  action: string;
  resource_type: string;
  resource_id: string | null;
  metadata: string | null;
  created_at: DateTimeString;
};

export type Maintainer = {
  id: ApiId;
  display_name: string;
  email: string;
  created_at: DateTimeString;
};

export type MaintainerMember = {
  id: ApiId;
  maintainer_id: ApiId;
  user_id: ApiId;
  role: string;
  created_at: DateTimeString;
};

export type Package = {
  id: ApiId;
  maintainer_id: ApiId;
  slug: string;
  name: string;
  version: string;
  description: string | null;
  created_at: DateTimeString;
  status: string;
  repository_url: string | null;
  documentation_url: string | null;
  updated_at: DateTimeString;
};

export type Service = {
  id: ApiId;
  maintainer_id: ApiId;
  slug: string;
  name: string;
  lifecycle_status: string;
  health_status: string;
  description: string | null;
  repository_url: string | null;
  dashboard_url: string | null;
  runbook_url: string | null;
  last_checked_at: DateTimeString | null;
  created_at: DateTimeString;
  updated_at: DateTimeString;
  source: string;
  external_id: string | null;
};

export type ServiceHealthCheck = {
  id: ApiId;
  service_id: ApiId;
  connector_run_id: ApiId | null;
  source: string;
  external_id: string | null;
  health_status: string;
  previous_health_status: string | null;
  checked_at: DateTimeString;
  response_time_ms: number | null;
  message: string | null;
  created_at: DateTimeString;
};

export type WorkCard = {
  id: ApiId;
  source: string;
  external_id: string | null;
  title: string;
  status: string;
  priority: string;
  assignee: string | null;
  due_at: DateTimeString | null;
  url: string | null;
  created_at: DateTimeString;
  updated_at: DateTimeString;
};

export type Notification = {
  id: ApiId;
  source: string;
  title: string;
  body: string | null;
  severity: string;
  is_read: boolean;
  url: string | null;
  created_at: DateTimeString;
  updated_at: DateTimeString;
  external_id: string | null;
};

export type Connector = {
  id: ApiId;
  source: string;
  kind: string;
  display_name: string;
  status: string;
  last_run_at: DateTimeString | null;
  last_success_at: DateTimeString | null;
  created_at: DateTimeString;
  updated_at: DateTimeString;
};

export type ConnectorRun = {
  id: ApiId;
  source: string;
  target: string;
  status: string;
  success_count: number;
  failure_count: number;
  duration_ms: number;
  error_message: string | null;
  started_at: DateTimeString;
  finished_at: DateTimeString | null;
  trigger: string;
  claimed_at: DateTimeString | null;
  worker_id: string | null;
};

export type ConnectorRunItem = {
  id: ApiId;
  connector_run_id: ApiId;
  source: string;
  target: string;
  record_id: ApiId | null;
  external_id: string | null;
  status: string;
  snapshot: string | null;
  created_at: DateTimeString;
};

export type ConnectorRunItemError = {
  id: ApiId;
  connector_run_id: ApiId;
  source: string;
  target: string;
  external_id: string | null;
  message: string;
  raw_item: string | null;
  created_at: DateTimeString;
};

export type MaintenanceRun = {
  id: ApiId;
  task: string;
  status: string;
  worker_id: string | null;
  started_at: DateTimeString;
  finished_at: DateTimeString;
  duration_ms: number;
  health_checks_deleted: number;
  connector_runs_deleted: number;
  audit_logs_deleted: number;
  error_message: string | null;
  created_at: DateTimeString;
};

export type ConnectorWorkerStatus = {
  id: ApiId;
  worker_id: string;
  status: string;
  scheduler_enabled: boolean;
  retention_enabled: boolean;
  current_run_id: ApiId | null;
  last_error: string | null;
  started_at: DateTimeString;
  last_seen_at: DateTimeString;
  updated_at: DateTimeString;
  seconds_since_last_seen: number;
  is_stale: boolean;
};

export type ConnectorOperationsResponse = {
  stale_after_seconds: number;
  workers: ConnectorWorkerStatus[];
  maintenance_runs: MaintenanceRun[];
};

export type ConnectorConfigResponse = {
  id: ApiId;
  source: string;
  target: string;
  enabled: boolean;
  schedule_cron: string | null;
  config: string;
  sample_payload: string;
  created_at: DateTimeString;
  updated_at: DateTimeString;
  last_scheduled_at: DateTimeString | null;
  next_run_at: DateTimeString | null;
  last_scheduled_run_id: ApiId | null;
};

export type ConnectorImportError = {
  external_id: string | null;
  message: string;
  raw_item: string | null;
};

export type ConnectorRunExecutionResponse = {
  source: string;
  target: string;
  imported: number;
  failed: number;
  run: ConnectorRun;
  data: JsonValue[];
  items: ConnectorRunItem[];
  errors: ConnectorImportError[];
  item_errors: ConnectorRunItemError[];
};

export type ConnectorRunDetail = {
  run: ConnectorRun;
  items: ConnectorRunItem[];
  item_errors: ConnectorRunItemError[];
  health_checks: ServiceHealthCheck[];
};

export type ServiceHealthHistorySummary = {
  window_hours: number;
  checks: number;
  healthy_checks: number;
  degraded_checks: number;
  down_checks: number;
  unknown_checks: number;
  changed_checks: number;
};

export type ServiceHealthHistory = {
  summary: ServiceHealthHistorySummary;
  recent_checks: ServiceHealthCheck[];
  recent_incidents: ServiceHealthCheck[];
};

export type DashboardPriorityItem = {
  key: string;
  kind: string;
  severity: string;
  rank?: number;
  title: string;
  detail: string;
  source?: string | null;
  target?: string | null;
  record_id?: ApiId | null;
  service_id?: ApiId | null;
  serviceId?: ApiId;
  url?: string | null;
  occurred_at?: DateTimeString | null;
};

export type ConnectorDrillTarget = {
  source?: string | null;
  target?: string | null;
  runId?: ApiId | null;
};

export type MeOperationsStatus = {
  worker_status: string;
  active_workers: number;
  stale_workers: number;
  latest_worker_seen_at: DateTimeString | null;
  worker_stale_after_seconds: number;
  latest_retention_cleanup: MaintenanceRun | null;
  latest_health_check_at: DateTimeString | null;
  health_data_stale: boolean;
  health_stale_after_hours: number;
};

export type MeOverviewSummary = {
  maintainers: number;
  services: number;
  unhealthy_services: number;
  packages: number;
  open_work_cards: number;
  unread_notifications: number;
  failed_connector_runs: number;
};

export type MeMaintainerOverview = {
  maintainer: Maintainer;
  role: string;
};

export type MeOverviewResponse = {
  user: MeResponse;
  maintainers: MeMaintainerOverview[];
  services: Service[];
  packages: Package[];
  open_work_cards: WorkCard[];
  unread_notifications: Notification[];
  failed_connector_runs: ConnectorRun[];
  priority_items: DashboardPriorityItem[];
  health_history: ServiceHealthHistory;
  operations: MeOperationsStatus;
  summary: MeOverviewSummary;
};

export type CatalogResponse = {
  maintainers: Maintainer[];
  services: Service[];
  packages: Package[];
  users: UserSummary[];
};

export type ServiceOverviewResponse = {
  service: Service;
  owner: {
    id: ApiId;
    display_name: string;
    email: string;
  };
  maintainer: Maintainer;
  maintainer_members: MaintainerMember[];
  packages: Package[];
  health: {
    status: string;
    lifecycle_status: string;
    last_checked_at: DateTimeString | null;
  };
  connector: Connector | null;
  recent_connector_runs: ConnectorRun[];
  links: {
    repository_url: string | null;
    dashboard_url: string | null;
    runbook_url: string | null;
  };
};

export type NewMaintainerPayload = {
  display_name: string;
  email: string;
};

export type NewServicePayload = {
  source: string;
  external_id: string | null;
  maintainer_id: ApiId;
  slug: string;
  name: string;
  lifecycle_status: string;
  health_status: string;
  description: string | null;
  repository_url: string | null;
  dashboard_url: string | null;
  runbook_url: string | null;
  last_checked_at: DateTimeString | null;
};

export type NewPackagePayload = {
  maintainer_id: ApiId;
  slug: string;
  name: string;
  version: string;
  status: string;
  description: string | null;
  repository_url: string | null;
  documentation_url: string | null;
};

export type NewConnectorPayload = {
  source: string;
  kind: string;
  display_name: string;
  status: string;
};

export type MaintainerMemberPayload = {
  user_id: ApiId;
  role: string;
};

export type ConnectorConfigForm = {
  target: string;
  enabled: boolean;
  schedule_cron: string;
  config: string;
  sample_payload: string;
};
