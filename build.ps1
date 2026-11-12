param([switch]$Shot, [switch]$Zip, [string]$Output = '')
# Compila y publica BuzzPrint autocontenido (incluye .NET 10, no requiere instalar nada).
# -Shot ejecuta además la verificación visual (biblioteca de prueba, nunca las capturas del usuario).
# -Zip arma además artifacts\BuzzPrint-<versión>-win-x64.zip para distribuir: sin FFmpeg (la app lo ofrece
#      descargar la primera vez que alguien graba), así el zip queda chico y todo bajo licencia MIT.
$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($Output)) { $Output = Join-Path $PSScriptRoot 'dist\BuzzPrint' }
# SDK: $env:BUZZPRINT_DOTNET, else the author's local SDK, else 'dotnet' on PATH (.NET 10 SDK).
$dotnet = if ($env:BUZZPRINT_DOTNET) { $env:BUZZPRINT_DOTNET } else { 'C:\chris\Coordinacion\DBMON_SQL\.tools\dotnet\dotnet.exe' }
if (-not (Test-Path -LiteralPath $dotnet)) { $dotnet = 'dotnet' }
$project = Join-Path $PSScriptRoot 'src\BuzzPrint\BuzzPrint.csproj'
$running = @(Get-Process BuzzPrint -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.StartsWith($Output, [StringComparison]::OrdinalIgnoreCase) })
if ($running.Count) { throw "BuzzPrint está abierto desde $Output. Cerralo (bandeja → Salir) antes de recompilar." }
& $dotnet publish $project -c Release -r win-x64 --self-contained true -o $Output --nologo
if ($LASTEXITCODE -ne 0) { throw 'La publicación falló.' }
# Grabación de video y audio: si ffmpeg está instalado en esta PC, va junto a la app (tools\ffmpeg.exe).
# Si no, la app ofrece descargarlo la primera vez que se graba.
$ffmpeg = (Get-Command ffmpeg.exe -ErrorAction SilentlyContinue).Source
if ($ffmpeg) {
    $real = (Get-Item -LiteralPath $ffmpeg).Target
    if ($real -and (Test-Path -LiteralPath $real)) { $ffmpeg = $real }
    New-Item -ItemType Directory -Force (Join-Path $Output 'tools') | Out-Null
    $dest = Join-Path $Output 'tools\ffmpeg.exe'
    if (-not (Test-Path -LiteralPath $dest) -or (Get-Item -LiteralPath $dest).Length -ne (Get-Item -LiteralPath $ffmpeg).Length) { Copy-Item -LiteralPath $ffmpeg $dest -Force }
    Write-Output "ffmpeg: $ffmpeg"
} else { Write-Output 'ffmpeg.exe no está instalado: BuzzPrint lo va a ofrecer descargar la primera vez que grabes.' }
Write-Output "BuzzPrint publicado en $Output"
if ($Zip) {
    $version = ([xml](Get-Content -LiteralPath $project -Raw)).Project.PropertyGroup.Version | Where-Object { $_ } | Select-Object -First 1
    $stage = Join-Path $PSScriptRoot 'artifacts\zip\BuzzPrint'
    if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
    New-Item -ItemType Directory -Force $stage | Out-Null
    Copy-Item -Path (Join-Path $Output '*') -Destination $stage -Recurse -Force
    $tools = Join-Path $stage 'tools'
    if (Test-Path -LiteralPath $tools) { Remove-Item -LiteralPath $tools -Recurse -Force }
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'LICENSE') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'THIRD-PARTY-NOTICES.md') -Destination $stage
    $zipPath = Join-Path $PSScriptRoot "artifacts\BuzzPrint-$version-win-x64.zip"
    if (Test-Path -LiteralPath $zipPath) { Remove-Item -LiteralPath $zipPath -Force }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::CreateFromDirectory($stage, $zipPath, [IO.Compression.CompressionLevel]::Optimal, $true)
    Remove-Item -LiteralPath (Split-Path $stage) -Recurse -Force
    Write-Output ("Zip: $zipPath ({0:N0} MB)" -f ((Get-Item -LiteralPath $zipPath).Length / 1MB))
}
if ($Shot) {
    $dir = Join-Path $PSScriptRoot 'artifacts\shots'
    $p = Start-Process -FilePath (Join-Path $Output 'BuzzPrint.exe') -ArgumentList ('--shot "' + $dir + '"') -PassThru
    if (-not $p.WaitForExit(120000)) { $p.Kill(); throw 'La verificación visual no terminó en 120 s.' }
    Get-Content (Join-Path $dir 'shots.log')
}
