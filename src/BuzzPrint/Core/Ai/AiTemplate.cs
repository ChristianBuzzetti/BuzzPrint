#if PREVIEW
using System.IO;
using System.IO.Compression;
using System.Text;
using System.Text.Json;
using System.Xml.Linq;

namespace BuzzPrint.Ai;

/// <summary>
/// Turns a person's own file into a document template. Word and PowerPoint: their theme (colors and fonts) is read
/// directly, plus the embedded thumbnail when there is one. PDF: the first page is rendered with Windows' PDF engine.
/// Images: used as they are. The AI then fills the template's design fields from that; without AI, Word and
/// PowerPoint still give a template built from their theme alone.
/// </summary>
public static class AiTemplate
{
    public static async Task<DocTemplate> InterpretAsync(string path, bool useAi, CancellationToken ct = default)
    {
        string ext = Path.GetExtension(path).ToLowerInvariant();
        string name = Path.GetFileNameWithoutExtension(path);
        var facts = new StringBuilder();
        var images = new List<byte[]>();
        DocTemplate fromTheme = new() { Name = name, Description = L.F("Importada de {0}", Path.GetFileName(path)) };

        if (ext is ".docx" or ".pptx")
        {
            using var zip = ZipFile.OpenRead(path);
            var theme = zip.Entries.FirstOrDefault(e => e.FullName.EndsWith("theme/theme1.xml", StringComparison.OrdinalIgnoreCase));
            if (theme != null)
            {
                using var s = theme.Open();
                var x = XDocument.Load(s);
                XNamespace a = "http://schemas.openxmlformats.org/drawingml/2006/main";
                string? Color(string slot)
                {
                    var el = x.Descendants(a + slot).FirstOrDefault();
                    return el?.Element(a + "srgbClr")?.Attribute("val")?.Value ?? el?.Element(a + "sysClr")?.Attribute("lastClr")?.Value;
                }
                string? Font(string kind) => x.Descendants(a + kind).FirstOrDefault()?.Element(a + "latin")?.Attribute("typeface")?.Value;
                var slots = new[] { "dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6" };
                facts.AppendLine("Theme colors: " + string.Join(", ", slots.Select(sl => sl + "=" + (Color(sl) ?? "?"))));
                facts.AppendLine("Theme fonts: headings=" + (Font("majorFont") ?? "?") + ", body=" + (Font("minorFont") ?? "?"));
                if (Color("accent1") is { Length: 6 } acc) fromTheme.Accent = acc;
                if (Color("dk1") is { Length: 6 } ink && ink != "000000") fromTheme.Ink = ink;
                if (Font("majorFont") is { Length: > 0 } hf && !hf.StartsWith('+')) fromTheme.HeadingFont = hf;
                if (Font("minorFont") is { Length: > 0 } bf && !bf.StartsWith('+')) fromTheme.BodyFont = bf;
            }
            var thumb = zip.Entries.FirstOrDefault(e => e.FullName.StartsWith("docProps/thumbnail", StringComparison.OrdinalIgnoreCase));
            if (thumb != null) { using var s = thumb.Open(); using var ms = new MemoryStream(); s.CopyTo(ms); images.Add(Fit(ms.ToArray())); }
            facts.AppendLine(ext == ".pptx" ? "Source: a PowerPoint presentation." : "Source: a Word document.");
        }
        else if (ext == ".pdf")
        {
            images.Add(Fit(await RenderPdfAsync(path, ct)));
            facts.AppendLine("Source: the first page of a PDF document (image attached).");
        }
        else
        {
            images.Add(Fit(await File.ReadAllBytesAsync(path, ct)));
            facts.AppendLine("Source: an image of a document page or slide (attached).");
        }

        if (!useAi) return fromTheme.Normalized();

        string system = AiInput.Language + " " + AiInput.Untrusted + """

            You turn the look of a document into a design template for exported screenshot guides (PowerPoint, Word, PDF).
            Return ONLY JSON with these fields:
            {"name": "...", "description": "...", "accent": "RRGGBB", "ink": "RRGGBB", "muted": "RRGGBB", "faint": "RRGGBB",
             "rule": "RRGGBB", "page": "RRGGBB", "coverFill": "RRGGBB", "coverInk": "RRGGBB", "frame": "none|accent|RRGGBB",
             "headingFont": "...", "bodyFont": "...", "cover": "bar|band|full|minimal", "steps": "inline|pill|number",
             "topRule": true|false, "footer": "..."}
            - Colors are six hex digits without #. accent = the brand / highlight color; ink = main text; muted = secondary text;
              faint = small print; rule = thin lines; page = page background (usually white or near white).
            - cover: "band" when the title sits on a colored band, "full" when the whole first page is colored, "minimal" when it is
              centered and plain, otherwise "bar".
            - steps: "pill" for filled labels, "number" for big numerals, otherwise "inline".
            - Fonts: use common Windows / Office font names (e.g. Calibri, Segoe UI, Arial, Georgia, Cambria, Consolas).
            - footer: a short company or team name if one is clearly visible, else "".
            - name: 2-4 words describing the style. description: one short sentence.
            """;
        string prompt = facts + "\nFile name: " + Path.GetFileName(path) + "\nBuild the template.";
        var client = AiClient.FromSettings(Store.Settings);
        string answer = await client.CompleteAsync(new AiRequest(system, prompt, images, 1500), ct);
        using var doc = AiInput.Json(answer) ?? throw new AiException(L.T("La IA no devolvió el formato esperado."));
        var t = JsonSerializer.Deserialize<DocTemplate>(doc.RootElement.GetRawText(), new JsonSerializerOptions { PropertyNameCaseInsensitive = true }) ?? fromTheme;
        t.Id = "";
        if (string.IsNullOrWhiteSpace(t.Name)) t.Name = name;
        if (string.IsNullOrWhiteSpace(t.Description)) t.Description = fromTheme.Description;
        return t.Normalized();
    }

    /// <summary>Page 1 of a PDF as PNG, via Windows.Data.Pdf (offline).</summary>
    static async Task<byte[]> RenderPdfAsync(string path, CancellationToken ct)
    {
        var file = await global::Windows.Storage.StorageFile.GetFileFromPathAsync(Path.GetFullPath(path));
        var pdf = await global::Windows.Data.Pdf.PdfDocument.LoadFromFileAsync(file);
        if (pdf.PageCount == 0) throw new AiException(L.T("El PDF no tiene páginas."));
        using var page = pdf.GetPage(0);
        using var ras = new global::Windows.Storage.Streams.InMemoryRandomAccessStream();
        await page.RenderToStreamAsync(ras, new global::Windows.Data.Pdf.PdfPageRenderOptions { DestinationWidth = 1400 });
        ct.ThrowIfCancellationRequested();
        ras.Seek(0);
        using var s = ras.AsStreamForRead();
        using var ms = new MemoryStream();
        await s.CopyToAsync(ms, ct);
        return ms.ToArray();
    }

    /// <summary>At most 1568 px on the long side, as PNG.</summary>
    static byte[] Fit(byte[] image)
    {
        using var input = new MemoryStream(image);
        using var src = System.Drawing.Image.FromStream(input);
        double scale = Math.Min(1, 1568.0 / Math.Max(src.Width, src.Height));
        int w = Math.Max(1, (int)(src.Width * scale)), h = Math.Max(1, (int)(src.Height * scale));
        using var dst = new System.Drawing.Bitmap(w, h);
        using (var g = System.Drawing.Graphics.FromImage(dst)) { g.Clear(System.Drawing.Color.White); g.InterpolationMode = System.Drawing.Drawing2D.InterpolationMode.HighQualityBicubic; g.DrawImage(src, 0, 0, w, h); }
        using var output = new MemoryStream();
        dst.Save(output, System.Drawing.Imaging.ImageFormat.Png);
        return output.ToArray();
    }
}
#endif
