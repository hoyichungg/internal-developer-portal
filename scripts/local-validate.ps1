param(
    [ValidateSet("Fast", "Full")]
    [string]$Mode = "Fast",

    [switch]$NoRestart,

    [string]$DatabaseUrl = $env:DATABASE_URL,

    [string]$ConnectorSecretKey = $(if ($env:CONNECTOR_SECRET_KEY) { $env:CONNECTOR_SECRET_KEY } else { "dev-connector-secret-key" }),

    [string]$Port = $(if ($env:ROCKET_PORT) { $env:ROCKET_PORT } else { "8000" })
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$DefaultDatabaseUrl = "postgres://postgres:postgres@localhost:5432/app_db"
$ServiceTargetDir = Join-Path $RepoRoot "target\local-services"
$LogDir = Join-Path $RepoRoot "target\local-validation-logs"

if (-not $DatabaseUrl) {
    $DatabaseUrl = $DefaultDatabaseUrl
}

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
        $containerId = (& docker compose ps -q $service 2>$null | Select-Object -First 1)
        if ($LASTEXITCODE -ne 0 -or -not $containerId) {
            continue
        }

        $isRunning = (& docker inspect -f "{{.State.Running}}" $containerId 2>$null | Select-Object -First 1)
        if ($isRunning -eq "true") {
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
    & docker compose start @Services
    if ($LASTEXITCODE -ne 0) {
        throw "docker compose start failed"
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
        [string]$Stderr
    )

    $env:DATABASE_URL = $DatabaseUrl
    $env:ROCKET_PORT = $Port
    $env:CONNECTOR_SECRET_KEY = $ConnectorSecretKey
    $env:APP_ENV = "development"

    Write-Host "Starting $Name from $FilePath"
    Start-Process `
        -FilePath $FilePath `
        -WorkingDirectory $RepoRoot `
        -WindowStyle Hidden `
        -RedirectStandardOutput $Stdout `
        -RedirectStandardError $Stderr `
        -PassThru
}

function Wait-ForHealth {
    $healthUrl = "http://127.0.0.1:$Port/health"
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
            CARGO_TARGET_DIR = $ServiceTargetDir
            DATABASE_URL = $DatabaseUrl
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
        -Stderr (Join-Path $LogDir "server.stderr.log")

    Wait-ForHealth

    $worker = Start-PortalProcess `
        -Name "worker" `
        -FilePath $workerExe `
        -Stdout (Join-Path $LogDir "worker.stdout.log") `
        -Stderr (Join-Path $LogDir "worker.stderr.log")

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
            -Stderr $stderr |
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
        Invoke-CommandStep -Name "Rust Clippy" -FilePath "cargo" -Arguments @("clippy", "--all-targets", "--", "-D", "warnings")
        Invoke-CommandStep `
            -Name "Rust library tests" `
            -FilePath "cargo" `
            -Arguments @("test", "--lib") `
            -Environment @{
                DATABASE_URL = $DatabaseUrl
                CONNECTOR_SECRET_KEY = $ConnectorSecretKey
            }
        Write-Host ""
        Write-Host "Fast validation passed." -ForegroundColor Green
        exit 0
    }

    $originalProcesses = @(Get-PortalServiceSnapshots)
    $composeServices = @(Get-RunningComposeServices)
    $hasOriginalServer = @($originalProcesses | Where-Object { $_.ProcessName -eq "server" }).Count -gt 0
    $isolatedProcesses = @()

    try {
        if ($originalProcesses.Count -gt 0) {
            Stop-PortalServiceProcesses -Processes $originalProcesses
        }
        if ($composeServices.Count -gt 0) {
            Stop-ComposeServices -Services $composeServices
        }
        Wait-ForPortRelease

        Invoke-CommandStep -Name "Rust format check" -FilePath "cargo" -Arguments @("fmt", "--check")
        Invoke-CommandStep -Name "Frontend build" -FilePath "pnpm" -Arguments @("--dir", "frontend", "build")
        Invoke-CommandStep -Name "Rust Clippy" -FilePath "cargo" -Arguments @("clippy", "--all-targets", "--", "-D", "warnings")
        Build-IsolatedServices
        $isolatedProcesses = @(Start-IsolatedServices)
        Invoke-CommandStep `
            -Name "Full Rust test suite" `
            -FilePath "cargo" `
            -Arguments @("test") `
            -Environment @{
                DATABASE_URL = $DatabaseUrl
                CONNECTOR_SECRET_KEY = $ConnectorSecretKey
            }

        Write-Host ""
        Write-Host "Full validation passed." -ForegroundColor Green
    }
    finally {
        if ($isolatedProcesses.Count -gt 0) {
            Stop-PortalServiceProcesses -Processes $isolatedProcesses
            Wait-ForPortRelease
        }
        Restart-OriginalServices -OriginalProcesses $originalProcesses
        if ($hasOriginalServer -and $composeServices.Count -gt 0 -and -not $NoRestart) {
            Write-Host "Leaving Docker compose app/worker stopped because a local server was restored on port $Port."
        }
        elseif ($composeServices.Count -gt 0) {
            Start-ComposeServices -Services $composeServices
        }
    }
}
finally {
    Pop-Location
}
