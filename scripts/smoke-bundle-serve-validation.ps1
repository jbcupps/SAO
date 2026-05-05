# Live test: with an installer_sources row that points at a non-MSI URL, the
# bundle download endpoint must refuse rather than producing a broken ZIP.
#
# Assumes Compose is up with SAO_LOCAL_BOOTSTRAP=true already executed and a
# bad installer_sources row already registered (the user's original failure
# state). Creates a fresh smoke agent so we don't depend on prior state.

[CmdletBinding()]
param(
    [string]$BaseUrl = 'http://localhost:3100',
    [string]$AdminUsername = 'local-admin'
)

$ErrorActionPreference = 'Stop'

function Get-DevToken {
    docker compose -f docker/docker-compose.yml --env-file .env run --rm `
        -e SAO_LOCAL_BOOTSTRAP=true `
        -e SAO_LOCAL_ADMIN_USERNAME="$AdminUsername" `
        sao sao-server mint-dev-token | Select-Object -Last 1
}

function New-CsrfSession {
    $session = [Microsoft.PowerShell.Commands.WebRequestSession]::new()
    $null = Invoke-WebRequest -Uri "$BaseUrl/api/health" -WebSession $session -UseBasicParsing
    $cookies = $session.Cookies.GetCookies($BaseUrl)
    $csrf = $cookies | Where-Object { $_.Name -eq 'sao_csrf' } | Select-Object -ExpandProperty Value
    if (-not $csrf) { throw 'Server did not issue an sao_csrf cookie.' }
    return [pscustomobject]@{ Session = $session; CsrfToken = $csrf }
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

Push-Location (Resolve-Path (Join-Path $PSScriptRoot '..'))
try {
    Wait-ForHealth
    $token = Get-DevToken
    if (-not $token) { throw 'Failed to mint dev bearer token.' }
    Write-Host 'Got dev bearer token.'

    $csrf = New-CsrfSession

    # Configure Ollama so we can create an agent (any provider works; bundle path is provider-agnostic).
    $providerHeaders = @{
        'Authorization' = "Bearer $token"
        'X-CSRF-Token'  = $csrf.CsrfToken
    }
    $providerBody = @{
        enabled         = $true
        base_url        = 'http://host.docker.internal:11434'
        approved_models = @('llama3.2')
        default_model   = 'llama3.2'
    } | ConvertTo-Json
    Invoke-RestMethod -Uri "$BaseUrl/api/admin/llm-providers/ollama" -Method Put `
        -Headers $providerHeaders -ContentType 'application/json' -Body $providerBody `
        -WebSession $csrf.Session | Out-Null

    $agentName = "smoke-bundle-$([guid]::NewGuid().ToString().Substring(0,8))"
    $agentBody = @{
        name              = $agentName
        default_provider  = 'ollama'
        default_id_model  = 'llama3.2'
        default_ego_model = 'llama3.2'
    } | ConvertTo-Json
    $agent = Invoke-RestMethod -Uri "$BaseUrl/api/agents" -Method Post `
        -Headers $providerHeaders -ContentType 'application/json' -Body $agentBody `
        -WebSession $csrf.Session
    $agentId = $agent.agent_id
    if (-not $agentId) { throw "Agent create did not return an id: $($agent | ConvertTo-Json -Depth 5)" }
    Write-Host "Created smoke agent $agentName ($agentId)."

    try {
        $bundleResp = Invoke-WebRequest -Uri "$BaseUrl/api/agents/$agentId/bundle" `
            -Headers $providerHeaders -WebSession $csrf.Session -UseBasicParsing
        # If we reach here, the bundle download succeeded — this is the BUG state, not the fix state.
        $bytes = $bundleResp.Content
        $magicHex = ([System.BitConverter]::ToString($bytes[0..3])).Replace('-', '').ToLower()
        throw "Bundle download succeeded with $($bytes.Length) bytes (magic=$magicHex). Expected 503."
    }
    catch {
        $resp = $_.Exception.Response
        if (-not $resp) { throw }
        $status = [int]$resp.StatusCode
        $rawBody = $null
        if ($_.ErrorDetails -and $_.ErrorDetails.Message) { $rawBody = $_.ErrorDetails.Message }
        $parsed = $null
        if ($rawBody) {
            try { $parsed = $rawBody | ConvertFrom-Json } catch { $parsed = $rawBody }
        }
        if ($status -ne 503) {
            throw "Expected 503 from bundle endpoint; got $status $($parsed | ConvertTo-Json -Compress -Depth 5)"
        }
        if ($parsed.error -notmatch 'installer' -and $parsed.error -notmatch 'msi' -and $parsed.error -notmatch 'Installer') {
            throw "Expected installer-related error message; got $($parsed | ConvertTo-Json -Compress -Depth 5)"
        }
        Write-Host "OK: bundle endpoint returned 503 with a clear installer error:"
        Write-Host "    $($parsed.error)"
    }

    Write-Host ''
    Write-Host 'Bundle-serve format validation smoke test passed.'
}
finally {
    Pop-Location
}
