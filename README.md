# Internal Developer Portal

An internal developer portal built with Rocket, Diesel Async, PostgreSQL,
React, TypeScript, and pnpm. It includes a software catalog, service health
snapshots, DevOps work cards, connector operations, notifications, CLI user
management, local password and Microsoft Entra ID sign-in, session tokens,
request validation, and integration tests for the main REST resources.

## Stack

- Rust 1.85
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

The development Compose file binds PostgreSQL and HTTP only to `127.0.0.1`.
It mounts `src/` for backend hot reload without mounting over the image's
prebuilt `frontend/dist`, so a clean clone serves the UI immediately. Re-run
`docker compose up --build` after frontend or Cargo manifest changes. Redis is
not part of the stack because the current scheduler and worker use PostgreSQL.

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

## Production Compose

The default `Dockerfile` and `docker-compose.yml` are development-oriented:
they run `cargo watch`, mount the source tree, seed demo data, and use local
development credentials. Use the production files when you want a release-style
container:

```sh
cp .env.production.example .env.production
```

Edit `.env.production` so `POSTGRES_PASSWORD`, `DATABASE_URL`,
`CONNECTOR_SECRET_KEY`, and the optional seed admin credentials are real
environment-specific secrets. When Entra sign-in is enabled, also set its
tenant, client, exact HTTPS redirect URI, client secret, and an independent OIDC
transaction key. Keep `AUTH_COOKIE_SECURE=true`: production startup rejects an
insecure browser-session cookie. Validate the effective manifest and build an
immutable image, then follow the maintenance, final-backup, migrate-once,
smoke, and rollback procedure in
[`docs/production-runbook.md`](docs/production-runbook.md). Do not use a direct
`up --build` as an upgrade procedure while old writers are running.

```sh
docker compose --env-file .env.production -f docker-compose.prod.yml config --quiet
docker compose --env-file .env.production -f docker-compose.prod.yml build
```

The production image builds release binaries for `server`, `worker`, and `cli`,
copies the built `frontend/dist` assets into the runtime image, runs as a
non-root user, and uses `/app/server` instead of `cargo watch`. The production
Compose file runs migrations as a one-shot `migrate` service and keeps the HTTP
server and connector worker as separate long-running services.
The published HTTP port defaults to `127.0.0.1:${PORT:-8000}`; terminate TLS at
a same-host reverse proxy or use a reviewed private-container-network layout
instead of exposing Rocket directly.

The application container healthcheck calls `GET /readyz`, which runs a real
PostgreSQL query. The worker container healthcheck verifies that the worker is
still its container's active process. `GET /livez` is available for process-only
liveness probes that must not depend on PostgreSQL.

Create or update an initial admin account only when needed:

```sh
docker compose --env-file .env.production -f docker-compose.prod.yml --profile seed-admin run --rm seed-admin
```

The production stack does not seed demo data.

The `2026-07-11-100000_harden_sessions` migration replaces plaintext session
token storage with token hashes. Existing sessions cannot be converted safely,
so the migration deletes them and every signed-in user must sign in again after
this upgrade. Notify users before the maintenance window and keep a named admin
account available for the post-deploy smoke test.

The `2026-07-12-140000_use_timestamptz` migration is a non-rolling schema
boundary. It converts the portal's historical naive UTC columns to PostgreSQL
`TIMESTAMPTZ` using an explicit UTC interpretation and requires exclusive table
locks. Drain traffic, stop both app and worker writers, take the final verified
backup, run the migration once, and then start the matching app and worker
image. Do not run an old binary against the converted schema or start the new
binary before migration. All API datetimes after this boundary are RFC3339 and
include `Z` or an explicit numeric offset; connector imports and notification
snooze actions reject ambiguous values such as `2026-07-10T09:00:00`. Follow
the Graph Calendar preflight and recovery procedure in
[`docs/production-runbook.md`](docs/production-runbook.md).

## Environment

Copy `.env.example` to `.env` when running tools directly on the host or when
overriding the development Compose authentication settings. Compose forwards
the Entra variables only to the HTTP app; keep client secrets out of the worker
and migration jobs. The
server and CLI load `.env` automatically. The Docker Compose setup already
provides the required environment variables.

Application-level config is loaded from environment variables:

- `APP_ENV`
- `AUTH_TOKEN_TTL_SECONDS`
- `AUTH_MAX_ACTIVE_SESSIONS_PER_USER`
- `AUTH_COOKIE_SECURE`
- `AUTH_LOGIN_MAX_FAILURES`
- `AUTH_LOGIN_ACCOUNT_MAX_FAILURES`
- `AUTH_LOGIN_WINDOW_SECONDS`
- `AUTH_LOGIN_LOCKOUT_SECONDS`
- `AUTH_PASSWORD_LOGIN_ENABLED`
- `AUTH_ENTRA_ENABLED`
- `AUTH_ENTRA_TENANT_ID`
- `AUTH_ENTRA_CLIENT_ID`
- `AUTH_ENTRA_CLIENT_SECRET`
- `AUTH_ENTRA_REDIRECT_URI`
- `AUTH_OIDC_TRANSACTION_KEY`
- `AUTH_ENTRA_ISSUER`
- `AUTH_ENTRA_AUTHORIZATION_URL`
- `AUTH_ENTRA_TOKEN_URL`
- `AUTH_ENTRA_JWKS_URL`
- `AUTH_ENTRA_JIT_PROVISIONING`
- `AUTH_ENTRA_REQUIRED_ROLE`
- `AUTH_OIDC_TRANSACTION_TTL_SECONDS`
- `AUTH_ENTRA_JWKS_CACHE_SECONDS`
- `AUTH_ENTRA_CLOCK_SKEW_SECONDS`
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
- `CONNECTOR_RUN_LEASE_SECONDS`
- `CONNECTOR_RUN_LEASE_RENEW_INTERVAL_SECONDS`
- `CONNECTOR_RUN_MAX_ATTEMPTS`
- `CONNECTOR_RUN_RETRY_BASE_SECONDS`
- `CONNECTOR_RUN_RETRY_MAX_SECONDS`
- `CONNECTOR_HEALTH_RETENTION_DAYS`
- `CONNECTOR_RUN_RETENTION_DAYS`
- `AUDIT_LOG_RETENTION_DAYS`
- `CONNECTOR_RETENTION_CLEANUP_INTERVAL_SECONDS`
- `ROCKET_ADDRESS`
- `ROCKET_PORT`
- `ROCKET_DATABASES`

`APP_ENV` must be `development`, `test`, or `production`.
`AUTH_TOKEN_TTL_SECONDS` defaults to `86400` only when it is absent; a supplied
value must be an integer greater than zero.
`AUTH_MAX_ACTIVE_SESSIONS_PER_USER` defaults to `20` and accepts `1` through
`100`. The limit is shared by password and Entra sessions: a successful login
removes that user's expired sessions and, at capacity, atomically evicts the
oldest active session before creating the new one. Sessions belonging to other
users are never counted or evicted. Lowering the setting takes full effect for
each user on their next successful login; use `POST /sessions/revoke-all` when
immediate revocation is required. `AUTH_COOKIE_SECURE` defaults to `false`
outside production and `true` in production, where setting it to false is
rejected. Login throttling defaults to five failures per normalized
username/client-IP pair and 50 failures account-wide in a shared 900-second
window, followed by a 900-second lockout. Thresholds must be positive, and the
account-wide threshold must be at least twice the per-client threshold. In
production, startup also requires `DATABASE_URL` (or
`ROCKET_DATABASES` for the HTTP server) and a `CONNECTOR_SECRET_KEY` of at least
32 bytes. Placeholder and low-diversity keys are rejected. Generate a random
key and keep it stable so previously encrypted connector credentials remain
decryptable. Invalid startup configuration exits with a non-zero status instead
of falling back silently.

Portal usernames are case-insensitive authentication identifiers. Login and
all user-management CLI selectors trim surrounding whitespace and lowercase
the supplied value; newly created users are stored in that canonical form.
Historical users keep their original casing for display compatibility, but a
database unique index on `lower(username)` ensures that `Admin` and `admin`
cannot be separate accounts. The canonical-username migration deliberately
fails if existing case-only collisions or surrounding-whitespace usernames are
present. Rename those accounts explicitly before retrying; the portal never
silently merges users, roles, sessions, or external identities.

### Microsoft Entra ID sign-in

`AUTH_PASSWORD_LOGIN_ENABLED` defaults to `true` and `AUTH_ENTRA_ENABLED`
defaults to `false`. At least one login method must remain enabled. A production
Entra configuration requires:

- tenant-specific UUID values in `AUTH_ENTRA_TENANT_ID` and
  `AUTH_ENTRA_CLIENT_ID`;
- an exact HTTPS `AUTH_ENTRA_REDIRECT_URI` whose path is fixed to
  `/auth/entra/callback` (no path prefix or trailing slash), normally
  `https://portal.internal.example/auth/entra/callback`;
- `AUTH_ENTRA_CLIENT_SECRET` for the confidential authorization-code exchange;
- a separate, high-entropy `AUTH_OIDC_TRANSACTION_KEY` of at least 32 bytes.

Unauthenticated `GET /auth/config` returns only
`password_login_enabled` and `entra_login_enabled` booleans so the login screen
can select the available methods without exposing provider configuration or
secrets.

The Entra browser flow currently requires the frontend and API to share one
public origin. This is the default production layout, where Rocket serves
`frontend/dist`, and the supported Vite development layout proxies `/auth` and
`/sessions` to the API. Do not deploy the frontend on a separate origin until a
reviewed cross-origin cookie, CORS, CSRF, and callback-origin design is added.

The current implementation uses client-secret authentication. Certificate or
`private_key_jwt` client authentication is not implemented, and PKCE does not
replace the client secret. The browser authorization-code flow uses S256 PKCE;
the transaction key protects the short-lived server-side verifier and must not
be reused as `CONNECTOR_SECRET_KEY`. Production Compose injects the client
secret and transaction key only into the HTTP `app`, not `worker`, `migrate`, or
`seed-admin`.

Issuer, authorization, token, and JWKS URLs default to tenant-specific Microsoft
endpoints. The `AUTH_ENTRA_*_URL` overrides exist for controlled test providers
or an explicitly reviewed endpoint change; production overrides must use HTTPS.
The OIDC transaction TTL defaults to 600 seconds and accepts 60-1800, JWKS cache
TTL defaults to 300 seconds and accepts 30-86400, and clock skew defaults to 120
seconds and accepts 1-300.

`AUTH_ENTRA_JIT_PROVISIONING=false` requires the Entra external identity to
already be linked to a portal user. When JIT is enabled in production,
`AUTH_ENTRA_REQUIRED_ROLE` is mandatory. Whenever a required role is configured,
every Entra login must contain that exact app-role value. This is an admission
check only: a JIT user receives the portal `member` role, and the Entra role is
not mapped to portal `admin`. Assign portal admin and maintainer permissions
through the portal's existing role and ownership controls.

Register the application as a single-tenant **Web** application in Entra ID,
enter the redirect URI exactly, leave implicit/hybrid token issuance disabled,
and create the configured app role before assigning users or groups. Keep local
password login enabled during rollout and retain named, vaulted recovery
accounts until Entra sign-in and rollback have been rehearsed. The current
`POST /logout` revokes the portal session only; it does not propagate logout to
the Microsoft session. See `docs/production-runbook.md` for rollout, rotation,
acceptance, and rollback procedures.

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

With Entra enabled in the current environment, pre-link an existing portal user
to the immutable Entra object ID before using JIT-off mode:

```sh
cargo run --bin cli -- users link-entra \
  --username alice \
  --object-id aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa
```

Use `--user-id` instead of `--username` when appropriate. The command takes the
tenant and issuer from the enabled Entra configuration, is idempotent for the
same user/object link, rejects cross-user conflicts, and writes an audit record.
Do not select or link an identity by email or UPN. `--subject` is optional and
should be supplied only from an authoritative identity record.

Seed local demo data:

```sh
cargo run --bin cli -- demo seed
```

The demo seed is idempotent and uses the `demo-workday` connector source.

## Tests

For the normal local loop, use the validation script. It avoids the Windows
`server.exe` / `worker.exe` file-lock issue that can make `cargo test` fail
before tests start.

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\local-validate.ps1 -Mode Fast
```

Before larger changes or commits, run the full workflow:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\local-validate.ps1 -Mode Full
```

See `docs/local-validation.md` for details. The CI workflow runs frontend
builds and regression tests, formatting, build checks, Clippy, Diesel
migrations, a real Rocket server, and the integration tests against PostgreSQL
16. Normal integration coverage uses the disposable `portal_integration_test`
database through `PORTAL_TEST_DATABASE_URL`; destructive retention coverage
alone uses `portal_retention_test`. Both require `APP_ENV=test`, verify
PostgreSQL's actual `current_database()`, and never fall back to `app_db`.
`PORTAL_TEST_BASE_URL` is the single HTTP origin shared by the isolated server
and request tests. Full local validation recreates both test databases and
proves every public-table row count in the development database is unchanged.

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

Browser login creates an `idp_session` cookie in development and test. In
production it uses the host-prefixed `__Host-idp_session` cookie to prevent
sibling subdomains from injecting a session cookie. Both use `HttpOnly`,
`SameSite=Lax`, and `Path=/`; production also requires the `Secure` attribute,
so the browser must reach the portal over HTTPS. The frontend relies on this
cookie and does not need JavaScript access to the session token. A successful
production login also expires the legacy `idp_session` cookie.
Cookie-authenticated write requests must also send `X-IDP-CSRF: 1`; the shared
frontend client adds it automatically. The API rejects cookie writes without
this non-simple header so an untrusted same-site subdomain cannot submit
mutations with a plain HTML form.

`POST /login` is cookie-only: its `{ "data": ... }` response contains safe
metadata such as `expires_at` and `auth_method`, but never a raw session token.
Automation that exercises the browser login flow must use an HTTP cookie jar and
send `X-IDP-CSRF: 1` on protected writes. The request guard retains Bearer
compatibility for separately provisioned non-browser credentials, but `/login`
is not a token-minting endpoint and browser code must never read or reconstruct
the `HttpOnly` cookie value. `POST /logout` revokes only the current session.
`POST /sessions/revoke-all` revokes every session for the authenticated user,
clears the browser cookie, and returns the number of revoked sessions. Use it
after suspected credential exposure or to sign out other browsers/devices.

## Ownership and Permissions

- Read routes require the browser session cookie or a separately provisioned
  Bearer credential.
  `GET /health`, `GET /livez`, `GET /readyz`, `GET /auth/config`, the Entra
  start/callback endpoints, and enabled `POST /login` remain public so probes
  and login work before a session exists. `/health` is a compatibility alias
  with readiness semantics; use `/livez` for process liveness and `/readyz` for
  traffic readiness.
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
- `calendar-events` represent structured meetings with start/end time,
  organizer, location, time zone, and join/event links.
- `notifications` represent unread messages from systems such as ERP, mail, or monitoring.
- `dashboard` aggregates the morning work context for the portal home screen.
  It can be scoped by `maintainer_id` and connector `source`.
- Connector imports normalize external systems into dashboard sources using
  `source` and `external_id`, so repeated sync runs update existing records.
- `connector_runs` record each import execution with source, target, status,
  success count, failure count, duration, and final or queued execution state.
- Connector run status is one of `queued`, `running`, `success`,
  `partial_success`, `failed`, or `cancelled`.
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
- `GET /livez`
- `GET /readyz`
- `GET /auth/config`
- `GET /auth/entra/start?return_to=<allow-listed-portal-route>`
- `GET /auth/entra/callback`
- `GET /dashboard`
- `GET /dashboard?maintainer_id=<id>`
- `GET /dashboard?source=<connector>`
- `POST /login`
- `GET /me`
- `POST /logout`
- `POST /sessions/revoke-all`
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
- `GET /me/work-cards?status=<status>&due=<overdue|today|next_7_days|none>&project=<project>&work_item_type=<type>&source=<connector>&sort=<attention|due_asc|source_updated_desc>&page=<n>&page_size=<n>`
- `POST /work-cards`
- `GET /work-cards/<id>`
- `PUT /work-cards/<id>`
- `DELETE /work-cards/<id>`
- `GET /calendar-events`
- `GET /calendar-events/<id>`
- `GET /notifications`
- `POST /notifications`
- `GET /notifications/<id>`
- `PUT /notifications/<id>`
- `DELETE /notifications/<id>`
- `POST /connectors/<source>/service-health/import`
- `POST /connectors/<source>/calendar-events/import`
- `POST /connectors/<source>/work-cards/import`
- `POST /connectors/<source>/notifications/import`
- `GET /connectors`
- `POST /connectors`
- `GET /connectors/<source>`
- `PUT /connectors/<source>`
- `PUT /connectors/<source>/scope`
- `DELETE /connectors/<source>`
- `GET /connectors/operations`
- `GET /connectors/<source>/config`
- `PUT /connectors/<source>/config`
- `POST /connectors/<source>/runs`
- `GET /connectors/runs`
- `GET /connectors/runs?source=<connector>&target=<target>`
- `GET /connectors/runs/<id>`
- `POST /connectors/runs/<id>/retry`
- `POST /connectors/runs/<id>/cancel`

`GET /me/work-cards` powers the My Work screen. It returns only cards that are
explicitly mapped to the signed-in portal user and already fall within that
user's connector/maintainer visibility. It never treats a display name or
email address as an identity key. The response is paginated and includes
authorized project, work-item-type, source, and status facets so filters remain
stable in the URL and can be shared or restored after sign-in.

## Connector Runtime

Connectors can now be configured and executed by the platform instead of only
receiving import payloads. `PUT /connectors/<source>/config` stores the runtime
target, enabled flag, optional schedule metadata, config JSON, and a
`sample_payload`. `POST /connectors/<source>/runs` creates a run from that
config.

Manual runs move through `queued` and `running` before ending as `success`,
`partial_success`, `failed`, or `cancelled`. A request body of
`{ "mode": "queue" }` creates a queued run; the background worker atomically
claims it, executes the stored payload snapshot, sets `claimed_at`, `worker_id`,
and a renewable lease, and writes the final status only while it still owns an
unexpired lease.
Runtime executions record item-level errors, which are returned in the run
response and available through `GET /connectors/runs/<id>`. Run detail also
returns `health_checks` for service-health runs, letting the UI trace a
homepage incident back to the connector execution that imported it.

Work-card and notification payloads may declare
`"snapshot_complete": true` only when `items` contains the full result set for
that connector query. If every item imports successfully, records previously
owned by that connector but absent from the new snapshot are archived in the
same transaction; the run exposes `snapshot_complete` and `archived_count`.
Omitted/false declarations, page/item limits, item errors, cancellation, and
failed runs never archive missing records. Microsoft Graph adapters calculate
this flag from pagination completion, Azure DevOps treats a WIQL response that
fills `max_items` as incomplete, and ERP reconciliation is opt-in through the
boolean config field `snapshot_complete` because ERP endpoints may be
incremental or lookback-based.

The built-in scheduler reads enabled connector configs with `schedule_cron` and
creates queued runs when `next_run_at` is due. Supported schedule values are
`@every <n>s`, `@every <n>m`, `@every <n>h`, `@hourly`, and `@daily`.
The minimum effective interval is 60 seconds. The API rejects new sub-minute
schedules, and workers clamp legacy sub-minute configs to one minute so a stale
configuration cannot flood run history and audit logs.

Connector configs can also select a real adapter. Supported adapters include
`azure_devops` for the `work_cards` target, `monitoring` for the
`service_health` target, and `microsoft_graph_calendar`,
`microsoft_graph_mail`, and `erp_private_messages` for the `notifications`
target. Microsoft Graph Calendar also supports the first-class
`calendar_events` target. When `config` contains `"adapter": "azure_devops"`, the worker calls
Azure DevOps WIQL and work item batch APIs, then normalizes work items into the
existing `work_cards` payload. When `config` contains
`"adapter": "microsoft_graph_calendar"`, the worker calls Microsoft Graph
Calendar View for the configured time window and normalizes Outlook events into
structured homepage meetings (legacy `notifications` configs remain supported). When `config` contains
`"adapter": "microsoft_graph_mail"`, the worker calls Microsoft Graph messages
and normalizes Outlook mail into the same notification feed. When `config`
contains `"adapter": "erp_private_messages"`, the worker calls a configured ERP
private-message HTTP endpoint and normalizes messages, approvals, and pending
requests into homepage notifications. Microsoft Graph adapters can use either a
short-lived `access_token` or OAuth refresh credentials; when a refresh token is
configured and the access token is missing, expired, or near expiry, the worker
refreshes the access token and stores the rotated token values back into the
encrypted connector config.
For Microsoft Graph adapters, admins can use the Connect Microsoft or Reconnect
Microsoft button in the connector config editor after setting `tenant_id`,
`client_id`, optional `client_secret`, and `scope`. Register
`<portal origin>/oauth/microsoft/callback` as the redirect URI in the Microsoft
Entra app. The backend creates the authorize URL, validates callback state,
exchanges the authorization code, and stores the returned access and refresh
tokens in the encrypted connector config. `authorization_url` and `token_url`
can be overridden for local mocks or proxy testing. The callback response sets
`Cache-Control: no-store` and `Referrer-Policy: no-referrer`, and production
proxies must log this exact path without its query string or Referer because it
carries short-lived authorization code/state values.
For product walkthroughs and local development, three notification adapters are
available without external credentials: `calendar_sample`, `outlook_mail_sample`,
and `erp_messages_sample`. These target `notifications` and normalize sample
calendar events, mail messages, and ERP-style private messages into the same
payload accepted by `POST /connectors/<source>/notifications/import`.
Config responses redact secret-looking keys such as `personal_access_token`,
`pat`, `token`, `password`, `secret`, `client_secret`, `bearer_token`,
`access_token`, `refresh_token`, `api_key`, `x-api-key`, and `authorization`.
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
  "due_date_field": "Custom.TargetDate",
  "assignee_user_mappings": {
    "aad.REPLACE_WITH_AZURE_DESCRIPTOR": 27
  },
  "timeout_seconds": 15
}
```

`due_date_field` is optional because Azure DevOps process templates do not all
use the same due-date field. `assignee_user_mappings` must map a stable Azure
IdentityRef `descriptor` (or its `id` fallback) to an existing portal user ID.
Unmapped identities remain unassigned in My Work; the portal deliberately does
not guess from `displayName`, `uniqueName`, or email.

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

Microsoft Graph Calendar adapter config:

```json
{
  "adapter": "microsoft_graph_calendar",
  "user_id": "me",
  "tenant_id": "organizations",
  "client_id": "...",
  "client_secret": "...",
  "refresh_token": "",
  "scope": "https://graph.microsoft.com/Calendars.Read offline_access",
  "lookahead_hours": 24,
  "top": 25,
  "timeout_seconds": 15
}
```

By default the adapter calls
`https://graph.microsoft.com/v1.0/me/calendarView`. Set `user_id` to a user
principal name to call `/users/<user_id>/calendarView`, or set
`calendar_view_url` for a custom proxy or mock server. `start_at` and `end_at`
can be provided explicitly as offset-aware RFC3339 instants; otherwise the
adapter imports the next `lookahead_hours` hours from the current time. The
adapter requests the default UTC Graph response and emits normalized `Z`
instants while retaining the source event's original time-zone label for
display. Legacy configs that set a non-UTC `time_zone` must complete the
documented UTC preflight/resync before the TIMESTAMPTZ migration. Events are
normalized into notification records with `Calendar: ...` titles,
organizer/location/time details, importance-derived severity, and Outlook or
Teams join links when available.

Microsoft Graph Mail adapter config:

```json
{
  "adapter": "microsoft_graph_mail",
  "user_id": "me",
  "mail_folder_id": "Inbox",
  "tenant_id": "organizations",
  "client_id": "...",
  "client_secret": "...",
  "refresh_token": "",
  "scope": "https://graph.microsoft.com/Mail.Read offline_access",
  "unread_only": true,
  "lookback_hours": 24,
  "top": 25,
  "timeout_seconds": 15
}
```

By default the mail adapter calls
`https://graph.microsoft.com/v1.0/me/messages`, or
`/me/mailFolders/<folder>/messages` when `mail_folder_id` is set. Set `user_id`
to a user principal name to call `/users/<user_id>/...`, or set `messages_url`
for a custom proxy or mock server. Messages are normalized into notification
records with `Mail: ...` titles, sender/received/preview details,
importance-derived severity, read state, and Outlook web links when available.

ERP private message adapter config:

```json
{
  "adapter": "erp_private_messages",
  "messages_url": "https://erp.example.test/api/private-messages",
  "bearer_token": "...",
  "api_key": "...",
  "api_key_header": "x-api-key",
  "unread_only": true,
  "lookback_hours": 24,
  "top": 25,
  "timeout_seconds": 15
}
```

The ERP adapter calls `messages_url`, `private_messages_url`, or `url`. Optional
`since`, `updated_after`, `received_after`, `lookback_hours`, `unread_only`,
`top`, and `limit` settings are appended as query parameters when provided. The
response can be `{ "items": [...] }`, `{ "messages": [...] }`,
`{ "private_messages": [...] }`, the same arrays under `data`, or a top-level
array. Messages are normalized into notification records using common fields
such as `id`, `message_id`, `request_id`, `approval_id`, `title`,
`request_type`, `message`, `summary`, `severity`, `priority`, `status`,
`requires_approval`, `is_read`, and `url`.

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

- `CONNECTOR_SECRET_KEY=<stable random secret of at least 32 bytes in production>`
- `CONNECTOR_WORKER_ENABLED=true`
- `CONNECTOR_SCHEDULER_ENABLED=true`
- `CONNECTOR_WORKER_POLL_MS=500`
- `CONNECTOR_WORKER_HEARTBEAT_INTERVAL_SECONDS=15`
- `CONNECTOR_WORKER_STALE_AFTER_SECONDS=45`
- `CONNECTOR_RUN_LEASE_SECONDS=60`
- `CONNECTOR_RUN_LEASE_RENEW_INTERVAL_SECONDS=15`
- `CONNECTOR_RUN_MAX_ATTEMPTS=3`
- `CONNECTOR_RUN_RETRY_BASE_SECONDS=5`
- `CONNECTOR_RUN_RETRY_MAX_SECONDS=300`
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
longer 365-day window. Every enabled retention pass also removes expired
sessions, expired OIDC login transactions, and inactive login-throttle buckets
older than 30 days; these authentication tables are bounded even when no new
interactive login occurs. Setting all three configurable history values to `0`
does not disable this authentication cleanup.

The worker writes heartbeat status to `connector_workers`, and each retention
cleanup writes a `maintenance_runs` history row. `GET /connectors/operations`
returns recent worker status and cleanup history for the Connectors operations
panel. Failed, partially successful, or cancelled connector runs can be retried with
`POST /connectors/runs/<id>/retry`, which creates a queued retry run using the
original run payload when available or the current connector config otherwise.
Only one queued/running retry may exist for an original run at a time. Queued
or running runs can be cancelled with `POST /connectors/runs/<id>/cancel`;
queued runs become `cancelled` immediately, while running runs record a request
that prevents a worker from committing a successful final state. Cancellation
is terminal and is never auto-requeued; an admin must explicitly call the retry
endpoint to create a fresh bounded-attempt child run.

Admins can change an existing connector between global, maintainer-team, and
private-user visibility with `PUT /connectors/<source>/scope` or the Edit
visibility control. The connector and all work cards/notifications previously
imported by it move scopes in one database transaction, so the connector and
its imported records cannot expose different audiences.

Each claimed run has its own database lease heartbeat, separate from the
worker-process heartbeat. Before claiming more work, workers recover expired
leases with bounded exponential backoff. A crashed run is requeued while
`attempt_count < max_attempts`; after the configured limit it becomes
`failed`. This makes crash retries bounded and keeps the scheduler from adding
another run for the same source/target while a delayed retry is pending.

Run the worker separately from the HTTP server:

```sh
cargo run --bin worker
```

An explicitly disabled worker (`CONNECTOR_WORKER_ENABLED=false`) exits
successfully. An enabled worker without `DATABASE_URL`, or with invalid
application configuration, exits non-zero so a process supervisor does not
mistake a missing worker for a healthy stopped service. The worker-enabled flag
accepts `true`/`false`, `1`/`0`, or `yes`/`no`; misspellings also fail startup.

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
