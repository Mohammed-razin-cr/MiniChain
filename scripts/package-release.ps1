param([string]$OutputDirectory)

$ErrorActionPreference = 'Stop'
$projectRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $projectRoot 'release' }
$outputRoot = [System.IO.Path]::GetFullPath($OutputDirectory)
[System.IO.Directory]::CreateDirectory($outputRoot) | Out-Null
$archive = Join-Path $outputRoot 'minichain-0.1.0-source.zip'
$staging = Join-Path ([System.IO.Path]::GetTempPath()) ("minichain-package-" + [guid]::NewGuid().ToString('N'))
$stageProject = Join-Path $staging 'minichain-0.1.0'

$excludedDirectories = @('target', 'node_modules', 'dist', '.vinext', '.next', '.git', 'data', 'release', 'coverage', '.wrangler')
$excludedExtensions = @('.redb', '.key', '.log', '.tmp')

try {
    [System.IO.Directory]::CreateDirectory($stageProject) | Out-Null
    Get-ChildItem -LiteralPath $projectRoot -Recurse -File | ForEach-Object {
        $relative = [System.IO.Path]::GetRelativePath($projectRoot, $_.FullName)
        $parts = $relative -split '[\\/]'
        if ($parts | Where-Object { $excludedDirectories -contains $_ }) { return }
        if ($excludedExtensions -contains $_.Extension.ToLowerInvariant()) { return }
        if ($_.Name -eq '.env') { return }
        $destination = Join-Path $stageProject $relative
        [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($destination)) | Out-Null
        [System.IO.File]::Copy($_.FullName, $destination, $true)
    }
    if ([System.IO.File]::Exists($archive)) { [System.IO.File]::Delete($archive) }
    Compress-Archive -Path $stageProject -DestinationPath $archive -CompressionLevel Optimal
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $forbidden = @($zip.Entries | Where-Object { $_.FullName -match '(^|/)(target|node_modules|data|dist|\.vinext|\.git)/' -or $_.FullName -match '\.(redb|key|log)$' })
        if ($forbidden.Count -gt 0) { throw "Package contains forbidden generated files: $($forbidden.FullName -join ', ')" }
        $required = @('README.md', 'LICENSE', 'Cargo.toml', 'src/main.rs', 'frontend/package.json', '.github/workflows/ci.yml')
        foreach ($name in $required) {
            if (-not ($zip.Entries.FullName -contains "minichain-0.1.0/$name")) { throw "Package is missing $name" }
        }
    } finally {
        $zip.Dispose()
    }
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
    Write-Output ([pscustomobject]@{ Path = $archive; SizeBytes = (Get-Item $archive).Length; Sha256 = $hash })
} finally {
    if ([System.IO.Directory]::Exists($staging)) { [System.IO.Directory]::Delete($staging, $true) }
}
