using System.IO;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace BuzzPrint;

/// <summary>
/// The design of an exported document (PowerPoint, Word, PDF and the stitched image): colors, fonts, how the cover
/// and the step headings look, the frame around captures and the footer. Colors are six hex digits without "#".
/// </summary>
public sealed class DocTemplate
{
    public string Id { get; set; } = "";
    public string Name { get; set; } = "";
    public string Description { get; set; } = "";
    /// <summary>Hex, or "app" for the app's current accent.</summary>
    public string Accent { get; set; } = "app";
    public string Ink { get; set; } = "1F2328";
    public string Muted { get; set; } = "6B7280";
    public string Faint { get; set; } = "9CA3AF";
    public string Rule { get; set; } = "E5E7EB";
    /// <summary>Page / slide background.</summary>
    public string Page { get; set; } = "FFFFFF";
    /// <summary>Cover fill for "band" and "full" covers; empty = the accent.</summary>
    public string CoverFill { get; set; } = "";
    /// <summary>Text on a filled cover.</summary>
    public string CoverInk { get; set; } = "FFFFFF";
    /// <summary>"none", "accent" or a hex color.</summary>
    public string Frame { get; set; } = "D1D5DB";
    public string HeadingFont { get; set; } = "Segoe UI Semibold";
    public string BodyFont { get; set; } = "Segoe UI";
    /// <summary>bar (accent bar beside the title) · band (band on top) · full (whole cover in color) · minimal (centered) ·
    /// hero (title on the left, the first capture as a large image on the right).</summary>
    public string Cover { get; set; } = "bar";
    /// <summary>inline ("Step 1" in accent) · pill (filled label) · number (large "01").</summary>
    public string Steps { get; set; } = "inline";
    /// <summary>A thin accent rule at the top of every page.</summary>
    public bool TopRule { get; set; }
    public string Footer { get; set; } = "BuzzPrint";
    /// <summary>Organization / area line on the cover (bottom right) and in the footer bar.</summary>
    public string Org { get; set; } = "";
    /// <summary>Step headings: rule (text over a thin line) · band (a colored band across the top).</summary>
    public string Header { get; set; } = "rule";
    /// <summary>Footer: text (small print) · bar (a colored bar with the footer and the page number).</summary>
    public string FooterStyle { get; set; } = "text";
    /// <summary>PowerPoint steps: stack (heading, capture, note) · split (text column on the left, capture on the right).</summary>
    public string Layout { get; set; } = "stack";
    /// <summary>Logo image file name inside the templates folder (PNG); empty = none.</summary>
    public string Logo { get; set; } = "";

    [System.Text.Json.Serialization.JsonIgnore]
    public string? LogoPath => Logo.Length > 0 && File.Exists(Path.Combine(DocTemplates.Dir, Logo)) ? Path.Combine(DocTemplates.Dir, Logo) : null;

    [System.Text.Json.Serialization.JsonIgnore]
    public bool BuiltIn => DocTemplates.BuiltIns.Any(b => b.Id == Id);
    public string AccentHex(string appAccent) => Accent == "app" ? appAccent : Accent;
    public string CoverFillHex(string accent) => CoverFill.Length == 6 ? CoverFill : accent;
    public string FrameHex(string accent) => Frame == "accent" ? accent : Frame;
    public DocTemplate Copy() => JsonSerializer.Deserialize<DocTemplate>(JsonSerializer.Serialize(this))!;

    /// <summary>Fixes anything out of range (hand-edited files, AI output): bad colors fall back to the defaults.</summary>
    public DocTemplate Normalized()
    {
        var d = new DocTemplate();
        static string Hex(string? v, string fallback) { v = (v ?? "").Trim().TrimStart('#').ToUpperInvariant(); return Regex.IsMatch(v, "^[0-9A-F]{6}$") ? v : fallback; }
        var t = Copy();
        t.Name = (t.Name ?? "").Trim(); if (t.Name.Length == 0) t.Name = L.T("Plantilla");
        if (t.Name.Length > 40) t.Name = t.Name[..40];
        t.Description = (t.Description ?? "").Trim(); if (t.Description.Length > 120) t.Description = t.Description[..120];
        t.Accent = (t.Accent ?? "").Trim().Equals("app", StringComparison.OrdinalIgnoreCase) ? "app" : Hex(t.Accent, "app");
        t.Ink = Hex(t.Ink, d.Ink); t.Muted = Hex(t.Muted, d.Muted); t.Faint = Hex(t.Faint, d.Faint); t.Rule = Hex(t.Rule, d.Rule); t.Page = Hex(t.Page, d.Page);
        t.CoverFill = string.IsNullOrWhiteSpace(t.CoverFill) ? "" : Hex(t.CoverFill, "");
        t.CoverInk = Hex(t.CoverInk, d.CoverInk);
        t.Frame = (t.Frame ?? "").Trim().ToLowerInvariant() is "none" or "accent" ? t.Frame!.Trim().ToLowerInvariant() : Hex(t.Frame, d.Frame);
        t.HeadingFont = Font(t.HeadingFont, d.HeadingFont); t.BodyFont = Font(t.BodyFont, d.BodyFont);
        t.Cover = t.Cover is "bar" or "band" or "full" or "minimal" or "hero" ? t.Cover : d.Cover;
        t.Header = t.Header is "rule" or "band" ? t.Header : d.Header;
        t.FooterStyle = t.FooterStyle is "text" or "bar" ? t.FooterStyle : d.FooterStyle;
        t.Layout = t.Layout is "stack" or "split" ? t.Layout : d.Layout;
        t.Org = (t.Org ?? "").Trim(); if (t.Org.Length > 80) t.Org = t.Org[..80];
        t.Logo = Regex.IsMatch(t.Logo ?? "", @"^[\w\-]+\.png$") ? t.Logo! : "";
        t.Steps = t.Steps is "inline" or "pill" or "number" ? t.Steps : d.Steps;
        t.Footer = (t.Footer ?? "").Trim(); if (t.Footer.Length > 60) t.Footer = t.Footer[..60];
        return t;
    }
    static string Font(string? f, string fallback)
    {
        f = (f ?? "").Trim();
        return f.Length is > 0 and <= 40 && Regex.IsMatch(f, @"^[\p{L}\p{N} .\-]+$") ? f : fallback;
    }

    /// <summary>PDF has only the standard fonts: serif faces map to Times, monospace to Courier, the rest to Helvetica.</summary>
    public static string PdfFamily(string font)
    {
        string f = font.ToLowerInvariant();
        if (new[] { "consolas", "courier", "mono", "code", "cascadia" }.Any(f.Contains)) return "Courier";
        if (new[] { "georgia", "times", "garamond", "cambria", "serif", "book", "palatino", "baskerville", "constantia" }.Any(f.Contains) && !f.Contains("sans")) return "Times";
        return "Helvetica";
    }
}

/// <summary>Built-in designs plus up to <see cref="MaxCustom"/> of the person's own, stored as JSON in LocalAppData.</summary>
public static class DocTemplates
{
    public const int MaxCustom = 12;
    internal static string Dir => Path.Combine(DataFolder.Root, "templates");
    static readonly JsonSerializerOptions Json = new() { WriteIndented = true };

    public static readonly IReadOnlyList<DocTemplate> BuiltIns =
    [
        new() { Id = "buzzprint", Name = "BuzzPrint", Description = "Limpio, con una barra del color de acento.", Accent = "app" },
        new() { Id = "executive", Name = "Informe ejecutivo", Description = "Portada con imagen, banda de color en cada página y texto al costado de la captura.", Accent = "0A84C6", Ink = "13233A", Muted = "4A5A70", Faint = "8A97A8", Rule = "D7E0EA", CoverFill = "13233A", Frame = "none", HeadingFont = "Segoe UI Semibold", BodyFont = "Segoe UI", Cover = "hero", Header = "band", FooterStyle = "bar", Layout = "split", Steps = "pill", Footer = "", Org = "" },
        new() { Id = "consulting", Name = "Consultora", Description = "Sobrio y elegante: títulos con serif, pie en barra y detalles dorados.", Accent = "B7791F", Ink = "1F2933", Muted = "52606D", Faint = "9AA5B1", Rule = "E4E7EB", CoverFill = "1F2933", Frame = "D9DEE3", HeadingFont = "Georgia", BodyFont = "Segoe UI", Cover = "full", CoverInk = "FFFFFF", Header = "rule", FooterStyle = "bar", Layout = "stack", Steps = "number", TopRule = false },
        new() { Id = "institutional", Name = "Institucional", Description = "Verde azulado, banda en la portada y en los títulos, pie con número de página.", Accent = "0F766E", Ink = "102A2A", Muted = "4B6363", Faint = "8FA3A3", Rule = "D6E6E4", CoverFill = "0F766E", Frame = "C9DAD8", HeadingFont = "Segoe UI Semibold", BodyFont = "Segoe UI", Cover = "band", Header = "band", FooterStyle = "bar", Layout = "stack", Steps = "inline" },
        new() { Id = "techreport", Name = "Reporte técnico", Description = "Portada con imagen sobre fondo oscuro, títulos monoespaciados y vista lado a lado.", Accent = "22D3EE", Ink = "0B1220", Muted = "475569", Faint = "94A3B8", Rule = "E2E8F0", CoverFill = "0B1220", Frame = "0B1220", HeadingFont = "Consolas", BodyFont = "Segoe UI", Cover = "hero", Header = "rule", FooterStyle = "bar", Layout = "split", Steps = "pill", TopRule = true },
        new() { Id = "audit", Name = "Auditoría", Description = "Gris y rojo, números grandes por paso y pie formal; para hallazgos y controles.", Accent = "B91C1C", Ink = "111827", Muted = "4B5563", Faint = "9CA3AF", Rule = "E5E7EB", CoverFill = "111827", Frame = "D1D5DB", HeadingFont = "Arial", BodyFont = "Arial", Cover = "minimal", Header = "rule", FooterStyle = "bar", Layout = "stack", Steps = "number" },
        new() { Id = "presentation", Name = "Presentación", Description = "Blanco y azul, portada con imagen y diapositivas con texto a un lado.", Accent = "2563EB", Ink = "0F172A", Muted = "475569", Faint = "94A3B8", Rule = "E2E8F0", Frame = "E2E8F0", HeadingFont = "Segoe UI Semibold", BodyFont = "Segoe UI", Cover = "hero", Header = "rule", FooterStyle = "text", Layout = "split", Steps = "inline" },
        new() { Id = "corporate", Name = "Corporativo", Description = "Azul institucional, banda en la portada y pasos destacados.", Accent = "1E3A8A", Ink = "111827", Rule = "DBE3F0", Frame = "CBD5E1", HeadingFont = "Calibri", BodyFont = "Calibri", Cover = "band", Steps = "pill", TopRule = true },
        new() { Id = "minimal", Name = "Minimal", Description = "Blanco y negro, mucho aire, sin marcos.", Accent = "111827", Muted = "6B7280", Rule = "EEEEEE", Frame = "none", HeadingFont = "Segoe UI Semilight", BodyFont = "Segoe UI", Cover = "minimal", Steps = "number", Footer = "" },
        new() { Id = "editorial", Name = "Editorial", Description = "Tipografía con serif y papel cálido, como una revista.", Accent = "B45309", Ink = "1C1917", Muted = "78716C", Faint = "A8A29E", Rule = "E7DCCB", Page = "FFFBF5", Frame = "E7DCCB", HeadingFont = "Georgia", BodyFont = "Georgia", Cover = "minimal", Steps = "inline" },
        new() { Id = "technical", Name = "Técnico", Description = "Portada oscura, títulos monoespaciados, ideal para documentación.", Accent = "0EA5E9", Ink = "0F172A", Muted = "475569", Faint = "94A3B8", Rule = "E2E8F0", Frame = "94A3B8", HeadingFont = "Consolas", BodyFont = "Segoe UI", Cover = "full", CoverFill = "0F172A", CoverInk = "F8FAFC", Steps = "pill", TopRule = true },
        new() { Id = "vibrant", Name = "Vibrante", Description = "Portada a todo color y marcos del color de acento.", Accent = "7C3AED", Ink = "1F1235", Rule = "EDE9FE", Frame = "accent", HeadingFont = "Segoe UI Black", BodyFont = "Segoe UI", Cover = "full", CoverInk = "FFFFFF", Steps = "pill" },
    ];

    public static List<DocTemplate> Custom()
    {
        var list = new List<DocTemplate>();
        try
        {
            if (!Directory.Exists(Dir)) return list;
            foreach (var f in Directory.GetFiles(Dir, "*.json"))
            {
                try { if (JsonSerializer.Deserialize<DocTemplate>(File.ReadAllText(f)) is { } t) { t.Id = Path.GetFileNameWithoutExtension(f); list.Add(t.Normalized()); } }
                catch (JsonException) { }
            }
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }
        return list.OrderBy(t => t.Name, StringComparer.CurrentCultureIgnoreCase).ToList();
    }

    public static List<DocTemplate> All() => BuiltIns.Concat(Custom()).ToList();
    public static DocTemplate Get(string? id) => All().FirstOrDefault(t => t.Id == id) ?? BuiltIns[0];

    /// <summary>Saves a custom template (new when its id is empty or built in). Returns it with its id.</summary>
    public static DocTemplate Save(DocTemplate t)
    {
        t = t.Normalized();
        if (t.Id.Length == 0 || t.BuiltIn)
        {
            if (Custom().Count >= MaxCustom) throw new InvalidOperationException(L.F("Ya tenés {0} plantillas propias. Eliminá alguna para crear otra.", MaxCustom));
            t.Id = "custom-" + Guid.NewGuid().ToString("N")[..10];
        }
        Directory.CreateDirectory(Dir);
        File.WriteAllText(Path.Combine(Dir, t.Id + ".json"), JsonSerializer.Serialize(t, Json));
        return t;
    }

    /// <summary>Copies an image into the templates folder as PNG (at most 800 px wide) and returns its file name.</summary>
    public static string ImportLogo(string imagePath)
    {
        Directory.CreateDirectory(Dir);
        string name = "logo-" + Guid.NewGuid().ToString("N")[..10] + ".png";
        using var src = System.Drawing.Image.FromFile(imagePath);
        double scale = Math.Min(1, 800.0 / src.Width);
        int w = Math.Max(1, (int)(src.Width * scale)), h = Math.Max(1, (int)(src.Height * scale));
        using var dst = new System.Drawing.Bitmap(w, h, System.Drawing.Imaging.PixelFormat.Format32bppArgb);
        using (var g = System.Drawing.Graphics.FromImage(dst)) { g.InterpolationMode = System.Drawing.Drawing2D.InterpolationMode.HighQualityBicubic; g.DrawImage(src, 0, 0, w, h); }
        dst.Save(Path.Combine(Dir, name), System.Drawing.Imaging.ImageFormat.Png);
        return name;
    }

    public static void Delete(string id)
    {
        if (BuiltIns.Any(b => b.Id == id)) return;
        var t = Custom().FirstOrDefault(x => x.Id == id);
        if (t?.LogoPath is string logo) try { File.Delete(logo); } catch (IOException) { }
        try { File.Delete(Path.Combine(Dir, id + ".json")); } catch (IOException) { }
    }
}
