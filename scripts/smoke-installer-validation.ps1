# Live smoke for the installer-source format validator.
#
# Verifies that:
#   1. /api/admin/installer-sources/probe rejects a non-MSI URL with a clear
#      format_ok=false + format_hint signal.
#   2. /api/admin/installer-sources refuses to persist a non-MSI URL even when
#      the caller has a matching sha256 (the original failure mode the user hit).
#
# Assumes Compose is up with SAO_LOCAL_BOOTSTRAP=true already executed and an
# admin user provisioned. Intentionally uses a real public URL that is known
# to return a non-MSI: GitHub's auto-generated source-tarball alias.

[CmdletBinding()]
param(
    [string]$BaseUrl = 'http://localhost:3100',
    [string]$AdminUsername = 'local-admin',
    # GitHub auto-generated source archive — NOT an MSI. This is the exact URL
    # convention that bit the user.
    [string]$BadUrl =
      'https://github.com/jbcupps/OrionII/archive/refs/tags/dev0.1.zip'
)

$ErrorActionPreference = 'Stop'

function Get-DevToken {
    docker compose -f docker/docker-compose.yml --env-file .env run --rm `
        -e SAO_LOCAL_BOOTSTRAP=true `
        -e SAO_LOCAL_ADMIN_USERNAME="$AdminUsername" `
        sao sao-server mint-dev-token | Select-Object -Last 1
}

function Wait-ForHealth {
    for ($i = 0; $i -lt 60; $i++) {
        try {
            Invoke-RestMethod -Uri "$BaseUrl/api/health" -TimeoutSec 3 | Out-Null
            return
        }
        catch { Start-Sleep -Seconds 2 }
    }
    throw "SAO did not become healthy at $BaseUrl"
}

function New-CsrfSession {
    $session = [Microsoft.PowerShell.Commands.WebRequestSession]::new()
    $null = Invoke-WebRequest -Uri "$BaseUrl/api/health" -WebSession $session -UseBasicParsing
    $cookies = $session.Cookies.GetCookies($BaseUrl)
    $csrf = $cookies | Where-Object { $_.Name -eq 'sao_csrf' } | Select-Object -ExpandProperty Value
    if (-not $csrf) { throw 'Server did not issue an sao_csrf cookie.' }
    return [pscustomobject]@{ Session = $session; CsrfToken = $csrf }
}

function Invoke-AdminApi {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string]$Path,
        [Parameter(Mandatory)] $Body,
        [Parameter(Mandatory)] [string]$Token,
        [Parameter(Mandatory)] $Csrf
    )
    $headers = @{
        'Authorization' = "Bearer $Token"
        'X-CSRF-Token'  = $Csrf.CsrfToken
    }
    $bodyJson = ConvertTo-Json -InputObject $Body -Depth 5
    try {
        $response = Invoke-WebRequest -Uri "$BaseUrl$Path" -Method Post -Headers $headers `
            -ContentType 'application/json' -Body $bodyJson `
            -WebSession $Csrf.Session -UseBasicParsing
        $parsed = if ($response.Content) { $response.Content | ConvertFrom-Json } else { $null }
        return [pscustomobject]@{ Status = [int]$response.StatusCode; Body = $parsed }
    }
    catch {
        $resp = $_.Exception.Response
        if (-not $resp) { throw }
        $rawBody = $null
        if ($_.ErrorDetails -and $_.ErrorDetails.Message) {
            $rawBody = $_.ErrorDetails.Message
        }
        $parsed = $null
        if ($rawBody) {
            try { $parsed = $rawBody | ConvertFrom-Json } catch { $parsed = $rawBody }
        }
        return [pscustomobject]@{ Status = [int]$resp.StatusCode; Body = $parsed }
    }
}

Push-Location (Resolve-Path (Join-Path $PSScriptRoot '..'))
try {
    Wait-ForHealth
    $token = Get-DevToken
    if (-not $token) { throw 'Failed to mint dev bearer token.' }
    Write-Host 'Got dev bearer token.'

    $csrf = New-CsrfSession
    Write-Host 'Acquired sao_csrf cookie.'

    # --- 1. Probe rejects a non-MSI URL with format_ok=false ---------------------
    $probe = Invoke-AdminApi -Path '/api/admin/installer-sources/probe' -Token $token -Csrf $csrf -Body @{
        url  = $BadUrl
        kind = 'orion-msi'
    }
    if ($probe.Status -ne 200) {
        throw "Expected 200 from probe; got $($probe.Status) $($probe.Body | ConvertTo-Json -Compress -Depth 5)"
    }
    if ($probe.Body.format_ok -ne $false) {
        throw "Expected probe format_ok=false; got $($probe.Body | ConvertTo-Json -Compress -Depth 5)"
    }
    if (-not $probe.Body.format_hint -or $probe.Body.format_hint -notmatch 'ZIP') {
        throw "Expected ZIP hint; got format_hint=$($probe.Body.format_hint)"
    }
    Write-Host "OK: probe of GitHub source-tarball returned format_ok=false (hint: $($probe.Body.format_hint))."

    # --- 2. Create refuses to persist a non-MSI URL even with a matching sha ----
    $sha = $probe.Body.sha256
    $create = Invoke-AdminApi -Path '/api/admin/installer-sources' -Token $token -Csrf $csrf -Body @{
        kind            = 'orion-msi'
        url             = $BadUrl
        filename        = 'OrionII_0.1.0_x64_en-US.msi'
        version         = 'invalid'
        expected_sha256 = $sha
        is_default      = $false
    }
    if ($create.Status -ne 400 -or $create.Body.code -ne 'invalid_installer_artifact') {
        throw "Expected 400 invalid_installer_artifact; got $($create.Status) $($create.Body | ConvertTo-Json -Compress -Depth 5)"
    }
    Write-Host "OK: registration of non-MSI URL refused with code=invalid_installer_artifact."
    Write-Host "    looks_like: $($create.Body.looks_like)"

    Write-Host ''
    Write-Host 'Installer-source format validation smoke test passed.'
}
finally {
    Pop-Location
}
