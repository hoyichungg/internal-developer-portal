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
    [ValidateSet("Any", "Enabled", "Disabled")]
    [string]$ExpectedEntraState = "Any",

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

$script:Rfc3339PropertyNames = @(
    "archived_at",
    "cancel_requested_at",
    "cancelled_at",
    "checked_at",
    "claimed_at",
    "created_at",
    "dismissed_at",
    "due_at",
    "ends_at",
    "expires_at",
    "finished_at",
    "heartbeat_at",
    "last_checked_at",
    "last_run_at",
    "last_scheduled_at",
    "last_seen_at",
    "last_success_at",
    "latest_health_check_at",
    "latest_worker_seen_at",
    "lease_expires_at",
    "locked_until",
    "next_attempt_at",
    "next_run_at",
    "occurred_at",
    "read_at",
    "snoozed_until",
    "source_updated_at",
    "started_at",
    "starts_at",
    "updated_at",
    "window_started_at"
)

function Assert-Rfc3339Timestamp {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Value,

        [Parameter(Mandatory = $true)]
        [string]$Location
    )

    if ($Value -isnot [string] -or
        [string]$Value -notmatch '^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$') {
        throw "$Location must be RFC3339 with Z or an explicit numeric offset; got '$Value'."
    }

    $parsed = [DateTimeOffset]::MinValue
    $parsedOk = [DateTimeOffset]::TryParse(
        [string]$Value,
        [Globalization.CultureInfo]::InvariantCulture,
        [Globalization.DateTimeStyles]::None,
        [ref]$parsed
    )
    if (-not $parsedOk) {
        throw "$Location is not a valid RFC3339 instant; got '$Value'."
    }
}

function Assert-Rfc3339Properties {
    param(
        [Parameter(Mandatory = $true)]
        [AllowNull()]
        [object]$Value,

        [Parameter(Mandatory = $true)]
        [string]$Location
    )

    if ($null -eq $Value -or $Value -is [string] -or $Value -is [ValueType]) {
        return
    }

    if ($Value -is [System.Collections.IEnumerable] -and
        $Value -isnot [System.Collections.IDictionary] -and
        $Value -isnot [PSCustomObject]) {
        $index = 0
        foreach ($item in $Value) {
            Assert-Rfc3339Properties -Value $item -Location "$Location[$index]"
            $index++
        }
        return
    }

    foreach ($property in $Value.PSObject.Properties) {
        $propertyLocation = "$Location.$($property.Name)"
        if ($script:Rfc3339PropertyNames -contains $property.Name -and $null -ne $property.Value) {
            Assert-Rfc3339Timestamp -Value $property.Value -Location $propertyLocation
        }
        elseif ($null -ne $property.Value -and $property.Value -isnot [string]) {
            Assert-Rfc3339Properties -Value $property.Value -Location $propertyLocation
        }
    }
}

function Assert-BooleanProperty {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Object,

        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string]$Location
    )

    Assert-HasProperty -Object $Object -Name $Name -Location $Location
    $value = $Object.PSObject.Properties[$Name].Value
    if ($value -isnot [System.Boolean]) {
        throw "$Location.$Name must be a JSON boolean."
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
        WebSession      = $script:PortalSession
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
        Headers    = $response.Headers
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
    $script:PortalSession = New-Object Microsoft.PowerShell.Commands.WebRequestSession

    $livez = Invoke-PortalRequest -Method GET -Path "/livez" -ExpectedStatus 200
    Assert-HealthyResponse -Data $livez.Json.data -Path "/livez"
    Write-Host "[PASS] GET /livez: API process is alive."

    $readyz = Invoke-PortalRequest -Method GET -Path "/readyz" -ExpectedStatus 200
    Assert-HealthyResponse -Data $readyz.Json.data -Path "/readyz" -IncludeDatabase
    Write-Host "[PASS] GET /readyz: API and PostgreSQL are ready."

    $authConfig = Invoke-PortalRequest -Method GET -Path "/auth/config" -ExpectedStatus 200
    $authConfigData = $authConfig.Json.data
    Assert-BooleanProperty -Object $authConfigData -Name "password_login_enabled" -Location "/auth/config data"
    Assert-BooleanProperty -Object $authConfigData -Name "entra_login_enabled" -Location "/auth/config data"
    if (-not [bool]$authConfigData.password_login_enabled) {
        throw "/auth/config reports password_login_enabled=false; this smoke script intentionally tests the local recovery path."
    }
    if ($ExpectedEntraState -eq "Enabled" -and -not [bool]$authConfigData.entra_login_enabled) {
        throw "/auth/config reports entra_login_enabled=false; expected enabled."
    }
    if ($ExpectedEntraState -eq "Disabled" -and [bool]$authConfigData.entra_login_enabled) {
        throw "/auth/config reports entra_login_enabled=true; expected disabled."
    }
    Write-Host "[PASS] GET /auth/config: login-method flags match the local smoke requirements."

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
    Assert-HasProperty -Object $loginData -Name "expires_at" -Location "/login data"
    Assert-HasProperty -Object $loginData -Name "auth_method" -Location "/login data"
    if ($null -ne $loginData.PSObject.Properties["token"] -or
        $null -ne $loginData.PSObject.Properties["token_type"]) {
        throw "/login returned raw session credentials; the browser login contract must be cookie-only."
    }
    Assert-NonEmptyString -Value $loginData.expires_at -Location "/login data.expires_at"
    Assert-Rfc3339Properties -Value $loginData -Location "/login data"
    if ([string]$loginData.auth_method -ne "password") {
        throw "/login data.auth_method was '$($loginData.auth_method)'; expected 'password'."
    }

    $requiresProductionCookie = $parsedBaseUrl.Scheme -eq "https"
    $expectedCookieName = if ($requiresProductionCookie) { "__Host-idp_session" } else { "idp_session" }
    $sessionCookies = $script:PortalSession.Cookies.GetCookies([System.Uri]$script:PortalBaseUrl)
    $sessionCookie = $sessionCookies | Where-Object {
        $_.Name -eq $expectedCookieName
    } | Select-Object -First 1
    if ($null -eq $sessionCookie -or [string]::IsNullOrWhiteSpace([string]$sessionCookie.Value)) {
        throw "POST /login did not establish the expected '$expectedCookieName' cookie in the HTTP cookie jar."
    }
    if (-not $sessionCookie.HttpOnly) {
        throw "POST /login session cookie is missing HttpOnly."
    }
    $setCookieText = (@($login.Headers["Set-Cookie"]) -join "`n")
    $cookiePattern = "(?m)(?:^|,\s*)$([regex]::Escape($expectedCookieName))=[^,`r`n]*"
    $cookieMatch = [regex]::Match($setCookieText, $cookiePattern)
    if (-not $cookieMatch.Success) {
        throw "POST /login response is missing the expected '$expectedCookieName' Set-Cookie header."
    }
    $sessionSetCookie = $cookieMatch.Value.TrimStart(", ")
    foreach ($attribute in @("HttpOnly", "Path=/", "SameSite=Lax")) {
        if ($sessionSetCookie -notmatch "(?i)(?:^|;\s*)$([regex]::Escape($attribute))(?:;|$)") {
            throw "POST /login session cookie is missing $attribute."
        }
    }
    if ($sessionSetCookie -match "(?i)(?:^|;\s*)Domain=") {
        throw "POST /login session cookie must be host-only and must not contain Domain."
    }
    if ($requiresProductionCookie -and $sessionSetCookie -notmatch "(?i)(?:^|;\s*)Secure(?:;|$)") {
        throw "Production POST /login session cookie is missing Secure."
    }
    $writeHeaders = @{ "X-IDP-CSRF" = "1" }

    try {
        Write-Host "[PASS] POST /login: HttpOnly cookie session established without exposing a raw token."

        $me = Invoke-PortalRequest -Method GET -Path "/me" -ExpectedStatus 200
        $meData = $me.Json.data
        foreach ($property in @("id", "username", "expires_at", "auth_method", "roles", "capabilities", "maintainer_access")) {
            Assert-HasProperty -Object $meData -Name $property -Location "/me data"
        }
        Assert-NonEmptyString -Value $meData.username -Location "/me data.username"
        Assert-NonEmptyString -Value $meData.expires_at -Location "/me data.expires_at"
        Assert-Rfc3339Properties -Value $meData -Location "/me data"
        if ([string]$meData.auth_method -ne "password") {
            throw "/me data.auth_method was '$($meData.auth_method)'; expected 'password' for the local-login smoke path."
        }
        foreach ($capability in @("manage_connectors", "view_audit", "manage_maintainers", "view_user_directory")) {
            Assert-HasProperty -Object $meData.capabilities -Name $capability -Location "/me data.capabilities"
        }
        Write-Host "[PASS] GET /me: password identity, roles, and capabilities are present."

        if (-not $SkipOverview) {
            $overview = Invoke-PortalRequest -Method GET -Path "/me/overview" -ExpectedStatus 200
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
            Assert-Rfc3339Properties -Value $overviewData -Location "/me/overview data"
            Write-Host "[PASS] GET /me/overview: operational overview and RFC3339 datetime contracts are complete."
        }
        else {
            Write-Host "[SKIP] GET /me/overview was skipped by request."
        }

        $myWork = Invoke-PortalRequest -Method GET -Path "/me/work-cards?sort=attention&page=1&page_size=25" -ExpectedStatus 200
        $myWorkData = $myWork.Json.data
        foreach ($property in @("items", "total", "page", "page_size", "facets")) {
            Assert-HasProperty -Object $myWorkData -Name $property -Location "/me/work-cards data"
        }
        foreach ($property in @("statuses", "projects", "work_item_types", "sources")) {
            Assert-HasProperty -Object $myWorkData.facets -Name $property -Location "/me/work-cards data.facets"
        }
        if ([int64]$myWorkData.page -ne 1 -or [int64]$myWorkData.page_size -ne 25) {
            throw "/me/work-cards did not honor the requested page contract."
        }
        Assert-Rfc3339Properties -Value $myWorkData -Location "/me/work-cards data"
        Write-Host "[PASS] GET /me/work-cards: assigned-work pagination, facets, and RFC3339 contracts are complete."
    }
    finally {
        if ($null -ne $sessionCookie) {
            $logout = Invoke-PortalRequest -Method POST -Path "/logout" -ExpectedStatus 204 -Headers $writeHeaders -NoJson
            $remainingSessionCookie = $script:PortalSession.Cookies.GetCookies([System.Uri]$script:PortalBaseUrl) |
                Where-Object { $_.Name -eq "__Host-idp_session" -or $_.Name -eq "idp_session" } |
                Select-Object -First 1
            if ($null -ne $remainingSessionCookie) {
                throw "POST /logout returned success but the portal session remains in the HTTP cookie jar."
            }
            Write-Host "[PASS] POST /logout: smoke-test session was removed."
            $writeHeaders.Clear()
            $sessionCookie = $null
        }
    }

    Write-Host "Production smoke test passed."
}
catch {
    Write-Error "Production smoke test failed: $($_.Exception.Message)"
    exit 1
}
