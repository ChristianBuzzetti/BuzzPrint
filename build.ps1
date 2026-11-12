param([switch]$Shot, [string]$Output = '')
# Compila y publica BuzzPrint autocontenido (incluye .NET 10, no requiere instalar nada), igual que DBMON SQL.
# -Shot ejecuta además la verificación visual (biblioteca de prueba, nunca las capturas del usuario).
$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($Output)) { $Output = Join-Path $PSScriptRoot 'dist\BuzzPrint' }
# SDK: $env:BUZZPRINT_DOTNET, else the author's local SDK, else 'dotnet' on PATH (.NET 10 SDK).
$dotnet = if ($env:BUZZPRINT_DOTNET) { $env:BUZZPRINT_DOTNET } else { 'C:\chris\Coordinacion\DBMON_SQL\.tools\dotnet\dotnet.exe' }
if (-not (Test-Path -LiteralPath $dotnet)) { $dotnet = 'dotnet' }
$running = @(Get-Process BuzzPrint -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.StartsWith($Output, [StringComparison]::OrdinalIgnoreCase) })
if ($running.Count) { throw "BuzzPrint está abierto desde $Output. Cerralo (bandeja → Salir) antes de recompilar." }
& $dotnet publish (Join-Path $PSScriptRoot 'src\BuzzPrint\BuzzPrint.csproj') -c Release -r win-x64 --self-contained true -o $Output --nologo
if ($LASTEXITCODE -ne 0) { throw 'La publicación falló.' }
# Grabación de video y audio: ffmpeg va junto a la app (tools\ffmpeg.exe) para que funcione en cualquier PC.
$ffmpeg = (Get-Command ffmpeg.exe -ErrorAction SilentlyContinue).Source
if ($ffmpeg) {
    $real = (Get-Item -LiteralPath $ffmpeg).Target
    if ($real -and (Test-Path -LiteralPath $real)) { $ffmpeg = $real }
    New-Item -ItemType Directory -Force (Join-Path $Output 'tools') | Out-Null
    $dest = Join-Path $Output 'tools\ffmpeg.exe'
    if (-not (Test-Path -LiteralPath $dest) -or (Get-Item -LiteralPath $dest).Length -ne (Get-Item -LiteralPath $ffmpeg).Length) { Copy-Item -LiteralPath $ffmpeg $dest -Force }
    Write-Output "ffmpeg: $ffmpeg"
} else { Write-Warning 'ffmpeg.exe no está instalado: la grabación de video y audio no va a funcionar. Instalalo con: winget install Gyan.FFmpeg' }
Write-Output "BuzzPrint publicado en $Output"
if ($Shot) {
    $dir = Join-Path $PSScriptRoot 'artifacts\shots'
    $p = Start-Process -FilePath (Join-Path $Output 'BuzzPrint.exe') -ArgumentList ('--shot "' + $dir + '"') -PassThru
    if (-not $p.WaitForExit(120000)) { $p.Kill(); throw 'La verificación visual no terminó en 120 s.' }
    Get-Content (Join-Path $dir 'shots.log')
}
