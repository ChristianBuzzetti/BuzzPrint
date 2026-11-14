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
    /// <summary>bar (accent bar beside the title) · band (accent band on top) · full (whole cover in color) · minimal (centered).</summary>
    public string Cover { get; set; } = "bar";
    /// <summary>inline ("Step 1" in accent) · pill (filled label) · number (large "01").</summary>
    public string Steps { get; set; } = "inline";
    /// <summary>A thin accent rule at the top of every page.</summary>
    public bool TopRule { get; set; }
    public string Footer { get; set; } = "BuzzPrint";

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
        t.Cover = t.Cover is "bar" or "band" or "full" or "minimal" ? t.Cover : d.Cover;
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
    static string Dir => Path.Combine(DataFolder.Root, "templates");
    static readonly JsonSerializerOptions Json = new() { WriteIndented = true };

    public static readonly IReadOnlyList<DocTemplate> BuiltIns =
    [
        new() { Id = "buzzprint", Name = "BuzzPrint", Description = "Limpio, con una barra del color de acento.", Accent = "app" },
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

    public static void Delete(string id)
    {
        if (BuiltIns.Any(b => b.Id == id)) return;
        try { File.Delete(Path.Combine(Dir, id + ".json")); } catch (IOException) { }
    }
}
