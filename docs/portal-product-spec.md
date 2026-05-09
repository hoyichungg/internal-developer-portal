# Internal Developer Portal Product Spec

This document is the product memory for the repository. Future contributors and
coding agents should read this before changing product direction, architecture,
or major user flows.

## Product Goal

This project should become an Internal Developer Portal: a daily engineering
workspace that helps a team start the workday from one home page.

When a user opens the portal in the morning, they should quickly understand
today's important work, messages, service health, and system events without
switching between DevOps, Outlook, ERP, monitoring tools, and internal systems.

The home page should eventually show:

- Team calendar and today's meetings.
- DevOps work cards and active tasks.
- Outlook mail or important message summaries.
- Internal ERP private messages and pending requests.
- Server and service machine state.
- Application monitoring records.
- Connector/plugin import and run status.
- Packages, services, work cards, and notifications related to the user's team.

The goal is not a marketing landing page. The goal is a real operational work
surface that engineering teams can use every day.

## System Architecture

Use a frontend/backend split so the frontend and backend can be developed and
tested independently. The current default deployment is hybrid: the frontend is
served separately by Vite during development through API proxying, while Rocket
serves the built `frontend/dist` files for a single-origin app. A fully
independent frontend deployment should remain possible because the frontend
talks to the backend through HTTP APIs.

- Backend API runs on port `8000` by default.
- Frontend talks to the backend through HTTP API requests.
- External systems should be integrated through connector/plugin boundaries.
- During development, specific connectors/plugins, sample payloads, or mock
  servers can be enabled for testing.

The system is organized into three layers:

1. Data Layer
2. Logic Layer
3. Presentation Layer

Each layer should have clear responsibilities and should remain customizable.

## Data Layer

The Data Layer manages and stores application data. It should provide
repository/helper methods so developers can quickly and consistently read and
write data.

The Data Layer includes:

- Users, roles, and sessions.
- Maintainer ownership and membership.
- Packages, services, work cards, and notifications.
- Connector registry.
- Connector config, sample payloads, and encrypted secrets.
- Connector runs, item-level errors, scheduler state, and worker claim state.
- Connector run item snapshots and retention-managed history.
- Connector worker heartbeat and maintenance run history.
- Audit logs with retention-managed history.

Data Layer principles:

- Manage schema with Diesel migrations.
- Encapsulate common reads and writes behind repository methods.
- Keep common lists, dashboard queries, and connector run queries ordered and
  indexed.
- Store secrets encrypted.
- Return only redacted secret values from APIs.
- Never persist `***redacted***` as a real credential during config round trips.

## Logic Layer

The Logic Layer provides the backend application behavior: API routes,
validation, authorization, connector runtime, worker/scheduler behavior, and
data normalization.

The Logic Layer includes:

- Rocket API routes.
- Consistent API responses.
- Auth, session, and role handling.
- Maintainer ownership write access.
- Connector import endpoints.
- Connector runtime: manual runs, queued runs, and scheduled runs.
- Worker/scheduler behavior that is safe with multiple workers.
- Worker heartbeat, retention cleanup history, failed run retry, and stale-data
  warnings.
- External adapters such as Azure DevOps, Outlook, ERP, monitoring, and
  calendar systems.
- Validation, normalization, error handling, and audit logging.

Logic Layer principles:

- Successful API responses use `{ "data": ... }`.
- Error responses use `{ "error": { "code", "message", "details" } }`.
- Worker claim and scheduler enqueue logic must use transactions or DB locks to
  avoid races.
- Every connector execution should write run history.
- Item-level failures should be traceable and debuggable.
- Health, connector run, and audit history should have retention so
  high-frequency monitoring and append-only logs do not grow storage without
  bound.
- Operators should be able to tell when workers are alive, when retention last
  ran, and whether homepage health data is stale.
- Adapters should be testable with sample payloads or mock servers.

## Presentation Layer

The Presentation Layer provides a consistent, reusable, data-first frontend.
The built-in frontend should provide cards, tables, charts, badges, loading
states, empty states, and error handling so future features can use the same
design language.

The Presentation Layer includes:

- Dashboard / morning work view.
- Metric cards.
- Data tables.
- Status badges.
- Connector registry.
- Connector runtime config editor.
- Service overview.
- Catalog view.
- Audit/log view.

Presentation Layer principles:

- The first screen should be the actual work surface, not a landing page.
- Prioritize actionable and scannable information.
- Keep the UI dense but clear for daily engineering and operations work.
- Handle loading, error, refresh, and stale response states.
- The connector config editor must not allow redacted secrets to overwrite real
  stored secrets.

## Plugin And Connector Model

External systems should enter through connector/plugin boundaries. Each
connector should describe:

- `source`: the concrete data source, such as `azure-devops`, `outlook`, `erp`,
  or `monitoring`.
- `kind`: connector type, such as `azure_devops`, `outlook`, `erp`, or
  `monitoring`.
- `target`: import target, such as `work_cards`, `notifications`, or
  `service_health`.
- `config`: adapter settings. Secrets must be encrypted.
- `sample_payload`: development and testing payload.
- Run history: status, success count, failure count, duration, and item errors.
- Schedule metadata: enabled, schedule, next run, and last scheduled run.

When adding a connector/plugin, consider:

- Can it run manually?
- Can it queue work for a worker?
- Can it run on a schedule?
- Can it be tested with a sample payload?
- Can it be tested against a mock server for real API adapter behavior?

## Morning Workflow

"Start a new workday" is the most important product scenario.

After opening the home page, a user should be able to:

1. See today's meetings, messages, and tasks.
2. Identify services, systems, or connector runs that need attention.
3. Jump into related service, package, work card, or notification detail.

This experience should eventually integrate:

- Calendar connector.
- DevOps connector.
- Outlook/mail connector.
- ERP/private-message connector.
- Monitoring/service-health connector.

## Current Alignment Notes

The project already has:

- Rust/Rocket backend.
- PostgreSQL + Diesel Async.
- React/Vite/Mantine frontend.
- Auth, sessions, and roles.
- Maintainer ownership.
- Packages, services, work cards, and notifications.
- Connector registry, config, and runs.
- Worker and scheduler.
- Audit logs.
- Azure DevOps work card adapter.
- Generic monitoring service health adapter.
- Service health check history for homepage trend and incident summaries.
- Frontend dashboard, catalog, connectors, audit, and service overview views.

Future improvements can include:

- Calendar connector.
- Outlook mail connector.
- ERP private message connector.
- More complete home page aggregation.
- OpenAPI or API docs.
- Deeper charts, alerting, and incident drill-down workflows.

## Definition Of Done

Before finishing a change, confirm:

- The feature follows the connector/plugin boundary or existing API style.
- APIs do not leak secrets.
- Permissions follow admin and maintainer ownership rules.
- Common queries have stable ordering and indexes when needed.
- Frontend screens include loading, error, and empty states.
- `cargo fmt --check` passes.
- `cargo clippy --all-targets -- -D warnings` passes.
- `npm run build --prefix frontend` passes.
- `cargo test` passes.
