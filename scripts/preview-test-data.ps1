[CmdletBinding()]
param(
    [Parameter()]
    [string]$DatabaseUrl = $env:DATABASE_URL,

    [Parameter()]
    [string]$ReportPath,

    [Parameter()]
    [ValidateRange(1, 300)]
    [int]$StatementTimeoutSeconds = 30,

    [Parameter()]
    [ValidateRange(1, 60)]
    [int]$ConnectTimeoutSeconds = 10
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-ValidatedConnection {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Url
    )

    if ([string]::IsNullOrWhiteSpace($Url)) {
        throw "DatabaseUrl is required. Pass -DatabaseUrl or set DATABASE_URL."
    }

    $uri = $null
    if (-not [Uri]::TryCreate($Url, [UriKind]::Absolute, [ref]$uri)) {
        throw "DatabaseUrl must be an absolute PostgreSQL URL."
    }
    if ($uri.Scheme -notin @("postgres", "postgresql")) {
        throw "DatabaseUrl must use the postgres or postgresql scheme."
    }
    if ([string]::IsNullOrWhiteSpace($uri.Host)) {
        throw "DatabaseUrl must include a host."
    }
    if ([string]::IsNullOrWhiteSpace($uri.UserInfo)) {
        throw "DatabaseUrl must include a user."
    }
    if (-not [string]::IsNullOrEmpty($uri.Fragment)) {
        throw "DatabaseUrl fragments are not supported."
    }

    $userInfoSeparator = $uri.UserInfo.IndexOf(':')
    $user = if ($userInfoSeparator -ge 0) {
        [Uri]::UnescapeDataString($uri.UserInfo.Substring(0, $userInfoSeparator))
    }
    else {
        [Uri]::UnescapeDataString($uri.UserInfo)
    }
    $credentialSecret = if ($userInfoSeparator -ge 0) {
        [Uri]::UnescapeDataString($uri.UserInfo.Substring($userInfoSeparator + 1))
    }
    else {
        ""
    }
    $database = [Uri]::UnescapeDataString($uri.AbsolutePath.TrimStart('/'))
    $port = if ($uri.IsDefaultPort -or $uri.Port -lt 1) { 5432 } else { $uri.Port }

    if ($database.Contains('/') -or [string]::IsNullOrWhiteSpace($database)) {
        throw "DatabaseUrl must identify exactly one database."
    }
    if ($user -notmatch '^[A-Za-z0-9_.-]+$') {
        throw "DatabaseUrl user contains unsupported characters."
    }
    if ($database -notmatch '^[A-Za-z0-9_.-]+$') {
        throw "DatabaseUrl database contains unsupported characters."
    }
    if ($uri.Host -notmatch '^[A-Za-z0-9.:[\]-]+$') {
        throw "DatabaseUrl host contains unsupported characters."
    }
    if ($port -lt 1 -or $port -gt 65535) {
        throw "DatabaseUrl port is outside the valid range."
    }

    $sslMode = $null
    if (-not [string]::IsNullOrWhiteSpace($uri.Query)) {
        foreach ($pair in $uri.Query.TrimStart('?').Split('&')) {
            if ([string]::IsNullOrWhiteSpace($pair)) {
                continue
            }

            $separator = $pair.IndexOf('=')
            $name = if ($separator -ge 0) {
                [Uri]::UnescapeDataString($pair.Substring(0, $separator))
            }
            else {
                [Uri]::UnescapeDataString($pair)
            }
            $value = if ($separator -ge 0) {
                [Uri]::UnescapeDataString($pair.Substring($separator + 1))
            }
            else {
                ""
            }
            if ($name -ne "sslmode") {
                throw "DatabaseUrl contains an unsupported query parameter."
            }
            if ($value -notin @("disable", "allow", "prefer", "require", "verify-ca", "verify-full")) {
                throw "DatabaseUrl sslmode is invalid."
            }
            $sslMode = $value
        }
    }

    [PSCustomObject]@{
        Host             = $uri.Host
        Port             = $port
        User             = $user
        CredentialSecret = $credentialSecret
        Database         = $database
        SslMode          = $sslMode
        IsLoopback       = $uri.Host -in @("localhost", "127.0.0.1", "[::1]", "::1")
    }
}

function Get-SafeReportPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Workspace,

        [Parameter()]
        [string]$RequestedPath
    )

    $timestamp = [DateTime]::UtcNow.ToString("yyyyMMdd'T'HHmmssfff'Z'", [Globalization.CultureInfo]::InvariantCulture)
    $candidate = if ([string]::IsNullOrWhiteSpace($RequestedPath)) {
        Join-Path $Workspace "target/test-data-preview-$timestamp.json"
    }
    elseif ([IO.Path]::IsPathRooted($RequestedPath)) {
        $RequestedPath
    }
    else {
        Join-Path $Workspace $RequestedPath
    }

    $workspaceFull = [IO.Path]::GetFullPath($Workspace).TrimEnd('\', '/')
    $reportFull = [IO.Path]::GetFullPath($candidate)
    $workspacePrefix = $workspaceFull + [IO.Path]::DirectorySeparatorChar
    if (-not $reportFull.StartsWith($workspacePrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "ReportPath must stay inside the workspace."
    }
    if ([IO.Path]::GetExtension($reportFull) -ne ".json") {
        throw "ReportPath must use the .json extension."
    }
    if (Test-Path -LiteralPath $reportFull) {
        throw "ReportPath already exists; refusing to overwrite it."
    }

    $parent = Split-Path -Parent $reportFull
    if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
        $defaultTarget = [IO.Path]::GetFullPath((Join-Path $workspaceFull "target"))
        if ($parent -ne $defaultTarget) {
            throw "A custom ReportPath parent directory must already exist."
        }
        New-Item -ItemType Directory -Path $parent | Out-Null
    }
    $resolvedParent = (Resolve-Path -LiteralPath $parent).Path.TrimEnd('\', '/')
    if ($resolvedParent -ne $workspaceFull -and
        -not $resolvedParent.StartsWith($workspacePrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Resolved report directory must stay inside the workspace."
    }

    $cursor = Get-Item -LiteralPath $resolvedParent
    while ($cursor.FullName.Length -ge $workspaceFull.Length) {
        if (($cursor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "ReportPath cannot traverse a reparse point."
        }
        if ($cursor.FullName.TrimEnd('\', '/') -eq $workspaceFull) {
            break
        }
        $cursor = $cursor.Parent
        if ($null -eq $cursor) {
            throw "ReportPath parent could not be verified."
        }
    }

    $reportFull
}

function ConvertTo-SafeNativeArguments {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    ($Arguments | ForEach-Object {
        if ($_.Contains('"')) {
            throw "Native command argument contains an unsupported quote."
        }
        '"' + $_ + '"'
    }) -join ' '
}

function Invoke-PreviewQuery {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Connection,

        [Parameter(Mandatory = $true)]
        [string]$Workspace,

        [Parameter(Mandatory = $true)]
        [string]$Sql,

        [Parameter(Mandatory = $true)]
        [int]$ConnectTimeout
    )

    $psql = Get-Command psql -ErrorAction SilentlyContinue
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    if ($null -ne $psql) {
        $startInfo.FileName = $psql.Source
        $arguments = @(
            "--no-psqlrc",
            "--no-password",
            "--quiet",
            "--tuples-only",
            "--no-align",
            "--set=ON_ERROR_STOP=1",
            "--host=$($Connection.Host)",
            "--port=$($Connection.Port)",
            "--username=$($Connection.User)",
            "--dbname=$($Connection.Database)"
        )
        $startInfo.Arguments = ConvertTo-SafeNativeArguments -Arguments $arguments
        $startInfo.EnvironmentVariables["PGCONNECT_TIMEOUT"] = [string]$ConnectTimeout
        if (-not [string]::IsNullOrEmpty($Connection.CredentialSecret)) {
            $startInfo.EnvironmentVariables["PGPASSWORD"] = $Connection.CredentialSecret
        }
        if (-not [string]::IsNullOrEmpty($Connection.SslMode)) {
            $startInfo.EnvironmentVariables["PGSSLMODE"] = $Connection.SslMode
        }
    }
    else {
        $docker = Get-Command docker -ErrorAction SilentlyContinue
        if ($null -eq $docker) {
            throw "psql is unavailable, and the safe local Docker fallback is unavailable."
        }
        if (-not $Connection.IsLoopback -or $Connection.Port -ne 5432) {
            throw "psql is required for non-loopback or non-default-port PostgreSQL URLs."
        }
        if (-not (Test-Path -LiteralPath (Join-Path $Workspace "docker-compose.yml") -PathType Leaf)) {
            throw "psql is unavailable and no workspace Docker Compose file was found."
        }

        # The local fallback uses the PostgreSQL service's Unix socket. It never
        # places the URL credential in command arguments or container output.
        $startInfo.FileName = $docker.Source
        $arguments = @(
            "compose",
            "--project-directory", $Workspace,
            "exec", "-T", "postgres",
            "psql",
            "--no-psqlrc",
            "--no-password",
            "--quiet",
            "--tuples-only",
            "--no-align",
            "--set=ON_ERROR_STOP=1",
            "--username=$($Connection.User)",
            "--dbname=$($Connection.Database)"
        )
        $startInfo.Arguments = ConvertTo-SafeNativeArguments -Arguments $arguments
    }

    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardInput = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        throw "Could not start the PostgreSQL preview client."
    }

    try {
        $process.StandardInput.Write($Sql)
        $process.StandardInput.Close()
        $stdout = $process.StandardOutput.ReadToEnd()
        $stderr = $process.StandardError.ReadToEnd()
        $process.WaitForExit()
        $exitCode = $process.ExitCode
    }
    finally {
        $process.Dispose()
    }

    if ($exitCode -ne 0) {
        $safeError = [string]$stderr
        if (-not [string]::IsNullOrEmpty($Connection.CredentialSecret)) {
            $safeError = $safeError.Replace($Connection.CredentialSecret, "[REDACTED]")
        }
        if ($safeError.Length -gt 4000) {
            $safeError = $safeError.Substring(0, 4000)
        }
        throw "PostgreSQL read-only preview failed: $safeError"
    }

    $stdout.Trim()
}

$workspace = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$connection = Get-ValidatedConnection -Url $DatabaseUrl
$safeReportPath = Get-SafeReportPath -Workspace $workspace -RequestedPath $ReportPath
$statementTimeoutMilliseconds = $StatementTimeoutSeconds * 1000

$sql = @'
BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL TIME ZONE 'UTC';
SET LOCAL statement_timeout = '__TIMEOUT_MS__ms';

WITH
source_events AS (
    SELECT source, 'connectors'::text AS entity, count(*)::bigint AS row_count,
           min(created_at) AS oldest_at, max(updated_at) AS newest_at
      FROM connectors GROUP BY source
    UNION ALL
    SELECT source, 'connector_configs', count(*)::bigint, min(created_at), max(updated_at)
      FROM connector_configs GROUP BY source
    UNION ALL
    SELECT source, 'connector_runs', count(*)::bigint, min(started_at), max(coalesce(finished_at, started_at))
      FROM connector_runs GROUP BY source
    UNION ALL
    SELECT source, 'connector_run_items', count(*)::bigint, min(created_at), max(created_at)
      FROM connector_run_items GROUP BY source
    UNION ALL
    SELECT source, 'connector_run_item_errors', count(*)::bigint, min(created_at), max(created_at)
      FROM connector_run_item_errors GROUP BY source
    UNION ALL
    SELECT source, 'services', count(*)::bigint, min(created_at), max(updated_at)
      FROM services GROUP BY source
    UNION ALL
    SELECT source, 'service_health_checks', count(*)::bigint, min(checked_at), max(checked_at)
      FROM service_health_checks GROUP BY source
    UNION ALL
    SELECT source, 'work_cards', count(*)::bigint, min(created_at), max(updated_at)
      FROM work_cards GROUP BY source
    UNION ALL
    SELECT source, 'notifications', count(*)::bigint, min(created_at), max(updated_at)
      FROM notifications GROUP BY source
    UNION ALL
    SELECT source, 'calendar_events', count(*)::bigint, min(created_at), max(updated_at)
      FROM calendar_events GROUP BY source
),
source_rollup AS (
    SELECT source,
           sum(row_count)::bigint AS total_rows,
           min(oldest_at) AS oldest_at,
           max(newest_at) AS newest_at,
           jsonb_object_agg(entity, row_count ORDER BY entity) AS counts_by_relationship
      FROM source_events
     GROUP BY source
),
source_with_registration AS (
    SELECT rollup.*,
           EXISTS (SELECT 1 FROM connectors connector WHERE connector.source = rollup.source) AS has_connector
      FROM source_rollup rollup
),
source_candidates AS (
    SELECT source,
           total_rows,
           oldest_at,
           newest_at,
           counts_by_relationship,
           has_connector,
           CASE
             WHEN lower(source) ~ '(^|[-_.])(test|fixture|e2e|smoke|dummy|tmp)([-_.0-9]|$)|_[0-9]+_[0-9]{12,}_[0-9]+$'
               THEN 'high'
             WHEN lower(source) ~ '^(scope_|reconcile_|receipt_|meov_|priority_|overview_|invalid_scoped_)'
               THEN 'medium'
             WHEN NOT has_connector THEN 'low'
             ELSE NULL
           END AS confidence,
           CASE
             WHEN lower(source) ~ '(^|[-_.])(test|fixture|e2e|smoke|dummy|tmp)([-_.0-9]|$)|_[0-9]+_[0-9]{12,}_[0-9]+$'
               THEN 'generated test identifier pattern'
             WHEN lower(source) ~ '^(scope_|reconcile_|receipt_|meov_|priority_|overview_|invalid_scoped_)'
               THEN 'known integration-test source prefix'
             WHEN NOT has_connector THEN 'connector run/data source has no connector registration'
             ELSE NULL
           END AS reason
      FROM source_with_registration
),
user_candidates AS (
    SELECT id AS user_id,
           created_at,
           CASE
             WHEN lower(username) ~ '(^|[-_.])(test|fixture|e2e|smoke|dummy|tmp)([-_.0-9]|$)|_[0-9]+_[0-9]{12,}_[0-9]+$'
               THEN 'high'
             WHEN lower(username) ~ '^(auth_user|bearer_compat_user|revoke_all_user|throttled_user|entra_prelinked)'
               THEN 'medium'
             ELSE NULL
           END AS confidence,
           CASE
             WHEN lower(username) ~ '(^|[-_.])(test|fixture|e2e|smoke|dummy|tmp)([-_.0-9]|$)|_[0-9]+_[0-9]{12,}_[0-9]+$'
               THEN 'generated test username pattern'
             WHEN lower(username) ~ '^(auth_user|bearer_compat_user|revoke_all_user|throttled_user|entra_prelinked)'
               THEN 'known authentication-test username prefix'
             ELSE NULL
           END AS reason
      FROM users
),
candidate_rows AS (
    SELECT confidence,
           'source'::text AS entity_type,
           source AS identifier,
           reason,
           total_rows AS row_count,
           oldest_at,
           newest_at
      FROM source_candidates
     WHERE confidence IS NOT NULL
    UNION ALL
    SELECT confidence,
           'user_id',
           user_id::text,
           reason,
           1::bigint,
           created_at,
           created_at
      FROM user_candidates
     WHERE confidence IS NOT NULL
),
orphan_sources AS (
    SELECT run.source,
           count(*)::bigint AS run_count,
           min(run.started_at) AS oldest_at,
           max(coalesce(run.finished_at, run.started_at)) AS newest_at
      FROM connector_runs run
      LEFT JOIN connectors connector ON connector.source = run.source
     WHERE connector.id IS NULL
     GROUP BY run.source
),
table_sizes AS (
    SELECT table_class.relname AS table_name,
           coalesce(stats.n_live_tup, 0)::bigint AS estimated_rows,
           pg_relation_size(table_class.oid)::bigint AS table_bytes,
           pg_indexes_size(table_class.oid)::bigint AS index_bytes,
           pg_total_relation_size(table_class.oid)::bigint AS total_bytes
      FROM pg_class table_class
      JOIN pg_namespace namespace ON namespace.oid = table_class.relnamespace
      LEFT JOIN pg_stat_user_tables stats ON stats.relid = table_class.oid
     WHERE namespace.nspname = 'public'
       AND table_class.relkind = 'r'
       AND table_class.relname <> '__diesel_schema_migrations'
),
user_impact_candidates AS (
    SELECT *
      FROM user_candidates
     WHERE confidence IS NOT NULL
     ORDER BY CASE confidence WHEN 'high' THEN 1 ELSE 2 END, created_at
     LIMIT 100
),
session_change_counts AS (
    SELECT coalesce(sum(n_tup_ins), 0)::bigint AS inserted,
           coalesce(sum(n_tup_upd), 0)::bigint AS updated,
           coalesce(sum(n_tup_del), 0)::bigint AS deleted
      FROM pg_stat_xact_user_tables
)
SELECT jsonb_build_object(
    'report_version', 1,
    'generated_at', transaction_timestamp(),
    'database', jsonb_build_object(
        'name', current_database(),
        'server_version_num', current_setting('server_version_num'),
        'database_bytes', pg_database_size(current_database()),
        'latest_migration', (SELECT max(version) FROM __diesel_schema_migrations)
    ),
    'safety', jsonb_build_object(
        'transaction_isolation', current_setting('transaction_isolation'),
        'transaction_read_only', current_setting('transaction_read_only')::boolean,
        'statement_timeout', current_setting('statement_timeout'),
        'rows_inserted', (SELECT inserted FROM session_change_counts),
        'rows_updated', (SELECT updated FROM session_change_counts),
        'rows_deleted', (SELECT deleted FROM session_change_counts),
        'rows_changed', (SELECT inserted + updated + deleted FROM session_change_counts),
        'result', '0 rows changed'
    ),
    'legacy_test_candidates', jsonb_build_object(
        'counts', jsonb_build_object(
            'high', jsonb_build_object(
                'total', (SELECT count(*) FROM candidate_rows WHERE confidence = 'high'),
                'sources', (SELECT count(*) FROM candidate_rows WHERE confidence = 'high' AND entity_type = 'source'),
                'users', (SELECT count(*) FROM candidate_rows WHERE confidence = 'high' AND entity_type = 'user_id')
            ),
            'medium', jsonb_build_object(
                'total', (SELECT count(*) FROM candidate_rows WHERE confidence = 'medium'),
                'sources', (SELECT count(*) FROM candidate_rows WHERE confidence = 'medium' AND entity_type = 'source'),
                'users', (SELECT count(*) FROM candidate_rows WHERE confidence = 'medium' AND entity_type = 'user_id')
            ),
            'low', jsonb_build_object(
                'total', (SELECT count(*) FROM candidate_rows WHERE confidence = 'low'),
                'sources', (SELECT count(*) FROM candidate_rows WHERE confidence = 'low' AND entity_type = 'source'),
                'users', (SELECT count(*) FROM candidate_rows WHERE confidence = 'low' AND entity_type = 'user_id')
            )
        ),
        'maximum_rows_per_confidence', 100,
        'high', coalesce((
            SELECT jsonb_agg(to_jsonb(candidate) ORDER BY candidate.row_count DESC, candidate.entity_type, candidate.identifier)
              FROM (
                    SELECT entity_type, identifier, reason, row_count, oldest_at, newest_at
                      FROM candidate_rows
                     WHERE confidence = 'high'
                     ORDER BY row_count DESC, entity_type, identifier
                     LIMIT 100
              ) candidate
        ), '[]'::jsonb),
        'medium', coalesce((
            SELECT jsonb_agg(to_jsonb(candidate) ORDER BY candidate.row_count DESC, candidate.entity_type, candidate.identifier)
              FROM (
                    SELECT entity_type, identifier, reason, row_count, oldest_at, newest_at
                      FROM candidate_rows
                     WHERE confidence = 'medium'
                     ORDER BY row_count DESC, entity_type, identifier
                     LIMIT 100
              ) candidate
        ), '[]'::jsonb),
        'low', coalesce((
            SELECT jsonb_agg(to_jsonb(candidate) ORDER BY candidate.row_count DESC, candidate.entity_type, candidate.identifier)
              FROM (
                    SELECT entity_type, identifier, reason, row_count, oldest_at, newest_at
                      FROM candidate_rows
                     WHERE confidence = 'low'
                     ORDER BY row_count DESC, entity_type, identifier
                     LIMIT 100
              ) candidate
        ), '[]'::jsonb)
    ),
    'key_relationship_impacts', jsonb_build_object(
        'sources', coalesce((
            SELECT jsonb_agg(jsonb_build_object(
                       'source', candidate.source,
                       'confidence', candidate.confidence,
                       'reason', candidate.reason,
                       'has_connector', candidate.has_connector,
                       'total_related_rows', candidate.total_rows,
                       'counts_by_relationship', candidate.counts_by_relationship,
                       'oldest_at', candidate.oldest_at,
                       'newest_at', candidate.newest_at
                   ) ORDER BY candidate.total_rows DESC, candidate.source)
              FROM (
                    SELECT * FROM source_candidates
                     WHERE confidence IS NOT NULL
                     ORDER BY total_rows DESC, source
                     LIMIT 50
              ) candidate
        ), '[]'::jsonb),
        'users', coalesce((
            SELECT jsonb_agg(jsonb_build_object(
                       'user_id', candidate.user_id,
                       'confidence', candidate.confidence,
                       'reason', candidate.reason,
                       'created_at', candidate.created_at,
                       'sessions', (SELECT count(*) FROM sessions row WHERE row.user_id = candidate.user_id),
                       'role_assignments', (SELECT count(*) FROM users_roles row WHERE row.user_id = candidate.user_id),
                       'maintainer_memberships', (SELECT count(*) FROM maintainer_members row WHERE row.user_id = candidate.user_id),
                       'external_identity_links', (SELECT count(*) FROM external_identities row WHERE row.user_id = candidate.user_id),
                       'owned_connectors', (SELECT count(*) FROM connectors row WHERE row.owner_user_id = candidate.user_id),
                       'owned_work_cards', (SELECT count(*) FROM work_cards row WHERE row.owner_user_id = candidate.user_id),
                       'owned_notifications', (SELECT count(*) FROM notifications row WHERE row.owner_user_id = candidate.user_id),
                       'owned_calendar_events', (SELECT count(*) FROM calendar_events row WHERE row.owner_user_id = candidate.user_id)
                   ) ORDER BY candidate.created_at, candidate.user_id)
              FROM user_impact_candidates candidate
        ), '[]'::jsonb)
    ),
    'orphan_connector_run_sources', jsonb_build_object(
        'distinct_source_count', (SELECT count(*) FROM orphan_sources),
        'run_count', (SELECT coalesce(sum(run_count), 0) FROM orphan_sources),
        'top_20', coalesce((
            SELECT jsonb_agg(to_jsonb(orphan) ORDER BY orphan.run_count DESC, orphan.source)
              FROM (
                    SELECT source, run_count, oldest_at, newest_at
                      FROM orphan_sources
                     ORDER BY run_count DESC, source
                     LIMIT 20
              ) orphan
        ), '[]'::jsonb)
    ),
    'top_20_heavy_sources', coalesce((
        SELECT jsonb_agg(to_jsonb(heavy) ORDER BY heavy.total_rows DESC, heavy.source)
          FROM (
                SELECT source, total_rows, counts_by_relationship, oldest_at, newest_at, has_connector
                  FROM source_with_registration
                 ORDER BY total_rows DESC, source
                 LIMIT 20
          ) heavy
    ), '[]'::jsonb),
    'approximate_table_sizes', coalesce((
        SELECT jsonb_agg(to_jsonb(size_row) ORDER BY size_row.total_bytes DESC, size_row.table_name)
          FROM (
                SELECT table_name, estimated_rows, table_bytes, index_bytes, total_bytes
                  FROM table_sizes
                 ORDER BY total_bytes DESC, table_name
          ) size_row
    ), '[]'::jsonb)
);

COMMIT;
'@
$sql = $sql.Replace("__TIMEOUT_MS__", [string]$statementTimeoutMilliseconds)

$jsonText = Invoke-PreviewQuery `
    -Connection $connection `
    -Workspace $workspace `
    -Sql $sql `
    -ConnectTimeout $ConnectTimeoutSeconds

if ([string]::IsNullOrWhiteSpace($jsonText)) {
    throw "PostgreSQL preview returned no report."
}

try {
    $report = $jsonText | ConvertFrom-Json
}
catch {
    throw "PostgreSQL preview did not return valid JSON."
}

if ($null -eq $report.safety -or
    $report.safety.transaction_read_only -ne $true -or
    [int64]$report.safety.rows_changed -ne 0) {
    throw "Read-only safety proof was missing or invalid; no report was written."
}

$formatted = $report | ConvertTo-Json -Depth 30
$temporaryPath = "$safeReportPath.tmp-$PID"
try {
    [IO.File]::WriteAllText(
        $temporaryPath,
        $formatted + [Environment]::NewLine,
        (New-Object System.Text.UTF8Encoding($false))
    )
    Move-Item -LiteralPath $temporaryPath -Destination $safeReportPath
}
finally {
    if (Test-Path -LiteralPath $temporaryPath) {
        Remove-Item -LiteralPath $temporaryPath -Force
    }
}

Write-Output "Read-only test-data preview written to: $safeReportPath"
Write-Output "Database rows changed by preview: 0"
