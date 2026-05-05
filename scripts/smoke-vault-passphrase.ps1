# Live smoke test for /api/vault/configure + /api/vault/rotate-passphrase.
# Assumes Compose is up with SAO_LOCAL_BOOTSTRAP=true already executed and
# the vault initialized with $InitialPassphrase. Idempotent: rotates twice so
# the runbook's expected passphrase remains in place at the end.

[CmdletBinding()]
param(
    [string]$BaseUrl = 'http://localhost:3100',
    [string]$InitialPassphrase = 'local-dev-only-change-me',
    [string]$IntermediatePassphrase = 'rotated-dev-passphrase-#1',
    [string]$AdminUsername = 'local-admin'
)

$ErrorActionPreference = 'Stop'

function Get-DevToken {
    $token = docker compose -f docker/docker-compose.yml --env-file .env run --rm `
        -e SAO_LOCAL_BOOTSTRAP=true `
        -e SAO_LOCAL_ADMIN_USERNAME="$AdminUsername" `
        sao sao-server mint-dev-token | Select-Object -Last 1
    if (-not $token) { throw 'Failed to mint dev token.' }
    return $token
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
    # First request seeds the sao_csrf cookie for subsequent mutating calls.
    $null = Invoke-WebRequest -Uri "$BaseUrl/api/health" -WebSession $session -UseBasicParsing
    $cookies = $session.Cookies.GetCookies($BaseUrl)
    $csrf = $cookies | Where-Object { $_.Name -eq 'sao_csrf' } | Select-Object -ExpandProperty Value
    if (-not $csrf) { throw 'Server did not issue an sao_csrf cookie.' }
    return [pscustomobject]@{ Session = $session; CsrfToken = $csrf }
}

function Invoke-VaultEndpoint {
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
    Write-Host "Got dev token; running vault lifecycle smoke checks."

    $csrf = New-CsrfSession
    Write-Host "Acquired sao_csrf cookie; proceeding with mutating calls."

    # 1. configure must refuse on an already-initialized vault.
    $r1 = Invoke-VaultEndpoint -Path '/api/vault/configure' -Token $token -Csrf $csrf -Body @{
        passphrase              = 'should-not-apply-here-1234'
        passphrase_confirmation = 'should-not-apply-here-1234'
    }
    if ($r1.Status -ne 409 -or $r1.Body.code -ne 'vault_already_initialized') {
        throw "Expected 409 vault_already_initialized; got $($r1.Status) $($r1.Body | ConvertTo-Json -Compress -Depth 5)"
    }
    Write-Host "OK: configure on initialized vault returned 409 (vault_already_initialized)."

    # 2. rotate must reject the wrong current passphrase.
    $r2 = Invoke-VaultEndpoint -Path '/api/vault/rotate-passphrase' -Token $token -Csrf $csrf -Body @{
        current_passphrase          = 'definitely-wrong-passphrase-1234'
        new_passphrase              = 'irrelevant-since-401-1234'
        new_passphrase_confirmation = 'irrelevant-since-401-1234'
    }
    if ($r2.Status -ne 401 -or $r2.Body.code -ne 'invalid_credentials') {
        throw "Expected 401 invalid_credentials; got $($r2.Status) $($r2.Body | ConvertTo-Json -Compress -Depth 5)"
    }
    Write-Host "OK: rotate with wrong current returned 401 (invalid_credentials)."

    # 3. rotate must reject same-as-current.
    $r3 = Invoke-VaultEndpoint -Path '/api/vault/rotate-passphrase' -Token $token -Csrf $csrf -Body @{
        current_passphrase          = $InitialPassphrase
        new_passphrase              = $InitialPassphrase
        new_passphrase_confirmation = $InitialPassphrase
    }
    if ($r3.Status -ne 400 -or $r3.Body.code -ne 'same_as_current') {
        throw "Expected 400 same_as_current; got $($r3.Status) $($r3.Body | ConvertTo-Json -Compress -Depth 5)"
    }
    Write-Host "OK: rotate with same-as-current returned 400 (same_as_current)."

    # 4. rotate to intermediate, verify the new passphrase unseals.
    $r4 = Invoke-VaultEndpoint -Path '/api/vault/rotate-passphrase' -Token $token -Csrf $csrf -Body @{
        current_passphrase          = $InitialPassphrase
        new_passphrase              = $IntermediatePassphrase
        new_passphrase_confirmation = $IntermediatePassphrase
    }
    if ($r4.Status -ne 200 -or $r4.Body.status -ne 'unsealed') {
        throw "Expected 200 unsealed; got $($r4.Status) $($r4.Body | ConvertTo-Json -Compress -Depth 5)"
    }
    Write-Host "OK: rotate to intermediate succeeded."

    # 5. seal vault, then unseal with the intermediate passphrase to prove the new envelope works.
    $sealHeaders = @{
        'Authorization' = "Bearer $token"
        'X-CSRF-Token'  = $csrf.CsrfToken
    }
    Invoke-RestMethod -Method Post -Uri "$BaseUrl/api/vault/seal" -Headers $sealHeaders `
        -WebSession $csrf.Session | Out-Null
    $unsealBody = @{ passphrase = $IntermediatePassphrase } | ConvertTo-Json
    $unseal = Invoke-RestMethod -Method Post -Uri "$BaseUrl/api/vault/unseal" `
        -Headers $sealHeaders -ContentType 'application/json' -Body $unsealBody `
        -WebSession $csrf.Session
    if ($unseal.status -ne 'unsealed') {
        throw "Unseal with intermediate passphrase failed: $($unseal | ConvertTo-Json -Compress -Depth 5)"
    }
    Write-Host "OK: unseal with intermediate passphrase succeeded."

    # 6. rotate back to the initial passphrase so the runbook's expected state is preserved.
    $r6 = Invoke-VaultEndpoint -Path '/api/vault/rotate-passphrase' -Token $token -Csrf $csrf -Body @{
        current_passphrase          = $IntermediatePassphrase
        new_passphrase              = $InitialPassphrase
        new_passphrase_confirmation = $InitialPassphrase
    }
    if ($r6.Status -ne 200) {
        throw "Failed to rotate back to initial: $($r6.Status) $($r6.Body | ConvertTo-Json -Compress -Depth 5)"
    }
    Write-Host "OK: rotated back to initial passphrase; vault state preserved for follow-up runs."

    Write-Host ''
    Write-Host 'Vault passphrase lifecycle smoke test passed.'
}
finally {
    Pop-Location
}
