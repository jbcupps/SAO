param(
    [switch]$StartCompose,
    [string]$BaseUrl = "http://localhost:3100",
    [string]$PostgresPassword = "local-dev-only-change-me",
    [string]$JwtSecret = "local-dev-only-change-me",
    [string]$VaultPassphrase = "local-dev-only-change-me",
    [string]$AdminUsername = "local-admin",
    # Bundle / LLM-provider checks. Set OllamaBaseUrl to your reachable Ollama; leave blank to skip.
    [string]$OllamaBaseUrl = "",
    [string]$OllamaModel = "llama3.2",
    # Pass -SkipBundle to avoid attempting bundle download when no installer is staged.
    [switch]$SkipBundle
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $repoRoot
try {
    $env:POSTGRES_PASSWORD = $PostgresPassword
    $env:SAO_JWT_SECRET = $JwtSecret
    $env:SAO_LOCAL_BOOTSTRAP = "true"
    $env:SAO_LOCAL_VAULT_PASSPHRASE = $VaultPassphrase
    $env:SAO_LOCAL_ADMIN_USERNAME = $AdminUsername

    if ($StartCompose) {
        docker compose -f docker/docker-compose.yml up -d --build
    }

    $deadline = (Get-Date).AddSeconds(90)
    do {
        try {
            Invoke-RestMethod -Uri "$BaseUrl/api/health" -Method Get | Out-Null
            break
        }
        catch {
            if ((Get-Date) -gt $deadline) {
                throw "SAO health endpoint did not become ready at $BaseUrl."
            }
            Start-Sleep -Seconds 3
        }
    } while ($true)

    docker compose -f docker/docker-compose.yml run --rm `
        -e SAO_LOCAL_BOOTSTRAP=true `
        -e SAO_LOCAL_VAULT_PASSPHRASE="$VaultPassphrase" `
        -e SAO_LOCAL_ADMIN_USERNAME="$AdminUsername" `
        sao sao-server bootstrap-local | Write-Host

    $token = docker compose -f docker/docker-compose.yml run --rm `
        -e SAO_LOCAL_BOOTSTRAP=true `
        -e SAO_LOCAL_ADMIN_USERNAME="$AdminUsername" `
        sao sao-server mint-dev-token | Select-Object -Last 1

    if (-not $token) {
        throw "Failed to mint local SAO bearer token."
    }

    $setup = Invoke-RestMethod -Uri "$BaseUrl/api/setup/status" -Method Get
    if ($setup.bootstrap_mode -ne "operational") {
        throw "Expected setup bootstrap_mode=operational; got $($setup.bootstrap_mode)."
    }

    $headers = @{ Authorization = "Bearer $token" }
    $policy = Invoke-RestMethod -Uri "$BaseUrl/api/orion/policy" -Method Get -Headers $headers
    if ($policy.source -ne "sao") {
        throw "Expected SAO policy source; got $($policy.source)."
    }

    $eventId = [guid]::NewGuid()
    $orionId = [guid]::NewGuid()
    $correlationId = [guid]::NewGuid()
    $body = @{
        orionId = $orionId
        events = @(
            @{
                id = $eventId
                enqueuedAt = (Get-Date).ToUniversalTime().ToString("o")
                attempts = 1
                event = @{
                    auditAction = @{
                        action = "local MVP smoke test"
                        correlationId = $correlationId
                    }
                }
            }
        )
    } | ConvertTo-Json -Depth 8

    $egress = Invoke-RestMethod -Uri "$BaseUrl/api/orion/egress" -Method Post -Headers $headers -ContentType "application/json" -Body $body
    if ($egress.accepted -lt 1 -and $egress.duplicate -lt 1) {
        throw "Expected Orion egress ack or duplicate; got $($egress | ConvertTo-Json -Depth 8)."
    }

    # ---- Optional: configure Ollama provider, create an agent, exercise the bundle endpoint.

    if ($OllamaBaseUrl) {
        $providerBody = @{
            enabled         = $true
            base_url        = $OllamaBaseUrl
            approved_models = @($OllamaModel)
            default_model   = $OllamaModel
        } | ConvertTo-Json -Depth 8

        Invoke-RestMethod -Uri "$BaseUrl/api/admin/llm-providers/ollama" `
            -Method Put -Headers $headers -ContentType "application/json" -Body $providerBody | Out-Null
        Write-Host "Configured Ollama provider at $OllamaBaseUrl with model $OllamaModel."

        $agentBody = @{
            name               = "smoke-orion-$([guid]::NewGuid().ToString().Substring(0,8))"
            default_provider   = "ollama"
            default_id_model   = $OllamaModel
            default_ego_model  = $OllamaModel
        } | ConvertTo-Json -Depth 8
        Invoke-RestMethod -Uri "$BaseUrl/api/agents" -Method Post -Headers $headers `
            -ContentType "application/json" -Body $agentBody | Out-Null
        $agents = (Invoke-RestMethod -Uri "$BaseUrl/api/agents" -Method Get -Headers $headers).agents
        $smokeAgent = $agents | Where-Object { $_.name -like 'smoke-orion-*' } | Select-Object -Last 1
        if (-not $smokeAgent) { throw "Smoke agent was not created." }
        Write-Host "Created smoke agent $($smokeAgent.id)."

        if (-not $SkipBundle) {
            $bundleResponse = Invoke-WebRequest -Uri "$BaseUrl/api/agents/$($smokeAgent.id)/bundle" `
                -Headers $headers -Method Get -ErrorAction SilentlyContinue
            if ($bundleResponse.StatusCode -eq 200) {
                $tempZip = Join-Path ([System.IO.Path]::GetTempPath()) "smoke-bundle.zip"
                [System.IO.File]::WriteAllBytes($tempZip, $bundleResponse.Content)
                Write-Host "Bundle downloaded ($($bundleResponse.Content.Length) bytes) to $tempZip."
            }
            else {
                Write-Warning "Bundle download returned $($bundleResponse.StatusCode). If 503, set SAO_ORION_INSTALLER_DIR + SAO_ORION_INSTALLER_FILENAME and re-run docker compose up."
            }
        }
    }
    else {
        Write-Host "Skipped LLM provider + bundle checks (OllamaBaseUrl not set)."
    }

    Write-Host "SAO local MVP smoke test passed."
    Write-Host "SAO_BASE_URL=$BaseUrl"
    Write-Host "SAO_DEV_BEARER_TOKEN=$token"
}
finally {
    Pop-Location
}
