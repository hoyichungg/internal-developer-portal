# Internal Developer Portal

An internal developer portal built with Rocket, Diesel Async, PostgreSQL,
React, TypeScript, and pnpm. It includes a software catalog, service health
snapshots, DevOps work cards, connector operations, notifications, CLI user
management, Argon2 password hashing, session tokens, request validation, and
integration tests for the main REST resources.

## Stack

- Rust 1.81
- Rocket 0.5
- Diesel 2.1 and diesel-async
- React 18, Vite, and Mantine
- PostgreSQL 16
- Docker Compose for local development
- GitHub Actions CI

## Local Development

Start the database and application:

```sh
docker compose up --build
```

Docker Compose runs database migrations once, then starts the HTTP server and
the connector worker as separate services. The HTTP API is intentionally not
responsible for executing queued connector work.

The frontend and backend are separated for development: the Vite dev server
proxies API requests to the backend on port `8000`. The default built app is
served by Rocket from `frontend/dist`, so local Docker deployment uses one
origin even though the UI still talks to the backend through HTTP APIs.

The application listens on:

```text
http://127.0.0.1:8000
```

Open the same URL in a browser to use the built-in management UI.

The `migrate` container runs database migrations before `app` and `worker`
start. It also ensures a development admin user exists and seeds local demo
data so the dashboard has services, health checks, work cards, notifications,
connector run history, and sample Calendar/Outlook/ERP connector configs on
first launch.

Default local credentials:

```text
username: admin
password: admin123
```

## Environment

Copy `.env.example` to `.env` when running tools directly on the host. The
server and CLI load `.env` automatically. The Docker Compose setup already
provides the required environment variables.

Application-level config is loaded from environment variables:

- `APP_ENV`
- `AUTH_TOKEN_TTL_SECONDS`
- `DATABASE_URL`
- `SEED_ADMIN_USERNAME`
- `SEED_ADMIN_PASSWORD`
- `SEED_ADMIN_ROLES`
- `CONNECTOR_SECRET_KEY`
- `CONNECTOR_WORKER_ENABLED`
- `CONNECTOR_SCHEDULER_ENABLED`
- `CONNECTOR_WORKER_POLL_MS`
- `CONNECTOR_WORKER_HEARTBEAT_INTERVAL_SECONDS`
- `CONNECTOR_WORKER_STALE_AFTER_SECONDS`
- `CONNECTOR_HEALTH_RETENTION_DAYS`
- `CONNECTOR_RUN_RETENTION_DAYS`
- `AUDIT_LOG_RETENTION_DAYS`
- `CONNECTOR_RETENTION_CLEANUP_INTERVAL_SECONDS`
- `ROCKET_ADDRESS`
- `ROCKET_PORT`
- `ROCKET_DATABASES`

## CLI

Create a user with roles:

```sh
cargo run --bin cli -- users create alice password123 admin,member
```

List users:

```sh
cargo run --bin cli -- users list
```

Delete a user:

```sh
cargo run --bin cli -- users delete 1
```

Ensure a local admin user exists:

```sh
cargo run --bin cli -- users ensure-admin --username admin --password admin123 --roles admin,member
```

Use `--reset-password` when you intentionally want to replace the password for
an existing seed account.

Seed local demo data:

```sh
cargo run --bin cli -- demo seed
```

The demo seed is idempotent and uses the `demo-workday` connector source.

## Tests

Build the frontend before starting the backend when running from a clean
checkout:

```sh
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend build
```

Integration tests expect the server to be running on `127.0.0.1:8000`.
Tests that cover queued connector runs also expect the worker to be running.

```sh
cargo run --bin server
cargo run --bin worker
cargo test
```

The CI workflow runs formatting, build checks, Clippy, Diesel migrations, a
real Rocket server, and the integration tests against PostgreSQL 16.

## API Shape

The generated OpenAPI 3.1 document is available from the running backend:

```text
GET /openapi.json
```

The spec is generated with `utoipa` from the Rust API/request/response types.
Connector registry, config, run history, manual run, retry, and direct import
endpoints are tagged under `Connectors`, including the three import payloads:
`service-health`, `work-cards`, and `notifications`.

Successful JSON responses use a consistent wrapper:

```json
{
  "data": {}
}
```

Errors use a consistent error body:

```json
{
  "error": {
    "code": "validation_failed",
    "message": "Request validation failed.",
    "details": []
  }
}
```

Authenticated routes expect:

```text
Authorization: Bearer <token>
```

## Ownership and Permissions

- Read routes require a bearer token. `GET /health` and `POST /login` remain
  public so health checks and login can work before a session exists.
- `admin` users can manage maintainers, work cards, notifications, connector
  registry, connector imports, connector run history, audit logs, and
  maintainer membership.
- Maintainer membership connects users to a maintainer with one of
  `owner`, `maintainer`, or `viewer`.
- `owner` and `maintainer` members can create, update, and delete packages and
  services owned by that maintainer.
- `owner` members can list and manage membership for their maintainer.
- `viewer` members are read-only.

## Catalog Domain

- `maintainers` represent teams or people responsible for internal packages.
- `maintainer_members` represent ownership and maintainer-level permissions.
- `packages` represent cataloged software packages owned by a maintainer.
- Package lifecycle `status` is one of `active`, `deprecated`, or `archived`.
- Packages can link to source repositories and documentation.
- `services` represent internal applications with lifecycle and health status.
- `work-cards` represent DevOps or task items from external work systems.
- `notifications` represent unread messages from systems such as ERP, mail, or monitoring.
- `dashboard` aggregates the morning work context for the portal home screen.
  It can be scoped by `maintainer_id` and connector `source`.
- Connector imports normalize external systems into dashboard sources using
  `source` and `external_id`, so repeated sync runs update existing records.
- `connector_runs` record each import execution with source, target, status,
  success count, failure count, duration, and final or queued execution state.
- Connector run status is one of `queued`, `running`, `success`,
  `partial_success`, or `failed`.
- `connector_run_item_errors` preserve item-level failures for replay and
  debugging.
- `connector_configs` store per-source runtime config, target, enabled state,
  optional schedule metadata, next scheduled run, and a sample payload for
  manual or scheduled runs.
- `connectors` represent configured external systems and track the latest run
  state with `last_run_at`, `last_success_at`, and operational status.
- `audit_logs` record user and system actions against core resources with actor,
  action, resource identity, metadata, and timestamp.

## API

- `GET /`
- `GET /openapi.json`
- `GET /health`
- `GET /dashboard`
- `GET /dashboard?maintainer_id=<id>`
- `GET /dashboard?source=<connector>`
- `POST /login`
- `GET /me`
- `POST /logout`
- `GET /audit-logs`
- `GET /audit-logs?resource_type=<type>&resource_id=<id>`
- `GET /maintainers`
- `POST /maintainers`
- `GET /maintainers/<id>`
- `PUT /maintainers/<id>`
- `DELETE /maintainers/<id>`
- `GET /maintainers/<id>/members`
- `POST /maintainers/<id>/members`
- `DELETE /maintainers/<id>/members/<user_id>`
- `GET /packages`
- `POST /packages`
- `GET /packages/<id>`
- `PUT /packages/<id>`
- `DELETE /packages/<id>`
- `GET /services`
- `POST /services`
- `GET /services/<id>`
- `GET /services/<id>/overview`
- `PUT /services/<id>`
- `DELETE /services/<id>`
- `GET /work-cards`
- `POST /work-cards`
- `GET /work-cards/<id>`
- `PUT /work-cards/<id>`
- `DELETE /work-cards/<id>`
- `GET /notifications`
- `POST /notifications`
- `GET /notifications/<id>`
- `PUT /notifications/<id>`
- `DELETE /notifications/<id>`
- `POST /connectors/<source>/service-health/import`
- `POST /connectors/<source>/work-cards/import`
- `POST /connectors/<source>/notifications/import`
- `GET /connectors`
- `POST /connectors`
- `GET /connectors/<source>`
- `PUT /connectors/<source>`
- `DELETE /connectors/<source>`
- `GET /connectors/operations`
- `GET /connectors/<source>/config`
- `PUT /connectors/<source>/config`
- `POST /connectors/<source>/runs`
- `GET /connectors/runs`
- `GET /connectors/runs?source=<connector>&target=<target>`
- `GET /connectors/runs/<id>`
- `POST /connectors/runs/<id>/retry`

## Connector Runtime

Connectors can now be configured and executed by the platform instead of only
receiving import payloads. `PUT /connectors/<source>/config` stores the runtime
target, enabled flag, optional schedule metadata, config JSON, and a
`sample_payload`. `POST /connectors/<source>/runs` creates a run from that
config.

Manual runs move through `queued` and `running` before ending as `success`,
`partial_success`, or `failed`. A request body of `{ "mode": "queue" }` creates
a queued run; the background worker claims it, executes the stored payload
snapshot, sets `claimed_at` and `worker_id`, and writes the final status.
Runtime executions record item-level errors, which are returned in the run
response and available through `GET /connectors/runs/<id>`. Run detail also
returns `health_checks` for service-health runs, letting the UI trace a
homepage incident back to the connector execution that imported it.

The built-in scheduler reads enabled connector configs with `schedule_cron` and
creates queued runs when `next_run_at` is due. Supported schedule values are
`@every <n>s`, `@every <n>m`, `@every <n>h`, `@hourly`, and `@daily`.

Connector configs can also select a real adapter. Supported adapters include
`azure_devops` for the `work_cards` target and `monitoring` for the
`service_health` target. When `config` contains `"adapter": "azure_devops"`,
the worker calls Azure DevOps WIQL and work item batch APIs, then normalizes
work items into the existing `work_cards` payload.
For product walkthroughs and local development, three notification adapters are
available without external credentials: `calendar_sample`, `outlook_mail_sample`,
and `erp_messages_sample`. These target `notifications` and normalize sample
calendar events, mail messages, and ERP-style private messages into the same
payload accepted by `POST /connectors/<source>/notifications/import`. The ERP
adapter is intentionally a mock/sample adapter, so it does not require a real ERP
instance.
Config responses redact secret-looking keys such as `personal_access_token`,
`pat`, `token`, `password`, `secret`, `client_secret`, `bearer_token`, and
`api_key`.
Those secret values are encrypted before they are stored in
`connector_configs.config`; the worker decrypts them only while preparing an
adapter request. Set `CONNECTOR_SECRET_KEY` to a stable high-entropy value in
shared environments. Development and test fall back to an insecure local key,
but production refuses to encrypt or decrypt connector secrets without this
variable.
Minimal config:

```json
{
  "adapter": "azure_devops",
  "organization": "acme",
  "project": "platform",
  "personal_access_token": "...",
  "wiql": "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project",
  "timeout_seconds": 15
}
```

For development or custom Azure DevOps proxies, `wiql_url` and
`work_items_url` can be provided directly.

Monitoring adapter config:

```json
{
  "adapter": "monitoring",
  "url": "https://monitoring.example.test/api/service-health",
  "default_maintainer_id": 1,
  "bearer_token": "...",
  "timeout_seconds": 15
}
```

The monitoring response can be `{ "items": [...] }`, `{ "services": [...] }`,
or a top-level array. Items are normalized into service health records using
common fields such as `id`, `name`, `status`, `health`, `summary`,
`dashboard_url`, `repository_url`, and `runbook_url`.

Sample notification adapter configs:

```json
{ "adapter": "calendar_sample", "events": [{ "id": "standup", "subject": "Daily standup" }] }
```

```json
{ "adapter": "outlook_mail_sample", "messages": [{ "id": "mail-1", "subject": "Release brief", "importance": "high" }] }
```

```json
{ "adapter": "erp_messages_sample", "messages": [{ "id": "approval-1", "title": "Access approval waiting", "requires_approval": true }] }
```

Each successful service health item also writes an append-only
`service_health_checks` record tied to the connector run. `/dashboard` and
`/me/overview` include a 24-hour `health_history` summary with recent checks,
status counts, status changes, and recent degraded/down incidents for the
homepage.

Runtime environment flags:

- `CONNECTOR_SECRET_KEY=<stable secret>`
- `CONNECTOR_WORKER_ENABLED=true`
- `CONNECTOR_SCHEDULER_ENABLED=true`
- `CONNECTOR_WORKER_POLL_MS=500`
- `CONNECTOR_WORKER_HEARTBEAT_INTERVAL_SECONDS=15`
- `CONNECTOR_WORKER_STALE_AFTER_SECONDS=45`
- `CONNECTOR_HEALTH_RETENTION_DAYS=30`
- `CONNECTOR_RUN_RETENTION_DAYS=90`
- `AUDIT_LOG_RETENTION_DAYS=365`
- `CONNECTOR_RETENTION_CLEANUP_INTERVAL_SECONDS=3600`

The worker also performs retention cleanup. `CONNECTOR_HEALTH_RETENTION_DAYS`
deletes old `service_health_checks` by `checked_at`,
`CONNECTOR_RUN_RETENTION_DAYS` deletes finished `connector_runs` by
`finished_at`, and `AUDIT_LOG_RETENTION_DAYS` deletes old `audit_logs` by
`created_at`. Run item snapshots and item errors cascade with their run. Set a
retention value to `0` to disable that cleanup path. Defaults keep high-volume
health history shorter, connector run history at 90 days, and audit logs at a
longer 365-day window.

The worker writes heartbeat status to `connector_workers`, and each retention
cleanup writes a `maintenance_runs` history row. `GET /connectors/operations`
returns recent worker status and cleanup history for the Connectors operations
panel. Failed or partially successful connector runs can be retried with
`POST /connectors/runs/<id>/retry`, which creates a queued retry run using the
original run payload when available or the current connector config otherwise.

Run the worker separately from the HTTP server:

```sh
cargo run --bin worker
```

The worker reuses a PostgreSQL connection between poll cycles and reconnects
when database operations fail. For local experiments only, the HTTP server can
embed the worker with `CONNECTOR_EMBEDDED_WORKER_ENABLED=true`; normal
development and CI run it as a separate process.

## Connector Imports

Connector import endpoints are the boundary for external systems such as Azure
DevOps, Outlook, ERP, and monitoring tools. They accept normalized payloads and
upsert records by `source + external_id`.

Each import uses the same connector runtime history. The run captures `source`,
`target`, `status`, `success_count`, `failure_count`, `duration_ms`,
timestamps, optional `error_message`, and item-level errors for failed records.

The connector registry stores configured sources and is refreshed by imports.
When an import succeeds, the connector becomes `active` and updates
`last_success_at`; failed imports mark the connector as `error` while preserving
the previous successful timestamp. Partial success updates `last_run_at` while
preserving the previous `last_success_at`.

## Service Overview

`GET /services/<id>/overview` returns the service, owner/maintainer,
maintainer membership, related packages, explicit health status, runbook,
dashboard, repository links, connector registry entry, and recent service
health connector runs. This gives the frontend one practical endpoint for a
service detail page.

Examples:

- `POST /connectors/azure-devops/work-cards/import`
- `POST /connectors/outlook/notifications/import`
- `POST /connectors/erp/notifications/import`
- `POST /connectors/monitoring/service-health/import`
