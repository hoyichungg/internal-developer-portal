# Project Memory

This repository is an Internal Developer Portal. Keep future work aligned with
the product and architecture goals in `docs/portal-product-spec.md`.

## Product Intent

The portal should help an engineering team start the workday from one home page:
team calendar, DevOps work cards, Outlook mail, ERP private messages, service
machine state, application monitoring records, connector run state, and key
system notifications should be visible at a glance.

## Architecture Direction

- Use a frontend/backend split. The backend API should run on port `8000` by
  default. The frontend calls the backend through API requests.
- The current deployment is hybrid: Vite is separate during frontend
  development through API proxying, while Rocket serves the built
  `frontend/dist` files for the default single-origin app.
- Keep the system organized into three layers:
  - Data layer: users, sessions, roles, ownership, connector config, imported
    records, audit logs, and persistence helpers.
  - Logic layer: Rocket API routes, authorization, connector execution,
    scheduler/worker behavior, validation, encryption, and normalization.
  - Presentation layer: React/Mantine views, information cards, tables, charts,
    and consistent reusable UI components.
- Prefer plugin/connector boundaries for external systems. A connector should be
  testable with stored sample payloads and should record run history.

## Implementation Priorities

- Preserve secrets on connector config round trips. Never write redacted values
  back as real credentials.
- Keep connector scheduler and worker operations safe for multiple workers.
- Keep connector health, run history, and audit logs bounded with retention
  cleanup.
- Keep worker heartbeat, maintenance history, retry, and stale-data warnings
  visible enough for daily operations.
- Keep API responses consistent with `{ "data": ... }` and structured error
  bodies.
- Keep read/write permissions aligned with admin and maintainer ownership rules.
- Keep frontend screens operational and data-first; avoid marketing pages.
- Add tests for backend behavior that changes API, authorization, connector, or
  worker semantics.

## Validation

Use these checks before considering work complete:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
pnpm --dir frontend build
cargo test
```
