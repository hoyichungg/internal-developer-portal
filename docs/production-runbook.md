# Production and Pilot Runbook

This runbook is the minimum operating procedure for a real internal pilot of
the Internal Developer Portal. It assumes the release image is built from a
reviewed commit, PostgreSQL 16 is the database, Rocket serves both the API and
the built frontend on port `8000`, and the worker runs as a separate process.

The portal supports local username/password login and optional single-tenant
Microsoft Entra ID sign-in. Browsers authenticate with an `HttpOnly`,
`SameSite=Lax` session cookie. Local `POST /login` is cookie-only and never
returns the raw session credential in JSON. Keep the pilot behind the
corporate network or VPN even after Entra rollout, and apply the organization's
MFA, Conditional Access, assignment, and offboarding policy before accepting it
for daily use.

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
   file defaults to
   `127.0.0.1:${PORT:-8000}:${ROCKET_PORT:-8000}`. Do not remove the host-IP
   prefix unless a reviewed private-network design replaces it. Verify the
   effective listener; do not rely on the Compose source alone.
3. Do not publish PostgreSQL. The worker needs no inbound listener.
4. Permit database access only from the app, worker, migration job, and backup
   job. Restrict host SSH and Docker access to named operators.
5. Use an organization-issued certificate, TLS 1.2 or newer, automatic renewal,
   and an expiry alert at 21 days.
6. Never disable certificate verification in probes or the smoke test.

Example Nginx edge configuration (adapt certificate paths and allow-lists):

```nginx
# Put these directives in the http context. The callback log format deliberately
# uses $uri, never $request_uri or $request, so code/state query values are absent.
log_format portal_no_query '$remote_addr - $remote_user [$time_local] '
                           '"$request_method $uri $server_protocol" $status $body_bytes_sent '
                           '"$http_user_agent"';
# When and only when a reviewed LB/WAF sits directly in front of Nginx, replace
# the example CIDR with that proxy network. This makes $remote_addr and the
# rate-limit key the original client rather than one shared proxy address.
# set_real_ip_from 10.20.0.0/16;
# real_ip_header X-Forwarded-For;
# real_ip_recursive on;
limit_req_zone $binary_remote_addr zone=portal_auth_start:10m rate=10r/m;
limit_req_zone $binary_remote_addr zone=portal_password_login:10m rate=20r/m;
limit_req_zone $binary_remote_addr zone=portal_auth_callback:10m rate=120r/m;

server {
    listen 80;
    server_name portal.internal.example;
    # Also omit query strings on the HTTP redirect listener. A mistaken HTTP
    # callback must not be copied into the default `$request` access log.
    access_log /var/log/nginx/portal-http-redirect.log portal_no_query;
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
    limit_req_status 429;

    location / {
        proxy_pass http://127.0.0.1:8000;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Host $host;
        proxy_set_header X-Forwarded-Proto https;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_connect_timeout 5s;
        proxy_read_timeout 120s;
    }

    location = /auth/entra/start {
        limit_req zone=portal_auth_start burst=5 nodelay;
        proxy_pass http://127.0.0.1:8000;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Host $host;
        proxy_set_header X-Forwarded-Proto https;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Real-IP $remote_addr;

        add_header Strict-Transport-Security "max-age=31536000" always;
        add_header X-Content-Type-Options "nosniff" always;
        add_header Referrer-Policy "no-referrer" always;
        add_header X-Frame-Options "DENY" always;
        add_header Cache-Control "no-store" always;
    }

    location = /login {
        limit_req zone=portal_password_login burst=10 nodelay;
        proxy_pass http://127.0.0.1:8000;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Host $host;
        proxy_set_header X-Forwarded-Proto https;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Real-IP $remote_addr;
    }

    location = /auth/entra/callback {
        # Keep a separate, more generous callback budget for shared enterprise
        # egress/NAT addresses; never reuse the tighter password-login limit.
        limit_req zone=portal_auth_callback burst=30 nodelay;
        access_log /var/log/nginx/portal-auth-callback.log portal_no_query;
        # Nginx error records can include the complete request line even at high
        # severity. Suppress this location's error stream and rely on the
        # sanitized access status/metrics for callback failures.
        error_log /dev/null crit;
        proxy_pass http://127.0.0.1:8000;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Host $host;
        proxy_set_header X-Forwarded-Proto https;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Real-IP $remote_addr;

        add_header Strict-Transport-Security "max-age=31536000" always;
        add_header X-Content-Type-Options "nosniff" always;
        add_header Referrer-Policy "no-referrer" always;
        add_header X-Frame-Options "DENY" always;
        add_header Cache-Control "no-store" always;
        add_header Pragma "no-cache" always;
        add_header Expires "0" always;
    }

    location = /oauth/microsoft/callback {
        # The Microsoft Graph Connector authorization response also carries
        # code/state in the query string and needs the same log/referrer rules.
        limit_req zone=portal_auth_callback burst=30 nodelay;
        access_log /var/log/nginx/portal-connector-oauth-callback.log portal_no_query;
        error_log /dev/null crit;
        proxy_pass http://127.0.0.1:8000;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Host $host;
        proxy_set_header X-Forwarded-Proto https;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Real-IP $remote_addr;

        add_header Strict-Transport-Security "max-age=31536000" always;
        add_header X-Content-Type-Options "nosniff" always;
        add_header Referrer-Policy "no-referrer" always;
        add_header X-Frame-Options "DENY" always;
        add_header Cache-Control "no-store" always;
        add_header Pragma "no-cache" always;
        add_header Expires "0" always;
    }
}
```

Test a Content Security Policy in report-only mode before enforcing it; an
untested strict policy can break the React/Mantine application. Configure the
proxy access log to omit `Authorization` headers, request bodies, and the query
string on both `/auth/entra/callback` and `/oauth/microsoft/callback`;
authorization codes, state values, connector credentials, and login payloads
must never appear in logs. Pass callback paths and queries through unchanged,
do not cache `/auth/config` or either OAuth callback, and ensure source-IP
limits intended for local `POST /login` do not block legitimate callbacks.

The callback log format also omits `Referer`; the portal cannot rely on every
identity provider and browser to remove authorization query parameters before
sending that header. The callback-specific `/dev/null` error log is deliberate:
Nginx error records at any severity may contain the complete request line,
including OAuth query parameters. Do not replace it with a normal file or
syslog target unless a tested intermediary removes request/query context first.
Validate the same redaction at every outer load balancer, WAF, CDN, APM agent,
tracing collector, ingress controller, and centralized error-log pipeline
before enabling Entra. Use the sanitized callback status log and status-only
metrics for routine diagnosis; reproduce detailed failures in a controlled
environment without real authorization codes.

The application also enforces a global cap of 10,000 unexpired OIDC login
transactions under a PostgreSQL advisory lock, and the hourly retention pass
deletes expired OIDC transactions, expired sessions, and inactive
login-throttle buckets older than 30 days. The edge IP limit remains required:
the cap bounds disk growth but is not a substitute for abuse prevention. Keep
distinct edge limits for password login, Entra start, and OAuth callbacks; the
callback budget must be large enough for the organization's shared egress
addresses while still bounding invalid-state traffic before it occupies the
database pool. Monitor edge 429 counts and tune from measured pilot traffic
rather than disabling the limits.

If an upstream LB or WAF exists, configure Nginx's real-IP module with only its
exact source CIDRs before enabling these limits, then verify two distinct test
clients produce distinct `$remote_addr` values. Never trust forwarded IP
headers from arbitrary internet or LAN clients. Without this step all users
behind the proxy share one rate-limit bucket; with an overbroad trusted CIDR an
attacker can spoof buckets.

The production server and Compose file force `ROCKET_LOG_LEVEL=critical`.
Rocket's normal request log includes the full URI, so raising it back to
`normal` or `debug` would copy Entra or Graph Connector callback authorization
codes and state into application logs. Warning/error diagnostics remain enabled
at `critical`.

The supported Entra deployment is single-origin: the browser-visible frontend,
`/auth/*`, `/sessions/*`, and the registered callback URI must use the same
scheme and host. Rocket serving `frontend/dist` is the default. The Vite
development server is supported through its API proxy. A separate public
frontend origin is not supported until its cookie, CORS, CSRF, and callback
trust boundary has been designed and tested.

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
- a deliberate `AUTH_MAX_ACTIVE_SESSIONS_PER_USER` (default 20, supported range
  1-100) sized for the expected browser and device count;
- `AUTH_COOKIE_SECURE=true` (production refuses to start when it is false);
- deliberate login-throttle values: `AUTH_LOGIN_MAX_FAILURES` (per
  username/client-IP, default 5), `AUTH_LOGIN_ACCOUNT_MAX_FAILURES`
  (account-wide, default 50),
  `AUTH_LOGIN_WINDOW_SECONDS` (default 900), and
  `AUTH_LOGIN_LOCKOUT_SECONDS` (default 900);
- an explicit authentication mode using `AUTH_PASSWORD_LOGIN_ENABLED` and
  `AUTH_ENTRA_ENABLED`; at least one must be true;
- worker, scheduler, heartbeat, lease, retry, retention, and port values.

The effective Compose configuration must expose `CONNECTOR_SECRET_KEY` only to
`app` and `worker`, which encrypt/decrypt Connector credentials. The `migrate`
and `seed-admin` jobs need database credentials but must not receive this master
key. Likewise, Entra client/OIDC secrets belong only to `app`.

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

Generate separate random values for `CONNECTOR_SECRET_KEY` and
`AUTH_OIDC_TRANSACTION_KEY`; they have different purposes and lifecycles. When
Entra is enabled, keep its client secret and OIDC transaction key scoped to the
HTTP app. `docker-compose.prod.yml` deliberately does not expose them to the
worker, migration job, or `seed-admin` job. The current server reads these
values from environment variables, not `*_FILE` settings, so use the
orchestrator's protected environment-secret injection or the locked-down env
file described above.

Imported notification bodies, work-card content, usernames, and other portal
records are normal database data, not application-encrypted fields. Require
encrypted database disks, encrypted backups, restricted DB roles, and an
approved retention period.

### Microsoft Entra ID application registration

Use a dedicated Portal app registration. Do not reuse the Microsoft Graph
Connector registration: interactive portal sign-in and connector Graph access
have different scopes, owners, credentials, and rollback paths.

In the Entra admin center:

1. Register a **single-tenant** application for accounts in the organization's
   directory only. Record the Directory (tenant) ID and Application (client) ID;
   both must be UUIDs.
2. Add a **Web** redirect URI that exactly matches
   `AUTH_ENTRA_REDIRECT_URI`, for example
   `https://portal.internal.example/auth/entra/callback`. The server route is
   fixed, so the path must be exactly `/auth/entra/callback`, with no path prefix
   or trailing slash. Do not add a wildcard, query, fragment, localhost URI, or
   HTTP production URI.
3. Leave implicit and hybrid access/ID token issuance disabled. The portal uses
   the authorization-code flow with an S256 PKCE challenge, plus confidential
   client authentication during the server-side token exchange. Its sign-in
   request is limited to `openid profile email`; do not add Graph permissions or
   `offline_access` merely to enable portal login.
4. Create a client secret, store its value immediately in the approved secret
   manager, and configure it as `AUTH_ENTRA_CLIENT_SECRET`. The current release
   does not implement certificate or `private_key_jwt` client authentication.
5. If admission is role-gated, create an app role such as `Portal.Member`, set
   `AUTH_ENTRA_REQUIRED_ROLE` to that exact value, require assignment for the
   enterprise application, and assign only approved users or security groups.
6. Apply the organization's Conditional Access and MFA policy, then test a
   member, an unassigned user, and a recovery administrator before cutover.

Identity owners should review Microsoft's current guidance for
[Web redirect URIs](https://learn.microsoft.com/en-us/entra/identity-platform/how-to-add-redirect-uri),
[authorization code flow and PKCE](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-auth-code-flow),
and [application roles](https://learn.microsoft.com/en-us/entra/identity-platform/howto-add-app-roles-in-apps)
as part of the change review.

The server derives tenant-specific issuer, authorization, token, and JWKS URLs
from `AUTH_ENTRA_TENANT_ID`. Leave the four endpoint overrides unset for normal
Microsoft Entra production use. They exist for controlled test providers or an
explicitly reviewed endpoint change; all production overrides must use HTTPS.
HTTPS validation alone does not prove an override is Microsoft-controlled, and
a malicious token URL would receive the authorization code and client secret.
The security owner must approve the exact hosts before any production override.

### Entra configuration contract

| Setting | Default / required behavior |
| --- | --- |
| `AUTH_MAX_ACTIVE_SESSIONS_PER_USER` | Default 20; allowed range 1-100. Password and Entra sessions share this per-user capacity. A successful login evicts the oldest active session when the user is already at capacity. |
| `AUTH_PASSWORD_LOGIN_ENABLED` | Defaults to `true`. `false` disables local password sign-in. |
| `AUTH_ENTRA_ENABLED` | Defaults to `false`. When true, all required Entra settings below are validated at startup. |
| `AUTH_ENTRA_TENANT_ID` | Required UUID when Entra is enabled. Use the exact tenant, never `common`, `organizations`, or `consumers`. |
| `AUTH_ENTRA_CLIENT_ID` | Required UUID when Entra is enabled. |
| `AUTH_ENTRA_CLIENT_SECRET` | Required in production when Entra is enabled. Current client-authentication mechanism. |
| `AUTH_ENTRA_REDIRECT_URI` | Required; absolute HTTP(S), no query/fragment, and HTTPS in production. Its path must be exactly `/auth/entra/callback` with no prefix or trailing slash, and the complete URI must exactly match the Web registration. |
| `AUTH_OIDC_TRANSACTION_KEY` | Required when Entra is enabled; at least 32 high-entropy bytes and independent from every other secret. |
| `AUTH_ENTRA_ISSUER` | Optional; defaults to `https://login.microsoftonline.com/<tenant-id>/v2.0`. |
| `AUTH_ENTRA_AUTHORIZATION_URL` | Optional tenant-specific authorization endpoint override. |
| `AUTH_ENTRA_TOKEN_URL` | Optional tenant-specific token endpoint override. |
| `AUTH_ENTRA_JWKS_URL` | Optional tenant-specific signing-key endpoint override. |
| `AUTH_ENTRA_JIT_PROVISIONING` | Defaults to `false`. See the identity rules below. |
| `AUTH_ENTRA_REQUIRED_ROLE` | Optional exact app-role value; mandatory in production when JIT is true. When set, every Entra login must carry it. |
| `AUTH_OIDC_TRANSACTION_TTL_SECONDS` | Default 600; allowed range 60-1800. |
| `AUTH_ENTRA_JWKS_CACHE_SECONDS` | Default 300; allowed range 30-86400. |
| `AUTH_ENTRA_CLOCK_SKEW_SECONDS` | Default 120; allowed range 1-300. |

The supported rollout combinations are:

| Password | Entra | Use |
| --- | --- | --- |
| `true` | `false` | Existing local-only operation. |
| `true` | `true` | Recommended staged rollout and recovery posture. |
| `false` | `true` | Entra-only operation after acceptance and an approved recovery plan. |
| `false` | `false` | Invalid; the server refuses to start. |

Production startup also rejects non-HTTPS Entra endpoints, a missing client
secret, a weak transaction key, and JIT without a required role. Treat a
configuration failure as a deployment stop; do not replace a failed setting
with a shared or placeholder secret.

### Entra identity, JIT, and role boundary

An Entra identity is bound by tenant ID and immutable object ID. Display name,
email, and preferred username are profile data and must not be used to merge or
authorize accounts.

With `AUTH_ENTRA_JIT_PROVISIONING=false`, only a pre-linked external identity
may sign in. Establish links through an approved provisioning or migration path;
do not insert a link based only on matching email. With JIT enabled, a first
successful sign-in may provision an identity only after the exact
`AUTH_ENTRA_REQUIRED_ROLE` value is present. A missing or mismatched role is a
denial, not a reason to create a permissive account.

For the supported production pre-link path, first create the named portal user,
obtain the immutable Entra object ID from the identity owner, then run:

```sh
docker compose --env-file /opt/internal-developer-portal/config/.env.production \
  -f docker-compose.prod.yml run --rm --no-deps app \
  /app/cli users link-entra \
  --username alice \
  --object-id aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa
```

The CLI derives tenant and issuer from the enabled Entra configuration, rejects
UUID/issuer/subject and cross-user conflicts, is idempotent for the same link,
and audits changes. `--user-id` may replace `--username`. Do not link by email or
UPN. Omit optional `--subject` unless the identity owner supplies the
authoritative opaque subject value.

Whenever `AUTH_ENTRA_REQUIRED_ROLE` is configured, its exact app-role value is
checked on every Entra login, including pre-linked users. It is an admission
gate, not a role mapper. A newly JIT-provisioned user receives the portal
`member` role; the server does **not** translate Entra roles or groups into the
portal's `admin`, maintainer owner, maintainer, or viewer permissions. Continue
to grant those permissions explicitly and test them with separate accounts.
Group-to-app-role assignment can be managed in Entra, but the portal consumes
the resulting app-role value rather than acting as a general
group-to-portal-role synchronizer.

For JIT offboarding, first remove the user's Entra application assignment or
required app role and wait for directory propagation. Then revoke their portal
sessions and delete or disable the portal-side account under an audited change.
Deleting only the portal user is not a permanent block: while JIT and the Entra
admission role remain enabled, the next valid login can provision a new member
account. This release has no deny tombstone, supported unlink command, or
per-user disable flag, so production offboarding must begin at Entra.

Portal `POST /logout` and session revocation invalidate portal sessions only.
They do not sign the user out of Microsoft or propagate a logout to other Entra
applications. Record this limitation in user guidance and use Conditional
Access/session policy for Microsoft-side controls.

### Local recovery access

Keep `AUTH_PASSWORD_LOGIN_ENABLED=true` during the rollout and keep at least two
named local recovery administrators, owned by different people, in the approved
password vault. Exercise them through the same HTTPS proxy before and after
every auth change, rotate them on the security schedule, and alert on their use.

The current switch is global: `true` enables password login for every valid
local account; there is no built-in "break-glass accounts only" mode. After
cutover, either retain this known limitation behind the corporate VPN and a
proxy source-IP restriction on `POST /login`, or set the switch to false only
after security owners accept how recovery will work. Never set it false before
an Entra administrator and a tested rollback operator are available.

### Browser sessions and login throttling

Successful production browser login sets the host-prefixed cookie
`__Host-idp_session` with `HttpOnly`, `Secure`, `SameSite=Lax`, and `Path=/`.
The `__Host-` prefix prevents sibling subdomains from injecting the portal's
session cookie because browsers require Secure, a root path, and no Domain
attribute. Development and test continue to use `idp_session`. A successful
production login expires that legacy cookie during rollout. This means the
public portal origin must be HTTPS even when the reverse proxy talks to Rocket
over private HTTP. Never disable Secure cookies to work around a proxy or
certificate problem; repair TLS and the public origin instead.

The frontend uses the cookie and does not persist a token in browser-readable
storage. `POST /login` returns only safe session metadata (`expires_at` and
`auth_method`); the raw credential exists only in the `HttpOnly` `Set-Cookie`
header. The production smoke script therefore uses an in-memory HTTP cookie jar
and sends `X-IDP-CSRF: 1` for protected writes. Separately provisioned
non-browser Bearer credentials remain request-guard compatible, but local login
does not issue one. `POST /logout` removes the current session. An authenticated
`POST /sessions/revoke-all` removes every
session for that user and clears the current browser cookie; use it after a lost
device, suspected token exposure, or as an explicit "sign out everywhere"
operation.

Every Cookie-authenticated write also requires `X-IDP-CSRF: 1`. The portal
frontend supplies this header. Keep cross-origin credentialed CORS disabled;
otherwise a permissive CORS policy could undo this same-site subdomain CSRF
boundary. Bearer-authenticated automation is not required to send the header.

Login failures are stored only as normalized SHA-256 bucket identifiers. The
low bucket combines username and client IP and locks after
`AUTH_LOGIN_MAX_FAILURES` failures (default 5). A separate account-wide bucket
locks only after `AUTH_LOGIN_ACCOUNT_MAX_FAILURES` failures (default 50), which
must be at least twice the low threshold. Both share the configured window and
lockout duration. Requests without a usable client IP enter a stable
per-username `unknown-client` bucket and never bypass throttling.

A valid password clears the account-wide bucket and the current client bucket;
it does not clear locked buckets belonging to other client IPs. Validate this
behavior for each recovery admin during the authentication drill. Keep the
trusted proxy's IP-aware `POST /login` limit as the first line of defense
against username spraying and Argon2 resource exhaustion. Port `8000` must not
be directly reachable, because an untrusted caller must never be able to choose
the forwarding header Rocket treats as the client IP. Alert on unusual 401/429
rates and require security-owner approval before lowering either threshold.

Usernames use one trimmed, lowercase identity for password lookup, throttling,
CLI create/ensure/link operations, and JIT-created users. Historical mixed-case
rows retain their display casing and remain login-compatible. Before applying
the canonical-username migration, inspect collisions and invalid whitespace:

```sql
WITH policy(trim_characters) AS (
  VALUES (
    E' \t\n\v\f\r' ||
    U&'\0085\00A0\1680\2000\2001\2002\2003\2004\2005\2006\2007\2008\2009\200A\2028\2029\202F\205F\3000'
  )
)
SELECT lower(btrim(username, trim_characters)) AS canonical_username,
       array_agg(username ORDER BY username) AS conflicting_users
FROM users CROSS JOIN policy
GROUP BY lower(btrim(username, trim_characters))
HAVING count(*) > 1;

WITH policy(trim_characters) AS (
  VALUES (
    E' \t\n\v\f\r' ||
    U&'\0085\00A0\1680\2000\2001\2002\2003\2004\2005\2006\2007\2008\2009\200A\2028\2029\202F\205F\3000'
  )
)
SELECT id, quote_literal(username)
FROM users CROSS JOIN policy
WHERE username <> btrim(username, trim_characters)
   OR btrim(username, trim_characters) = '';
```

The migration stops with a remediation hint when either query returns rows.
Rename each affected account explicitly and verify its roles, sessions,
maintainer memberships, and external identities before retrying. Do not merge
rows by deleting one account merely to make the index build succeed.

### TIMESTAMPTZ and Graph Calendar preflight

Migration `2026-07-12-140000_use_timestamptz` is a non-rolling maintenance
boundary. It changes every application timestamp column from a naive UTC wall
clock to `TIMESTAMPTZ` with an explicit `AT TIME ZONE 'UTC'` conversion. The
statements require exclusive table locks and the old and new binaries use
different database type contracts. Do not run old and new app/worker replicas
together, start the new image before migration, or restart an old writer after
the migration commits. Rehearse the migration against a restored production
copy and size the window from the measured lock/rewrite time, not from an empty
database.

Historical Microsoft Graph Calendar connectors could set `config.time_zone` to
a non-UTC Windows or IANA zone. Graph then returned a naive local wall time, and
the old adapter stored that value without its offset. Interpreting those rows as
UTC during migration would move the meeting instant. Find every affected
connector before entering the maintenance window:

```sql
SELECT cc.source,
       cc.config::jsonb ->> 'time_zone' AS requested_time_zone,
       count(ce.id) AS stored_events,
       max(ce.updated_at) AS latest_event_update
FROM connector_configs cc
JOIN connectors c ON c.source = cc.source
LEFT JOIN calendar_events ce ON ce.connector_id = c.id
WHERE lower(coalesce(cc.config::jsonb ->> 'adapter', '')) IN (
        'microsoft_graph_calendar', 'graph_calendar', 'outlook_calendar'
      )
  AND lower(coalesce(nullif(btrim(cc.config::jsonb ->> 'time_zone'), ''), 'utc'))
      NOT IN ('utc', 'etc/utc', 'gmt', 'etc/gmt')
GROUP BY cc.source, cc.config::jsonb ->> 'time_zone'
ORDER BY cc.source;
```

For each returned source, while the old release is still running:

1. Record the original zone in the change record. Remove `time_zone` from the
   connector config (or set it to `UTC`) without changing credentials, scope,
   or the configured calendar window.
2. Run the connector manually and wait for a terminal `success`. Require
   `failure_count = 0`, `snapshot_complete IS TRUE`, no item errors, and no
   pagination/item-limit warning. A partial or bounded run is not a resync.
3. Verify all active events in the configured window have that run's
   `last_seen_run_id`, then spot-check meetings around midnight and a daylight
   saving transition with the calendar owner. Compare the UTC instant with
   Outlook, not only the displayed wall-clock text.
4. Rerun the SQL above. It must return no connector with stored events. The
   migration also checks this condition and intentionally fails before taking
   the schema locks when unsafe non-UTC data remains.

Keep Graph Calendar on its default UTC response after the upgrade. The new
adapter emits offset-aware RFC3339 instants while retaining the original event
time-zone label for display. If an explicitly reviewed future connector needs a
non-UTC Graph response, introduce it only after the migration and require a full
resync plus the same midnight/DST comparison.

During the window, after both writers are stopped and the final backup is
verified, run the migration once and confirm the exact boundary and column
types before starting the new image:

```sql
SELECT version
FROM __diesel_schema_migrations
ORDER BY version DESC
LIMIT 1;
-- expected for the current release: 20260712150000

SELECT table_name, column_name, data_type
FROM information_schema.columns
WHERE table_schema = 'public'
  AND table_name <> '__diesel_schema_migrations'
  AND data_type = 'timestamp without time zone';
-- expected: zero rows
```

Post-deploy smoke must verify that returned datetimes include `Z` or an explicit
numeric offset. Treat a naive value such as `2026-07-10T09:00:00` as a failed
deployment contract, even if one browser happens to display the expected time.

### My Work additive migration and rollout

Migration `2026-07-12-150000_add_my_work_fields` is additive: it adds nullable
work-card metadata, an `ON DELETE SET NULL` portal-assignee foreign key, and
partial indexes. Run it before starting the My Work-capable app/worker. The
previous image can continue to read the expanded table during an app-only
rollback, so leave the added columns in place until the rollback window closes;
do not run the destructive `down.sql` while either writer is serving traffic.

Production does not load demo assignments. Before pilot, use the authorized
user directory to obtain portal user IDs, configure each Azure DevOps connector
with explicit `assignee_user_mappings`, run a complete sync, and verify unmapped
Azure descriptors remain absent from My Work. Never map by display name,
principal name, or email. If the Azure process exposes a due field, configure
its exact reference name through `due_date_field` and compare a sample of due
dates with Azure DevOps before enabling the schedule.

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

## First admin and recovery accounts

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
   Keep at least two recovery admins owned by different people, test them while
   Entra is unavailable, and test offboarding before the pilot.

`ensure-admin` does not replace an existing password unless reset is explicitly
requested. Password reset is a controlled recovery action and should be
recorded in the incident/change ticket. Run the guarded Compose profile shown
above; do not invoke the CLI with an omitted password because its direct-use
development defaults are not acceptable recovery credentials.

## Entra secret and transaction-key rotation

Rotate the Entra client secret with overlap; do not wait for its expiry:

1. Create a second client secret in the same app registration while the old
   secret remains valid. Record both expiry times and the rollback owner.
2. Replace only `AUTH_ENTRA_CLIENT_SECRET` in the protected app secret source.
3. Recreate only the HTTP app; the worker, migration, and seed jobs do not need
   this secret:

   ```sh
   docker compose --env-file /opt/internal-developer-portal/config/.env.production \
     -f docker-compose.prod.yml up -d --no-deps --force-recreate app
   ```

4. Verify `/livez`, `/readyz`, public auth configuration, and a real browser
   Entra sign-in. Confirm `/me` reports `auth_method=entra`.
5. Keep the old secret active through the observation and rollback window. If
   validation fails, restore the old value and recreate only `app`.
6. Remove the old Entra credential only after the new credential is proven and
   the change record is closed.

The current release supports a client secret, not certificate client
authentication. Do not document a certificate rotation as if the server can use
it; implementing certificate or `private_key_jwt` support is separate work.

`AUTH_OIDC_TRANSACTION_KEY` protects short-lived OIDC transactions and has no
dual-key decryption window. Rotating it makes in-flight sign-ins created with
the old key fail, but does not require changing the Entra client secret. Schedule
the change, stop initiating new sign-ins, wait at least the configured
`AUTH_OIDC_TRANSACTION_TTL_SECONDS` or accept that pending attempts will restart,
replace the key, recreate only `app`, and run both password-recovery and Entra
browser checks. Do not rotate `CONNECTOR_SECRET_KEY` as part of either procedure.

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

For the first Entra rollout, complete these preflight steps before the normal
maintenance procedure:

1. Review the dedicated app registration, enterprise-app assignments, exact Web
   redirect URI, client-secret expiry, Conditional Access, and MFA policy with
   the identity owner.
2. Start in mixed mode with both `AUTH_PASSWORD_LOGIN_ENABLED=true` and
   `AUTH_ENTRA_ENABLED=true`. Confirm the client secret and transaction key are
   injected only into `app` in the effective deployment.
3. For `AUTH_ENTRA_JIT_PROVISIONING=false`, prepare the approved portal-user to
   Entra-object-ID mapping and pre-link it after the new migration runs but
   before traffic resumes. Alternatively, set JIT true only with an exact
   `AUTH_ENTRA_REQUIRED_ROLE` and approved pilot assignments. Do not use JIT as
   an implicit grant of portal admin access.
4. Verify two named local recovery admins, the old application image, the old
   auth env revision, and the current Entra client secret are available for the
   rollback window.
5. Exercise issuer, audience, tenant, nonce, state, PKCE, expiry, replay, JWKS
   refresh, missing-role, and JIT behavior against a mock OIDC provider in the
   integration environment. Do not intentionally send malformed tokens or
   secrets through production proxy logs.

Before the maintenance window:

1. Require green CI for `cargo fmt --check`, Clippy, frontend build/tests, and
   Rust tests.
2. Review every new `migrations/*/up.sql` and `down.sql`; identify locks,
   rewrites, destructive changes, and compatibility with the previous image.
3. Verify disk space for both the database and backup.
4. Verify the current `/livez`, `/readyz`, worker heartbeat, and latest backup.
5. Take and validate a preliminary custom-format backup to prove the backup
   path before the window. This is not the final rollback recovery point while
   writers are still running.
6. Preserve the currently running image by immutable registry digest. For the
   local Compose tag, preserve it before building:

   ```sh
   release="$(date -u +%Y%m%dT%H%M%SZ)"
   old_id="$(docker image inspect -f '{{.Id}}' internal-developer-portal:prod)"
   docker image tag "$old_id" "internal-developer-portal:rollback-$release"
   docker compose --env-file /opt/internal-developer-portal/config/.env.production \
     -f docker-compose.prod.yml build app
   ```

When the release includes `2026-07-11-100000_harden_sessions`, notify users that
the migration deliberately deletes every existing session before hashing is
introduced. Users and automation must obtain a new browser session or
separately provisioned credential after deployment. This is expected security
behavior, not a rollback signal.

When the release includes `2026-07-12-140000_use_timestamptz`, this procedure is
strictly non-rolling. Keep every old and new app/worker instance stopped between
the final backup and the successful migration/type checks. Start the matching
new app and worker only after version `20260712140000` is present. A failed
Graph non-UTC preflight is a stop condition, not permission to bypass or edit
the migration.

During the window, put the reverse proxy into maintenance/drain mode so no new
writes arrive, then stop both writers. With `app` and `worker` stopped, take and
validate a new final custom-format backup using the procedure above, record its
checksum and immutable off-host location, and only then run migration once. If
the final backup or verification fails, do not migrate:

```sh
docker compose --env-file /opt/internal-developer-portal/config/.env.production \
  -f docker-compose.prod.yml stop app worker

# Run the documented pg_dump -Fc, pg_restore --list, checksum, encryption, and
# off-host-copy procedure here while both writers remain stopped.

docker compose --env-file /opt/internal-developer-portal/config/.env.production \
  -f docker-compose.prod.yml run --rm migrate

# First JIT-off Entra rollout only: run the documented `users link-entra`
# command for each approved pilot identity here, before starting app traffic.

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

Check the unauthenticated runtime auth contract without following an Entra
redirect:

```sh
curl --fail --silent --show-error \
  https://portal.internal.example/auth/config
```

During mixed-mode rollout the response must be a `{ "data": ... }` envelope
with `password_login_enabled: true` and `entra_login_enabled: true`, and must not
contain tenant metadata, client IDs, endpoints, secrets, or transaction keys.
Treat a 404, unexpected toggle, or secret-bearing response as a deployment
failure.

Then run the authenticated smoke test from a client on the same path users take:

```powershell
.\scripts\production-smoke.ps1 `
  -BaseUrl https://portal.internal.example `
  -Username pilot.admin `
  -ExpectedEntraState Enabled
```

The password prompt is a `SecureString`. Do not place the password on the
command line. In CI, inject `PORTAL_SMOKE_USERNAME` and
`PORTAL_SMOKE_PASSWORD` from the CI secret store and omit the username/password
arguments. The script removes the password env variable from its child process,
keeps the `HttpOnly` session only in an in-memory cookie jar, verifies that
`/login` did not return a raw token, sends `X-IDP-CSRF: 1` for logout, and always
attempts logout.
If the overview is intentionally unavailable during a narrow diagnostic, add
`-SkipOverview`; the normal deployment gate must not skip it. Use
`-ExpectedEntraState Enabled` for an Entra rollout, `Disabled` for an intentional
local-only rollback, or omit it when the Entra toggle is outside this change.
The script checks `/auth/config` before prompting for credentials and refuses to
run its local-login path when password login is disabled.

That refusal is intentional for an Entra-only deployment: MFA/Conditional
Access must not be bypassed with a non-interactive test credential. In an
Entra-only change, run the unauthenticated probes above and keep maintenance
mode enabled until an assigned human operator completes the interactive Entra
acceptance steps below and records `/me.auth_method=entra`. There is no claim of
an authenticated automated smoke pass in that mode.

The smoke script validates the same cookie-and-CSRF path used by the frontend.
Also use a private browser session to verify that the login response sets
`__Host-idp_session` with `HttpOnly`, `Secure`, `SameSite=Lax`, and `Path=/`,
does not set a `Domain` attribute, and that no token is
written to local/session storage. Do not copy the `Set-Cookie` header or cookie
value into a ticket or log. Open two private browser sessions, invoke
`POST /sessions/revoke-all` from one, and confirm both sessions require login again.

The password smoke is intentionally not a substitute for interactive Entra
acceptance. In a private supported browser:

1. Start Entra sign-in from the portal and confirm the redirect uses the exact
   tenant-specific authorization endpoint, the registered callback, and
   `code_challenge_method=S256`. In production, confirm the short-lived
   `__Host-idp_oidc` transaction cookie has `Secure`, `HttpOnly`, `SameSite=Lax`,
   and `Path=/`. Never copy the complete redirect URL or cookie into a ticket
   because they contain state/binding material and a PKCE challenge.
2. Complete MFA with an assigned low-privilege user. Confirm the browser returns
   to an allow-listed portal route, the address bar no longer contains code or
   state, `/me` reports `auth_method=entra`, and the expected non-admin portal
   permissions apply.
3. Attempt sign-in with a user missing the configured required role and confirm
   no portal session or JIT identity is created.
4. Test the selected JIT mode: a pre-linked identity succeeds when JIT is false;
   or one assigned first-time user is provisioned once, remains the same account
   with the portal `member` role on a second login, and is not made portal admin
   when JIT is true.
5. Confirm the app/proxy logs contain no authorization code, state, nonce,
   client secret, ID token, PKCE verifier, or session cookie.
6. Sign out and confirm the portal session is gone. Do not record a failure
   merely because Microsoft can sign the browser back in without prompting;
   Microsoft-session logout propagation is not implemented.

Keep mixed mode through the observation window. If the approved target is
Entra-only, change `AUTH_PASSWORD_LOGIN_ENABLED=false` only in a later, separate
change after the recovery plan is accepted. If local recovery accounts must
remain usable, leave it true and apply the documented proxy/VPN restriction;
the current release has no break-glass-only mode.

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

For the TIMESTAMPTZ boundary, prefer restoring the verified pre-upgrade backup
into a new database and pairing it with the preserved old image. Do not use an
in-place down migration as a shortcut while traffic is flowing: it requires the
same exclusive locks, changes the binary/schema contract again, and cannot
repair Graph rows that were wrong before the UTC resync. Keep maintenance mode
enabled until the database owner has selected and validated one coherent
image/schema pair.

Both directions of the session-hardening migration invalidate sessions: token
hashes cannot be converted back into usable bearer tokens. Even a deliberate
app/schema rollback therefore requires users and automation to authenticate
again.

For an Entra authentication rollback, first restore a usable login path in the
protected env file:

```dotenv
AUTH_PASSWORD_LOGIN_ENABLED=true
AUTH_ENTRA_ENABLED=false
```

Then recreate only the HTTP app and verify a named local recovery account:

```sh
docker compose --env-file /opt/internal-developer-portal/config/.env.production \
  -f docker-compose.prod.yml up -d --no-deps --force-recreate app

pwsh ./scripts/production-smoke.ps1 \
  -BaseUrl https://portal.internal.example \
  -Username recovery.operator \
  -ExpectedEntraState Disabled
```

Disabling Entra prevents new Entra sign-ins but does not by itself revoke
already-created portal sessions. If the rollback is caused by an authorization
or identity incident, the database owner must count and revoke Entra sessions as
an exceptional, recorded intervention. Take the incident backup first, adapt
the database role/name, and run the deletion in a transaction:

```sql
BEGIN;
SELECT COUNT(*) FROM sessions WHERE auth_method = 'entra';
DELETE FROM sessions WHERE auth_method = 'entra';
COMMIT;
```

The current release has no operator API that globally revokes only Entra
sessions. Do not imply that restarting the app or changing the auth toggle does
so. Preserve the affected database and proxy/audit evidence without logging
tokens.

Keep the Entra app registration, old/new overlapping client secrets, redirect
URI, and external identity records intact through the rollback window. App image
rollback, runtime auth-config rollback, and Entra app-registration rollback are
separate decisions. Do not run the Entra migration down merely to disable SSO;
retaining external identity links prevents duplicate users on a later rollout.

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
and the canary connector pass. For an Entra-capable image, also verify
`/auth/config`, one local recovery login, and either a successful Entra login or
the deliberate disabled state. Document the decision and exact recovery point.

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
| Security | Unusual 401/429 rate, repeated account lockouts, admin use, or connector-config changes | Review proxy/audit logs without recording credentials, revoke affected sessions, and follow incident procedure. |
| Entra sign-in | Callback/configuration errors exceed baseline, assigned users cannot sign in, or required-role denials change unexpectedly | Check runtime toggles, tenant-specific endpoints, app assignments, Conditional Access, secret expiry, and sanitized app logs. Use local recovery access rather than weakening validation. |
| Entra credential | Client secret has less than 30 days remaining | Create an overlapping secret and run the documented app-only rotation; never wait for expiry. |

Daily operator review:

- app/worker restart count and error logs;
- worker heartbeat, current run, and last error;
- failed/partial connector runs, exhausted attempts, and stale queued/running
  work;
- stale service-health data and priority notifications;
- latest successful retention cleanup and table/disk growth;
- latest backup timestamp, verification, and off-host copy;
- expiring upstream OAuth tokens/PATs and TLS certificates;
- Entra sign-in failures, app-role assignments, and client-secret expiry;
- admin/audit activity, recovery-account use, and departed-user access.

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
- [ ] Reverse-proxy limits for password login, Entra start, and Entra callback
      return 429 at their reviewed thresholds; shared-NAT callback traffic fits
      within the measured burst budget and 429 metrics are alerted.
- [ ] Callback access, redirect-listener, error, WAF/LB/CDN, APM, tracing, and
      centralized logs were sampled and contain no query string, Referer,
      authorization code, state, or nonce.
- [ ] There are no default/shared credentials; every pilot user has a named
      account and an owner for offboarding.
- [ ] The Portal uses a dedicated, single-tenant Entra Web app registration;
      the exact HTTPS callback is registered and implicit/hybrid flow is off.
- [ ] Production issuer/authorization/token/JWKS overrides are unset, or the
      security owner approved each exact HTTPS host and recorded why the
      tenant-derived Microsoft default could not be used.
- [ ] Runtime `/auth/config` exposes only the two expected login-method booleans
      and matches the reviewed deployment mode; no secret or provider metadata
      is returned.
- [ ] The Entra start request uses tenant-specific authorization and S256 PKCE;
      callback state/nonce/code replay, wrong tenant/issuer/audience, expired
      tokens, and invalid signatures fail closed in integration tests.
- [ ] An assigned member signs in with MFA and `/me.auth_method=entra`; an
      unassigned or missing-role user receives no session or JIT account.
- [ ] JIT-off accepts only approved pre-linked immutable identities, or JIT-on
      creates the same non-admin account exactly once behind the configured app
      role. Email/UPN changes do not create another account.
- [ ] Entra app roles are treated only as the documented admission gate; portal
      admin and maintainer permissions remain explicit and were tested
      independently.
- [ ] Entra client secret and OIDC transaction key are independent, scoped only
      to the app container, absent from logs/config output, and have named
      rotation owners and expiry alerts.
- [ ] Two named local recovery admins work through HTTPS and their credentials
      are vaulted. The accepted limitation that password login is global rather
      than break-glass-only is mitigated by the documented VPN/proxy control.
- [ ] Browser login sets an `HttpOnly`, `Secure`, `SameSite=Lax`, `Path=/`
      cookie over HTTPS; the token is absent from browser-readable storage.
- [ ] `POST /sessions/revoke-all` invalidates two independently established
      sessions for the same user, while `POST /logout` invalidates only the
      current session.
- [ ] The forced re-login caused by the session-hardening migration was
      communicated and browser/CLI users can obtain fresh sessions afterward.
- [ ] Every legacy Graph Calendar connector with a non-UTC `time_zone` completed
      a successful, complete UTC resync; midnight and DST-boundary meetings were
      compared with Outlook and the preflight query returns no unsafe rows.
- [ ] The TIMESTAMPTZ change was rehearsed as a non-rolling maintenance event on
      a restored production copy; both writers stayed stopped, migration
      `20260712140000` is present and the current schema version is
      `20260712150000`, no application column remains a PostgreSQL `timestamp
      without time zone`, and smoke responses contain only offset-aware RFC3339
      datetimes.
- [ ] My Work was tested with separate Alice/Bob/team accounts: assignment only
      narrows existing record visibility, unmapped or duplicate display names
      are never guessed, filters/facets cannot reveal another scope, and detail
      navigation preserves the original filter/page URL.
- [ ] Admin, maintainer owner, maintainer writer, and read-only member
      permissions were tested with separate accounts.
- [ ] User-scoped and maintainer-scoped connector/work/notification data cannot
      be listed or opened by an unrelated user (including direct URL/API tests).
- [ ] Connector config round-trips preserve redacted secrets and secrets do not
      appear in API responses, browser logs, app logs, or audit metadata.
- [ ] Effective container environments show `CONNECTOR_SECRET_KEY` only in app
      and worker, and Entra client/OIDC secrets only in app; migrate and
      seed-admin receive neither master secret.
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
- [ ] Entra rollback was rehearsed: local login was restored, Entra was disabled,
      existing Entra sessions were handled deliberately, identity links were
      retained, and the old overlapping client secret remained usable.
- [ ] Client-secret overlap rotation and OIDC transaction-key rotation were
      rehearsed without exposing secrets or changing `CONNECTOR_SECRET_KEY`.
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
