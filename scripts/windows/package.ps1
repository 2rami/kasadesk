# KasaDesk MSI 굽기 — dist\KasaDesk-<판>-windows-x86_64.msi 와 .sha256. WiX 3.14 바이너리를 받아 쓴다(설치 불필요).
param([string]$ExpectedVersion = "")
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"$' | Select-Object -First 1).Matches[0].Groups[1].Value
if ($ExpectedVersion -and ($ExpectedVersion.TrimStart('v') -ne $version)) { throw "태그 $ExpectedVersion 와 Cargo.toml 판 $version 이 다르다" }

cargo build --locked --release
if ($LASTEXITCODE -ne 0) { throw "cargo build 실패" }
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "target" }
$binDir = Join-Path $targetDir "release"

$tools = Join-Path $targetDir "tools"
$wix = Join-Path $tools "wix-3.14.1"
if (-not (Test-Path (Join-Path $wix "candle.exe"))) {
    New-Item -ItemType Directory -Force -Path $tools | Out-Null
    $zip = Join-Path $tools "wix314-binaries.zip"
    Invoke-WebRequest -Uri "https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip" -OutFile $zip
    Expand-Archive -LiteralPath $zip -DestinationPath $wix -Force
}
$work = Join-Path $targetDir "wix"
New-Item -ItemType Directory -Force -Path $work, (Join-Path $root "dist") | Out-Null
$obj = Join-Path $work "kasadesk.wixobj"
& (Join-Path $wix "candle.exe") -nologo -arch x64 -ext WixFirewallExtension "-dVersion=$version" "-dBinDir=$binDir" -out $obj wix\kasadesk.wxs
if ($LASTEXITCODE -ne 0) { throw "candle 실패" }
$msi = Join-Path $root "dist\KasaDesk-$version-windows-x86_64.msi"
& (Join-Path $wix "light.exe") -nologo -ext WixFirewallExtension -cultures:ko-KR -sice:ICE61 -out $msi $obj
if ($LASTEXITCODE -ne 0) { throw "light 실패" }
$hash = (Get-FileHash -Algorithm SHA256 $msi).Hash.ToLower()
"$hash  $(Split-Path $msi -Leaf)" | Set-Content -NoNewline -Encoding ascii "$msi.sha256"
Write-Host "built $(Split-Path $msi -Leaf) ($([math]::Round((Get-Item $msi).Length/1MB,1)) MB) sha256 $hash"
