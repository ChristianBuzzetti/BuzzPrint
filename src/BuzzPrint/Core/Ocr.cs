using System.Globalization;
using System.IO;
using System.Text.Json;

namespace BuzzPrint;

/// <summary>One recognized word and where it sits in the capture, in the capture's own pixels.</summary>
public sealed record OcrWord(string Text, double X, double Y, double W, double H);

/// <summary>Text read from a capture: lines joined with newlines, plus every word with its box (for redaction later).</summary>
public sealed class OcrResult
{
    public string Text { get; set; } = "";
    public List<OcrWord> Words { get; set; } = [];
    public string Language { get; set; } = "";
    public long Stamp { get; set; }   // capture's last write (UTC ticks) + size, to notice edits
}

/// <summary>
/// Reads the text inside captures with the OCR engine built into Windows (Windows.Media.Ocr): offline, free,
/// nothing leaves the PC. Results are cached per capture in LocalAppData\&lt;app&gt;\ocr as small JSON files and
/// refreshed when the capture changes (an edit rewrites the PNG). Needs the Windows SDK projection (WINRT builds);
/// elsewhere <see cref="Supported"/> is false and every call returns nothing.
/// </summary>
public static class Ocr
{
    static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = false };
    static string CacheDir => Path.Combine(DataFolder.Root, "ocr");
    static string CachePath(CaptureItem it) => Path.Combine(CacheDir, it.Id + ".json");

    static long StampOf(string path)
    {
        try { var fi = new FileInfo(path); return fi.LastWriteTimeUtc.Ticks ^ fi.Length; } catch (IOException) { return 0; } catch (UnauthorizedAccessException) { return 0; }
    }

    /// <summary>Cached result if it is still current for the capture, else null.</summary>
    public static OcrResult? Cached(CaptureItem it)
    {
        try
        {
            string p = CachePath(it);
            if (!File.Exists(p)) return null;
            var r = JsonSerializer.Deserialize<OcrResult>(File.ReadAllText(p), JsonOptions);
            return r != null && r.Stamp == StampOf(it.Path) ? r : null;
        }
        catch (Exception ex) when (ex is IOException or JsonException or UnauthorizedAccessException) { return null; }
    }

    /// <summary>Cached text when current, otherwise reads the capture now and caches it. Null if OCR is unavailable.</summary>
    public static async Task<OcrResult?> GetAsync(CaptureItem it, CancellationToken ct = default)
    {
        if (!Supported || it.IsRecording) return null;
        var cached = Cached(it);
        if (cached != null) return cached;
        long stamp = StampOf(it.Path);
        var r = await RecognizeAsync(it.Path, ct);
        if (r == null) return null;
        r.Stamp = stamp;
        try { Directory.CreateDirectory(CacheDir); AtomicFiles.SaveUtf8(CachePath(it), JsonSerializer.Serialize(r, JsonOptions)); }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { System.Diagnostics.Debug.WriteLine("OCR cache: " + ex.Message); }
        return r;
    }

    /// <summary>Removes cache files of captures that no longer exist.</summary>
    public static void Prune(IEnumerable<CaptureItem> alive)
    {
        try
        {
            if (!Directory.Exists(CacheDir)) return;
            var ids = new HashSet<string>(alive.Select(x => x.Id), StringComparer.OrdinalIgnoreCase);
            foreach (var f in Directory.GetFiles(CacheDir, "*.json"))
                if (!ids.Contains(Path.GetFileNameWithoutExtension(f))) try { File.Delete(f); } catch (IOException) { }
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }
    }

#if WINRT
    static global::Windows.Media.Ocr.OcrEngine? engine;
    static bool engineTried;

    static global::Windows.Media.Ocr.OcrEngine? Engine
    {
        get
        {
            if (!engineTried)
            {
                engineTried = true;
                try { engine = global::Windows.Media.Ocr.OcrEngine.TryCreateFromUserProfileLanguages(); }
                catch (Exception ex) when (ex is System.Runtime.InteropServices.COMException or InvalidOperationException) { engine = null; }
            }
            return engine;
        }
    }

    /// <summary>True when this build has OCR and Windows has an OCR language installed (usually the display language).</summary>
    public static bool Supported => Engine != null;

    static async Task<OcrResult?> RecognizeAsync(string path, CancellationToken ct)
    {
        var eng = Engine;
        if (eng == null || !File.Exists(path)) return null;
        try
        {
            byte[] bytes = await File.ReadAllBytesAsync(path, ct);
            using var ms = new global::Windows.Storage.Streams.InMemoryRandomAccessStream();
            using (var writer = new global::Windows.Storage.Streams.DataWriter(ms))
            {
                writer.WriteBytes(bytes);
                await writer.StoreAsync();
                await writer.FlushAsync();
                writer.DetachStream();
            }
            ms.Seek(0);
            var decoder = await global::Windows.Graphics.Imaging.BitmapDecoder.CreateAsync(ms);
            uint w = decoder.PixelWidth, h = decoder.PixelHeight;
            // Small UI text reads better enlarged; the engine refuses images above MaxImageDimension.
            double max = global::Windows.Media.Ocr.OcrEngine.MaxImageDimension;
            double scale = Math.Max(w, h) < 1600 ? 2.0 : 1.0;
            scale = Math.Min(scale, max / Math.Max(w, h));
            var transform = new global::Windows.Graphics.Imaging.BitmapTransform
            {
                ScaledWidth = (uint)Math.Max(1, Math.Round(w * scale)),
                ScaledHeight = (uint)Math.Max(1, Math.Round(h * scale)),
                InterpolationMode = global::Windows.Graphics.Imaging.BitmapInterpolationMode.Fant
            };
            using var bmp = await decoder.GetSoftwareBitmapAsync(
                global::Windows.Graphics.Imaging.BitmapPixelFormat.Bgra8, global::Windows.Graphics.Imaging.BitmapAlphaMode.Premultiplied,
                transform, global::Windows.Graphics.Imaging.ExifOrientationMode.IgnoreExifOrientation,
                global::Windows.Graphics.Imaging.ColorManagementMode.DoNotColorManage);
            ct.ThrowIfCancellationRequested();
            var res = await eng.RecognizeAsync(bmp);
            var result = new OcrResult { Language = eng.RecognizerLanguage?.LanguageTag ?? "" };
            var lines = new List<string>();
            foreach (var line in res.Lines)
            {
                lines.Add(line.Text);
                foreach (var word in line.Words)
                {
                    var b = word.BoundingRect;
                    result.Words.Add(new OcrWord(word.Text, b.X / scale, b.Y / scale, b.Width / scale, b.Height / scale));
                }
            }
            result.Text = string.Join("\n", lines);
            return result;
        }
        catch (OperationCanceledException) { throw; }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or System.Runtime.InteropServices.COMException or ArgumentException or InvalidOperationException)
        {
            System.Diagnostics.Debug.WriteLine("OCR: " + ex.Message);
            return null;
        }
    }
#else
    public static bool Supported => false;
    static Task<OcrResult?> RecognizeAsync(string path, CancellationToken ct) => Task.FromResult<OcrResult?>(null);
#endif
}

/// <summary>
/// Keeps the text of every capture in memory for the library search, reading new or edited captures in the
/// background one at a time (only the OCR cache in LocalAppData is written, never the captures themselves).
/// </summary>
public static class OcrIndex
{
    static readonly Dictionary<string, string> text = new(StringComparer.OrdinalIgnoreCase);
    static readonly object gate = new();
    static CancellationTokenSource? run;

    /// <summary>Raised on the thread pool while indexing: (done, total). Total 0 = finished.</summary>
    public static event Action<int, int>? Progress;
    /// <summary>Raised after a batch of captures got new text, so an active search can refresh.</summary>
    public static event Action? Updated;

    /// <summary>Accent- and case-insensitive match against the text read from the capture.</summary>
    public static bool Matches(CaptureItem it, string query)
    {
        if (query.Length == 0) return false;
        string? t;
        lock (gate) if (!text.TryGetValue(it.Id, out t)) return false;
        return CultureInfo.InvariantCulture.CompareInfo.IndexOf(t, query, CompareOptions.IgnoreCase | CompareOptions.IgnoreNonSpace) >= 0;
    }

    public static void Remember(CaptureItem it, OcrResult r) { lock (gate) text[it.Id] = r.Text; }

    /// <summary>Starts (or restarts) the background pass over the library. Cheap when everything is cached.</summary>
    public static void Refresh()
    {
        if (!Ocr.Supported) return;
        run?.Cancel();
        var cts = run = new CancellationTokenSource();
        var items = Store.All().ToList();
        _ = Task.Run(async () =>
        {
            var todo = new List<CaptureItem>();
            foreach (var it in items)
            {
                var c = Ocr.Cached(it);
                if (c != null) Remember(it, c); else todo.Add(it);
            }
            if (items.Count > 0) Updated?.Invoke();
            Ocr.Prune(items);
            int done = 0, sinceUpdate = 0;
            foreach (var it in todo)
            {
                if (cts.IsCancellationRequested) return;
                Progress?.Invoke(done, todo.Count);
                try
                {
                    var r = await Ocr.GetAsync(it, cts.Token);
                    if (r != null) { Remember(it, r); sinceUpdate++; }
                }
                catch (OperationCanceledException) { return; }
                done++;
                if (sinceUpdate >= 5) { sinceUpdate = 0; Updated?.Invoke(); }
                await Task.Delay(150, cts.Token).ContinueWith(_ => { });   // stay light: one capture at a time, paced
            }
            if (sinceUpdate > 0) Updated?.Invoke();
            Progress?.Invoke(todo.Count, 0);
        });
    }
}
