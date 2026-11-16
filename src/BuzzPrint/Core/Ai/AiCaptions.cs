using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace BuzzPrint.Ai;

/// <summary>A short title and an explanatory caption ("pie") for one capture.</summary>
public sealed record CaptionResult(string Title, string Caption);

/// <summary>
/// Title and caption for captures, written by the configured AI from the image plus the text Windows OCR read
/// from it. The caption goes to the capture's note, which exports already print under each image; the title
/// replaces only automatic names (Region_14_11_2026_10_33_00), never one the person typed.
/// </summary>
public static partial class AiCaptions
{
    static string SystemPrompt => AiInput.Language + " " + AiInput.Untrusted + """

        You describe a screenshot for a library of captures and step-by-step documents. You receive one screenshot and,
        when available, the text OCR read from it. Return ONLY a JSON object, no code fences:
        {"title": "...", "summary": "..."}
        - title: a short name, AT MOST 6 words and 45 characters: the system, screen or error it shows
          (e.g. "ORA-01555 in nightly export", "Users settings in admin panel"). No dates, no file names, no period.
        - summary: 2 or 3 plain sentences explaining what the screenshot shows and why it matters, citing the key
          values, messages or buttons visible. No markdown.
        - Do not invent what is not visible. If something is unreadable, leave it out.
        """;

    [GeneratedRegex(@"_\d{2}_\d{2}_\d{4}_\d{2}_\d{2}_\d{2}$")]
    private static partial Regex DefaultNamePattern();

    /// <summary>True when the title is still the automatic name, so AI may replace it.</summary>
    public static bool HasDefaultTitle(CaptureItem it) => it.Title.Trim().Length == 0 || DefaultNamePattern().IsMatch(it.Title.Trim());

    public static async Task<CaptionResult> DescribeAsync(AiClient client, CaptureItem it, CancellationToken ct = default)
    {
        byte[] png = await Task.Run(() => Downscaled(it.Path, 1568), ct);
        string ocr = (await Ocr.GetAsync(it, ct))?.Text.Trim() ?? "";
        if (ocr.Length > 6000) ocr = ocr[..6000];
        string prompt = ocr.Length > 0
            ? "Text read from the screenshot by OCR (may contain errors):\n<ocr>\n" + ocr + "\n</ocr>\nDescribe the screenshot."
            : "Describe the screenshot.";
        string answer = await client.CompleteAsync(new AiRequest(SystemPrompt, prompt, [png], 2500), ct);
        return Parse(answer);
    }

    /// <summary>Describes each capture in turn and stores the result. Returns how many were updated.</summary>
    public static async Task<int> ApplyAsync(IReadOnlyList<CaptureItem> items, IProgress<(int done, int total)>? progress, CancellationToken ct = default)
    {
        var client = AiClient.FromSettings(Store.Settings);
        int done = 0, updated = 0;
        foreach (var it in items)
        {
            ct.ThrowIfCancellationRequested();
            progress?.Report((done, items.Count));
            var r = await DescribeAsync(client, it, ct);
            if (r.Caption.Length > 0) { Store.UpdateMeta(it, "note", r.Caption); updated++; }
            if (r.Title.Length > 0 && HasDefaultTitle(it)) Store.UpdateMeta(it, "title", r.Title);
            done++;
        }
        progress?.Report((done, items.Count));
        Store.RaiseChanged();
        return updated;
    }

    /// <summary>Reads {"title","caption"} from the answer, tolerating code fences or text around the JSON.</summary>
    static CaptionResult Parse(string answer)
    {
        int a = answer.IndexOf('{'), b = answer.LastIndexOf('}');
        if (a >= 0 && b > a)
        {
            try
            {
                using var doc = JsonDocument.Parse(answer[a..(b + 1)]);
                string Get(string k) => doc.RootElement.TryGetProperty(k, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString()!.Trim() : "";
                string summary = Get("summary"); if (summary.Length == 0) summary = Get("caption");
                return new CaptionResult(ShortTitle(Get("title")), Clean(summary, 700));
            }
            catch (JsonException) { }
        }
        // Not JSON: keep the whole answer as the caption rather than losing it.
        return new CaptionResult("", Clean(answer, 700));
    }

    /// <summary>Titles stay short even when a model ignores the limit: six words, 48 characters.</summary>
    static string ShortTitle(string t)
    {
        var words = Clean(t, 200).TrimEnd('.').Split(' ', StringSplitOptions.RemoveEmptyEntries);
        string s = string.Join(" ", words.Take(6));
        return s.Length > 48 ? s[..48].TrimEnd() : s;
    }

    static string Clean(string s, int max)
    {
        s = s.Replace("\r", " ").Replace("\n", " ").Trim().Trim('"').Trim();
        while (s.Contains("  ")) s = s.Replace("  ", " ");
        return s.Length > max ? s[..max].TrimEnd() + "…" : s;
    }

    /// <summary>The capture as PNG with its long side at most <paramref name="maxSide"/> px (fewer tokens, same legibility).</summary>
    static byte[] Downscaled(string path, int maxSide)
    {
        using var src = new Bitmap(path);
        double scale = Math.Min(1.0, (double)maxSide / Math.Max(src.Width, src.Height));
        using var ms = new MemoryStream();
        if (scale >= 1.0) { src.Save(ms, ImageFormat.Png); return ms.ToArray(); }
        int w = Math.Max(1, (int)Math.Round(src.Width * scale)), h = Math.Max(1, (int)Math.Round(src.Height * scale));
        using var dst = new Bitmap(w, h);
        using (var g = Graphics.FromImage(dst))
        {
            g.InterpolationMode = InterpolationMode.HighQualityBicubic; g.SmoothingMode = SmoothingMode.HighQuality; g.PixelOffsetMode = PixelOffsetMode.HighQuality;
            g.DrawImage(src, 0, 0, w, h);
        }
        dst.Save(ms, ImageFormat.Png);
        return ms.ToArray();
    }
}
