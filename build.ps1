param([switch]$Shot, [switch]$Zip, [switch]$Msix, [switch]$TestSign, [string]$Output = '')
# Compila y publica BuzzPrint autocontenido (incluye .NET 10, no requiere instalar nada).
# -Shot ejecuta además la verificación visual (biblioteca de prueba, nunca las capturas del usuario).
# -Zip arma además artifacts\BuzzPrint-<versión>-win-x64.zip para distribuir: sin FFmpeg (la app lo ofrece
#      descargar la primera vez que alguien graba), así el zip queda chico y todo bajo licencia MIT.
# -Msix arma además artifacts\store\BuzzPrint_<versión>.0_x64.msix para Microsoft Store: compilación STORE
#      (inicio con Windows por StartupTask, sin descargas de código), FFmpeg incluido, logos y resources.pri.
#      La identidad sale de packaging\store.json (valores de Partner Center). El .msix va sin firmar: la Store lo firma.
# -TestSign (con -Msix) deja también un ..._test.msix firmado con un certificado autofirmado del usuario actual
#      (CN = Publisher de store.json) y exporta el .cer, para instalarlo en esta PC antes de subirlo.
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
if ($Msix) {
    $version = ([xml](Get-Content -LiteralPath $project -Raw)).Project.PropertyGroup.Version | Where-Object { $_ } | Select-Object -First 1
    $msixVersion = "$version.0"                        # la Store exige 4 partes con la última en 0
    $store = Join-Path $PSScriptRoot 'artifacts\store'
    $cache = Join-Path $PSScriptRoot 'artifacts\cache'
    $stage = Join-Path $store 'app'
    New-Item -ItemType Directory -Force $store, $cache | Out-Null
    $identity = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'packaging\store.json') -Raw | ConvertFrom-Json

    # 1. Compilación STORE.
    if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
    & $dotnet publish $project -c Release -r win-x64 --self-contained true -o $stage --nologo -p:Store=true
    if ($LASTEXITCODE -ne 0) { throw 'La publicación STORE falló.' }

    # 2. Herramientas del Windows SDK (makeappx, makepri, signtool) desde el paquete NuGet oficial de Microsoft.
    $btVersion = '10.0.28000.2957'
    $bt = Join-Path $cache "sdk-buildtools-$btVersion"
    if (-not (Test-Path -LiteralPath (Join-Path $bt 'bin'))) {
        $nupkg = Join-Path $cache "sdk-buildtools-$btVersion.zip"
        Invoke-WebRequest -UseBasicParsing "https://api.nuget.org/v3-flatcontainer/microsoft.windows.sdk.buildtools/$btVersion/microsoft.windows.sdk.buildtools.$btVersion.nupkg" -OutFile $nupkg
        Expand-Archive -LiteralPath $nupkg -DestinationPath $bt -Force
        Remove-Item -LiteralPath $nupkg
    }
    $sdkBin = Get-ChildItem -LiteralPath (Join-Path $bt 'bin') -Directory | Sort-Object Name | Select-Object -Last 1 | ForEach-Object { Join-Path $_.FullName 'x64' }
    $makeappx = Join-Path $sdkBin 'makeappx.exe'; $makepri = Join-Path $sdkBin 'makepri.exe'; $signtool = Join-Path $sdkBin 'signtool.exe'

    # 3. FFmpeg dentro del paquete (la versión Store no descarga código): build "essentials" de gyan.dev, SHA-256 verificado.
    $ffUrl = 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip'
    $ffZip = Join-Path $cache 'ffmpeg-release-essentials.zip'
    $expected = ((Invoke-WebRequest -UseBasicParsing "$ffUrl.sha256").Content.Trim() -split '\s')[0].ToLowerInvariant()
    $have = if (Test-Path -LiteralPath $ffZip) { (Get-FileHash -LiteralPath $ffZip -Algorithm SHA256).Hash.ToLowerInvariant() } else { '' }
    if ($have -ne $expected) {
        Invoke-WebRequest -UseBasicParsing $ffUrl -OutFile $ffZip
        if ((Get-FileHash -LiteralPath $ffZip -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { Remove-Item -LiteralPath $ffZip; throw 'FFmpeg: el SHA-256 no coincide con el publicado.' }
    }
    $tools = Join-Path $stage 'tools'
    New-Item -ItemType Directory -Force $tools | Out-Null
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $z = [IO.Compression.ZipFile]::OpenRead($ffZip)
    try {
        $exe = $z.Entries | Where-Object { $_.FullName -like '*/bin/ffmpeg.exe' } | Select-Object -First 1
        $lic = $z.Entries | Where-Object { $_.FullName -match '^[^/]+/LICENSE(\.txt)?$' } | Select-Object -First 1
        $ffBuild = ($exe.FullName -split '/')[0]
        [IO.Compression.ZipFileExtensions]::ExtractToFile($exe, (Join-Path $tools 'ffmpeg.exe'), $true)
        if ($lic) { [IO.Compression.ZipFileExtensions]::ExtractToFile($lic, (Join-Path $tools 'FFmpeg-LICENSE.txt'), $true) }
    } finally { $z.Dispose() }
    Set-Content -LiteralPath (Join-Path $tools 'FFmpeg-SOURCE.txt') -Encoding UTF8 -Value @"
BuzzPrint runs FFmpeg as a separate program (tools\ffmpeg.exe) to record video and audio.
Build: $ffBuild, from https://www.gyan.dev/ffmpeg/builds/ (unmodified).
FFmpeg is licensed under the GNU GPL version 3 (see FFmpeg-LICENSE.txt).
Source code: https://ffmpeg.org/download.html#get-sources and https://github.com/FFmpeg/FFmpeg (release tag matching the build above);
the build configuration and the sources of the bundled libraries are listed at https://www.gyan.dev/ffmpeg/builds/.
"@
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'LICENSE') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'THIRD-PARTY-NOTICES.md') -Destination $stage

    # 4. Logos (desde el ícono de la app) y manifiesto.
    Add-Type -AssemblyName System.Drawing
    $images = Join-Path $stage 'Images'
    New-Item -ItemType Directory -Force $images | Out-Null
    $src = [Drawing.Image]::FromFile((Join-Path $PSScriptRoot 'src\BuzzPrint\Assets\buzzprint.png'))
    function Save-Logo([string]$name, [int]$w, [int]$h, [double]$fill) {
        $bmp = New-Object Drawing.Bitmap $w, $h
        $g = [Drawing.Graphics]::FromImage($bmp)
        $g.InterpolationMode = 'HighQualityBicubic'; $g.SmoothingMode = 'HighQuality'; $g.PixelOffsetMode = 'HighQuality'
        $s = [int]([Math]::Min($w, $h) * $fill)
        $g.DrawImage($src, [int](($w - $s) / 2), [int](($h - $s) / 2), $s, $s)
        $g.Dispose(); $bmp.Save((Join-Path $images $name), [Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
    }
    try {
        foreach ($sc in 100, 200) {
            $f = $sc / 100
            Save-Logo "Square44x44Logo.scale-$sc.png" (44 * $f) (44 * $f) 1
            Save-Logo "Square150x150Logo.scale-$sc.png" (150 * $f) (150 * $f) 0.6
            Save-Logo "Wide310x150Logo.scale-$sc.png" (310 * $f) (150 * $f) 0.6
            Save-Logo "StoreLogo.scale-$sc.png" (50 * $f) (50 * $f) 1
        }
        foreach ($t in 16, 24, 32, 48, 256) {
            Save-Logo "Square44x44Logo.targetsize-$t.png" $t $t 1
            Save-Logo "Square44x44Logo.targetsize-${t}_altform-unplated.png" $t $t 1
        }
    } finally { $src.Dispose() }
    $manifest = (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'packaging\AppxManifest.xml') -Raw).
        Replace('{{IdentityName}}', $identity.IdentityName).Replace('{{Publisher}}', [Security.SecurityElement]::Escape($identity.Publisher)).
        Replace('{{PublisherDisplayName}}', [Security.SecurityElement]::Escape($identity.PublisherDisplayName)).Replace('{{Version}}', $msixVersion)
    [IO.File]::WriteAllText((Join-Path $stage 'AppxManifest.xml'), $manifest, (New-Object Text.UTF8Encoding $false))

    # 5. resources.pri (para que Windows elija la escala y el ícono sin placa de la barra de tareas) y empaquetado.
    $priConfig = Join-Path $store 'priconfig.xml'
    & $makepri createconfig /cf $priConfig /dq en-US /pv 10.0.0 /o | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'makepri createconfig falló.' }
    & $makepri new /pr $stage /cf $priConfig /mn (Join-Path $stage 'AppxManifest.xml') /of (Join-Path $stage 'resources.pri') /o | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'makepri new falló.' }
    $msixPath = Join-Path $store "BuzzPrint_${msixVersion}_x64.msix"
    & $makeappx pack /d $stage /p $msixPath /o /nv | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'makeappx pack falló.' }
    Write-Output ("MSIX (para subir a la Store): $msixPath ({0:N0} MB)" -f ((Get-Item -LiteralPath $msixPath).Length / 1MB))

    if ($TestSign) {
        $cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq $identity.Publisher -and $_.NotAfter -gt (Get-Date) -and $_.HasPrivateKey } | Select-Object -First 1
        if (-not $cert) {
            $cert = New-SelfSignedCertificate -Type Custom -Subject $identity.Publisher -KeyUsage DigitalSignature -FriendlyName 'BuzzPrint MSIX test' `
                -CertStoreLocation Cert:\CurrentUser\My -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
        }
        $testPath = Join-Path $store "BuzzPrint_${msixVersion}_x64_test.msix"
        Copy-Item -LiteralPath $msixPath $testPath -Force
        & $signtool sign /q /fd SHA256 /sha1 $cert.Thumbprint /s My $testPath
        if ($LASTEXITCODE -ne 0) { throw 'signtool falló.' }
        $cer = Join-Path $store 'BuzzPrint-test.cer'
        Export-Certificate -Cert $cert -FilePath $cer | Out-Null
        Write-Output "MSIX de prueba firmado: $testPath"
        Write-Output "Para instalarlo, una vez y como administrador: Import-Certificate -FilePath '$cer' -CertStoreLocation Cert:\LocalMachine\TrustedPeople"
    }
}
if ($Shot) {
    $dir = Join-Path $PSScriptRoot 'artifacts\shots'
    $p = Start-Process -FilePath (Join-Path $Output 'BuzzPrint.exe') -ArgumentList ('--shot "' + $dir + '"') -PassThru
    if (-not $p.WaitForExit(120000)) { $p.Kill(); throw 'La verificación visual no terminó en 120 s.' }
    Get-Content (Join-Path $dir 'shots.log')
}
