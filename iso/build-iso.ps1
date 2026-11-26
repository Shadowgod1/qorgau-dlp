# Build Qorgau DLP v2.0 Bootable Offline Debian 12 Appliance ISO on Windows via Docker
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

Write-Host "=== 1. Ensuring Linux qorgau-server & React dashboard images are up to date ==="
docker compose build server nginx

Write-Host "=== 2. Staging payload (Linux qorgau-server + dashboard/dist + Windows qorgau-agent.exe) ==="
$payload = Join-Path $root "iso\payload"
if (Test-Path $payload) { Remove-Item $payload -Recurse -Force }
New-Item -ItemType Directory -Force -Path "$payload\dashboard" | Out-Null

# Extract compiled Linux binary from Docker image
$cid = docker create qorgau_dlp-server:latest
try {
    docker cp "${cid}:/usr/local/bin/qorgau-server" "$payload\qorgau-server"
} finally {
    docker rm $cid | Out-Null
}

# Extract compiled React dashboard dist from nginx image
$ncid = docker create qorgau_dlp-nginx:latest
try {
    docker cp "${ncid}:/usr/share/nginx/html/." "$payload\dashboard\"
} finally {
    docker rm $ncid | Out-Null
}

# Copy Windows Rust agent if built
$agentExe = Join-Path $root "target\release\qorgau-agent.exe"
if (Test-Path $agentExe) {
    Copy-Item $agentExe "$payload\qorgau-agent.exe" -Force
}

Write-Host "=== 3. Building ISO builder image ==="
docker build -f iso/Dockerfile.builder -t qorgau-iso-builder .

Write-Host "=== 4. Producing bootable offline ISO -> dist\qorgau-dlp-2.0-amd64.iso ==="
New-Item -ItemType Directory -Force -Path "$root\iso\cache", "$root\dist" | Out-Null
docker run --rm `
    -v "${root}\iso:/work/iso:ro" `
    -v "${payload}:/work/payload:ro" `
    -v "${root}\iso\cache:/work/cache" `
    -v "${root}\dist:/work/dist" `
    qorgau-iso-builder

Write-Host "=== DONE ==="
Get-Item "$root\dist\qorgau-dlp-2.0-amd64.iso" | Select-Object FullName, @{N='SizeMB';E={[math]::Round($_.Length/1MB, 2)}}
