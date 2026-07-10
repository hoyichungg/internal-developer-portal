# Production and Pilot Runbook

This runbook is the minimum operating procedure for a real internal pilot of
the Internal Developer Portal. It assumes the release image is built from a
reviewed commit, PostgreSQL 16 is the database, Rocket serves both the API and
the built frontend on port `8000`, and the worker runs as a separate process.

The portal currently uses username/password login and bearer sessions. Until
an organization-approved identity provider, MFA, and centralized offboarding
are available, keep the pilot behind the corporate network or VPN. Do not
publish it directly to the public Internet.

## Ownership and target service levels

Assign these roles before deployment:

- release owner: approves the image digest and migration;
- database owner: owns backup, restore, and migration recovery;
- portal operator: owns health checks, worker health, and connector failures;
- security/contact owner: owns credentials, incident response, and user
  offboarding;
- pilot product owner: makes the final go/no-go decision.

Record an initial RPO and RTO. A practical pilot baseline is an RPO of 24 hours
and an RTO of 4 hours, with an extra backup immediately before every migration.
If the business needs a shorter RPO, `pg_dump` alone is insufficient: add
PostgreSQL WAL archiving or a managed point-in-time-recovery service.

## Network and TLS layout

Only the TLS reverse proxy should accept client traffic:

```text
Corporate client / VPN
        |
      HTTPS :443
        |
TLS reverse proxy
        |
  127.0.0.1:8000 or a private container network
        |
   Rocket app ---- private PostgreSQL :5432
        |
   separate worker (no inbound port)
```

Required controls:

1. Publish TCP `443` only. Port `80` may exist solely to redirect to HTTPS.
2. Bind the app's published port to `127.0.0.1`, or do not publish it at all
   when the proxy shares its private container network. The production Compose
   file's default port mapping has no host-IP restriction, so the deployment
   copy must change it to
   `127.0.0.1:${PORT:-8000}:${ROCKET_PORT:-8000}`, or the host firewall must
   deny remote access to `8000`. Verify the result; do not rely on intent.
3. Do not publish PostgreSQL. The worker needs no inbound listener.
4. Permit database access only from the app, worker, migration job, and backup
   job. Restrict host SSH and Docker access to named operators.
5. Use an organization-issued certificate, TLS 1.2 or newer, automatic renewal,
   and an expiry alert at 21 days.
6. Never disable certificate verification in probes or the smoke test.

Example Nginx edge configuration (adapt certificate paths and allow-lists):

```nginx
server {
    listen 80;
    server_name portal.internal.example;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl http2;
    server_name portal.internal.example;

    ssl_certificate     /etc/ssl/portal/fullchain.pem;
    ssl_certificate_key /etc/ssl/portal/private.key;
    ssl_protocols TLSv1.2 TLSv1.3;

    add_header Strict-Transport-Security "max-age=31536000" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header Referrer-Policy "same-origin" always;
    add_header X-Frame-Options "DENY" always;

    client_max_body_size 2m;

    location / {
        proxy_pass http://127.0.0.1:8000;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Host $host;
        proxy_set_header X-Forwarded-Proto https;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_connect_timeout 5s;
        proxy_read_timeout 120s;
    }
}
```

Test a Content Security Policy in report-only mode before enforcing it; an
untested strict policy can break the React/Mantine application. Configure the
proxy access log to omit `Authorization` headers and request bodies. Connector
credentials and login payloads must never appear in logs.

After starting the stack, verify listeners from the host:

```sh
ss -ltnp
docker compose --env-file .env.production -f docker-compose.prod.yml ps
```

From another machine, `443` should be reachable and `8000`/`5432` should not.

## Production environment and secrets

Start from `.env.production.example`, not `.env.example`. The effective values
must include:

- `APP_ENV=production` (already supplied by the production Compose file);
- `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB`, and `DATABASE_URL`;
- one stable, high-entropy `CONNECTOR_SECRET_KEY` of at least 32 bytes;
- a deliberate `AUTH_TOKEN_TTL_SECONDS` (the default is 86400 seconds; a pilot
  may prefer 28800 to require login every eight hours);
- worker, scheduler, heartbeat, lease, retry, retention, and port values.

The worker lease constraints are important:

- `CONNECTOR_RUN_LEASE_RENEW_INTERVAL_SECONDS` must be less than
  `CONNECTOR_RUN_LEASE_SECONDS`;
- `CONNECTOR_RUN_RETRY_MAX_SECONDS` must be greater than or equal to
  `CONNECTOR_RUN_RETRY_BASE_SECONDS`;
- `CONNECTOR_RUN_MAX_ATTEMPTS` must be a positive integer.

Use a secrets manager where available. If an env file is required, keep it
outside the Git checkout, owned by the deployment account, and readable only
by that account:

```sh
install -d -m 0700 /opt/internal-developer-portal/config
install -m 0600 .env.production \
  /opt/internal-developer-portal/config/.env.production
```

Never commit, attach, paste, or archive the effective env file. Limit access to
Docker/container inspection because privileged users can inspect container
environment variables. Avoid printing `docker compose config` into CI logs: it
can render interpolated secrets. URL-encode reserved characters in the
database password before placing it in `DATABASE_URL`.

Generate independent values for the database password, first admin password,
and connector master key. For example, generate the master key locally and
write it directly to the secret store without logging it:

```sh
openssl rand -base64 48
```

Imported notification bodies, work-card content, usernames, and other portal
records are normal database data, not application-encrypted fields. Require
encrypted database disks, encrypted backups, restricted DB roles, and an
approved retention period.

### Connector master-key warning

`CONNECTOR_SECRET_KEY` is a master encryption key, not an ordinary rotatable
API token. Existing connector secrets are AES-GCM ciphertext derived from the
exact current key. Changing the environment value immediately makes existing
connector credentials undecryptable. A database backup without the key cannot
recover them.

Do **not** rotate this value by editing the env file and restarting. The current
application has no dual-key/key-ring rotation flow. A planned rotation requires
a tested data migration that decrypts every secret with the old key and
re-encrypts it with the new key, verifies all connectors, and switches the app
and worker atomically. Retain the old key in the protected recovery store until
verification and the rollback window are complete. If the old key is lost,
every external connector credential must be recreated. Third-party tokens can
still be rotated normally by updating the individual connector while the
master key remains stable.

## First admin

The production stack never loads demo data or the development `admin123`
credential. Before starting the pilot:

1. Put a unique username and a generated password in the protected deployment
   environment as `SEED_ADMIN_USERNAME` and `SEED_ADMIN_PASSWORD`.
2. Keep `SEED_ADMIN_ROLES=admin,member`.
3. Run the one-shot profile:

   ```sh
   docker compose --env-file /opt/internal-developer-portal/config/.env.production \
     -f docker-compose.prod.yml --profile seed-admin run --rm seed-admin
   ```

4. Run the production smoke test with that named account.
5. Remove the three `SEED_ADMIN_*` values from the env file/secret injection and
   retain the password only in the approved password manager.
6. Create individual named accounts; do not share the first admin account.
   Keep at least two recovery admins owned by different people and test
   offboarding before the pilot.

`ensure-admin` does not replace an existing password unless reset is explicitly
requested. Password reset is a controlled recovery action and should be
recorded in the incident/change ticket.

## Backup and restore drill

### Backup policy

Use PostgreSQL custom format (`pg_dump -Fc`) because it is compressed,
inspectable, and supports selective/parallel restore. At minimum:

- run an automated nightly backup and a backup immediately before migration;
- encrypt backups, store a copy off-host, and record a SHA-256 checksum;
- retain a pilot baseline such as 7 daily and 4 weekly backups;
- alert when the latest verified backup exceeds the agreed RPO;
- perform a restore drill before the first pilot and at least quarterly.

Create and verify a pre-upgrade backup before stopping the old release. Replace
the database user/name if the deployment does not use the defaults:

```sh
umask 077
mkdir -p backups
release="$(date -u +%Y%m%dT%H%M%SZ)"
backup="backups/portal-pre-${release}.dump"

docker compose --env-file /opt/internal-developer-portal/config/.env.production \
  -f docker-compose.prod.yml exec -T postgres \
  pg_dump -U portal -d portal -Fc --no-owner --no-acl > "$backup"

test -s "$backup"
docker compose --env-file /opt/internal-developer-portal/config/.env.production \
  -f docker-compose.prod.yml exec -T postgres \
  pg_restore --list < "$backup" > /dev/null
sha256sum "$backup" > "${backup}.sha256"
sha256sum --check "${backup}.sha256"
```

A successful `pg_dump` command is not proof of recoverability. Restore to an
isolated PostgreSQL instance, never over the live database. This concrete drill
uses a temporary Docker container and volume; run it on a staging/restore host:

```sh
set -eu
drill="portal-restore-drill-$(date -u +%Y%m%dT%H%M%SZ)"
volume="${drill}-data"
export POSTGRES_PASSWORD="$(openssl rand -hex 32)"

docker volume create "$volume" > /dev/null
docker run -d --name "$drill" \
  --mount "source=$volume,target=/var/lib/postgresql/data" \
  -e POSTGRES_USER=portal_restore \
  -e POSTGRES_PASSWORD \
  -e POSTGRES_DB=portal_restore \
  postgres:16 > /dev/null

until docker exec "$drill" pg_isready -U portal_restore -d portal_restore; do
  sleep 2
done

docker exec -i "$drill" pg_restore \
  -U portal_restore -d portal_restore \
  --no-owner --no-acl --exit-on-error < "$backup"

docker exec "$drill" psql -U portal_restore -d portal_restore \
  -v ON_ERROR_STOP=1 -c '\\dt'
docker exec "$drill" psql -U portal_restore -d portal_restore \
  -v ON_ERROR_STOP=1 -c \
  'SELECT COUNT(*) AS users FROM users; SELECT COUNT(*) AS migrations FROM __diesel_schema_migrations;'

docker rm -f "$drill"
docker volume rm "$volume"
unset POSTGRES_PASSWORD
```

Use a shell `trap` in the operating team's automation so the temporary
container, volume, and password are cleaned up on failure. A full quarterly
drill must additionally point an isolated app/worker at the restored database,
run `/readyz` and the smoke test, execute a non-destructive connector canary,
measure elapsed restore time, and compare it with the RTO.

## Controlled deployment

Do not use an unqualified `docker compose up --build` as the production change
procedure. Keep the reviewed commit SHA, image ID/digest, database backup name,
operator, and timestamps in the change record.

Before the maintenance window:

1. Require green CI for `cargo fmt --check`, Clippy, frontend build/tests, and
   Rust tests.
2. Review every new `migrations/*/up.sql` and `down.sql`; identify locks,
   rewrites, destructive changes, and compatibility with the previous image.
3. Verify disk space for both the database and backup.
4. Verify the current `/livez`, `/readyz`, worker heartbeat, and latest backup.
5. Take and validate the pre-upgrade custom-format backup described above.
6. Preserve the currently running image by immutable registry digest. For the
   local Compose tag, preserve it before building:

   ```sh
   release="$(date -u +%Y%m%dT%H%M%SZ)"
   old_id="$(docker image inspect -f '{{.Id}}' internal-developer-portal:prod)"
   docker image tag "$old_id" "internal-developer-portal:rollback-$release"
   docker compose --env-file /opt/internal-developer-portal/config/.env.production \
     -f docker-compose.prod.yml build app
   ```

During the window, put the reverse proxy into maintenance/drain mode so no new
writes arrive, then stop both writers and run migration once:

```sh
docker compose --env-file /opt/internal-developer-portal/config/.env.production \
  -f docker-compose.prod.yml stop app worker

docker compose --env-file /opt/internal-developer-portal/config/.env.production \
  -f docker-compose.prod.yml run --rm migrate

docker compose --env-file /opt/internal-developer-portal/config/.env.production \
  -f docker-compose.prod.yml up -d --no-deps --force-recreate app worker
```

Remove maintenance mode only after all post-deploy checks pass:

```sh
curl --fail --silent --show-error https://portal.internal.example/livez
curl --fail --silent --show-error https://portal.internal.example/readyz
docker compose --env-file /opt/internal-developer-portal/config/.env.production \
  -f docker-compose.prod.yml ps
docker compose --env-file /opt/internal-developer-portal/config/.env.production \
  -f docker-compose.prod.yml logs --since 10m app worker
```

Then run the authenticated smoke test from a client on the same path users take:

```powershell
.\scripts\production-smoke.ps1 `
  -BaseUrl https://portal.internal.example `
  -Username pilot.admin
```

The password prompt is a `SecureString`. Do not place the password on the
command line. In CI, inject `PORTAL_SMOKE_USERNAME` and
`PORTAL_SMOKE_PASSWORD` from the CI secret store and omit the username/password
arguments. The script removes the password env variable from its child process,
keeps the bearer token in memory, never prints it, and always attempts logout.
If the overview is intentionally unavailable during a narrow diagnostic, add
`-SkipOverview`; the normal deployment gate must not skip it.

Finally, sign in through the UI and verify:

- the dashboard has the expected user/team-scoped records;
- `operations.worker_status` is `healthy`, at least one worker is active, and
  `latest_worker_seen_at` advances within the configured stale threshold;
- the admin Connector Operations screen shows no stale worker or unexpected
  `last_error`;
- one low-privilege canary connector can queue and finish a run, its imported
  data appears once, `current_run_id` clears, and bounded/incomplete runs do not
  archive records;
- health freshness and the latest retention cleanup are credible.

## Migration failure and rollback

If migration fails, keep maintenance mode enabled and keep app/worker stopped.
Capture the migration output, app image ID, PostgreSQL logs, and the contents of
`__diesel_schema_migrations`. Take a second forensic backup of the failed state
before manual repair. Do not overwrite the validated pre-upgrade backup.

Never blindly run Diesel `migration revert` or every `down.sql`. A down migration
may drop columns/tables, discard data, or be unsafe after only part of an up
migration ran. First determine whether PostgreSQL rolled the migration back,
whether any non-transactional operation completed, and whether the old app is
compatible with the resulting schema.

There are two distinct rollback cases:

1. **App-only failure with a backward-compatible schema:** retag/redeploy the
   preserved old image, start app/worker, and run the smoke test. Do not revert
   the database merely because the app image is rolled back.
2. **Incompatible or partially migrated schema:** restore the pre-upgrade dump
   into a new database, point the preserved old image at that restored database,
   validate it in isolation, then switch traffic. Do not restore over the only
   copy of the failed database. Because writes were drained before backup and
   migration, the recovery point is unambiguous.

Example app-only image rollback (only after the database owner confirms schema
compatibility):

```sh
docker image tag internal-developer-portal:rollback-RELEASE \
  internal-developer-portal:prod
docker compose --env-file /opt/internal-developer-portal/config/.env.production \
  -f docker-compose.prod.yml up -d --no-deps --force-recreate app worker
```

Keep maintenance mode until `/readyz`, authenticated smoke, worker heartbeat,
and the canary connector pass. Document the decision and exact recovery point.

## Daily operations and alerting

Use the reverse proxy, container runtime, PostgreSQL monitoring, the Connector
Operations API/UI, and `/me/overview`. Suggested pilot alerts:

| Signal | Initial threshold | Operator action |
| --- | --- | --- |
| `/livez` | 2 failures in 2 minutes | Check app crash/restarts, CPU, memory, and logs. |
| `/readyz` | 2 failures in 2 minutes | Check PostgreSQL reachability, pool exhaustion, locks, and disk. If `/livez` is healthy, prioritize the DB path. |
| HTTP 5xx | More than 1% for 5 minutes | Correlate proxy/app logs; protect credentials when collecting evidence. |
| TLS certificate | Less than 21 days remaining | Repair automatic renewal and verify the full chain. |
| Worker heartbeat | No active worker or heartbeat older than `CONNECTOR_WORKER_STALE_AFTER_SECONDS` plus 30 seconds | Check worker process, DB, lease renewal, and `last_error`; avoid starting duplicate ad-hoc workers blindly. |
| Connector queue | A queued run is older than 5 minutes, or a running lease is expired | Inspect worker and retry state; use supported cancel/retry controls rather than editing status rows. |
| Connector result | Repeated `failed`/`partial_success`, or attempts reach `max_attempts` | Check scoped credentials, upstream availability/rate limits, and item errors. |
| Health freshness | `health_data_stale=true` for two checks | Check health connector schedule and latest successful run. |
| Retention cleanup | No successful cleanup for two configured cleanup intervals | Check worker logs and DB permissions; watch table growth. |
| Backup | Latest verified backup older than RPO, checksum/list verification failure, or off-host copy failure | Page the DB owner and block schema deployment. |
| PostgreSQL | Disk above 80%, sustained connection saturation, replication/PITR failure if configured | Increase capacity or remove the cause before data loss/outage. |
| Security | Unusual 401 rate, admin use, or connector-config changes | Review proxy/audit logs, disable affected account/token, follow incident procedure. |

Daily operator review:

- app/worker restart count and error logs;
- worker heartbeat, current run, and last error;
- failed/partial connector runs, exhausted attempts, and stale queued/running
  work;
- stale service-health data and priority notifications;
- latest successful retention cleanup and table/disk growth;
- latest backup timestamp, verification, and off-host copy;
- expiring upstream OAuth tokens/PATs and TLS certificates;
- admin/audit activity and departed-user access.

Do not fix queue state with direct SQL during normal operations. Preserve an
incident timeline and take a backup before exceptional database intervention.

## Pilot acceptance checklist

The product owner, operator, database owner, and security owner should all sign
the following checklist. A pilot is not accepted merely because the home page
loads.

### Security and access

- [ ] Portal is reachable only through corporate network/VPN and TLS; external
      scans confirm `8000` and `5432` are closed.
- [ ] Certificate chain, renewal, HSTS, frame protection, and no-sniff headers
      are verified in a browser/proxy test.
- [ ] There are no default/shared credentials; every pilot user has a named
      account and an owner for offboarding.
- [ ] Admin, maintainer owner, maintainer writer, and read-only member
      permissions were tested with separate accounts.
- [ ] User-scoped and maintainer-scoped connector/work/notification data cannot
      be listed or opened by an unrelated user (including direct URL/API tests).
- [ ] Connector config round-trips preserve redacted secrets and secrets do not
      appear in API responses, browser logs, app logs, or audit metadata.
- [ ] Env files, connector master key, database storage, and backups meet the
      organization's access/encryption policy.

### Reliability and recovery

- [ ] CI validation is green for the deployed commit and the deployed image
      digest is recorded.
- [ ] Pre-deploy `pg_dump -Fc`, `pg_restore --list`, checksum, encryption, and
      off-host copy all succeed automatically.
- [ ] A full isolated restore plus app smoke test completes inside the agreed
      RTO and meets the RPO.
- [ ] Migration-failure and app-only rollback procedures were rehearsed without
      blindly applying down migrations.
- [ ] Killing a worker during a canary run allows the expired lease to recover;
      attempts remain bounded and no duplicate active processing remains.
- [ ] App and worker restarts leave `/readyz`, heartbeat, scheduler, and
      retention cleanup healthy.

### Useful daily workflow

- [ ] At least two representative engineering teams and 5-10 named users use
      the pilot for at least two working weeks.
- [ ] Each enabled connector uses a least-privilege production credential, has
      a named owner, a tested sample payload, an expected schedule/freshness
      target, and a recovery contact.
- [ ] `/me/overview` shows the correct team services, today's structured
      calendar events, packages, open work, notifications, health freshness,
      and worker state for each pilot persona.
- [ ] A connector run becomes visible with useful item errors and run history;
      retry/cancel behavior is understood by operators.
- [ ] Full-snapshot reconciliation is enabled only for connectors whose source
      query is truly complete; a partial/error/capped canary proves existing
      records are retained, while a complete canary records `archived_count`.
- [ ] Per-user notification read, unread, dismiss, snooze, and restore state
      persists without changing another user's state.
- [ ] Dashboard and primary record screens are usable on the supported desktop
      browser, with no blocking console/API errors.
- [ ] Operators can identify stale data and a failed worker within five minutes
      using the documented alerts/UI.
- [ ] Pilot users know where to report data errors and security incidents, and
      the on-call owner can disable a connector or account.

### Go/no-go record

- [ ] Known limitations, accepted risks, retention policy, RPO/RTO, support
      hours, success metrics, pilot start/end dates, and rollback owner are in
      the change record.
- [ ] All critical/high findings are closed; every remaining finding has an
      owner and due date.
- [ ] Product, operations, database, and security owners explicitly approve the
      pilot. Any failed security isolation, restore, migration, smoke, or worker
      recovery check is an automatic no-go.
