using System.Diagnostics;
using System.IO;
using System.IO.Compression;
using System.Net.Http;
using System.Security.Cryptography;
using System.Security.Cryptography.X509Certificates;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Documents;
using System.Windows.Media;
using Microsoft.Web.WebView2.Core;

namespace BuzzPrint;

/// <summary>
/// Optional components the app can fetch by itself, so the published zip runs on a clean PC:
/// FFmpeg (only for recording) from gyan.dev's official Windows builds, checked against the published SHA-256 and kept
/// in %LOCALAPPDATA%\BuzzPrint\tools; and Microsoft Edge WebView2 (Boberto, Studio, Personalize preview) through
/// Microsoft's own bootstrapper, run only if it carries a valid Microsoft signature. .NET is inside the app already.
/// </summary>
public static class Dependencies
{
    const string FFmpegZip = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
    const string WebView2Setup = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";

    public static string FFmpegHome => Path.Combine(DataFolder.Root, "tools", "ffmpeg.exe");

    public static bool WebView2Installed()
    {
        try { return !string.IsNullOrEmpty(CoreWebView2Environment.GetAvailableBrowserVersionString()); }
        catch (Exception ex) when (ex is WebView2RuntimeNotFoundException or FileNotFoundException or DllNotFoundException) { return false; }
    }

    static HttpClient Client()
    {
        // System proxy with the signed-in user's credentials (corporate networks).
        var handler = new HttpClientHandler { UseProxy = true, DefaultProxyCredentials = System.Net.CredentialCache.DefaultCredentials, AllowAutoRedirect = true };
        var http = new HttpClient(handler) { Timeout = TimeSpan.FromMinutes(20) };
        http.DefaultRequestHeaders.UserAgent.ParseAdd("BuzzPrint/" + (typeof(App).Assembly.GetName().Version?.ToString(3) ?? "0"));
        return http;
    }

    /// <summary>Asks, downloads and unpacks FFmpeg. Returns the ffmpeg.exe path, or null if declined or failed.</summary>
    public static async Task<string?> OfferFFmpegAsync(Window? owner)
    {
        if (!PromptWindow.Confirm(owner, L.T("Falta FFmpeg"),
                L.T("Para grabar video y audio, BuzzPrint usa FFmpeg (gratuito y de código abierto). Lo puede descargar ahora desde gyan.dev, las compilaciones oficiales para Windows que recomienda ffmpeg.org (unos 110 MB), y guardarlo en su carpeta. Las capturas, el editor y la exportación funcionan sin él."),
                L.T("Descargar FFmpeg"), L.T("Ahora no"))) return null;
        try { return await DownloadFFmpegAsync(owner); }
        catch (Exception ex) when (ex is HttpRequestException or IOException or InvalidDataException or UnauthorizedAccessException)
        {
            PromptWindow.Info(owner, L.T("No se pudo descargar FFmpeg"), ex.Message + "\n\n" + L.T("Podés instalarlo a mano con:") + " winget install Gyan.FFmpeg");
            return null;
        }
    }

    /// <summary>Downloads, verifies (SHA-256) and unpacks FFmpeg with a progress window. Null if cancelled.</summary>
    public static async Task<string?> DownloadFFmpegAsync(Window? owner)
    {
        var progress = new DownloadWindow(L.T("Descargando FFmpeg"), owner);
        progress.Show();
        string temp = Path.Combine(Path.GetTempPath(), "BuzzPrint-ffmpeg-" + Guid.NewGuid().ToString("N") + ".zip");
        try
        {
            using var http = Client();
            string expected = (await http.GetStringAsync(FFmpegZip + ".sha256", progress.Token)).Trim().Split(' ', '\t', '\n')[0].ToLowerInvariant();
            await DownloadAsync(http, FFmpegZip, temp, progress);
            progress.Status(L.T("Verificando…"));
            string actual;
            using (var fs = File.OpenRead(temp)) actual = Convert.ToHexString(await SHA256.HashDataAsync(fs, progress.Token)).ToLowerInvariant();
            if (expected.Length != 64 || actual != expected) throw new InvalidDataException(L.T("La descarga no coincide con la firma SHA-256 publicada. Probá de nuevo."));
            progress.Status(L.T("Descomprimiendo…"));
            Directory.CreateDirectory(Path.GetDirectoryName(FFmpegHome)!);
            await Task.Run(() =>
            {
                using var zip = ZipFile.OpenRead(temp);
                var exe = zip.Entries.FirstOrDefault(e => e.FullName.EndsWith("/bin/ffmpeg.exe", StringComparison.OrdinalIgnoreCase))
                          ?? throw new InvalidDataException("ffmpeg.exe not found in the package.");
                exe.ExtractToFile(FFmpegHome, true);
                // Its license travels with it (FFmpeg is GPL; BuzzPrint only downloads it for the user).
                var license = zip.Entries.FirstOrDefault(e => e.FullName.EndsWith("/LICENSE", StringComparison.OrdinalIgnoreCase));
                license?.ExtractToFile(Path.Combine(Path.GetDirectoryName(FFmpegHome)!, "FFmpeg-LICENSE.txt"), true);
            });
            progress.Close();
            return File.Exists(FFmpegHome) ? FFmpegHome : null;
        }
        catch (OperationCanceledException) { progress.Close(); return null; }
        catch { progress.Close(); throw; }
        finally { try { File.Delete(temp); } catch (IOException) { } }
    }

    /// <summary>Asks, downloads Microsoft's WebView2 bootstrapper, checks its signature and runs it.</summary>
    public static async Task<bool> OfferWebView2Async(Window? owner)
    {
        if (!PromptWindow.Confirm(owner, L.T("Falta Microsoft Edge WebView2"),
                L.T("Boberto, Boberto Studio y la vista previa de Personalizar usan Microsoft Edge WebView2, que no está en esta PC. BuzzPrint puede bajar el instalador oficial de Microsoft (2 MB) e instalarlo. Las capturas, la grabación y el editor funcionan igual sin él."),
                L.T("Instalar WebView2"), L.T("Ahora no"))) return false;
        var progress = new DownloadWindow(L.T("Instalando WebView2"), owner);
        progress.Show();
        string temp = Path.Combine(Path.GetTempPath(), "MicrosoftEdgeWebview2Setup-" + Guid.NewGuid().ToString("N") + ".exe");
        try
        {
            using (var http = Client()) await DownloadAsync(http, WebView2Setup, temp, progress);
            if (!SignedByMicrosoft(temp)) throw new InvalidDataException(L.T("El instalador descargado no tiene una firma válida de Microsoft. No se ejecutó."));
            progress.Status(L.T("Instalando… puede tardar un minuto."));
            using var setup = Process.Start(new ProcessStartInfo(temp, "/silent /install") { UseShellExecute = true })!;
            await setup.WaitForExitAsync(progress.Token);
            progress.Close();
            return WebView2Installed();
        }
        catch (OperationCanceledException) { progress.Close(); return false; }
        catch (Exception ex) when (ex is HttpRequestException or IOException or InvalidDataException or UnauthorizedAccessException or System.ComponentModel.Win32Exception)
        {
            progress.Close();
            PromptWindow.Info(owner, L.T("No se pudo instalar WebView2"), ex.Message + "\n\n" + L.T("Podés instalarlo a mano desde:") + " developer.microsoft.com/microsoft-edge/webview2");
            return false;
        }
        finally { try { File.Delete(temp); } catch (IOException) { } catch (UnauthorizedAccessException) { } }
    }

    static bool SignedByMicrosoft(string file)
    {
        try
        {
#pragma warning disable SYSLIB0057
            using var cert = new X509Certificate2(X509Certificate.CreateFromSignedFile(file));
#pragma warning restore SYSLIB0057
            if (!cert.Subject.Contains("O=Microsoft Corporation", StringComparison.Ordinal)) return false;
            using var chain = new X509Chain();
            chain.ChainPolicy.RevocationMode = X509RevocationMode.NoCheck;   // proxies often block revocation lookups
            return chain.Build(cert);
        }
        catch (CryptographicException) { return false; }
    }

    static async Task DownloadAsync(HttpClient http, string url, string path, DownloadWindow progress)
    {
        using var response = await http.GetAsync(url, HttpCompletionOption.ResponseHeadersRead, progress.Token);
        response.EnsureSuccessStatusCode();
        long? total = response.Content.Headers.ContentLength;
        await using var source = await response.Content.ReadAsStreamAsync(progress.Token);
        await using var target = File.Create(path);
        var buffer = new byte[81920];
        long done = 0; int read;
        while ((read = await source.ReadAsync(buffer, progress.Token)) > 0)
        {
            await target.WriteAsync(buffer.AsMemory(0, read), progress.Token);
            done += read;
            progress.Report(done, total);
        }
    }
}

/// <summary>Small DBMON-style progress window with Cancel.</summary>
public sealed class DownloadWindow : Window
{
    readonly CancellationTokenSource cancel = new();
    readonly ProgressBar bar = new() { Height = 6, Minimum = 0, Maximum = 1, Margin = new Thickness(0, 12, 0, 8) };
    readonly TextBlock detail = new();
    public CancellationToken Token => cancel.Token;

    public DownloadWindow(string title, Window? owner)
    {
        SetResourceReference(StyleProperty, typeof(Window));
        Title = title + " · BuzzPrint"; Width = 460; SizeToContent = SizeToContent.Height; ResizeMode = ResizeMode.NoResize; ShowInTaskbar = false;
        WindowStartupLocation = owner != null && owner.IsVisible ? WindowStartupLocation.CenterOwner : WindowStartupLocation.CenterScreen;
        if (owner != null && owner.IsVisible) Owner = owner;
        var root = new StackPanel { Margin = new Thickness(24, 20, 24, 18) };
        var head = new TextBlock { FontWeight = FontWeights.SemiBold };
        var mark = new Run("◤ "); mark.SetResourceReference(TextElement.ForegroundProperty, "Accent");
        head.Inlines.Add(mark); head.Inlines.Add(new Run(title));
        root.Children.Add(head);
        bar.SetResourceReference(ForegroundProperty, "Accent");
        root.Children.Add(bar);
        detail.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        detail.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont");
        detail.Text = L.T("Conectando…");
        root.Children.Add(detail);
        var stop = new Button { Content = L.T("Cancelar"), HorizontalAlignment = HorizontalAlignment.Right, MinWidth = 96, Margin = new Thickness(0, 12, 0, 0) };
        stop.Click += (_, _) => cancel.Cancel();
        root.Children.Add(stop);
        Content = root;
        Closed += (_, _) => cancel.Cancel();
    }

    public void Report(long done, long? total) => Dispatcher.BeginInvoke(() =>
    {
        bar.IsIndeterminate = total is not > 0;
        if (total is > 0) bar.Value = (double)done / total.Value;
        detail.Text = (done / 1048576d).ToString("0.0") + " MB" + (total is > 0 ? " / " + (total.Value / 1048576d).ToString("0.0") + " MB" : "");
    });

    public void Status(string text) => Dispatcher.Invoke(() => { bar.IsIndeterminate = true; detail.Text = text; });
}
