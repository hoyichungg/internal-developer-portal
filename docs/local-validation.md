# Local Validation Workflow

Windows locks a running executable. If `target\debug\server.exe` or
`target\debug\worker.exe` is running, `cargo test` can fail before tests start
because Cargo cannot replace those files.

Use the validation script instead of remembering the manual stop/start dance.

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
cargo test --lib
```

It does not stop the server or worker, so it avoids the Windows file-lock
problem by running only library tests. The script gives the library test step
an unreachable local `DATABASE_URL`; these tests are intentionally
database-free. PostgreSQL-backed retention coverage lives in the full
integration suite instead.

## Full Validation

Use this before committing larger changes or when integration tests matter:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\local-validate.ps1 -Mode Full
```

Full mode:

1. Finds running `server.exe` and `worker.exe` processes from this repository.
2. Stops them so Cargo can update `target\debug`.
3. Stops running Docker Compose `app` / `worker` services so an older container
   cannot claim port `8000` while validation is running.
4. Runs formatting, frontend build, frontend regression tests, and Clippy.
5. Builds server and worker into `target\local-services`.
6. Starts those isolated binaries for integration tests.
7. Runs `cargo test`.
8. Stops the isolated services.
9. Restarts the local services that were running before the script started.

The retention integration test uses a separate database and refuses to run
unless both safeguards pass:

- `APP_ENV` is exactly `test` (the script sets this for the test process).
- PostgreSQL's actual `current_database()` name contains a standalone `test`
  segment, such as `portal_retention_test`.

Create and migrate the default dedicated database once while the Compose
PostgreSQL service is running:

```powershell
docker compose up -d postgres
docker compose exec postgres createdb -U postgres portal_retention_test
docker compose run --rm -e DATABASE_URL=postgres://postgres:postgres@postgres/portal_retention_test migrate diesel migration run
```

`RETENTION_TEST_DATABASE_URL` defaults to
`postgres://postgres:postgres@localhost:5432/portal_retention_test`. The
retention test never uses the normal application `DATABASE_URL`. The ordinary
isolated worker started by Full mode has all retention paths disabled; only the
guarded retention integration test performs cleanup.

Because the integration-test services run from `target\local-services`, Cargo
can freely rebuild `target\debug` during `cargo test`.

If a local `server.exe` was running before validation, the script restores that
local server and leaves Compose `app` / `worker` stopped. That prevents the old
container image from taking over port `8000` after the local server is stopped.

## Useful Options

Run against another database:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\local-validate.ps1 -Mode Full `
  -DatabaseUrl "postgres://postgres:postgres@localhost:5432/app_db" `
  -RetentionTestDatabaseUrl "postgres://postgres:postgres@localhost:5432/portal_retention_test"
```

The retention database may use a different name, but its name must include a
standalone `test` segment and it must already have all migrations applied.

Use another HTTP port for the isolated server:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\local-validate.ps1 -Mode Full -Port 8001
```

Use a different connector secret key when validating a database seeded with a
different key:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\local-validate.ps1 -Mode Full -ConnectorSecretKey "dev-connector-secret-key"
```

Leave services stopped after validation:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\local-validate.ps1 -Mode Full -NoRestart
```

Logs are written under `target\local-validation-logs`.

## Manual Fallback

If you do not want to use the script, either run the fast test target:

```powershell
cargo test --lib
```

or stop any running `target\debug\server.exe` and `target\debug\worker.exe`
before running. If Docker Compose `app` / `worker` are running, stop those too
so the tests do not hit an older server on port `8000`:

```powershell
docker compose stop app worker
cargo test
```

For the manual full command, set `APP_ENV=test` and
`RETENTION_TEST_DATABASE_URL` to the migrated dedicated test database first.
