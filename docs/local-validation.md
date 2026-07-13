# Local Validation Workflow

Windows locks a running executable. If `target\debug\server.exe` or
`target\debug\worker.exe` is running, `cargo test` can fail before tests start
because Cargo cannot replace those files. Use the validation script instead of
manually stopping and restarting services.

## Fast Loop

Use this while the local server or worker is running:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\local-validate.ps1 -Mode Fast
```

Fast mode runs:

```text
cargo fmt --check
pnpm --dir frontend build
pnpm --dir frontend test:run
cargo clippy --all-targets -- -D warnings
cargo test --lib -- --skip repository_db_tests
```

It does not stop services, recreate databases, or run PostgreSQL-backed
repository tests. The library test process receives an unreachable database URL
whose database name still has a standalone `test` segment, so an accidentally
enabled database guard fails closed instead of ever reaching the development
database.

## Full Validation

Use this before a commit or whenever integration behavior changes:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\local-validate.ps1 -Mode Full
```

Full mode:

1. Validates all database URLs before any destructive command is possible.
2. Stops this repository's host `server.exe` / `worker.exe` processes and any
   running Compose `app` / `worker` services.
3. Captures exact row counts for every public table in the development
   database (`app_db` by default).
4. Builds the current workspace's `migrate` image, force-drops and recreates
   both disposable test databases, runs every migration, and verifies
   `current_database()` and the latest migration version.
5. Runs formatting, frontend build and tests, and Clippy.
6. Builds and starts isolated server/worker binaries against the integration
   test database, then runs the full Rust test suite.
7. Stops the isolated services, proves the development database row counts are
   unchanged, and only then restores the services that were running before.

The default disposable databases are:

- `PORTAL_TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/portal_integration_test`
- `RETENTION_TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/portal_retention_test`

All repository, authentication, Entra, worker-lease, and HTTP integration tests
use `PORTAL_TEST_DATABASE_URL`. Only the global retention cleanup test uses
`RETENTION_TEST_DATABASE_URL`. Tests never fall back to `DATABASE_URL` or
`app_db`.

Full mode refuses to reset a database unless its URL:

- uses `postgres://` or `postgresql://`;
- points to loopback port `5432` with the local Compose `postgres:postgres`
  credentials;
- contains one simple database name with a standalone `test` segment; and
- names neither the other test database nor the development database.

After migration, the script independently queries PostgreSQL and rejects an
unexpected `current_database()` or migration version. Test writes also require
`APP_ENV=test` and repeat the actual-database-name check in Rust. These checks
are intentional safety boundaries; do not weaken them to make a custom setup
pass.

`PORTAL_TEST_BASE_URL` controls the isolated HTTP origin used by both the server
and HTTP integration tests. It defaults to `http://127.0.0.1:8000` and must be a
plain loopback HTTP origin without credentials, path, query, or fragment.

Because isolated services run from `target\local-services`, Cargo can freely
rebuild `target\debug` during `cargo test`.

## Useful Options

Use alternate disposable database names and an alternate test origin:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\local-validate.ps1 -Mode Full `
  -PortalTestDatabaseUrl "postgres://postgres:postgres@localhost:5432/my_portal_test" `
  -RetentionTestDatabaseUrl "postgres://postgres:postgres@localhost:5432/my_retention_test" `
  -PortalTestBaseUrl "http://127.0.0.1:8001"
```

`-DatabaseUrl` remains an alias for `-PortalTestDatabaseUrl`; therefore passing
`app_db` to it is rejected. `-DevelopmentDatabaseUrl` identifies only the
non-test database whose row counts are protected and whose host processes are
restored after validation. Full mode currently supports the local Compose
PostgreSQL instance for all three URLs.

Use a different connector secret when required by test fixtures:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\local-validate.ps1 -Mode Full `
  -ConnectorSecretKey "dev-connector-secret-key"
```

Leave previously running services stopped after validation:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\local-validate.ps1 -Mode Full -NoRestart
```

Logs are written under `target\local-validation-logs`.

## Manual Fallback

For database-free work, run:

```powershell
cargo test --lib -- --skip repository_db_tests
```

For a manual full run, stop all app/worker writers, create and migrate both
dedicated databases, and keep the process variables consistent:

```powershell
$env:APP_ENV = "test"
$env:PORTAL_TEST_DATABASE_URL = "postgres://postgres:postgres@localhost:5432/portal_integration_test"
$env:RETENTION_TEST_DATABASE_URL = "postgres://postgres:postgres@localhost:5432/portal_retention_test"
$env:PORTAL_TEST_BASE_URL = "http://127.0.0.1:8000"
$env:DATABASE_URL = $env:PORTAL_TEST_DATABASE_URL
$env:ROCKET_DATABASES = '{postgres={url="postgres://postgres:postgres@localhost:5432/portal_integration_test"}}'
```

Run migrations against each URL before starting the test server and worker.
Prefer the Full script because it also starts the correct binaries, verifies
the real database identities, isolates retention cleanup, and proves the
development database was untouched.
