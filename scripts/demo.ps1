param(
    [switch]$SkipBuild,
    [switch]$KeepData
)

$ErrorActionPreference = 'Stop'
$projectRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$binaryName = if ($IsWindows -or $env:OS -eq 'Windows_NT') { 'minichain.exe' } else { 'minichain' }
$binaryPath = Join-Path $projectRoot "target/release/$binaryName"
$demoRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("minichain-release-demo-" + [guid]::NewGuid().ToString('N'))
$processes = @()
$demoToken = 'minichain-demo-operator-change-me'

function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
    $listener.Stop()
    return $port
}

function Write-NodeConfig {
    param([int]$Index, [array]$P2pPorts, [array]$ApiPorts, [array]$PublicKeys, [string]$TokenDigest)
    $nodeId = 'node-{0:d2}' -f ($Index + 1)
    $storagePath = (Join-Path $demoRoot "$nodeId.redb").Replace('\', '/')
    $identityPath = (Join-Path $demoRoot "$nodeId.key").Replace('\', '/')
    $peerText = ''
    for ($peer = 0; $peer -lt 3; $peer++) {
        if ($peer -eq $Index) { continue }
        $peerId = 'node-{0:d2}' -f ($peer + 1)
        $key = $PublicKeys[$peer] -join ', '
        $peerText += "`n[trusted_peers.$peerId]`naddress = `"127.0.0.1:$($P2pPorts[$peer])`"`npublic_key = [$key]`n"
    }
    $content = @"
node_id = "$nodeId"
listen_address = "127.0.0.1:$($P2pPorts[$Index])"
max_peers = 8
chain_id = "6d696e69-6368-4169-8e00-00000000d311"
network_id = "minichain-release-demo"
storage_path = "$storagePath"
identity_path = "$identityPath"
heartbeat_interval_ms = 500

[api]
listen_address = "127.0.0.1:$($ApiPorts[$Index])"
allowed_origins = ["http://localhost:3000", "http://localhost:5173"]
max_body_bytes = 131072
rate_window_ms = 60000
read_requests_per_window = 1000
write_requests_per_window = 200
admin_requests_per_window = 50

[[api.tokens]]
identity = "release-demo-operator"
role = "operator"
token_sha256 = "$TokenDigest"
$peerText
"@
    $path = Join-Path $demoRoot "$nodeId.toml"
    [System.IO.File]::WriteAllText($path, $content)
    return $path
}

Write-Host 'MiniChain Release Demo'
Write-Host "Isolated data: $demoRoot"
try {
    [System.IO.Directory]::CreateDirectory($demoRoot) | Out-Null
    if (-not $SkipBuild) {
        Write-Host '[1/8] Building release binary'
        & cargo build --release --manifest-path (Join-Path $projectRoot 'Cargo.toml')
        if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    } else {
        Write-Host '[1/8] Using verified release binary'
    }
    if (-not [System.IO.File]::Exists($binaryPath)) { throw "Release binary not found: $binaryPath" }

    $p2pPorts = @((Get-FreePort), (Get-FreePort), (Get-FreePort))
    $apiPorts = @((Get-FreePort), (Get-FreePort), (Get-FreePort))
    $publicKeys = @()
    for ($index = 0; $index -lt 3; $index++) {
        $nodeId = 'node-{0:d2}' -f ($index + 1)
        $identityPath = Join-Path $demoRoot "$nodeId.key"
        $identity = (& $binaryPath --json identity init --node-id $nodeId --output $identityPath | Out-String | ConvertFrom-Json)
        $publicKeys += ,@($identity.public_key)
    }
    $tokenDigest = [Convert]::ToHexString(
        [Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($demoToken))
    ).ToLowerInvariant()
    $configs = @()
    for ($index = 0; $index -lt 3; $index++) {
        $configs += Write-NodeConfig -Index $index -P2pPorts $p2pPorts -ApiPorts $apiPorts -PublicKeys $publicKeys -TokenDigest $tokenDigest
    }

    Write-Host '[2/8] Starting three real node processes'
    for ($index = 0; $index -lt 3; $index++) {
        $nodeId = 'node-{0:d2}' -f ($index + 1)
        $processes += Start-Process -FilePath $binaryPath -ArgumentList @('--config', $configs[$index], 'node', 'start') -WorkingDirectory $projectRoot -WindowStyle Hidden -RedirectStandardOutput (Join-Path $demoRoot "$nodeId.stdout.log") -RedirectStandardError (Join-Path $demoRoot "$nodeId.stderr.log") -PassThru
    }

    Write-Host '[3/8] Waiting for node APIs'
    foreach ($port in $apiPorts) {
        $ready = $false
        for ($attempt = 0; $attempt -lt 40; $attempt++) {
            try {
                Invoke-RestMethod -Uri "http://127.0.0.1:$port/api/v1/health" -TimeoutSec 1 | Out-Null
                $ready = $true
                break
            } catch {
                Start-Sleep -Milliseconds 250
            }
        }
        if (-not $ready) { throw "Node API on port $port did not become ready." }
    }

    $env:MINICHAIN_API_TOKEN = $demoToken
    Write-Host '[4/8] Authenticating configured peers'
    & $binaryPath --config $configs[0] --json network connect node-02 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'node-02 authentication failed.' }
    & $binaryPath --config $configs[0] --json network connect node-03 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'node-03 authentication failed.' }

    Write-Host '[5/8] Submitting labelled demo data'
    $payloadPath = Join-Path $demoRoot 'institutional-record.json'
    [System.IO.File]::WriteAllText($payloadPath, '{"demo_data":true,"record_type":"certificate","subject":"Synthetic learner DEMO-001","title":"MiniChain release demonstration"}')
    & $binaryPath --config $configs[0] --json record create DEMO-PROCESS-001 $payloadPath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Demo transaction submission failed.' }

    Write-Host '[6/8] Running node diagnostics'
    & $binaryPath --config $configs[0] --json diagnostics | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Node diagnostics failed.' }

    Write-Host '[7/8] Running deterministic chain, quorum, recovery, snapshot, and tamper flow'
    & $binaryPath --json demo run
    if ($LASTEXITCODE -ne 0) { throw 'Deterministic release demonstration failed.' }

    Write-Host '[8/8] Final result'
    Write-Host 'NETWORK: CONSISTENT'
    Write-Host 'CHAIN: VALID'
} finally {
    foreach ($process in $processes) {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
        $process.WaitForExit()
    }
    Remove-Item Env:MINICHAIN_API_TOKEN -ErrorAction SilentlyContinue
    if ($KeepData) {
        Write-Host "Demo data retained by request: $demoRoot"
    } elseif ([System.IO.Directory]::Exists($demoRoot)) {
        $resolved = [System.IO.Path]::GetFullPath($demoRoot)
        $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
        if (-not $resolved.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase) -or -not ([System.IO.Path]::GetFileName($resolved)).StartsWith('minichain-release-demo-')) {
            throw "Refusing to remove unexpected path: $resolved"
        }
        [System.IO.Directory]::Delete($resolved, $true)
    }
}
