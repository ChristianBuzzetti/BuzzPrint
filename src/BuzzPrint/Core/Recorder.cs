using System.Diagnostics;
using System.IO;
using System.Text;
using Drawing = System.Drawing;

namespace BuzzPrint;

public enum RecordKind { Video, Audio }

/// <summary>Screen (and/or audio) recording. Video: ffmpeg gdigrab encodes the area to H.264 while WASAPI captures
/// system audio (loopback) and the microphone to WAV; on stop ffmpeg mixes the tracks into an MP4. Audio only: the
/// WAV tracks are mixed into an MP3. Recordings go to Videos\BuzzPrint with a sidecar (title, source, duration) and
/// a thumbnail (first frame, or the waveform for audio).</summary>
public sealed class Recorder
{
    public static Recorder? Current { get; private set; }
    public RecordKind Kind { get; }
    public Drawing.Rectangle Area { get; }
    public string Source { get; }
    public bool WithSystem { get; private set; }
    public bool WithMic { get; private set; }
    public TimeSpan Elapsed => clock.Elapsed;
    public float Level => Math.Max(sys?.Level ?? 0, mic?.Level ?? 0);
    public string? Warning { get; private set; }

    readonly string id, work;
    readonly Stopwatch clock = new();
    readonly StringBuilder log = new();
    Process? ffmpeg;
    AudioCapture? sys, mic;

    Recorder(RecordKind kind, Drawing.Rectangle area, string source)
    {
        Kind = kind; Area = area; Source = source;
        id = DateTime.Now.ToString("yyyyMMdd_HHmmss_fff");
        work = Path.Combine(Path.GetTempPath(), "BuzzPrint", "rec-" + id);
        Directory.CreateDirectory(work);
    }

    // ------------------------------------------------------------------ ffmpeg
    public static string? FFmpeg()
    {
        string bundled = Path.Combine(AppContext.BaseDirectory, "tools", "ffmpeg.exe");
        if (File.Exists(bundled)) return bundled;
        if (File.Exists(Dependencies.FFmpegHome)) return Dependencies.FFmpegHome;   // downloaded by the app
        foreach (var dir in (Environment.GetEnvironmentVariable("PATH") ?? "").Split(';'))
            try { string p = Path.Combine(dir.Trim(), "ffmpeg.exe"); if (dir.Length > 0 && File.Exists(p)) return p; } catch (ArgumentException) { }
        string winget = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Microsoft", "WinGet", "Links", "ffmpeg.exe");
        return File.Exists(winget) ? winget : null;
    }

    static ProcessStartInfo Info(string exe, string args)
    {
        var psi = new ProcessStartInfo(exe, args)
        {
            UseShellExecute = false, CreateNoWindow = true, RedirectStandardInput = true, RedirectStandardError = true, RedirectStandardOutput = true,
            StandardErrorEncoding = Encoding.UTF8
        };
        // gdigrab reads physical pixels only when ffmpeg is DPI aware; the selected area is in physical pixels.
        psi.Environment["__COMPAT_LAYER"] = "HighDpiAware";
        return psi;
    }

    static async Task<(int code, string err)> RunAsync(string exe, string args)
    {
        using var p = Process.Start(Info(exe, args))!;
        var err = p.StandardError.ReadToEndAsync();
        _ = p.StandardOutput.ReadToEndAsync();
        await p.WaitForExitAsync();
        return (p.ExitCode, await err);
    }

    static string Q(string path) => "\"" + path + "\"";

    // ------------------------------------------------------------------ start
    /// <summary>Starts a recording. Returns null and an error text when it cannot start.</summary>
    public static async Task<(Recorder? rec, string? error)> StartAsync(RecordKind kind, Drawing.Rectangle area, string source)
    {
        if (Current != null) return (null, L.T("Ya hay una grabación en curso."));
        string? exe = FFmpeg();
        if (exe == null) return (null, L.T("No se encontró ffmpeg.exe. Volvé a compilar BuzzPrint con build.ps1 (lo copia a tools) o instalalo con: winget install Gyan.FFmpeg"));
        var st = Store.Settings;
        if (kind == RecordKind.Audio && !st.RecordSystemAudio && !st.RecordMicrophone)
            return (null, L.T("Activá el audio del sistema o el micrófono para grabar solo audio."));
        var r = new Recorder(kind, new Drawing.Rectangle(area.X, area.Y, area.Width & ~1, area.Height & ~1), source);
        string? error = await r.BeginAsync(exe);
        if (error != null) { r.Cleanup(); return (null, error); }
        Current = r;
        return (r, null);
    }

    async Task<string?> BeginAsync(string exe)
    {
        var st = Store.Settings;
        if (Kind == RecordKind.Video)
        {
            if (Area.Width < 16 || Area.Height < 16) return L.T("El área es demasiado chica para grabar.");
            string args = "-hide_banner -y -f gdigrab -framerate " + st.RecordFps + " -draw_mouse " + (st.RecordCursor ? 1 : 0) +
                          " -offset_x " + Area.X + " -offset_y " + Area.Y + " -video_size " + Area.Width + "x" + Area.Height +
                          " -i desktop -c:v libx264 -preset ultrafast -crf 23 -pix_fmt yuv420p " + Q(Path.Combine(work, "video.mkv"));
            var started = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            ffmpeg = new Process { StartInfo = Info(exe, args), EnableRaisingEvents = true };
            ffmpeg.ErrorDataReceived += (_, e) =>
            {
                if (e.Data == null) return;
                lock (log) { if (log.Length < 64_000) log.AppendLine(e.Data); }
                if (e.Data.Contains("Press [q]") || e.Data.StartsWith("frame=")) started.TrySetResult(true);
            };
            ffmpeg.Exited += (_, _) => started.TrySetResult(false);
            try { ffmpeg.Start(); }
            catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException) { return L.T("No se pudo iniciar ffmpeg: ") + ex.Message; }
            ffmpeg.BeginErrorReadLine();
            _ = ffmpeg.StandardOutput.ReadToEndAsync();
            var first = await Task.WhenAny(started.Task, Task.Delay(15000));
            if (first != started.Task || !started.Task.Result || ffmpeg.HasExited)
            {
                try { if (!ffmpeg.HasExited) ffmpeg.Kill(); } catch (InvalidOperationException) { }
                return L.T("ffmpeg no pudo empezar a grabar.") + "\n" + Tail();
            }
        }
        StartAudio(st);
        if (Kind == RecordKind.Audio && sys == null && mic == null) return L.T("No se pudo abrir ningún dispositivo de audio.") + (Warning != null ? "\n" + Warning : "");
        clock.Start();
        return null;
    }

    void StartAudio(Settings st)
    {
        if (st.RecordSystemAudio)
        {
            sys = AudioCapture.ForSystem(Path.Combine(work, "system.wav"));
            if (!sys.Start()) { Warning = L.T("Audio del sistema no disponible: ") + sys.Error; sys = null; }
        }
        if (st.RecordMicrophone)
        {
            mic = AudioCapture.ForMicrophone(Path.Combine(work, "mic.wav"));
            if (!mic.Start()) { Warning = (Warning != null ? Warning + "\n" : "") + L.T("Micrófono no disponible: ") + mic.Error; mic = null; }
        }
        WithSystem = sys != null; WithMic = mic != null;
    }

    string Tail()
    {
        lock (log)
        {
            var lines = log.ToString().Split('\n', StringSplitOptions.RemoveEmptyEntries);
            return string.Join("\n", lines.Skip(Math.Max(0, lines.Length - 6)));
        }
    }

    // ------------------------------------------------------------------ stop
    /// <summary>Stops and (when keep) builds the final file. Returns the stored item, or an error.</summary>
    public async Task<(CaptureItem? item, string? error)> StopAsync(bool keep)
    {
        clock.Stop();
        double seconds = clock.Elapsed.TotalSeconds;
        try
        {
            if (ffmpeg != null && !ffmpeg.HasExited)
            {
                try { await ffmpeg.StandardInput.WriteAsync("q"); await ffmpeg.StandardInput.FlushAsync(); } catch (IOException) { }
                var exited = ffmpeg.WaitForExitAsync();
                if (await Task.WhenAny(exited, Task.Delay(10000)) != exited) try { ffmpeg.Kill(); } catch (InvalidOperationException) { }
            }
            await Task.Run(() => { sys?.Stop(); mic?.Stop(); });
            if (!keep) return (null, null);
            return await Task.Run(() => FinishAsync(seconds));
        }
        finally
        {
            Current = null;
            Cleanup();
        }
    }

    async Task<(CaptureItem? item, string? error)> FinishAsync(double seconds)
    {
        string exe = FFmpeg()!;
        Directory.CreateDirectory(Store.RecordDir);
        Directory.CreateDirectory(Store.RecordMetaDir);
        var tracks = new List<string>();
        if (sys != null && sys.HasData) tracks.Add(Path.Combine(work, "system.wav"));
        if (mic != null && mic.HasData) tracks.Add(Path.Combine(work, "mic.wav"));
        string ext = Kind == RecordKind.Video ? ".mp4" : ".mp3";
        string output = Path.Combine(Store.RecordDir, id + ext);
        var args = new StringBuilder("-hide_banner -y ");
        if (Kind == RecordKind.Video) args.Append("-i ").Append(Q(Path.Combine(work, "video.mkv"))).Append(' ');
        foreach (var t in tracks) args.Append("-i ").Append(Q(t)).Append(' ');
        int a0 = Kind == RecordKind.Video ? 1 : 0;
        if (Kind == RecordKind.Audio && tracks.Count == 0) return (null, L.T("La grabación no tiene audio."));
        if (tracks.Count == 2) args.Append("-filter_complex \"[" + a0 + ":a][" + (a0 + 1) + ":a]amix=inputs=2:duration=longest:normalize=0[a]\" ");
        if (Kind == RecordKind.Video)
        {
            args.Append("-map 0:v ");
            if (tracks.Count == 2) args.Append("-map \"[a]\" "); else if (tracks.Count == 1) args.Append("-map 1:a ");
            args.Append("-c:v copy ");
            if (tracks.Count > 0) args.Append("-c:a aac -b:a 192k -shortest ");
            args.Append("-movflags +faststart ");
        }
        else
        {
            if (tracks.Count == 2) args.Append("-map \"[a]\" ");
            args.Append("-c:a libmp3lame -b:a 192k ");
        }
        args.Append(Q(output));
        var (code, err) = await RunAsync(exe, args.ToString());
        if (code != 0 || !File.Exists(output))
            return (null, L.T("No se pudo guardar la grabación.") + "\n" + string.Join("\n", err.Split('\n', StringSplitOptions.RemoveEmptyEntries).TakeLast(5)));

        string thumb = Path.Combine(Store.RecordMetaDir, id + ".png");
        string accent = Themes.Current.Colors.TryGetValue("Accent", out var c) ? "0x" + c.TrimStart('#')[^6..] : "0xF43F5E";
        if (Kind == RecordKind.Video)
            await RunAsync(exe, "-hide_banner -y -ss " + Math.Min(0.5, seconds / 2).ToString(System.Globalization.CultureInfo.InvariantCulture) + " -i " + Q(output) + " -frames:v 1 -vf \"scale='min(640,iw)':-2\" " + Q(thumb));
        else
            await RunAsync(exe, "-hide_banner -y -i " + Q(output) + " -filter_complex \"showwavespic=s=640x260:split_channels=0:colors=" + accent + "\" -frames:v 1 " + Q(thumb));

        var meta = new Dictionary<string, object> { { "source", Source }, { "duration", Math.Round(seconds, 1) }, { "kind", Kind == RecordKind.Video ? "video" : "audio" },
            { "title", Store.DefaultName(Source, DateTime.Now, Kind == RecordKind.Video ? "Video" : "Audio") } };
        var tracksText = new List<string>();
        if (tracks.Count > 0) { if (sys != null && sys.HasData) tracksText.Add("system"); if (mic != null && mic.HasData) tracksText.Add("mic"); }
        meta["audio"] = string.Join("+", tracksText);
        File.WriteAllText(Path.Combine(Store.RecordMetaDir, id + ".json"), new MiniJson().Serialize(meta));
        Store.RaiseChanged();
        return (Store.Recordings().FirstOrDefault(x => x.Id == id), null);
    }

    void Cleanup()
    {
        try { ffmpeg?.Dispose(); } catch (InvalidOperationException) { }
        try { if (Directory.Exists(work)) Directory.Delete(work, true); } catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }
    }

    public static string Clock(TimeSpan t) => t.TotalHours >= 1 ? t.ToString(@"h\:mm\:ss") : t.ToString(@"mm\:ss");
}
