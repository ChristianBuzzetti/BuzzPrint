param([switch]$Zip, [switch]$Msix, [switch]$TestSign)
# Compila BuzzPrint (Rust) en release y, opcionalmente, arma los paquetes para distribuir.
# -Zip  artifacts\BuzzPrint-rs-<versión>-win-x64.zip: el .exe y la carpeta boberto (sin FFmpeg: la app ofrece bajarlo la primera vez
#       que alguien graba), con LICENSE y THIRD-PARTY-NOTICES.md.
# -Msix artifacts\store\BuzzPrint_<versión>.0_x64.msix para Microsoft Store: FFmpeg incluido en tools\ (la versión de
#       la Store no descarga código), logos y resources.pri. Identidad en packaging\store.json. Va sin firmar.
# -TestSign (con -Msix) agrega una copia firmada con un certificado autofirmado del usuario, para probarla en esta PC.
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$env:Path += ";$env:USERPROFILE\.cargo\bin"

$running = @(Get-Process buzzprint -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.StartsWith($root, [StringComparison]::OrdinalIgnoreCase) })
if ($running.Count) { throw 'BuzzPrint está abierto desde esta carpeta. Cerralo (bandeja → Salir) antes de compilar.' }

Push-Location $root
try {
    # cargo escribe su avance en stderr: con Stop, PowerShell 5.1 lo tomaría como error.
    $ErrorActionPreference = 'Continue'
    cargo build --release 2>&1 | ForEach-Object { "$_" }
    $code = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($code -ne 0) { throw 'La compilación falló.' }
} finally { Pop-Location }
$exe = Join-Path $root 'target\release\buzzprint.exe'
$version = (Select-String -LiteralPath (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
Write-Output ("buzzprint.exe {0} ({1:N1} MB)" -f $version, ((Get-Item -LiteralPath $exe).Length / 1MB))

if ($Zip) {
    $stage = Join-Path $root 'artifacts\zip\BuzzPrint'
    if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
    New-Item -ItemType Directory -Force $stage | Out-Null
    Copy-Item -LiteralPath $exe, (Join-Path $root 'LICENSE'), (Join-Path $root 'THIRD-PARTY-NOTICES.md') -Destination $stage
    # Boberto: el motor web va junto al .exe (se muestra con WebView2 solo mientras se ve).
    Copy-Item -LiteralPath (Join-Path $root 'assets\boberto') -Destination (Join-Path $stage 'boberto') -Recurse
    $zipPath = Join-Path $root "artifacts\BuzzPrint-rs-$version-win-x64.zip"
    if (Test-Path -LiteralPath $zipPath) { Remove-Item -LiteralPath $zipPath -Force }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::CreateFromDirectory($stage, $zipPath, [IO.Compression.CompressionLevel]::Optimal, $true)
    Remove-Item -LiteralPath (Split-Path $stage) -Recurse -Force
    Write-Output ("Zip: $zipPath ({0:N1} MB)" -f ((Get-Item -LiteralPath $zipPath).Length / 1MB))
}

if ($Msix) {
    $msixVersion = "$version.0"                        # la Store exige 4 partes con la última en 0
    $store = Join-Path $root 'artifacts\store'
    $cache = Join-Path $root 'artifacts\cache'
    $stage = Join-Path $store 'app'
    if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
    New-Item -ItemType Directory -Force $stage, $cache | Out-Null
    $identity = Get-Content -LiteralPath (Join-Path $root 'packaging\store.json') -Raw | ConvertFrom-Json
    Copy-Item -LiteralPath $exe, (Join-Path $root 'LICENSE'), (Join-Path $root 'THIRD-PARTY-NOTICES.md') -Destination $stage
    # Boberto: el motor web va junto al .exe (se muestra con WebView2 solo mientras se ve).
    Copy-Item -LiteralPath (Join-Path $root 'assets\boberto') -Destination (Join-Path $stage 'boberto') -Recurse

    # Herramientas del Windows SDK (makeappx, makepri, signtool) desde el paquete NuGet oficial de Microsoft.
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

    # FFmpeg dentro del paquete: build "essentials" de gyan.dev, verificado con el SHA-256 publicado.
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
        $ff = $z.Entries | Where-Object { $_.FullName -like '*/bin/ffmpeg.exe' } | Select-Object -First 1
        $lic = $z.Entries | Where-Object { $_.FullName -match '^[^/]+/LICENSE(\.txt)?$' } | Select-Object -First 1
        $ffBuild = ($ff.FullName -split '/')[0]
        [IO.Compression.ZipFileExtensions]::ExtractToFile($ff, (Join-Path $tools 'ffmpeg.exe'), $true)
        if ($lic) { [IO.Compression.ZipFileExtensions]::ExtractToFile($lic, (Join-Path $tools 'FFmpeg-LICENSE.txt'), $true) }
    } finally { $z.Dispose() }
    Set-Content -LiteralPath (Join-Path $tools 'FFmpeg-SOURCE.txt') -Encoding UTF8 -Value @"
BuzzPrint runs FFmpeg as a separate program (tools\ffmpeg.exe) to record video and audio.
Build: $ffBuild, from https://www.gyan.dev/ffmpeg/builds/ (unmodified).
FFmpeg is licensed under the GNU GPL version 3 (see FFmpeg-LICENSE.txt).
Source code: https://ffmpeg.org/download.html#get-sources and https://github.com/FFmpeg/FFmpeg.
"@

    # Logos (desde resources\buzzprint.png) y manifiesto.
    Add-Type -AssemblyName System.Drawing
    $images = Join-Path $stage 'Images'
    New-Item -ItemType Directory -Force $images | Out-Null
    $src = [Drawing.Image]::FromFile((Join-Path $root 'resources\buzzprint.png'))
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
    $manifest = (Get-Content -LiteralPath (Join-Path $root 'packaging\AppxManifest.xml') -Raw).
        Replace('{{IdentityName}}', $identity.IdentityName).Replace('{{Publisher}}', [Security.SecurityElement]::Escape($identity.Publisher)).
        Replace('{{PublisherDisplayName}}', [Security.SecurityElement]::Escape($identity.PublisherDisplayName)).Replace('{{Version}}', $msixVersion)
    [IO.File]::WriteAllText((Join-Path $stage 'AppxManifest.xml'), $manifest, (New-Object Text.UTF8Encoding $false))

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
