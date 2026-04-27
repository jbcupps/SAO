param(
    [switch]$StartCompose,
    [string]$BaseUrl = "http://localhost:3100",
    [string]$PostgresPassword = "local-dev-only-change-me",
    [string]$JwtSecret = "local-dev-only-change-me",
    [string]$VaultPassphrase = "local-dev-only-change-me",
    [string]$AdminUsername = "local-admin",
    [string]$Provider = "",
    [string]$IdModel = "",
    [string]$EgoModel = "",
    [string]$Prompt = "Reply with a short confirmation that the SAO seam is alive.",
    [string]$SystemPrompt = "You are Orion's Ego layer participating in a verification run.",
    [string]$AgentNamePrefix = "verify-orion",
    [string]$OutputDir = "",
    [switch]$PrepareOnly,
    [switch]$RunRegressionChecks
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Write-Section {
    param([string]$Message)
    Write-Host ""
    Write-Host "== $Message ==" -ForegroundColor Cyan
}

function Assert-Condition {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Wait-ForHealth {
    param(
        [string]$Url,
        [int]$TimeoutSeconds = 90
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        try {
            Invoke-RestMethod -Uri "$Url/api/health" -Method Get | Out-Null
            return
        }
        catch {
            if ((Get-Date) -gt $deadline) {
                throw "SAO health endpoint did not become ready at $Url."
            }
            Start-Sleep -Seconds 3
        }
    } while ($true)
}

function Invoke-JsonRequest {
    param(
        [string]$Method,
        [string]$Uri,
        [hashtable]$Headers = @{},
        [object]$Body = $null,
        [string]$ContentType = "application/json"
    )

    $request = @{
        Uri                = $Uri
        Method             = $Method
        Headers            = $Headers
        SkipHttpErrorCheck = $true
    }

    if ($null -ne $Body) {
        $request.Body = if ($Body -is [string]) { $Body } else { $Body | ConvertTo-Json -Depth 12 }
        $request.ContentType = $ContentType
    }

    $response = Invoke-WebRequest @request
    $rawBody = $response.Content
    $jsonBody = $null

    if ($rawBody) {
        try {
            $jsonBody = $rawBody | ConvertFrom-Json -AsHashtable
        }
        catch {
            $jsonBody = $null
        }
    }

    [pscustomobject]@{
        StatusCode = [int]$response.StatusCode
        RawBody    = $rawBody
        JsonBody   = $jsonBody
    }
}

function Download-File {
    param(
        [string]$Uri,
        [hashtable]$Headers,
        [string]$Destination
    )

    $response = Invoke-WebRequest -Uri $Uri -Headers $Headers -OutFile $Destination -SkipHttpErrorCheck -PassThru
    $statusCode = if ($null -ne $response -and $null -ne $response.PSObject.Properties["StatusCode"]) {
        [int]$response.StatusCode
    }
    elseif (Test-Path $Destination) {
        200
    }
    else {
        0
    }

    [pscustomobject]@{
        StatusCode = $statusCode
        Path       = $Destination
        SizeBytes  = (Get-Item $Destination).Length
    }
}

function Decode-Base64Url {
    param([string]$Value)

    $padded = $Value.Replace('-', '+').Replace('_', '/')
    switch ($padded.Length % 4) {
        2 { $padded += '==' }
        3 { $padded += '=' }
        0 { }
        default { throw "Invalid base64url segment length." }
    }

    [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($padded))
}

function Read-JwtPayload {
    param([string]$Jwt)

    $parts = $Jwt.Split('.')
    Assert-Condition ($parts.Length -eq 3) "Entity JWT did not have three segments."
    (Decode-Base64Url $parts[1]) | ConvertFrom-Json -AsHashtable
}

function Read-ZipEntryText {
    param(
        [string]$ZipPath,
        [string]$EntryName
    )

    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem

    $zip = [System.IO.Compression.ZipFile]::OpenRead($ZipPath)
    try {
        $entry = $zip.Entries | Where-Object { $_.FullName -eq $EntryName } | Select-Object -First 1
        Assert-Condition ($null -ne $entry) "Bundle is missing $EntryName."

        $stream = $entry.Open()
        $reader = New-Object System.IO.StreamReader($stream)
        try {
            return $reader.ReadToEnd()
        }
        finally {
            $reader.Dispose()
            $stream.Dispose()
        }
    }
    finally {
        $zip.Dispose()
    }
}

function Get-GitCommit {
    param([string]$Path)

    if (-not (Test-Path $Path)) {
        return $null
    }

    $commit = git -C $Path rev-parse HEAD 2>$null
    if ($LASTEXITCODE -ne 0) {
        return $null
    }

    $commit.Trim()
}

function Wait-ForAgentEvents {
    param(
        [string]$AgentId,
        [hashtable]$Headers,
        [string[]]$RequiredTypes,
        [int]$TimeoutSeconds = 10
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        $response = Invoke-JsonRequest -Method Get -Uri "$BaseUrl/api/agents/$AgentId/events?limit=50&offset=0" -Headers $Headers
        if ($response.StatusCode -eq 200) {
            $events = @($response.JsonBody.events)
            $types = @($events | ForEach-Object { $_.event_type })
            $missing = @($RequiredTypes | Where-Object { $_ -notin $types })
            if ($missing.Count -eq 0) {
                return $events
            }
        }

        if ((Get-Date) -gt $deadline) {
            throw "Timed out waiting for agent events: $($RequiredTypes -join ', ')."
        }

        Start-Sleep -Seconds 1
    } while ($true)
}

function Wait-ForAuditEntries {
    param(
        [string]$AgentId,
        [hashtable]$Headers,
        [string[]]$RequiredActions,
        [int]$TimeoutSeconds = 10
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        $response = Invoke-JsonRequest -Method Get -Uri "$BaseUrl/api/admin/audit?limit=500&offset=0" -Headers $Headers
        if ($response.StatusCode -eq 200) {
            $entries = @($response.JsonBody.audit_log | Where-Object { $_.agent_id -eq $AgentId })
            $actions = @($entries | ForEach-Object { $_.action })
            $missing = @($RequiredActions | Where-Object { $_ -notin $actions })
            if ($missing.Count -eq 0) {
                return $entries
            }
        }

        if ((Get-Date) -gt $deadline) {
            throw "Timed out waiting for audit actions: $($RequiredActions -join ', ')."
        }

        Start-Sleep -Seconds 1
    } while ($true)
}

function Get-AuditActionCount {
    param(
        [object[]]$Entries,
        [string]$Action
    )

    @($Entries | Where-Object { $_.action -eq $Action }).Count
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$orionRepo = "C:\Repo\OrionII"
$artifactRoot = if ($OutputDir) {
    $OutputDir
}
else {
    Join-Path ([System.IO.Path]::GetTempPath()) "sao-orion-verification"
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$runDir = Join-Path $artifactRoot "verify-orion-topic-shift-$timestamp"
$reportPath = Join-Path $runDir "report.json"
$bundlePath = Join-Path $runDir "bundle.zip"

New-Item -ItemType Directory -Force -Path $runDir | Out-Null

$report = [ordered]@{
    started_at   = (Get-Date).ToUniversalTime().ToString("o")
    base_url     = $BaseUrl
    mode         = if ($PrepareOnly) { "prepare-only" } else { "contract-exercise" }
    run_regression_checks = [bool]$RunRegressionChecks
    commits      = [ordered]@{
        sao     = Get-GitCommit $repoRoot
        orionii = Get-GitCommit $orionRepo
    }
    installer    = $null
    provider     = $null
    models       = $null
    agent        = $null
    bundle       = $null
    happy_path   = $null
    regressions  = $null
    result       = "in_progress"
}

Push-Location $repoRoot
try {
    $env:POSTGRES_PASSWORD = $PostgresPassword
    $env:SAO_JWT_SECRET = $JwtSecret
    $env:SAO_LOCAL_BOOTSTRAP = "true"
    $env:SAO_LOCAL_VAULT_PASSPHRASE = $VaultPassphrase
    $env:SAO_LOCAL_ADMIN_USERNAME = $AdminUsername

    if ($StartCompose) {
        Write-Section "Starting Compose"
        docker compose -f docker/docker-compose.yml up -d --build
    }

    Write-Section "Preflight"
    Wait-ForHealth -Url $BaseUrl

    docker compose -f docker/docker-compose.yml run --rm `
        -e SAO_LOCAL_BOOTSTRAP=true `
        -e SAO_LOCAL_VAULT_PASSPHRASE="$VaultPassphrase" `
        -e SAO_LOCAL_ADMIN_USERNAME="$AdminUsername" `
        sao sao-server bootstrap-local | Write-Host

    $adminToken = docker compose -f docker/docker-compose.yml run --rm `
        -e SAO_LOCAL_BOOTSTRAP=true `
        -e SAO_LOCAL_ADMIN_USERNAME="$AdminUsername" `
        sao sao-server mint-dev-token | Select-Object -Last 1

    Assert-Condition (-not [string]::IsNullOrWhiteSpace($adminToken)) "Failed to mint local SAO bearer token."

    $csrfToken = "verify-orion-$([guid]::NewGuid().ToString("N"))"
    $adminHeaders = @{
        Authorization  = "Bearer $adminToken"
        Cookie         = "sao_csrf=$csrfToken"
        "X-CSRF-Token" = $csrfToken
    }

    $health = Invoke-JsonRequest -Method Get -Uri "$BaseUrl/api/health"
    $setup = Invoke-JsonRequest -Method Get -Uri "$BaseUrl/api/setup/status"
    $vault = Invoke-JsonRequest -Method Get -Uri "$BaseUrl/api/vault/status"

    Assert-Condition ($health.StatusCode -eq 200) "Expected /api/health to return 200."
    Assert-Condition ($setup.StatusCode -eq 200) "Expected /api/setup/status to return 200."
    Assert-Condition ($setup.JsonBody.bootstrap_mode -eq "operational") "Expected bootstrap_mode=operational."
    Assert-Condition ($vault.StatusCode -eq 200) "Expected /api/vault/status to return 200."
    Assert-Condition ($vault.JsonBody.status -eq "unsealed") "Expected vault to be unsealed."

    $providersResponse = Invoke-JsonRequest -Method Get -Uri "$BaseUrl/api/admin/llm-providers" -Headers $adminHeaders
    Assert-Condition ($providersResponse.StatusCode -eq 200) "Expected /api/admin/llm-providers to return 200."
    $enabledProviders = @($providersResponse.JsonBody.providers | Where-Object { $_.enabled })
    Assert-Condition ($enabledProviders.Count -gt 0) "No enabled LLM providers found in SAO."

    $selectedProvider = $null
    if ($Provider) {
        $selectedProvider = $enabledProviders | Where-Object { $_.provider -eq $Provider } | Select-Object -First 1
        Assert-Condition ($null -ne $selectedProvider) "Configured provider '$Provider' is not enabled in SAO."
    }
    elseif ($enabledProviders.Count -eq 1) {
        $selectedProvider = $enabledProviders[0]
    }
    else {
        throw "Multiple enabled providers were found. Pass -Provider to pin the verification run."
    }

    if (-not $IdModel) {
        $IdModel = $selectedProvider.default_model
    }
    if (-not $EgoModel) {
        $EgoModel = if ($IdModel) { $IdModel } else { $selectedProvider.default_model }
    }

    Assert-Condition (-not [string]::IsNullOrWhiteSpace($IdModel)) "Id model is required for the verification subject."
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($EgoModel)) "Ego model is required for the verification subject."

    $installerResponse = Invoke-JsonRequest -Method Get -Uri "$BaseUrl/api/admin/installer-sources" -Headers $adminHeaders
    Assert-Condition ($installerResponse.StatusCode -eq 200) "Expected /api/admin/installer-sources to return 200."
    $defaultInstaller = @($installerResponse.JsonBody.sources | Where-Object { $_.is_default }) | Select-Object -First 1
    Assert-Condition ($null -ne $defaultInstaller) "No default OrionII installer source is registered in SAO."

    $report.installer = [ordered]@{
        id              = $defaultInstaller.id
        filename        = $defaultInstaller.filename
        version         = $defaultInstaller.version
        expected_sha256 = $defaultInstaller.expected_sha256
        url             = $defaultInstaller.url
    }
    $report.provider = $selectedProvider.provider
    $report.models = [ordered]@{
        id  = $IdModel
        ego = $EgoModel
    }

    Write-Section "Creating Verification Agent"
    $agentName = "$AgentNamePrefix-$([guid]::NewGuid().ToString().Substring(0, 8))"
    $createAgentBody = @{
        name              = $agentName
        default_provider  = $selectedProvider.provider
        default_id_model  = $IdModel
        default_ego_model = $EgoModel
    }

    $createAgent = Invoke-JsonRequest -Method Post -Uri "$BaseUrl/api/agents" -Headers $adminHeaders -Body $createAgentBody
    Assert-Condition ($createAgent.StatusCode -eq 201) "Expected /api/agents to return 201. Got $($createAgent.StatusCode)."

    $agentId = $createAgent.JsonBody.agent_id
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($agentId)) "Agent creation did not return agent_id."

    $report.agent = [ordered]@{
        id   = $agentId
        name = $agentName
    }

    Write-Section "Downloading Bundle"
    $bundleDownload = Download-File -Uri "$BaseUrl/api/agents/$agentId/bundle" -Headers $adminHeaders -Destination $bundlePath
    Assert-Condition ($bundleDownload.StatusCode -eq 200) "Expected /api/agents/$agentId/bundle to return 200. Got $($bundleDownload.StatusCode)."

    $configJson = Read-ZipEntryText -ZipPath $bundlePath -EntryName "config.json"
    $deploymentJson = Read-ZipEntryText -ZipPath $bundlePath -EntryName "deployment.json"
    $installCmd = Read-ZipEntryText -ZipPath $bundlePath -EntryName "Install-OrionII.cmd"
    $installPs1 = Read-ZipEntryText -ZipPath $bundlePath -EntryName "Install-OrionII.ps1"
    $readme = Read-ZipEntryText -ZipPath $bundlePath -EntryName "README-FIRST-RUN.txt"
    $config = $configJson | ConvertFrom-Json -AsHashtable
    $deployment = $deploymentJson | ConvertFrom-Json -AsHashtable

    Assert-Condition ($config.ContainsKey("sao_base_url")) "config.json is missing sao_base_url."
    Assert-Condition ($config.ContainsKey("agent_token")) "config.json is missing agent_token."
    Assert-Condition ($config.ContainsKey("agent_id")) "config.json is missing agent_id."
    Assert-Condition ($config.ContainsKey("bus_transport")) "config.json is missing bus_transport."
    Assert-Condition ($config.agent_id -eq $agentId) "config.json agent_id does not match the created agent."
    Assert-Condition ($config.bus_transport.kind -eq "nats_jetstream") "config.json bus_transport.kind was not nats_jetstream."
    Assert-Condition ($deployment.kind -eq "orionii.sao.deployment") "deployment.json kind did not match the OrionII deployment manifest."
    Assert-Condition ($deployment.downloaded_from -eq $config.sao_base_url) "deployment.json downloaded_from did not match config sao_base_url."
    Assert-Condition ($deployment.bus_transport.kind -eq "nats_jetstream") "deployment.json bus_transport.kind was not nats_jetstream."
    Assert-Condition ($installCmd -match "Install-OrionII.ps1") "Install-OrionII.cmd does not launch the PowerShell helper."
    Assert-Condition ($installPs1 -match "Copy-Item.*configSource.*configTarget") "Install-OrionII.ps1 does not copy config.json into APPDATA."
    Assert-Condition ($installPs1 -match "Copy-Item.*deploymentSource.*deploymentTarget") "Install-OrionII.ps1 does not copy deployment.json into APPDATA."
    Assert-Condition ($installPs1 -match 'Get-ItemProperty \$uninstallRoots') "Install-OrionII.ps1 does not detect existing OrionII MSI installs."
    Assert-Condition ($installPs1 -match '"/x", \$product\.PSChildName') "Install-OrionII.ps1 does not uninstall stale OrionII before update."
    Assert-Condition ($installPs1 -match '"/i", "`"\$msiPath`"", "/passive", "/norestart"') "Install-OrionII.ps1 does not install the bundled MSI after stale-product removal."
    Assert-Condition ($installPs1 -match "Stop-Process -Force") "Install-OrionII.ps1 does not close stale running OrionII before update."
    Assert-Condition ($readme -match "No JSON copy/paste is required") "README-FIRST-RUN.txt no longer promises no JSON copy/paste."
    Assert-Condition ($readme -match "self-enrolls from the sibling config.json") "README-FIRST-RUN.txt does not document MSI self-enrollment."

    $entityToken = $config.agent_token
    $jwtClaims = Read-JwtPayload -Jwt $entityToken

    Assert-Condition ($jwtClaims.sub -eq $agentId) "Entity JWT subject did not match the agent id."
    Assert-Condition ($jwtClaims.entity_kind -eq "orion") "Entity JWT entity_kind was not 'orion'."
    Assert-Condition ($jwtClaims.principal_type -eq "non_human") "Entity JWT principal_type was not 'non_human'."
    Assert-Condition ($jwtClaims.scope -match "orion:policy") "Entity JWT scope is missing orion:policy."
    Assert-Condition ($jwtClaims.scope -match "orion:egress") "Entity JWT scope is missing orion:egress."
    Assert-Condition ($jwtClaims.scope -match "llm:generate") "Entity JWT scope is missing llm:generate."

    $report.bundle = [ordered]@{
        downloaded_at = (Get-Date).ToUniversalTime().ToString("o")
        zip_path      = $bundlePath
        size_bytes    = $bundleDownload.SizeBytes
        install_launcher = [ordered]@{
            cmd_present = $true
            ps1_present = $true
        }
        deployment     = [ordered]@{
            kind            = $deployment.kind
            downloaded_from = $deployment.downloaded_from
            bus_transport   = $deployment.bus_transport
        }
        config        = [ordered]@{
            sao_base_url        = $config.sao_base_url
            agent_id            = $config.agent_id
            client_version_min  = $config.client_version_min
            default_provider    = $config.default_provider
            default_id_model    = $config.default_id_model
            default_ego_model   = $config.default_ego_model
            bus_transport       = $config.bus_transport
            fallback            = $config.fallback
        }
        jwt_claims = $jwtClaims
    }

    if ($PrepareOnly) {
        $report.result = "prepared"
        $report | ConvertTo-Json -Depth 12 | Set-Content -Path $reportPath

        Write-Section "Prepared"
        Write-Host "Verification subject is ready."
        Write-Host "Agent ID: $agentId"
        Write-Host "Bundle:   $bundlePath"
        Write-Host "Report:   $reportPath"
        Write-Host ""
        Write-Host "Next step: launch OrionII with this bundle in the other window, then watch /agents, /agents/$agentId/events, and /api/admin/audit in SAO."
        return
    }

    $entityHeaders = @{ Authorization = "Bearer $entityToken" }
    $orionId = $agentId
    $happyPath = [ordered]@{}

    Write-Section "Exercising Entity Contract"
    $birth = Invoke-JsonRequest -Method Get -Uri "$BaseUrl/api/orion/birth" -Headers $entityHeaders
    Assert-Condition ($birth.StatusCode -eq 200) "Expected /api/orion/birth to return 200. Got $($birth.StatusCode)."
    Assert-Condition ($birth.JsonBody.agent.id -eq $agentId) "Birth response agent id did not match the created agent."
    Assert-Condition ($birth.JsonBody.endpoints.egressUrl -eq "/api/orion/egress") "Birth response egressUrl drifted."
    Assert-Condition ($birth.JsonBody.endpoints.birthUrl -eq "/api/orion/birth") "Birth response birthUrl drifted."
    Assert-Condition ($birth.JsonBody.endpoints.llmUrl -eq "/api/llm/generate") "Birth response llmUrl drifted."

    $happyPath.birth = [ordered]@{
        status_code = $birth.StatusCode
        birthed_at  = $birth.JsonBody.birthedAt
    }

    $llmRequest = @{
        provider    = $selectedProvider.provider
        model       = $EgoModel
        system      = $SystemPrompt
        prompt      = $Prompt
        temperature = 0.2
        role        = "ego"
    }
    $llm = Invoke-JsonRequest -Method Post -Uri "$BaseUrl/api/llm/generate" -Headers $entityHeaders -Body $llmRequest
    Assert-Condition ($llm.StatusCode -eq 200) "Expected /api/llm/generate to return 200. Got $($llm.StatusCode)."
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($llm.JsonBody.text)) "LLM response text was empty."

    $llmLatencyMs = if ($llm.JsonBody.ContainsKey("latencyMs")) {
        $llm.JsonBody.latencyMs
    }
    else {
        $llm.JsonBody.latency_ms
    }

    $happyPath.llm = [ordered]@{
        status_code = $llm.StatusCode
        provider    = $selectedProvider.provider
        model       = $llm.JsonBody.model
        latency_ms  = $llmLatencyMs
    }

    $identityEventId = [guid]::NewGuid()
    $auditEventId = [guid]::NewGuid()
    $correlationId = [guid]::NewGuid()
    $egressRequest = @{
        agentId       = $agentId
        orionId       = $orionId
        clientVersion = $config.client_version_min
        events        = @(
            @{
                id         = $identityEventId
                enqueuedAt = (Get-Date).ToUniversalTime().ToString("o")
                attempts   = 1
                event      = @{
                    identitySync = @{
                        orionId = $orionId
                        version = 1
                    }
                }
            },
            @{
                id         = $auditEventId
                enqueuedAt = (Get-Date).ToUniversalTime().ToString("o")
                attempts   = 1
                event      = @{
                    auditAction = @{
                        action        = "verify-orion-topic-shift"
                        correlationId = $correlationId
                    }
                }
            }
        )
    }
    $egress = Invoke-JsonRequest -Method Post -Uri "$BaseUrl/api/orion/egress" -Headers $entityHeaders -Body $egressRequest
    Assert-Condition ($egress.StatusCode -eq 200) "Expected /api/orion/egress to return 200. Got $($egress.StatusCode)."
    Assert-Condition ($egress.JsonBody.accepted -ge 2) "Expected both egress events to be accepted."

    $events = Wait-ForAgentEvents -AgentId $agentId -Headers $adminHeaders -RequiredTypes @("identitySync", "auditAction")
    $agentStatus = Invoke-JsonRequest -Method Get -Uri "$BaseUrl/api/agents/$agentId" -Headers $adminHeaders
    Assert-Condition ($agentStatus.StatusCode -eq 200) "Expected /api/agents/$agentId to return 200."
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($agentStatus.JsonBody.last_heartbeat)) "Expected last_heartbeat after egress."

    $auditEntries = Wait-ForAuditEntries -AgentId $agentId -Headers $adminHeaders -RequiredActions @(
        "agents.bundle_downloaded",
        "orion.birth",
        "llm.generate",
        "orion.egress"
    )
    $llmFailures = Get-AuditActionCount -Entries $auditEntries -Action "llm.generate.failed"
    Assert-Condition ($llmFailures -eq 0) "Found llm.generate.failed audit rows during the happy path."

    $happyPath.egress = [ordered]@{
        status_code   = $egress.StatusCode
        accepted      = $egress.JsonBody.accepted
        duplicate     = $egress.JsonBody.duplicate
        event_types   = @($events | ForEach-Object { $_.event_type } | Select-Object -Unique)
        last_heartbeat = $agentStatus.JsonBody.last_heartbeat
    }
    $happyPath.audit = [ordered]@{
        bundle_downloaded = Get-AuditActionCount -Entries $auditEntries -Action "agents.bundle_downloaded"
        birth             = Get-AuditActionCount -Entries $auditEntries -Action "orion.birth"
        llm_generate      = Get-AuditActionCount -Entries $auditEntries -Action "llm.generate"
        llm_failed        = $llmFailures
        orion_egress      = Get-AuditActionCount -Entries $auditEntries -Action "orion.egress"
    }

    $report.happy_path = $happyPath

    if ($RunRegressionChecks) {
        Write-Section "Running Regression Checks"
        $regressions = [ordered]@{}

        $restartBirth = Invoke-JsonRequest -Method Get -Uri "$BaseUrl/api/orion/birth" -Headers $entityHeaders
        Assert-Condition ($restartBirth.StatusCode -eq 200) "Expected repeated /api/orion/birth to succeed before token rotation."
        $regressions.restart_continuity = [ordered]@{
            repeated_birth_status = $restartBirth.StatusCode
        }

        $duplicateEgress = Invoke-JsonRequest -Method Post -Uri "$BaseUrl/api/orion/egress" -Headers $entityHeaders -Body @{
            agentId       = $agentId
            orionId       = $orionId
            clientVersion = $config.client_version_min
            events        = @(
                @{
                    id         = $identityEventId
                    enqueuedAt = (Get-Date).ToUniversalTime().ToString("o")
                    attempts   = 2
                    event      = @{
                        identitySync = @{
                            orionId = $orionId
                            version = 1
                        }
                    }
                }
            )
        }
        Assert-Condition ($duplicateEgress.StatusCode -eq 200) "Expected duplicate /api/orion/egress call to return 200."
        Assert-Condition ($duplicateEgress.JsonBody.duplicate -ge 1) "Expected duplicate egress to be acknowledged as duplicate."
        $regressions.idempotency = [ordered]@{
            status_code = $duplicateEgress.StatusCode
            duplicate   = $duplicateEgress.JsonBody.duplicate
        }

        $rotatedBundlePath = Join-Path $runDir "bundle-rotated.zip"
        $rotatedDownload = Download-File -Uri "$BaseUrl/api/agents/$agentId/bundle" -Headers $adminHeaders -Destination $rotatedBundlePath
        Assert-Condition ($rotatedDownload.StatusCode -eq 200) "Expected rotated bundle download to return 200."
        $rotatedConfig = (Read-ZipEntryText -ZipPath $rotatedBundlePath -EntryName "config.json") | ConvertFrom-Json -AsHashtable
        $newEntityToken = $rotatedConfig.agent_token

        $oldBirthAfterRotation = Invoke-JsonRequest -Method Get -Uri "$BaseUrl/api/orion/birth" -Headers @{ Authorization = "Bearer $entityToken" }
        $newBirthAfterRotation = Invoke-JsonRequest -Method Get -Uri "$BaseUrl/api/orion/birth" -Headers @{ Authorization = "Bearer $newEntityToken" }
        Assert-Condition ($oldBirthAfterRotation.StatusCode -eq 401) "Expected the old entity token to fail after bundle rotation."
        Assert-Condition ($newBirthAfterRotation.StatusCode -eq 200) "Expected the new entity token to succeed after bundle rotation."
        $regressions.token_rotation = [ordered]@{
            old_token_birth_status = $oldBirthAfterRotation.StatusCode
            new_token_birth_status = $newBirthAfterRotation.StatusCode
        }

        $deleteAgent = Invoke-JsonRequest -Method Post -Uri "$BaseUrl/api/agents/$agentId/delete" -Headers $adminHeaders
        Assert-Condition ($deleteAgent.StatusCode -eq 200) "Expected agent deletion to return 200."
        $birthAfterDelete = Invoke-JsonRequest -Method Get -Uri "$BaseUrl/api/orion/birth" -Headers @{ Authorization = "Bearer $newEntityToken" }
        Assert-Condition ($birthAfterDelete.StatusCode -eq 401) "Expected entity token to be revoked after agent deletion."
        $regressions.revocation = [ordered]@{
            delete_status      = $deleteAgent.StatusCode
            revoked_birth_code = $birthAfterDelete.StatusCode
        }

        $report.regressions = $regressions
    }

    $report.result = "passed"
    $report.completed_at = (Get-Date).ToUniversalTime().ToString("o")
    $report | ConvertTo-Json -Depth 12 | Set-Content -Path $reportPath

    Write-Section "Verification Passed"
    Write-Host "Agent ID: $agentId"
    Write-Host "Provider: $($selectedProvider.provider)"
    Write-Host "Models:   id=$IdModel ego=$EgoModel"
    Write-Host "Bundle:   $bundlePath"
    Write-Host "Report:   $reportPath"
}
catch {
    $report.result = "failed"
    $report.error = $_.Exception.Message
    $report.failed_at = (Get-Date).ToUniversalTime().ToString("o")
    $report | ConvertTo-Json -Depth 12 | Set-Content -Path $reportPath
    throw
}
finally {
    Pop-Location
}
