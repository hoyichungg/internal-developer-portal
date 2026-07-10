[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$BaseUrl,

    [Parameter()]
    [string]$Username = $env:PORTAL_SMOKE_USERNAME,

    [Parameter()]
    [System.Security.SecureString]$Password,

    [Parameter()]
    [switch]$SkipOverview,

    [Parameter()]
    [ValidateRange(1, 300)]
    [int]$TimeoutSeconds = 15
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-HasProperty {
    param(
        [Parameter(Mandatory = $true)]
        [AllowNull()]
        [object]$Object,

        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string]$Location
    )

    if ($null -eq $Object -or $null -eq $Object.PSObject.Properties[$Name]) {
        throw "$Location is missing required property '$Name'."
    }
}

function Assert-NonEmptyString {
    param(
        [Parameter(Mandatory = $true)]
        [AllowNull()]
        [object]$Value,

        [Parameter(Mandatory = $true)]
        [string]$Location
    )

    if ($null -eq $Value -or [string]::IsNullOrWhiteSpace([string]$Value)) {
        throw "$Location must be a non-empty string."
    }
}

function Get-HttpStatusCode {
    param(
        [Parameter(Mandatory = $true)]
        [System.Exception]$Exception
    )

    try {
        if ($null -eq $Exception.Response) {
            return $null
        }

        return [int]$Exception.Response.StatusCode
    }
    catch {
        return $null
    }
}

function Invoke-PortalRequest {
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet("GET", "POST")]
        [string]$Method,

        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [int[]]$ExpectedStatus,

        [Parameter()]
        [hashtable]$Headers = @{},

        [Parameter()]
        [AllowNull()]
        [string]$Body,

        [Parameter()]
        [switch]$NoJson
    )

    $request = @{
        Uri             = "$script:PortalBaseUrl$Path"
        Method          = $Method
        Headers         = $Headers
        TimeoutSec      = $TimeoutSeconds
        ErrorAction     = "Stop"
        UseBasicParsing = $true
    }

    if ($PSBoundParameters.ContainsKey("Body") -and $null -ne $Body) {
        $request.Body = $Body
        $request.ContentType = "application/json"
    }

    try {
        $response = Invoke-WebRequest @request
    }
    catch {
        $statusCode = Get-HttpStatusCode -Exception $_.Exception
        $statusText = if ($null -eq $statusCode) { "" } else { " with HTTP $statusCode" }
        $failureReason = $_.Exception.Message
        throw [System.InvalidOperationException]::new(
            "$Method $Path failed$statusText ($failureReason). Check the service and reverse-proxy logs.",
            $_.Exception
        )
    }

    $actualStatus = [int]$response.StatusCode
    if ($ExpectedStatus -notcontains $actualStatus) {
        throw "$Method $Path returned HTTP $actualStatus; expected $($ExpectedStatus -join ' or ')."
    }

    $json = $null
    if (-not $NoJson) {
        if ([string]::IsNullOrWhiteSpace([string]$response.Content)) {
            throw "$Method $Path returned an empty response; a JSON data envelope was expected."
        }

        try {
            $json = $response.Content | ConvertFrom-Json
        }
        catch {
            throw "$Method $Path did not return valid JSON."
        }

        Assert-HasProperty -Object $json -Name "data" -Location "$Method $Path response"
        if ($null -eq $json.data) {
            throw "$Method $Path returned a null data envelope."
        }
    }

    return [PSCustomObject]@{
        StatusCode = $actualStatus
        Json       = $json
    }
}

function Assert-HealthyResponse {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Data,

        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter()]
        [switch]$IncludeDatabase
    )

    Assert-HasProperty -Object $Data -Name "status" -Location "$Path data"
    Assert-HasProperty -Object $Data -Name "service" -Location "$Path data"
    if ([string]$Data.status -ne "ok") {
        throw "$Path data.status was '$($Data.status)'; expected 'ok'."
    }
    Assert-NonEmptyString -Value $Data.service -Location "$Path data.service"

    if ($IncludeDatabase) {
        Assert-HasProperty -Object $Data -Name "checks" -Location "$Path data"
        Assert-HasProperty -Object $Data.checks -Name "database" -Location "$Path data.checks"
        if ([string]$Data.checks.database -ne "ok") {
            throw "$Path data.checks.database was '$($Data.checks.database)'; expected 'ok'."
        }
    }
}

try {
    try {
        $parsedBaseUrl = [System.Uri]::new($BaseUrl)
    }
    catch {
        throw "BaseUrl must be an absolute HTTP or HTTPS URL."
    }

    if (-not $parsedBaseUrl.IsAbsoluteUri -or $parsedBaseUrl.Scheme -notin @("http", "https")) {
        throw "BaseUrl must be an absolute HTTP or HTTPS URL."
    }
    if (-not [string]::IsNullOrEmpty($parsedBaseUrl.UserInfo)) {
        throw "BaseUrl must not contain credentials."
    }
    if (-not [string]::IsNullOrEmpty($parsedBaseUrl.Query) -or
        -not [string]::IsNullOrEmpty($parsedBaseUrl.Fragment) -or
        $parsedBaseUrl.AbsolutePath -ne "/") {
        throw "BaseUrl must be an origin such as https://portal.example.com, without a path, query, or fragment."
    }

    $loopbackHosts = @("localhost", "127.0.0.1", "::1", "[::1]")
    if ($parsedBaseUrl.Scheme -ne "https" -and $loopbackHosts -notcontains $parsedBaseUrl.Host) {
        throw "Plain HTTP is permitted only for a loopback BaseUrl. Use the production HTTPS origin."
    }

    $script:PortalBaseUrl = $BaseUrl.TrimEnd("/")

    if ([string]::IsNullOrWhiteSpace($Username)) {
        $Username = Read-Host "Portal username"
    }
    if ([string]::IsNullOrWhiteSpace($Username)) {
        throw "Username is required (parameter, prompt, or PORTAL_SMOKE_USERNAME)."
    }

    if ($null -eq $Password) {
        if (-not [string]::IsNullOrEmpty($env:PORTAL_SMOKE_PASSWORD)) {
            $Password = ConvertTo-SecureString -String $env:PORTAL_SMOKE_PASSWORD -AsPlainText -Force
            Remove-Item Env:PORTAL_SMOKE_PASSWORD -ErrorAction SilentlyContinue
        }
        else {
            $Password = Read-Host "Portal password" -AsSecureString
        }
    }
    if ($null -eq $Password -or $Password.Length -eq 0) {
        throw "Password is required (secure prompt, SecureString parameter, or PORTAL_SMOKE_PASSWORD)."
    }

    $livez = Invoke-PortalRequest -Method GET -Path "/livez" -ExpectedStatus 200
    Assert-HealthyResponse -Data $livez.Json.data -Path "/livez"
    Write-Host "[PASS] GET /livez: API process is alive."

    $readyz = Invoke-PortalRequest -Method GET -Path "/readyz" -ExpectedStatus 200
    Assert-HealthyResponse -Data $readyz.Json.data -Path "/readyz" -IncludeDatabase
    Write-Host "[PASS] GET /readyz: API and PostgreSQL are ready."

    $passwordPointer = [IntPtr]::Zero
    $plainPassword = $null
    $loginBody = $null
    try {
        $passwordPointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($Password)
        $plainPassword = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($passwordPointer)
        $loginBody = @{
            username = $Username
            password = $plainPassword
        } | ConvertTo-Json -Compress

        $login = Invoke-PortalRequest -Method POST -Path "/login" -ExpectedStatus 200 -Body $loginBody
    }
    finally {
        if ($passwordPointer -ne [IntPtr]::Zero) {
            [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($passwordPointer)
        }
        $plainPassword = $null
        $loginBody = $null
        Remove-Variable plainPassword, loginBody -ErrorAction SilentlyContinue
    }

    $loginData = $login.Json.data
    Assert-HasProperty -Object $loginData -Name "token" -Location "/login data"
    Assert-HasProperty -Object $loginData -Name "token_type" -Location "/login data"
    Assert-HasProperty -Object $loginData -Name "expires_at" -Location "/login data"
    Assert-NonEmptyString -Value $loginData.token -Location "/login data.token"
    if ([string]$loginData.token_type -ne "Bearer") {
        throw "/login data.token_type was '$($loginData.token_type)'; expected 'Bearer'."
    }
    Assert-NonEmptyString -Value $loginData.expires_at -Location "/login data.expires_at"

    # Keep the bearer token only in this process. Never print this variable or put it in a URL.
    $token = [string]$loginData.token
    $authorizationHeaders = @{ Authorization = "Bearer $token" }

    try {
        Write-Host "[PASS] POST /login: valid bearer session returned."

        $me = Invoke-PortalRequest -Method GET -Path "/me" -ExpectedStatus 200 -Headers $authorizationHeaders
        $meData = $me.Json.data
        foreach ($property in @("id", "username", "expires_at", "roles", "capabilities", "maintainer_access")) {
            Assert-HasProperty -Object $meData -Name $property -Location "/me data"
        }
        Assert-NonEmptyString -Value $meData.username -Location "/me data.username"
        Assert-NonEmptyString -Value $meData.expires_at -Location "/me data.expires_at"
        foreach ($capability in @("manage_connectors", "view_audit", "manage_maintainers", "view_user_directory")) {
            Assert-HasProperty -Object $meData.capabilities -Name $capability -Location "/me data.capabilities"
        }
        Write-Host "[PASS] GET /me: identity, roles, and capabilities are present."

        if (-not $SkipOverview) {
            $overview = Invoke-PortalRequest -Method GET -Path "/me/overview" -ExpectedStatus 200 -Headers $authorizationHeaders
            $overviewData = $overview.Json.data
            foreach ($property in @(
                    "user",
                    "maintainers",
                    "services",
                    "packages",
                    "today_calendar_events",
                    "open_work_cards",
                    "unread_notifications",
                    "priority_items",
                    "health_history",
                    "operations",
                    "summary"
                )) {
                Assert-HasProperty -Object $overviewData -Name $property -Location "/me/overview data"
            }
            Assert-HasProperty -Object $overviewData.user -Name "id" -Location "/me/overview data.user"
            if ([string]$overviewData.user.id -ne [string]$meData.id) {
                throw "/me/overview returned a different user id from /me."
            }
            foreach ($property in @("worker_status", "health_data_stale")) {
                Assert-HasProperty -Object $overviewData.operations -Name $property -Location "/me/overview data.operations"
            }
            foreach ($property in @(
                    "maintainers",
                    "services",
                    "unhealthy_services",
                    "packages",
                    "today_calendar_events",
                    "open_work_cards",
                    "unread_notifications",
                    "failed_connector_runs"
                )) {
                Assert-HasProperty -Object $overviewData.summary -Name $property -Location "/me/overview data.summary"
            }
            Write-Host "[PASS] GET /me/overview: operational overview contract is complete."
        }
        else {
            Write-Host "[SKIP] GET /me/overview was skipped by request."
        }
    }
    finally {
        if (-not [string]::IsNullOrEmpty($token)) {
            $logout = Invoke-PortalRequest -Method POST -Path "/logout" -ExpectedStatus 204 -Headers $authorizationHeaders -NoJson
            Write-Host "[PASS] POST /logout: smoke-test session was removed."
            $token = $null
            $authorizationHeaders.Clear()
        }
    }

    Write-Host "Production smoke test passed."
}
catch {
    Write-Error "Production smoke test failed: $($_.Exception.Message)"
    exit 1
}
