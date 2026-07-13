param(
    [ValidateSet("Fast", "Full")]
    [string]$Mode = "Fast",

    [switch]$NoRestart,

    [Alias("DatabaseUrl")]
    [string]$PortalTestDatabaseUrl = $env:PORTAL_TEST_DATABASE_URL,

    [string]$RetentionTestDatabaseUrl = $env:RETENTION_TEST_DATABASE_URL,

    [string]$PortalTestBaseUrl = $env:PORTAL_TEST_BASE_URL,

    [string]$DevelopmentDatabaseUrl = $env:DATABASE_URL,

    [string]$ConnectorSecretKey = $(if ($env:CONNECTOR_SECRET_KEY) { $env:CONNECTOR_SECRET_KEY } else { "dev-connector-secret-key" }),

    [string]$Port = $(if ($env:ROCKET_PORT) { $env:ROCKET_PORT } else { "8000" })
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
# Cargo and pnpm legitimately write progress to stderr. Keep native stderr from
# becoming a terminating PowerShell error; Invoke-CommandStep still checks
# $LASTEXITCODE for real command failures.
$PSNativeCommandUseErrorActionPreference = $false

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$DefaultPortalTestDatabaseUrl = "postgres://postgres:postgres@localhost:5432/portal_integration_test"
$DefaultRetentionTestDatabaseUrl = "postgres://postgres:postgres@localhost:5432/portal_retention_test"
$DefaultDevelopmentDatabaseUrl = "postgres://postgres:postgres@localhost:5432/app_db"
$FastValidationDatabaseUrl = "postgres://unused:unused@127.0.0.1:1/fast_validation_test_no_database"
$ServiceTargetDir = Join-Path $RepoRoot "target\local-services"
$ClippyTargetDir = Join-Path $RepoRoot "target\validation-clippy"
$LogDir = Join-Path $RepoRoot "target\local-validation-logs"

if (-not $PortalTestDatabaseUrl) {
    $PortalTestDatabaseUrl = $DefaultPortalTestDatabaseUrl
}
if (-not $RetentionTestDatabaseUrl) {
    $RetentionTestDatabaseUrl = $DefaultRetentionTestDatabaseUrl
}
if (-not $DevelopmentDatabaseUrl) {
    $DevelopmentDatabaseUrl = $DefaultDevelopmentDatabaseUrl
}
if (-not $PortalTestBaseUrl) {
    $PortalTestBaseUrl = "http://127.0.0.1:$Port"
}

try {
    $portalTestBaseUri = [Uri]$PortalTestBaseUrl
}
catch {
    throw "PORTAL_TEST_BASE_URL must be a valid absolute HTTP loopback origin."
}
if (-not $portalTestBaseUri.IsAbsoluteUri -or
    $portalTestBaseUri.Scheme -ne "http" -or
    $portalTestBaseUri.Host -notin @("localhost", "127.0.0.1", "::1", "[::1]") -or
    -not [string]::IsNullOrEmpty($portalTestBaseUri.UserInfo) -or
    -not [string]::IsNullOrEmpty($portalTestBaseUri.Query) -or
    -not [string]::IsNullOrEmpty($portalTestBaseUri.Fragment) -or
    $portalTestBaseUri.AbsolutePath -ne "/") {
    throw "PORTAL_TEST_BASE_URL must be an HTTP loopback origin without credentials, path, query, or fragment."
}
$PortalTestBaseUrl = $PortalTestBaseUrl.TrimEnd("/")
$Port = [string]$portalTestBaseUri.Port
$PortalTestRocketDatabases = "{postgres={url=`"$PortalTestDatabaseUrl`"}}"
$FastValidationRocketDatabases = "{postgres={url=`"$FastValidationDatabaseUrl`"}}"

New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

function Invoke-Step {
    param(
        [string]$Name,
        [scriptblock]$Script
    )

    Write-Host ""
    Write-Host "==> $Name" -ForegroundColor Cyan
    & $Script
}

function Invoke-CommandStep {
    param(
        [string]$Name,
        [string]$FilePath,
        [string[]]$Arguments,
        [hashtable]$Environment = @{}
    )

    Invoke-Step $Name {
        $previousValues = @{}
        foreach ($key in $Environment.Keys) {
            $previousValues[$key] = [Environment]::GetEnvironmentVariable($key, "Process")
            [Environment]::SetEnvironmentVariable($key, [string]$Environment[$key], "Process")
        }

        try {
            & $FilePath @Arguments
            if ($LASTEXITCODE -ne 0) {
                throw "$FilePath exited with code $LASTEXITCODE"
            }
        }
        finally {
            foreach ($key in $Environment.Keys) {
                [Environment]::SetEnvironmentVariable($key, $previousValues[$key], "Process")
            }
        }
    }
}

function Get-LocalComposeDatabaseDescriptor {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Url,

        [Parameter(Mandatory = $true)]
        [string]$VariableName,

        [switch]$RequireTestSegment
    )

    try {
        $uri = [Uri]$Url
    }
    catch {
        throw "$VariableName must be a valid PostgreSQL URL."
    }

    if (-not $uri.IsAbsoluteUri -or $uri.Scheme -notin @("postgres", "postgresql")) {
        throw "$VariableName must be an absolute postgres:// or postgresql:// URL."
    }
    if ($uri.Host -notin @("localhost", "127.0.0.1", "::1", "[::1]") -or $uri.Port -ne 5432) {
        throw "$VariableName must point to the local Compose PostgreSQL service on loopback port 5432."
    }
    if ([Uri]::UnescapeDataString($uri.UserInfo) -ne "postgres:postgres") {
        throw "$VariableName must use the local Compose postgres credentials."
    }
    if (-not [string]::IsNullOrEmpty($uri.Query) -or -not [string]::IsNullOrEmpty($uri.Fragment)) {
        throw "$VariableName must not contain a query or fragment."
    }

    $databaseName = [Uri]::UnescapeDataString($uri.AbsolutePath.Trim("/"))
    if ($databaseName -notmatch '^[A-Za-z0-9_-]+$') {
        throw "$VariableName must contain exactly one simple database name."
    }
    $nameSegments = @($databaseName.ToLowerInvariant() -split "[^a-z0-9]+" | Where-Object { $_ })
    if ($RequireTestSegment -and $nameSegments -notcontains "test") {
        throw "Refusing Full validation: $VariableName database '$databaseName' must contain a standalone 'test' segment."
    }

    return [pscustomobject]@{
        Url = $Url
        DatabaseName = $databaseName
        ContainerUrl = "postgres://postgres:postgres@postgres:5432/$databaseName"
    }
}

function Get-LatestMigrationVersion {
    $latest = Get-ChildItem (Join-Path $RepoRoot "migrations") -Directory |
        Sort-Object Name |
        Select-Object -Last 1
    if ($null -eq $latest) {
        throw "No Diesel migrations were found."
    }

    $version = $latest.Name -replace '[^0-9]', ''
    if ($version -notmatch '^\d{14}$') {
        throw "Latest migration directory '$($latest.Name)' does not contain a 14-digit Diesel version."
    }
    return $version
}

function Invoke-ComposeChecked {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments,

        [Parameter(Mandatory = $true)]
        [string]$FailureMessage
    )

    & docker compose @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw $FailureMessage
    }
}

function Ensure-TestDatabase {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Descriptor,

        [Parameter(Mandatory = $true)]
        [string]$ExpectedMigrationVersion
    )

    $databaseName = [string]$Descriptor.DatabaseName
    Invoke-ComposeChecked `
        -Arguments @(
            "exec",
            "-T",
            "postgres",
            "dropdb",
            "-U",
            "postgres",
            "--if-exists",
            "--force",
            $databaseName
        ) `
        -FailureMessage "Could not reset isolated test database '$databaseName'."
    Invoke-ComposeChecked `
        -Arguments @("exec", "-T", "postgres", "createdb", "-U", "postgres", $databaseName) `
        -FailureMessage "Could not create isolated test database '$databaseName'."

    Invoke-ComposeChecked `
        -Arguments @(
            "run",
            "--rm",
            "-e",
            "DATABASE_URL=$($Descriptor.ContainerUrl)",
            "migrate",
            "diesel",
            "migration",
            "run"
        ) `
        -FailureMessage "Diesel migrations failed for isolated test database '$databaseName'."

    $actualDatabase = (@(& docker compose exec -T postgres psql -U postgres -d $databaseName -Atc `
        "SELECT current_database();") -join "").Trim()
    if ($LASTEXITCODE -ne 0 -or $actualDatabase -ne $databaseName) {
        throw "Database URL '$databaseName' resolved to unexpected database '$actualDatabase'."
    }

    $actualSegments = @($actualDatabase.ToLowerInvariant() -split "[^a-z0-9]+" | Where-Object { $_ })
    if ($actualSegments -notcontains "test") {
        throw "Refusing validation: actual database '$actualDatabase' lacks a standalone 'test' segment."
    }

    $latestMigration = (@(& docker compose exec -T postgres psql -U postgres -d $databaseName -Atc `
        "SELECT version FROM __diesel_schema_migrations ORDER BY version DESC LIMIT 1;") -join "").Trim()
    if ($LASTEXITCODE -ne 0 -or $latestMigration -ne $ExpectedMigrationVersion) {
        throw "Database '$databaseName' latest migration is '$latestMigration'; expected '$ExpectedMigrationVersion'."
    }

    Write-Host "Verified isolated database '$databaseName' at migration $latestMigration."
}

function Get-DatabaseFingerprint {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Descriptor
    )

    $databaseName = [string]$Descriptor.DatabaseName
    $actualDatabase = (@(& docker compose exec -T postgres psql -U postgres -d $databaseName -Atc `
        "SELECT current_database();") -join "").Trim()
    if ($LASTEXITCODE -ne 0 -or $actualDatabase -ne $databaseName) {
        throw "Development database URL '$databaseName' resolved to unexpected database '$actualDatabase'."
    }

    $tableNames = @(& docker compose exec -T postgres psql -U postgres -d $databaseName -Atc `
        "SELECT tablename FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename;")
    if ($LASTEXITCODE -ne 0) {
        throw "Could not list public tables in development database '$databaseName'."
    }

    $entries = @()
    foreach ($rawTableName in $tableNames) {
        $tableName = ([string]$rawTableName).Trim()
        if (-not $tableName) {
            continue
        }

        $quotedTableName = '"' + $tableName.Replace('"', '""') + '"'
        $rowCount = (@(& docker compose exec -T postgres psql -U postgres -d $databaseName -Atc `
            "SELECT count(*) FROM public.$quotedTableName;") -join "").Trim()
        if ($LASTEXITCODE -ne 0 -or $rowCount -notmatch '^\d+$') {
            throw "Could not count development database table '$tableName'."
        }
        $entries += "$tableName=$rowCount"
    }

    return [pscustomobject]@{
        DatabaseName = $databaseName
        Signature = $entries -join "`n"
        TableCount = $entries.Count
    }
}

function Prepare-TestDatabases {
    param(
        [Parameter(Mandatory = $true)]
        [object]$IntegrationDescriptor,

        [Parameter(Mandatory = $true)]
        [object]$RetentionDescriptor
    )

    if ($IntegrationDescriptor.DatabaseName -eq $RetentionDescriptor.DatabaseName) {
        throw "PORTAL_TEST_DATABASE_URL and RETENTION_TEST_DATABASE_URL must name different databases."
    }

    Invoke-ComposeChecked `
        -Arguments @("up", "-d", "postgres") `
        -FailureMessage "Could not start the local Compose PostgreSQL service."
    Invoke-ComposeChecked `
        -Arguments @("build", "migrate") `
        -FailureMessage "Could not build the migration image for isolated test databases."

    $expectedMigrationVersion = Get-LatestMigrationVersion
    Ensure-TestDatabase -Descriptor $IntegrationDescriptor -ExpectedMigrationVersion $expectedMigrationVersion
    Ensure-TestDatabase -Descriptor $RetentionDescriptor -ExpectedMigrationVersion $expectedMigrationVersion
}

function Get-PortalServiceProcesses {
    Get-Process -ErrorAction SilentlyContinue |
        Where-Object {
            ($_.ProcessName -in @("server", "worker")) -and
            $_.Path -and
            $_.Path.StartsWith($RepoRoot, [System.StringComparison]::OrdinalIgnoreCase)
        } |
        Sort-Object ProcessName, Id
}

function Get-PortalServiceSnapshots {
    Get-PortalServiceProcesses |
        ForEach-Object {
            [pscustomobject]@{
                Id = $_.Id
                ProcessName = $_.ProcessName
                Path = $_.Path
            }
        }
}

function Get-RunningComposeServices {
    if (-not (Test-Path (Join-Path $RepoRoot "docker-compose.yml"))) {
        return @()
    }

    try {
        & docker compose version *> $null
    }
    catch {
        return @()
    }
    if ($LASTEXITCODE -ne 0) {
        return @()
    }

    $runningServices = @()
    foreach ($service in @("app", "worker")) {
        # Do not pipe a native process directly into Select-Object -First.
        # Windows PowerShell can report LASTEXITCODE=-1 when the downstream
        # command closes the pipe, which would hide active database writers.
        $containerIds = @(& docker compose ps -q $service 2>$null)
        $composeExitCode = $LASTEXITCODE
        $containerId = $containerIds | Select-Object -First 1
        if ($composeExitCode -ne 0 -or -not $containerId) {
            continue
        }

        $inspectOutput = @(& docker inspect -f "{{.State.Running}}" $containerId 2>$null)
        $inspectExitCode = $LASTEXITCODE
        $isRunning = $inspectOutput | Select-Object -First 1
        if ($inspectExitCode -eq 0 -and $isRunning -eq "true") {
            $runningServices += $service
        }
    }

    return $runningServices
}

function Stop-ComposeServices {
    param([string[]]$Services)

    if ($Services.Count -eq 0) {
        return
    }

    Write-Host "Stopping Docker compose services: $($Services -join ', ')"
    & docker compose stop @Services
    if ($LASTEXITCODE -ne 0) {
        throw "docker compose stop failed"
    }
}

function Start-ComposeServices {
    param([string[]]$Services)

    if ($NoRestart) {
        Write-Host "Skipping Docker compose restart because -NoRestart was provided."
        return
    }
    if ($Services.Count -eq 0) {
        return
    }

    Write-Host "Starting Docker compose services: $($Services -join ', ')"
    & docker compose up -d --no-deps @Services
    if ($LASTEXITCODE -ne 0) {
        throw "docker compose up failed"
    }
}

function Get-PortListeners {
    $portPattern = ":" + [regex]::Escape($Port) + "$"
    netstat -ano -p tcp |
        Select-String -Pattern "LISTENING" |
        ForEach-Object {
            $columns = $_.Line.Trim() -split "\s+"
            if ($columns.Count -lt 5) {
                return
            }

            $localAddress = $columns[1]
            if ($localAddress -notmatch $portPattern) {
                return
            }

            $processId = [int]$columns[-1]
            $process = Get-Process -Id $processId -ErrorAction SilentlyContinue
            [pscustomobject]@{
                Id = $processId
                ProcessName = if ($process) { $process.ProcessName } else { "unknown" }
                Path = if ($process) { $process.Path } else { "" }
            }
        }
}

function Wait-ForPortRelease {
    for ($attempt = 1; $attempt -le 20; $attempt++) {
        $listeners = @(Get-PortListeners)
        if ($listeners.Count -eq 0) {
            return
        }
        Start-Sleep -Milliseconds 500
    }

    $summary = @(Get-PortListeners | ForEach-Object { "$($_.ProcessName) pid=$($_.Id)" }) -join ", "
    throw "Port $Port is still in use by: $summary"
}

function Stop-PortalServiceProcesses {
    param([object[]]$Processes)

    foreach ($process in $Processes) {
        Write-Host "Stopping $($process.ProcessName) pid=$($process.Id) path=$($process.Path)"
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }

    foreach ($process in $Processes) {
        Wait-Process -Id $process.Id -Timeout 5 -ErrorAction SilentlyContinue
    }
}

function Start-PortalProcess {
    param(
        [string]$Name,
        [string]$FilePath,
        [string]$Stdout,
        [string]$Stderr,
        [string]$AppEnvironment = "development",
        [string]$ProcessDatabaseUrl = $PortalTestDatabaseUrl
    )

    $processEnvironment = @{
        DATABASE_URL = $ProcessDatabaseUrl
        ROCKET_DATABASES = "{postgres={url=`"$ProcessDatabaseUrl`"}}"
        ROCKET_PORT = $Port
        CONNECTOR_SECRET_KEY = $ConnectorSecretKey
        APP_ENV = $AppEnvironment
    }
    if ($AppEnvironment -eq "test") {
        # Full validation uses a separate guarded database for retention. The
        # ordinary integration server/worker must never run global cleanup
        # against the application DATABASE_URL.
        $processEnvironment.CONNECTOR_HEALTH_RETENTION_DAYS = "0"
        $processEnvironment.CONNECTOR_RUN_RETENTION_DAYS = "0"
        $processEnvironment.AUDIT_LOG_RETENTION_DAYS = "0"
        $processEnvironment.PORTAL_TEST_DATABASE_URL = $PortalTestDatabaseUrl
        $processEnvironment.PORTAL_TEST_BASE_URL = $PortalTestBaseUrl
    }

    $previousValues = @{}
    foreach ($key in $processEnvironment.Keys) {
        $previousValues[$key] = [Environment]::GetEnvironmentVariable($key, "Process")
        [Environment]::SetEnvironmentVariable($key, [string]$processEnvironment[$key], "Process")
    }

    try {
        Write-Host "Starting $Name from $FilePath"
        $process = Start-Process `
            -FilePath $FilePath `
            -WorkingDirectory $RepoRoot `
            -WindowStyle Hidden `
            -RedirectStandardOutput $Stdout `
            -RedirectStandardError $Stderr `
            -PassThru
    }
    finally {
        foreach ($key in $processEnvironment.Keys) {
            [Environment]::SetEnvironmentVariable($key, $previousValues[$key], "Process")
        }
    }

    return $process
}

function Wait-ForHealth {
    $healthUrl = "$PortalTestBaseUrl/health"
    for ($attempt = 1; $attempt -le 30; $attempt++) {
        try {
            $response = Invoke-WebRequest -Uri $healthUrl -UseBasicParsing -TimeoutSec 2
            if ($response.StatusCode -eq 200) {
                Write-Host "Health check passed at $healthUrl"
                return
            }
        }
        catch {
            Start-Sleep -Milliseconds 500
        }
    }

    throw "Server did not become healthy at $healthUrl"
}

function Build-IsolatedServices {
    Invoke-CommandStep `
        -Name "Build isolated server/worker binaries" `
        -FilePath "cargo" `
        -Arguments @("build", "--bin", "server", "--bin", "worker") `
        -Environment @{
            APP_ENV = "test"
            CARGO_TARGET_DIR = $ServiceTargetDir
            DATABASE_URL = $PortalTestDatabaseUrl
            ROCKET_DATABASES = $PortalTestRocketDatabases
            PORTAL_TEST_DATABASE_URL = $PortalTestDatabaseUrl
            PORTAL_TEST_BASE_URL = $PortalTestBaseUrl
            CONNECTOR_SECRET_KEY = $ConnectorSecretKey
        }
}

function Start-IsolatedServices {
    $serverExe = Join-Path $ServiceTargetDir "debug\server.exe"
    $workerExe = Join-Path $ServiceTargetDir "debug\worker.exe"

    if (-not (Test-Path $serverExe)) {
        throw "Missing isolated server binary: $serverExe"
    }
    if (-not (Test-Path $workerExe)) {
        throw "Missing isolated worker binary: $workerExe"
    }

    $server = Start-PortalProcess `
        -Name "server" `
        -FilePath $serverExe `
        -Stdout (Join-Path $LogDir "server.stdout.log") `
        -Stderr (Join-Path $LogDir "server.stderr.log") `
        -AppEnvironment "test"

    Wait-ForHealth

    $worker = Start-PortalProcess `
        -Name "worker" `
        -FilePath $workerExe `
        -Stdout (Join-Path $LogDir "worker.stdout.log") `
        -Stderr (Join-Path $LogDir "worker.stderr.log") `
        -AppEnvironment "test"

    return @($server, $worker)
}

function Restart-OriginalServices {
    param([object[]]$OriginalProcesses)

    if ($NoRestart) {
        Write-Host "Skipping restart because -NoRestart was provided."
        return
    }

    $uniqueServices = $OriginalProcesses |
        Where-Object { $_.Path -and (Test-Path $_.Path) } |
        Sort-Object ProcessName -Unique

    $startedServer = $false
    foreach ($service in $uniqueServices) {
        $stdout = Join-Path $LogDir "$($service.ProcessName).restart.stdout.log"
        $stderr = Join-Path $LogDir "$($service.ProcessName).restart.stderr.log"
        Start-PortalProcess `
            -Name $service.ProcessName `
            -FilePath $service.Path `
            -Stdout $stdout `
            -Stderr $stderr `
            -ProcessDatabaseUrl $DevelopmentDatabaseUrl |
            Out-Null

        if ($service.ProcessName -eq "server") {
            $startedServer = $true
        }
    }

    if ($startedServer) {
        Wait-ForHealth
    }
}

Push-Location $RepoRoot
try {
    if ($Mode -eq "Fast") {
        Invoke-CommandStep -Name "Rust format check" -FilePath "cargo" -Arguments @("fmt", "--check")
        Invoke-CommandStep -Name "Frontend build" -FilePath "pnpm" -Arguments @("--dir", "frontend", "build")
        Invoke-CommandStep -Name "Frontend regression tests" -FilePath "pnpm" -Arguments @("--dir", "frontend", "test:run")
        Invoke-CommandStep `
            -Name "Rust Clippy" `
            -FilePath "cargo" `
            -Arguments @("clippy", "--all-targets", "--", "-D", "warnings") `
            -Environment @{
                APP_ENV = "test"
                CARGO_TARGET_DIR = $ClippyTargetDir
                DATABASE_URL = $PortalTestDatabaseUrl
                ROCKET_DATABASES = $PortalTestRocketDatabases
                PORTAL_TEST_DATABASE_URL = $PortalTestDatabaseUrl
                PORTAL_TEST_BASE_URL = $PortalTestBaseUrl
                CONNECTOR_SECRET_KEY = $ConnectorSecretKey
            }
        Invoke-CommandStep `
            -Name "Rust library tests (database-free)" `
            -FilePath "cargo" `
            -Arguments @("test", "--lib", "--", "--skip", "repository_db_tests") `
            -Environment @{
                APP_ENV = "test"
                DATABASE_URL = $FastValidationDatabaseUrl
                ROCKET_DATABASES = $FastValidationRocketDatabases
                PORTAL_TEST_DATABASE_URL = $PortalTestDatabaseUrl
                PORTAL_TEST_BASE_URL = $PortalTestBaseUrl
                CONNECTOR_SECRET_KEY = $ConnectorSecretKey
            }
        Write-Host ""
        Write-Host "Fast validation passed." -ForegroundColor Green
        exit 0
    }

    $integrationDescriptor = Get-LocalComposeDatabaseDescriptor `
        -Url $PortalTestDatabaseUrl `
        -VariableName "PORTAL_TEST_DATABASE_URL" `
        -RequireTestSegment
    $retentionDescriptor = Get-LocalComposeDatabaseDescriptor `
        -Url $RetentionTestDatabaseUrl `
        -VariableName "RETENTION_TEST_DATABASE_URL" `
        -RequireTestSegment
    $developmentDescriptor = Get-LocalComposeDatabaseDescriptor `
        -Url $DevelopmentDatabaseUrl `
        -VariableName "DATABASE_URL"

    if ($developmentDescriptor.DatabaseName -in @(
            $integrationDescriptor.DatabaseName,
            $retentionDescriptor.DatabaseName
        )) {
        throw "DATABASE_URL must not name either disposable test database."
    }

    $originalProcesses = @(Get-PortalServiceSnapshots)
    $composeServices = @(Get-RunningComposeServices)
    $hasOriginalServer = @($originalProcesses | Where-Object { $_.ProcessName -eq "server" }).Count -gt 0
    $isolatedProcesses = @()
    $developmentFingerprint = $null

    try {
        if ($originalProcesses.Count -gt 0) {
            Stop-PortalServiceProcesses -Processes $originalProcesses
        }
        if ($composeServices.Count -gt 0) {
            Stop-ComposeServices -Services $composeServices
        }
        Wait-ForPortRelease
        Invoke-ComposeChecked `
            -Arguments @("up", "-d", "postgres") `
            -FailureMessage "Could not start the local Compose PostgreSQL service."
        $developmentFingerprint = Get-DatabaseFingerprint -Descriptor $developmentDescriptor
        Write-Host "Captured exact row counts for $($developmentFingerprint.TableCount) development database tables."
        Prepare-TestDatabases `
            -IntegrationDescriptor $integrationDescriptor `
            -RetentionDescriptor $retentionDescriptor

        Invoke-CommandStep -Name "Rust format check" -FilePath "cargo" -Arguments @("fmt", "--check")
        Invoke-CommandStep -Name "Frontend build" -FilePath "pnpm" -Arguments @("--dir", "frontend", "build")
        Invoke-CommandStep -Name "Frontend regression tests" -FilePath "pnpm" -Arguments @("--dir", "frontend", "test:run")
        Invoke-CommandStep `
            -Name "Rust Clippy" `
            -FilePath "cargo" `
            -Arguments @("clippy", "--all-targets", "--", "-D", "warnings") `
            -Environment @{
                APP_ENV = "test"
                CARGO_TARGET_DIR = $ClippyTargetDir
                DATABASE_URL = $PortalTestDatabaseUrl
                ROCKET_DATABASES = $PortalTestRocketDatabases
                PORTAL_TEST_DATABASE_URL = $PortalTestDatabaseUrl
                PORTAL_TEST_BASE_URL = $PortalTestBaseUrl
                CONNECTOR_SECRET_KEY = $ConnectorSecretKey
            }
        Build-IsolatedServices
        $isolatedProcesses = @(Start-IsolatedServices)
        Invoke-CommandStep `
            -Name "Full Rust test suite" `
            -FilePath "cargo" `
            -Arguments @("test") `
            -Environment @{
                APP_ENV = "test"
                DATABASE_URL = $PortalTestDatabaseUrl
                ROCKET_DATABASES = $PortalTestRocketDatabases
                PORTAL_TEST_DATABASE_URL = $PortalTestDatabaseUrl
                PORTAL_TEST_BASE_URL = $PortalTestBaseUrl
                RETENTION_TEST_DATABASE_URL = $RetentionTestDatabaseUrl
                CONNECTOR_SECRET_KEY = $ConnectorSecretKey
            }

        Write-Host ""
        Write-Host "Full validation passed." -ForegroundColor Green
    }
    finally {
        try {
            if ($isolatedProcesses.Count -gt 0) {
                Stop-PortalServiceProcesses -Processes $isolatedProcesses
                Wait-ForPortRelease
            }
            if ($null -ne $developmentFingerprint) {
                $afterFingerprint = Get-DatabaseFingerprint -Descriptor $developmentDescriptor
                if ($developmentFingerprint.Signature -cne $afterFingerprint.Signature) {
                    throw "Full validation changed row counts in development database '$($developmentDescriptor.DatabaseName)'."
                }
                Write-Host "Verified development database '$($developmentDescriptor.DatabaseName)' row counts are unchanged."
            }
        }
        finally {
            Restart-OriginalServices -OriginalProcesses $originalProcesses
            if ($hasOriginalServer -and $composeServices.Count -gt 0 -and -not $NoRestart) {
                Write-Host "Leaving Docker compose app/worker stopped because a local server was restored on port $Port."
            }
            elseif ($composeServices.Count -gt 0) {
                Start-ComposeServices -Services $composeServices
            }
        }
    }
}
finally {
    Pop-Location
}
