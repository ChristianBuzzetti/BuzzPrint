using System.Globalization;
using System.IO;
using System.IO.Compression;
using System.Security;
using System.Text;
using D = System.Drawing;
using DI = System.Drawing.Imaging;

namespace BuzzPrint;

public enum ExportFormat { Pptx, Docx, Pdf, Png }

/// <summary>One capture in the exported document, in document order.</summary>
public sealed class ExportPage
{
    public required CaptureItem Item { get; init; }
    public string Title { get; set; } = "";
    public string Note { get; set; } = "";
}

public sealed class ExportOptions
{
    public string Title = "";
    public string Intro = "";   // optional paragraph under the title (cover); written by hand or by AI
    public bool Cover = true, Numbers = true, Captions = true, Meta = true, OnePerPage = true, Landscape;
    public string Accent = "F43F5E";
    /// <summary>Design of the document; <see cref="Accent"/> is its resolved accent.</summary>
    public DocTemplate T = DocTemplates.BuiltIns[0];
}

/// <summary>
/// Writes captures, in order, to PowerPoint, Word, PDF or one tall PNG. The Office files are plain Open XML packages
/// and the PDF is written by hand (lossless Flate images, standard Helvetica), so nothing beyond .NET is needed.
/// </summary>
public static class Exporter
{
    public static string Extension(ExportFormat f) => f switch { ExportFormat.Pptx => ".pptx", ExportFormat.Docx => ".docx", ExportFormat.Pdf => ".pdf", _ => ".png" };

    public static void Write(string path, ExportFormat format, IReadOnlyList<ExportPage> pages, ExportOptions o)
    {
        if (pages.Count == 0) throw new InvalidOperationException("Nothing to export.");
        cur = o.T.Normalized();
        string temp = path + ".tmp";
        try
        {
            using (var fs = File.Create(temp))
            {
                switch (format)
                {
                    case ExportFormat.Pptx: Pptx(fs, pages, o); break;
                    case ExportFormat.Docx: Docx(fs, pages, o); break;
                    case ExportFormat.Pdf: Pdf(fs, pages, o); break;
                    default: using (var bmp = Stitch(pages, o)) bmp.Save(fs, DI.ImageFormat.Png); break;
                }
            }
            File.Move(temp, path, true);
        }
        finally { try { File.Delete(temp); } catch (IOException) { } cur = null; }
    }

    [ThreadStatic] static DocTemplate? cur;
    static DocTemplate Tp => cur ?? DocTemplates.BuiltIns[0];
    /// <summary>Picture outline (Office): none, the accent or a color.</summary>
    static string Ln(string accent, int width) => Tp.Frame == "none" ? "<a:ln><a:noFill/></a:ln>" : "<a:ln w=\"" + width + "\"><a:solidFill><a:srgbClr val=\"" + Tp.FrameHex(accent) + "\"/></a:solidFill></a:ln>";
    static string StepLabel(int i) => L.T("Paso") + " " + (i + 1);

    // ------------------------------------------------------------------ shared text
    static string Caption(ExportPage p, int i, ExportOptions o, bool numbersInline = true)
    {
        string t = o.Captions ? p.Title.Trim() : "";
        if (!o.Numbers || !numbersInline) return t;
        string step = L.T("Paso") + " " + (i + 1);
        return t.Length > 0 ? step + " — " + t : step;
    }
    static string MetaLine(ExportPage p)
    {
        var parts = new List<string>();
        if (!string.IsNullOrWhiteSpace(p.Item.Source) && p.Item.Source != p.Title) parts.Add(p.Item.Source.Trim());
        parts.Add(p.Item.Created.ToString("g", L.Culture));
        if (p.Item.Width > 0) parts.Add(p.Item.Width + "×" + p.Item.Height);
        return string.Join("  ·  ", parts);
    }
    static string Subtitle(int count) => L.Count(count, "captura", "capturas") + "  ·  " + DateTime.Now.ToString("D", L.Culture);
    static string X(string s) => SecurityElement.Escape(s ?? "") ?? "";
    static (int w, int h) Size(CaptureItem it)
    {
        if (it.Width > 0 && it.Height > 0) return (it.Width, it.Height);
        using var b = Store.LoadBitmap(it.Path);
        return (b.Width, b.Height);
    }
    static (long w, long h) Fit(long iw, long ih, long maxW, long maxH)
    {
        double s = Math.Min((double)maxW / iw, (double)maxH / ih);
        return ((long)(iw * s), (long)(ih * s));
    }
    static void Entry(ZipArchive zip, string name, string xml)
    {
        using var w = new StreamWriter(zip.CreateEntry(name, CompressionLevel.Optimal).Open(), new UTF8Encoding(false));
        w.Write("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
        w.Write(xml);
    }
    static void Entry(ZipArchive zip, string name, byte[] data)
    {
        using var s = zip.CreateEntry(name, CompressionLevel.NoCompression).Open();
        s.Write(data);
    }
    static byte[] Png(CaptureItem it)
    {
        if (string.Equals(Path.GetExtension(it.Path), ".png", StringComparison.OrdinalIgnoreCase)) return File.ReadAllBytes(it.Path);
        using var b = Store.LoadBitmap(it.Path);
        using var ms = new MemoryStream();
        b.Save(ms, DI.ImageFormat.Png);
        return ms.ToArray();
    }
    static void Props(ZipArchive zip, string title, string app)
    {
        string now = DateTime.UtcNow.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'");
        Entry(zip, "docProps/core.xml", "<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">" +
            "<dc:title>" + X(title) + "</dc:title><dc:creator>BuzzPrint</dc:creator><dcterms:created xsi:type=\"dcterms:W3CDTF\">" + now + "</dcterms:created><dcterms:modified xsi:type=\"dcterms:W3CDTF\">" + now + "</dcterms:modified></cp:coreProperties>");
        Entry(zip, "docProps/app.xml", "<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>" + app + "</Application></Properties>");
    }
    const string RootRelsHead = "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">";
    const string RelCore = "<Relationship Id=\"rIdCore\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/><Relationship Id=\"rIdApp\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties\" Target=\"docProps/app.xml\"/>";
    const string CtProps = "<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/><Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>";
    const string NsA = "http://schemas.openxmlformats.org/drawingml/2006/main", NsR = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

    // ------------------------------------------------------------------ PowerPoint (16:9, one capture per slide)
    const long SlideW = 12192000, SlideH = 6858000, In = 914400, Pt = 12700;

    static void Pptx(Stream output, IReadOnlyList<ExportPage> pages, ExportOptions o)
    {
        using var zip = new ZipArchive(output, ZipArchiveMode.Create, true);
        int slides = pages.Count + (o.Cover ? 1 : 0);
        var ct = new StringBuilder("<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/>" +
            "<Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/presProps.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presProps+xml\"/><Override PartName=\"/ppt/viewProps.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml\"/><Override PartName=\"/ppt/tableStyles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml\"/>" +
            "<Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/><Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/><Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>" + CtProps);
        for (int i = 1; i <= slides; i++) ct.Append("<Override PartName=\"/ppt/slides/slide" + i + ".xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>");
        Entry(zip, "[Content_Types].xml", ct.Append("</Types>").ToString());
        Entry(zip, "_rels/.rels", RootRelsHead + "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"ppt/presentation.xml\"/>" + RelCore + "</Relationships>");
        Props(zip, o.Title, "BuzzPrint");

        var pres = new StringBuilder("<p:presentation xmlns:a=\"" + NsA + "\" xmlns:r=\"" + NsR + "\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" saveSubsetFonts=\"1\"><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst><p:sldIdLst>");
        var presRels = new StringBuilder(RootRelsHead + "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster\" Target=\"slideMasters/slideMaster1.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"theme/theme1.xml\"/><Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/presProps\" Target=\"presProps.xml\"/><Relationship Id=\"rId4\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/viewProps\" Target=\"viewProps.xml\"/><Relationship Id=\"rId5\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/tableStyles\" Target=\"tableStyles.xml\"/>");
        for (int i = 1; i <= slides; i++)
        {
            pres.Append("<p:sldId id=\"" + (255 + i) + "\" r:id=\"rIdS" + i + "\"/>");
            presRels.Append("<Relationship Id=\"rIdS" + i + "\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide" + i + ".xml\"/>");
        }
        pres.Append("</p:sldIdLst><p:sldSz cx=\"" + SlideW + "\" cy=\"" + SlideH + "\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>");
        Entry(zip, "ppt/presentation.xml", pres.ToString());
        Entry(zip, "ppt/_rels/presentation.xml.rels", presRels.Append("</Relationships>").ToString());
        Entry(zip, "ppt/presProps.xml", "<p:presentationPr xmlns:a=\"" + NsA + "\" xmlns:r=\"" + NsR + "\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"/>");
        Entry(zip, "ppt/viewProps.xml", "<p:viewPr xmlns:a=\"" + NsA + "\" xmlns:r=\"" + NsR + "\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:normalViewPr><p:restoredLeft sz=\"15620\"/><p:restoredTop sz=\"94660\"/></p:normalViewPr><p:gridSpacing cx=\"76200\" cy=\"76200\"/></p:viewPr>");
        Entry(zip, "ppt/tableStyles.xml", "<a:tblStyleLst xmlns:a=\"" + NsA + "\" def=\"{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}\"/>");
        string grp = "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>";
        string pNs = "xmlns:a=\"" + NsA + "\" xmlns:r=\"" + NsR + "\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";
        Entry(zip, "ppt/slideMasters/slideMaster1.xml", "<p:sldMaster " + pNs + "><p:cSld><p:bg><p:bgRef idx=\"1001\"><a:schemeClr val=\"bg1\"/></p:bgRef></p:bg><p:spTree>" + grp + "</p:spTree></p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz=\"3200\"><a:latin typeface=\"+mj-lt\"/></a:defRPr></a:lvl1pPr></p:titleStyle><p:bodyStyle><a:lvl1pPr><a:defRPr sz=\"1800\"><a:latin typeface=\"+mn-lt\"/></a:defRPr></a:lvl1pPr></p:bodyStyle><p:otherStyle><a:lvl1pPr><a:defRPr sz=\"1800\"><a:latin typeface=\"+mn-lt\"/></a:defRPr></a:lvl1pPr></p:otherStyle></p:txStyles></p:sldMaster>");
        Entry(zip, "ppt/slideMasters/_rels/slideMaster1.xml.rels", RootRelsHead + "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"../theme/theme1.xml\"/></Relationships>");
        Entry(zip, "ppt/slideLayouts/slideLayout1.xml", "<p:sldLayout " + pNs + " type=\"blank\" preserve=\"1\"><p:cSld name=\"Blank\"><p:spTree>" + grp + "</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>");
        Entry(zip, "ppt/slideLayouts/_rels/slideLayout1.xml.rels", RootRelsHead + "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster\" Target=\"../slideMasters/slideMaster1.xml\"/></Relationships>");
        Entry(zip, "ppt/theme/theme1.xml", Theme(o.Accent));

        int n = 0;
        string bg = Tp.Page == "FFFFFF" ? "" : "<p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"" + Tp.Page + "\"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>";
        string Slide(string body) => "<p:sld " + pNs + "><p:cSld>" + bg + "<p:spTree>" + grp + body + "</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>";
        void Rels(int slide, string? image) => Entry(zip, "ppt/slides/_rels/slide" + slide + ".xml.rels", RootRelsHead + "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/>" +
            (image != null ? "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/" + image + "\"/>" : "") + "</Relationships>");
        const long M = In / 2;
        if (o.Cover)
        {
            n++;
            var body = new StringBuilder();
            int id = 2;
            string fill = Tp.CoverFillHex(o.Accent), cink = Tp.CoverInk, intro = o.Intro.Trim();
            long tx = M + In / 4, tw = SlideW - 2 * M - In / 4;
            string footColor = Tp.Faint;
            switch (Tp.Cover)
            {
                case "band":
                    body.Append(Rect(id++, 0, 0, SlideW, (long)(In * 3.2), fill));
                    body.Append(Text(id++, tx, (long)(In * 1.15), tw, (long)(In * 1.2), 4000, true, cink, o.Title, "b"));
                    body.Append(Text(id++, tx, (long)(In * 2.45), tw, (long)(In * .5), 1600, false, cink, Subtitle(pages.Count), "t"));
                    if (intro.Length > 0) body.Append(Text(id++, tx, (long)(In * 3.6), tw, (long)(In * 2.4), 1300, false, Tp.Muted, intro, "t"));
                    break;
                case "full":
                    body.Append(Rect(id++, 0, 0, SlideW, SlideH, fill));
                    body.Append(Rect(id++, tx, (long)(In * 2.0), (long)(In * .9), Pt * 4, o.Accent == fill ? cink : o.Accent));
                    body.Append(Text(id++, tx, (long)(In * 2.2), tw, (long)(In * 1.1), 4200, true, cink, o.Title, "b"));
                    body.Append(Text(id++, tx, (long)(In * 3.42), tw, (long)(In * .5), 1600, false, cink, Subtitle(pages.Count), "t"));
                    if (intro.Length > 0) body.Append(Text(id++, tx, (long)(In * 4.0), tw, (long)(In * 2.2), 1300, false, cink, intro, "t"));
                    footColor = cink;
                    break;
                case "minimal":
                    body.Append(Text(id++, M, (long)(In * 2.1), SlideW - 2 * M, (long)(In * 1.2), 4000, true, Tp.Ink, o.Title, "b", "ctr"));
                    body.Append(Rect(id++, SlideW / 2 - In / 3, (long)(In * 3.42), In * 2 / 3, Pt * 2, o.Accent));
                    body.Append(Text(id++, M, (long)(In * 3.6), SlideW - 2 * M, (long)(In * .5), 1500, false, Tp.Muted, Subtitle(pages.Count), "t", "ctr"));
                    if (intro.Length > 0) body.Append(Text(id++, M + In, (long)(In * 4.25), SlideW - 2 * M - 2 * In, (long)(In * 2.0), 1300, false, Tp.Muted, intro, "t", "ctr"));
                    break;
                default:
                    body.Append(Rect(id++, M, (long)(In * 2.35), (long)(Pt * 6), (long)(In * 1.55), o.Accent));
                    body.Append(Text(id++, tx, (long)(In * 2.2), tw, (long)(In * 1.1), 4000, true, Tp.Ink, o.Title, "b"));
                    body.Append(Text(id++, tx, (long)(In * 3.42), tw, (long)(In * 0.5), 1600, false, Tp.Muted, Subtitle(pages.Count), "t"));
                    if (intro.Length > 0) body.Append(Text(id++, tx, (long)(In * 4.0), tw, (long)(In * 2.2), 1300, false, Tp.Muted, intro, "t"));
                    break;
            }
            if (Tp.Footer.Length > 0) body.Append(Text(id++, M, SlideH - M - (long)(In * .3), SlideW - 2 * M, (long)(In * .3), 1000, false, footColor, Tp.Footer, "b"));
            Entry(zip, "ppt/slides/slide" + n + ".xml", Slide(body.ToString()));
            Rels(n, null);
        }
        for (int i = 0; i < pages.Count; i++)
        {
            n++;
            var p = pages[i];
            string image = "image" + (i + 1) + ".png";
            Entry(zip, "ppt/media/" + image, Png(p.Item));
            var (iw, ih) = Size(p.Item);
            var body = new StringBuilder();
            int id = 2;
            string title = o.Captions ? p.Title.Trim() : "";
            bool head = o.Numbers || title.Length > 0;
            long top = M;
            if (Tp.TopRule) body.Append(Rect(id++, 0, 0, SlideW, Pt * 5, o.Accent));
            if (head)
            {
                // Header: the step (inline text, a filled label or a large number) and the caption, on one line.
                long hx = M;
                if (o.Numbers && Tp.Steps == "pill")
                {
                    string label = StepLabel(i);
                    long pw = (long)(In * (0.42 + 0.105 * label.Length));
                    body.Append(Pill(id++, M, top + (long)(In * .07), pw, (long)(In * .42), label, o.Accent, "FFFFFF"));
                    hx = M + pw + In / 6;
                }
                else if (o.Numbers && Tp.Steps == "number")
                {
                    body.Append(Text(id++, M, top - (long)(In * .12), (long)(In * 1.0), (long)(In * .75), 3400, true, o.Accent, (i + 1).ToString("00"), "ctr"));
                    hx = M + (long)(In * 1.0);
                }
                var runs = new StringBuilder("<a:p>");
                if (o.Numbers && Tp.Steps == "inline") runs.Append(Run(StepLabel(i) + (title.Length > 0 ? "   " : ""), 2000, true, o.Accent));
                if (title.Length > 0) runs.Append(Run(title, 2000, true, Tp.Ink));
                runs.Append("</a:p>");
                body.Append(Shape(id++, hx, top, SlideW - M - hx, (long)(In * .55), runs.ToString(), "ctr"));
                body.Append(Rect(id++, M, top + (long)(In * .62), SlideW - 2 * M, Pt, Tp.Rule));
                top += (long)(In * .8);
            }
            string note = p.Note.Trim();
            long footer = (long)(In * .4), noteH = note.Length > 0 ? (long)(In * (note.Length > 160 ? 1.0 : .62)) : 0;
            long areaH = SlideH - top - M / 2 - footer - noteH, areaW = SlideW - 2 * M;
            var (w, h) = Fit(iw, ih, areaW, areaH);
            if (w > iw * In / 96 * 2) (w, h) = (iw * In / 96 * 2, ih * In / 96 * 2);   // tiny captures are not blown up past 2×
            body.Append(Pic(id++, M + (areaW - w) / 2, top + (areaH - h) / 2, w, h, p.Title, o.Accent));
            if (noteH > 0) body.Append(Text(id++, M, top + areaH + (long)(In * .08), areaW, noteH - (long)(In * .08), 1400, false, Tp.Ink, note, "t"));
            string meta = (o.Meta ? MetaLine(p) : "");
            body.Append(Text(id++, M, SlideH - M / 2 - footer + (long)(In * .1), areaW - In, footer - (long)(In * .1), 1000, false, Tp.Faint, meta, "ctr"));
            body.Append(Text(id++, SlideW - M - In, SlideH - M / 2 - footer + (long)(In * .1), In, footer - (long)(In * .1), 1000, false, Tp.Faint, (i + 1) + " / " + pages.Count, "ctr", "r"));
            Entry(zip, "ppt/slides/slide" + n + ".xml", Slide(body.ToString()));
            Rels(n, image);
        }
    }

    static string Xfrm(long x, long y, long w, long h) => "<a:xfrm><a:off x=\"" + x + "\" y=\"" + y + "\"/><a:ext cx=\"" + Math.Max(w, 1) + "\" cy=\"" + Math.Max(h, 1) + "\"/></a:xfrm>";
    static string Run(string text, int size, bool bold, string color) =>
        "<a:r><a:rPr lang=\"" + (L.Es ? "es-AR" : "en-US") + "\" sz=\"" + size + "\"" + (bold ? " b=\"1\"" : "") + " dirty=\"0\"><a:solidFill><a:srgbClr val=\"" + color + "\"/></a:solidFill><a:latin typeface=\"" + X(bold ? Tp.HeadingFont : Tp.BodyFont) + "\"/><a:cs typeface=\"" + X(bold ? Tp.HeadingFont : Tp.BodyFont) + "\"/></a:rPr><a:t>" + X(text) + "</a:t></a:r>";
    static string Shape(int id, long x, long y, long w, long h, string paragraphs, string anchor) =>
        "<p:sp><p:nvSpPr><p:cNvPr id=\"" + id + "\" name=\"Text " + id + "\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr>" + Xfrm(x, y, w, h) + "<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/></p:spPr>" +
        "<p:txBody><a:bodyPr wrap=\"square\" lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\" anchor=\"" + anchor + "\"><a:normAutofit/></a:bodyPr><a:lstStyle/>" + paragraphs + "</p:txBody></p:sp>";
    static string Text(int id, long x, long y, long w, long h, int size, bool bold, string color, string text, string anchor, string align = "l")
    {
        var ps = new StringBuilder();
        foreach (var line in (text ?? "").Replace("\r", "").Split('\n'))
            ps.Append("<a:p><a:pPr algn=\"" + align + "\"/>" + (line.Length > 0 ? Run(line, size, bold, color) : "") + "<a:endParaRPr lang=\"es-AR\" sz=\"" + size + "\" dirty=\"0\"/></a:p>");
        return Shape(id, x, y, w, h, ps.ToString(), anchor);
    }
    static string Rect(int id, long x, long y, long w, long h, string color) =>
        "<p:sp><p:nvSpPr><p:cNvPr id=\"" + id + "\" name=\"Rule " + id + "\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>" + Xfrm(x, y, w, h) + "<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val=\"" + color + "\"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr></p:sp>";
    static string Pill(int id, long x, long y, long w, long h, string text, string fill, string color) =>
        "<p:sp><p:nvSpPr><p:cNvPr id=\"" + id + "\" name=\"Step " + id + "\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>" + Xfrm(x, y, w, h) + "<a:prstGeom prst=\"roundRect\"><a:avLst><a:gd name=\"adj\" fmla=\"val 50000\"/></a:avLst></a:prstGeom><a:solidFill><a:srgbClr val=\"" + fill + "\"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr>" +
        "<p:txBody><a:bodyPr wrap=\"none\" lIns=\"45720\" tIns=\"0\" rIns=\"45720\" bIns=\"0\" anchor=\"ctr\"/><a:lstStyle/><a:p><a:pPr algn=\"ctr\"/>" + Run(text, 1400, true, color) + "</a:p></p:txBody></p:sp>";
    static string Pic(int id, long x, long y, long w, long h, string descr, string accent) =>
        "<p:pic><p:nvPicPr><p:cNvPr id=\"" + id + "\" name=\"Capture " + id + "\" descr=\"" + X(descr) + "\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"rId2\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>" +
        "<p:spPr>" + Xfrm(x, y, w, h) + "<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>" + Ln(accent, 9525) + "</p:spPr></p:pic>";

    static string Theme(string accent)
    {
        string Sys(string n, string v) => "<a:" + n + "><a:srgbClr val=\"" + v + "\"/></a:" + n + ">";
        string fill = "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>";
        string line = "<a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln>";
        return "<a:theme xmlns:a=\"" + NsA + "\" name=\"BuzzPrint\"><a:themeElements><a:clrScheme name=\"BuzzPrint\">" +
            Sys("dk1", Tp.Ink) + Sys("lt1", "FFFFFF") + Sys("dk2", "374151") + Sys("lt2", "F3F4F6") + Sys("accent1", accent) + Sys("accent2", "2684FF") + Sys("accent3", "10B981") +
            Sys("accent4", "F59E0B") + Sys("accent5", "8B5CF6") + Sys("accent6", "6B7280") + Sys("hlink", "2684FF") + Sys("folHlink", "8B5CF6") + "</a:clrScheme>" +
            "<a:fontScheme name=\"BuzzPrint\"><a:majorFont><a:latin typeface=\"" + X(Tp.HeadingFont) + "\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont><a:minorFont><a:latin typeface=\"" + X(Tp.BodyFont) + "\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont></a:fontScheme>" +
            "<a:fmtScheme name=\"BuzzPrint\"><a:fillStyleLst>" + fill + fill + fill + "</a:fillStyleLst><a:lnStyleLst>" + line + line + line + "</a:lnStyleLst>" +
            "<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>" +
            "<a:bgFillStyleLst>" + fill + fill + fill + "</a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>";
    }

    // ------------------------------------------------------------------ Word (A4, headings in the navigation pane)
    static void Docx(Stream output, IReadOnlyList<ExportPage> pages, ExportOptions o)
    {
        using var zip = new ZipArchive(output, ZipArchiveMode.Create, true);
        Entry(zip, "[Content_Types].xml", "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/>" +
            "<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/><Override PartName=\"/word/footer1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml\"/><Override PartName=\"/word/settings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml\"/>" + CtProps + "</Types>");
        Entry(zip, "word/settings.xml", "<w:settings xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">" + (Tp.Page != "FFFFFF" ? "<w:displayBackgroundShape/>" : "") + "</w:settings>");
        Entry(zip, "_rels/.rels", RootRelsHead + "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>" + RelCore + "</Relationships>");
        Props(zip, o.Title, "BuzzPrint");
        string lang = L.Es ? "es-AR" : "en-US";
        string hf = X(Tp.HeadingFont), bf = X(Tp.BodyFont), fill = Tp.CoverFillHex(o.Accent);
        bool filled = Tp.Cover is "band" or "full";
        string titlePPr = Tp.Cover switch
        {
            "band" or "full" => "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"" + fill + "\"/><w:spacing w:before=\"" + (Tp.Cover == "full" ? 2400 : 0) + "\" w:after=\"0\"/><w:ind w:left=\"0\"/>",
            "minimal" => "<w:pBdr><w:bottom w:val=\"single\" w:sz=\"8\" w:space=\"10\" w:color=\"" + o.Accent + "\"/></w:pBdr><w:jc w:val=\"center\"/><w:spacing w:before=\"1800\" w:after=\"160\"/>",
            _ => "<w:pBdr><w:left w:val=\"single\" w:sz=\"36\" w:space=\"12\" w:color=\"" + o.Accent + "\"/></w:pBdr><w:spacing w:after=\"60\"/><w:ind w:left=\"240\"/>",
        };
        string subPPr = Tp.Cover switch
        {
            "band" or "full" => "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"" + fill + "\"/><w:spacing w:after=\"480\"/>",
            "minimal" => "<w:jc w:val=\"center\"/><w:spacing w:after=\"480\"/>",
            _ => "<w:spacing w:after=\"480\"/><w:ind w:left=\"240\"/>",
        };
        string titleInk = filled ? Tp.CoverInk : Tp.Ink, subInk = filled ? Tp.CoverInk : Tp.Muted;
        string h2Rule = Tp.TopRule ? "<w:pBdr><w:top w:val=\"single\" w:sz=\"18\" w:space=\"8\" w:color=\"" + o.Accent + "\"/></w:pBdr>" : "";
        Entry(zip, "word/styles.xml", "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"" + bf + "\" w:hAnsi=\"" + bf + "\" w:cs=\"" + bf + "\"/><w:sz w:val=\"21\"/><w:szCs w:val=\"21\"/><w:lang w:val=\"" + lang + "\"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after=\"120\" w:line=\"276\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>" +
            "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/><w:rPr><w:color w:val=\"" + Tp.Ink + "\"/></w:rPr></w:style>" +
            "<w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr>" + titlePPr + "</w:pPr><w:rPr><w:rFonts w:ascii=\"" + hf + "\" w:hAnsi=\"" + hf + "\"/><w:color w:val=\"" + titleInk + "\"/><w:sz w:val=\"52\"/><w:szCs w:val=\"52\"/></w:rPr></w:style>" +
            "<w:style w:type=\"paragraph\" w:styleId=\"Subtitle\"><w:name w:val=\"Subtitle\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr>" + subPPr + "</w:pPr><w:rPr><w:color w:val=\"" + subInk + "\"/><w:sz w:val=\"24\"/></w:rPr></w:style>" +
            "<w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr><w:keepNext/>" + h2Rule + "<w:spacing w:before=\"240\" w:after=\"120\"/><w:outlineLvl w:val=\"1\"/></w:pPr><w:rPr><w:rFonts w:ascii=\"" + hf + "\" w:hAnsi=\"" + hf + "\"/><w:color w:val=\"" + Tp.Ink + "\"/><w:sz w:val=\"30\"/><w:szCs w:val=\"30\"/></w:rPr></w:style>" +
            "<w:style w:type=\"paragraph\" w:styleId=\"Caption\"><w:name w:val=\"caption\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/><w:pPr><w:spacing w:before=\"60\" w:after=\"160\"/></w:pPr><w:rPr><w:color w:val=\"" + Tp.Faint + "\"/><w:sz w:val=\"17\"/></w:rPr></w:style>" +
            "<w:style w:type=\"paragraph\" w:styleId=\"Footer\"><w:name w:val=\"footer\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:spacing w:after=\"0\"/></w:pPr><w:rPr><w:color w:val=\"" + Tp.Faint + "\"/><w:sz w:val=\"16\"/></w:rPr></w:style></w:styles>");
        Entry(zip, "word/footer1.xml", "<w:ftr xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:p><w:pPr><w:pStyle w:val=\"Footer\"/><w:tabs><w:tab w:val=\"right\" w:pos=\"" + (o.Landscape ? 14570 : 9638) + "\"/></w:tabs></w:pPr><w:r><w:t xml:space=\"preserve\">" + X(Tp.Footer.Length > 0 ? Tp.Footer + "  ·  " + o.Title : o.Title) + "</w:t></w:r><w:r><w:tab/></w:r>" +
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>" +
            "<w:r><w:t xml:space=\"preserve\"> / </w:t></w:r><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> NUMPAGES </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p></w:ftr>");

        // A4 with 2 cm margins; landscape swaps the sides.
        int pageW = o.Landscape ? 16838 : 11906, pageH = o.Landscape ? 11906 : 16838, margin = 1134;
        long textW = (pageW - 2 * margin) * 635L, maxH = (pageH - 2 * margin - 2600) * 635L;   // room for heading, caption and footer
        var rels = new StringBuilder(RootRelsHead + "<Relationship Id=\"rIdStyles\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/><Relationship Id=\"rIdFooter\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer\" Target=\"footer1.xml\"/><Relationship Id=\"rIdSettings\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings\" Target=\"settings.xml\"/>");
        var body = new StringBuilder();
        string P(string style, string runs, string extra = "") => "<w:p><w:pPr><w:pStyle w:val=\"" + style + "\"/>" + extra + "</w:pPr>" + runs + "</w:p>";
        string R(string text, string? color = null, bool bold = false) => "<w:r>" + (color != null || bold ? "<w:rPr>" + (bold ? "<w:b/>" : "") + (color != null ? "<w:color w:val=\"" + color + "\"/>" : "") + "</w:rPr>" : "") + "<w:t xml:space=\"preserve\">" + X(text) + "</w:t></w:r>";
        string Lines(string text) => string.Join("<w:r><w:br/></w:r>", text.Replace("\r", "").Split('\n').Select(l => R(l)));
        if (o.Cover)
        {
            body.Append(P("Title", R(o.Title)));
            body.Append(P("Subtitle", R(Subtitle(pages.Count))));
            if (o.Intro.Trim().Length > 0) body.Append("<w:p>" + Lines(o.Intro.Trim()) + "</w:p>");
        }
        for (int i = 0; i < pages.Count; i++)
        {
            var p = pages[i];
            string rid = "rIdImg" + (i + 1), file = "image" + (i + 1) + ".png";
            Entry(zip, "word/media/" + file, Png(p.Item));
            rels.Append("<Relationship Id=\"" + rid + "\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/" + file + "\"/>");
            bool breakBefore = o.OnePerPage && (i > 0 || o.Cover);
            string title = o.Captions ? p.Title.Trim() : "";
            string pageBreak = breakBefore ? "<w:pageBreakBefore/>" : "";
            if (o.Numbers || title.Length > 0)
            {
                string step = !o.Numbers ? "" : Tp.Steps switch
                {
                    "pill" => "<w:r><w:rPr><w:b/><w:color w:val=\"FFFFFF\"/><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"" + o.Accent + "\"/></w:rPr><w:t xml:space=\"preserve\"> " + X(StepLabel(i)) + " </w:t></w:r>" + R("  "),
                    "number" => "<w:r><w:rPr><w:color w:val=\"" + o.Accent + "\"/><w:sz w:val=\"48\"/><w:szCs w:val=\"48\"/></w:rPr><w:t xml:space=\"preserve\">" + (i + 1).ToString("00") + "  </w:t></w:r>",
                    _ => R(StepLabel(i) + (title.Length > 0 ? "  " : ""), o.Accent),
                };
                string runs = step + (title.Length > 0 ? R(title) : "");
                body.Append(P("Heading2", runs, pageBreak));
                pageBreak = "";
            }
            var (iw, ih) = Size(p.Item);
            var (w, h) = Fit(iw, ih, textW, maxH);
            if (w > iw * 9525L) (w, h) = (iw * 9525L, ih * 9525L);   // never upscale past 100 % at 96 dpi
            body.Append("<w:p><w:pPr>" + pageBreak + "<w:spacing w:after=\"0\"/><w:jc w:val=\"center\"/></w:pPr><w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"" + w + "\" cy=\"" + h + "\"/><wp:docPr id=\"" + (i + 1) + "\" name=\"Capture " + (i + 1) + "\" descr=\"" + X(p.Title) + "\"/>" +
                "<wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect=\"1\"/></wp:cNvGraphicFramePr><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"" + (i + 1) + "\" name=\"" + file + "\"/><pic:cNvPicPr/></pic:nvPicPr>" +
                "<pic:blipFill><a:blip r:embed=\"" + rid + "\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"" + w + "\" cy=\"" + h + "\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>" + Ln(o.Accent, 6350) + "</pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>");
            if (o.Meta) body.Append(P("Caption", R(MetaLine(p)), "<w:jc w:val=\"center\"/>"));
            else body.Append(P("Caption", ""));
            if (p.Note.Trim().Length > 0) body.Append(P("Normal", Lines(p.Note.Trim())));
        }
        body.Append("<w:sectPr><w:footerReference w:type=\"default\" r:id=\"rIdFooter\"/><w:pgSz w:w=\"" + pageW + "\" w:h=\"" + pageH + "\"" + (o.Landscape ? " w:orient=\"landscape\"" : "") + "/><w:pgMar w:top=\"" + margin + "\" w:right=\"" + margin + "\" w:bottom=\"" + margin + "\" w:left=\"" + margin + "\" w:header=\"567\" w:footer=\"567\" w:gutter=\"0\"/></w:sectPr>");
        Entry(zip, "word/document.xml", "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"" + NsR + "\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"" + NsA + "\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">" + (Tp.Page != "FFFFFF" ? "<w:background w:color=\"" + Tp.Page + "\"/>" : "") + "<w:body>" + body + "</w:body></w:document>");
        Entry(zip, "word/_rels/document.xml.rels", rels.Append("</Relationships>").ToString());
    }

    // ------------------------------------------------------------------ PDF (A4, lossless images, Helvetica)
    static void Pdf(Stream output, IReadOnlyList<ExportPage> pages, ExportOptions o)
    {
        var pdf = new PdfWriter(output);
        double W = o.Landscape ? 842 : 595, H = o.Landscape ? 595 : 842, M = 40;
        string bodyFam = DocTemplate.PdfFamily(Tp.BodyFont), headFam = DocTemplate.PdfFamily(Tp.HeadingFont);
        string Regular(string fam) => fam == "Times" ? "Times-Roman" : fam;
        int fontR = pdf.Add("<< /Type /Font /Subtype /Type1 /BaseFont /" + Regular(bodyFam) + " /Encoding /WinAnsiEncoding >>");
        int fontB = pdf.Add("<< /Type /Font /Subtype /Type1 /BaseFont /" + headFam + "-Bold /Encoding /WinAnsiEncoding >>");
        int pagesId = pdf.Reserve();
        var kids = new List<int>();
        var (ar, ag, ab) = Rgb(o.Accent);
        string C(double r, double g, double b) => F(r) + " " + F(g) + " " + F(b);
        string Txt(string font, double size, double x, double y, string color, string s) => "BT /" + font + " " + F(size) + " Tf " + color + " rg " + F(x) + " " + F(y) + " Td " + PdfWriter.Str(s) + " Tj ET\n";
        void AddPage(string content, string resources)
        {
            if (Tp.Page != "FFFFFF") content = HexC(Tp.Page) + " rg 0 0 " + F(W) + " " + F(H) + " re f\n" + content;
            int stream = pdf.AddStream("", Encoding.Latin1.GetBytes(content), true);
            kids.Add(pdf.Add("<< /Type /Page /Parent " + pagesId + " 0 R /MediaBox [0 0 " + F(W) + " " + F(H) + "] /Contents " + stream + " 0 R /Resources << /Font << /F1 " + fontR + " 0 R /F2 " + fontB + " 0 R >> " + resources + " >> >>"));
        }
        string HexC(string hex) { var (r, g, b) = Rgb(hex); return C(r, g, b); }
        string ink = HexC(Tp.Ink), muted = HexC(Tp.Muted), faint = HexC(Tp.Faint), accent = C(ar, ag, ab), rule = HexC(Tp.Rule);
        string fillC = HexC(Tp.CoverFillHex(o.Accent)), cink = HexC(Tp.CoverInk);
        if (o.Cover)
        {
            var c = new StringBuilder();
            string titleInk = ink, subInk = muted, introInk = ink, footInk = faint;
            bool centered = Tp.Cover == "minimal";
            double y = H * .5 + 32, sy = H * .5 - 6, iy = H * .5 - 34, tx = M + 20;
            switch (Tp.Cover)
            {
                case "band":
                    c.Append(fillC + " rg 0 " + F(H * .55) + " " + F(W) + " " + F(H * .45) + " re f\n");
                    titleInk = subInk = cink; y = H * .72 + 32; sy = H * .72 - 6; iy = H * .5;
                    break;
                case "full":
                    c.Append(fillC + " rg 0 0 " + F(W) + " " + F(H) + " re f\n");
                    if (Tp.CoverFillHex(o.Accent) != o.Accent) c.Append(accent + " rg " + F(tx) + " " + F(y + 40) + " 60 3 re f\n");
                    titleInk = subInk = introInk = footInk = cink;
                    break;
                case "minimal":
                    c.Append(accent + " rg " + F(W / 2 - 25) + " " + F(H * .5 + 8) + " 50 1.5 re f\n");
                    y = H * .5 + 40; sy = H * .5 - 14; iy = H * .5 - 44;
                    break;
                default:
                    c.Append(accent + " rg " + F(M) + " " + F(H * .5 - 10) + " 5 70 re f\n");
                    break;
            }
            double X0(string s, double size, string font) => centered ? (W - Width(s, size, font == "F2" ? headFam : bodyFam)) / 2 : tx;
            foreach (var line in Wrap(o.Title, 30, W - 2 * M - 20, headFam).Take(2)) { c.Append(Txt("F2", 30, X0(line, 30, "F2"), y, titleInk, line)); y -= 36; }
            string sub = Subtitle(pages.Count);
            c.Append(Txt("F1", 13, X0(sub, 13, "F1"), sy, subInk, sub));
            foreach (var line in Wrap(o.Intro.Trim(), 11, W - 2 * M - 20, bodyFam).Take(14)) { if (line.Length == 0) continue; c.Append(Txt("F1", 11, X0(line, 11, "F1"), iy, introInk, line)); iy -= 15; }
            if (Tp.Footer.Length > 0) c.Append(Txt("F1", 9, M, M - 10, footInk, Tp.Footer));
            AddPage(c.ToString(), "");
        }
        for (int i = 0; i < pages.Count; i++)
        {
            var p = pages[i];
            var (img, iw, ih) = PdfImage(pdf, p.Item);
            var c = new StringBuilder();
            double top = H - M;
            string title = o.Captions ? p.Title.Trim() : "";
            if (Tp.TopRule) c.Append(accent + " rg 0 " + F(H - 5) + " " + F(W) + " 5 re f\n");
            if (o.Numbers || title.Length > 0)
            {
                double hx = M;
                if (o.Numbers && Tp.Steps == "pill")
                {
                    string label = StepLabel(i);
                    double pw = Width(label, 11, headFam) + 18;
                    c.Append(accent + " rg " + RoundRect(M, top - 19, pw, 20, 10) + " f\n");
                    c.Append(Txt("F2", 11, M + 9, top - 13, "1 1 1", label));
                    hx = M + pw + 10;
                }
                else if (o.Numbers && Tp.Steps == "number")
                {
                    c.Append(Txt("F2", 24, M, top - 21, accent, (i + 1).ToString("00")));
                    hx = M + Width("00", 24, headFam) + 12;
                }
                c.Append("BT /F2 15 Tf " + F(hx) + " " + F(top - 15) + " Td ");
                if (o.Numbers && Tp.Steps == "inline") c.Append(accent + " rg " + PdfWriter.Str(StepLabel(i) + (title.Length > 0 ? "   " : "")) + " Tj ");
                if (title.Length > 0) c.Append(ink + " rg " + PdfWriter.Str(Clip(title, 15, W - M - hx - (o.Numbers && Tp.Steps == "inline" ? 70 : 0), headFam)) + " Tj ");
                c.Append("ET\n");
                c.Append(rule + " rg " + F(M) + " " + F(top - 26) + " " + F(W - 2 * M) + " 0.75 re f\n");
                top -= 38;
            }
            var noteLines = Wrap(p.Note.Trim(), 10.5, W - 2 * M, bodyFam).Take(8).ToList();
            double footer = 24, noteH = noteLines.Count > 0 ? noteLines.Count * 14 + 10 : 0;
            double areaH = top - M - footer - noteH, areaW = W - 2 * M;
            double s = Math.Min(Math.Min(areaW / iw, areaH / ih), 1.5);   // 1 px = 0.75 pt at 96 dpi; allow up to 2× for small captures
            double w = iw * s, h = ih * s, x = M + (areaW - w) / 2, y = top - (areaH + h) / 2;
            c.Append("q " + F(w) + " 0 0 " + F(h) + " " + F(x) + " " + F(y) + " cm /Im" + i + " Do Q\n");
            if (Tp.Frame != "none") c.Append(HexC(Tp.FrameHex(o.Accent)) + " RG " + (Tp.Frame == "accent" ? "1.2" : "0.5") + " w " + F(x) + " " + F(y) + " " + F(w) + " " + F(h) + " re S\n");
            double ny = top - areaH - 16;
            foreach (var line in noteLines) { c.Append(Txt("F1", 10.5, M, ny, ink, line)); ny -= 14; }
            if (o.Meta) c.Append(Txt("F1", 8, M, M - 6, faint, Clip(MetaLine(p), 8, W - 2 * M - 60, bodyFam)));
            string num = (i + 1) + " / " + pages.Count;
            c.Append(Txt("F1", 8, W - M - Width(num, 8, bodyFam), M - 6, faint, num));
            AddPage(c.ToString(), "/XObject << /Im" + i + " " + img + " 0 R >>");
        }
        pdf.Set(pagesId, "<< /Type /Pages /Kids [" + string.Join(" ", kids.Select(k => k + " 0 R")) + "] /Count " + kids.Count + " >>");
        int catalog = pdf.Add("<< /Type /Catalog /Pages " + pagesId + " 0 R >>");
        int info = pdf.Add("<< /Title " + PdfWriter.Unicode(o.Title) + " /Producer (BuzzPrint) /CreationDate (D:" + DateTime.Now.ToString("yyyyMMddHHmmss") + ") >>");
        pdf.Finish(catalog, info);
    }

    static (int id, int w, int h) PdfImage(PdfWriter pdf, CaptureItem it)
    {
        using var src = Store.LoadBitmap(it.Path);
        int w = src.Width, h = src.Height;
        using var rgb = new D.Bitmap(w, h, DI.PixelFormat.Format24bppRgb);
        using (var g = D.Graphics.FromImage(rgb)) { g.Clear(D.Color.White); g.DrawImageUnscaled(src, 0, 0); }
        var data = rgb.LockBits(new D.Rectangle(0, 0, w, h), DI.ImageLockMode.ReadOnly, DI.PixelFormat.Format24bppRgb);
        var raw = new byte[w * 3 * h];
        try
        {
            var row = new byte[data.Stride];
            for (int y = 0; y < h; y++)
            {
                System.Runtime.InteropServices.Marshal.Copy(data.Scan0 + y * data.Stride, row, 0, data.Stride);
                for (int x = 0; x < w; x++) { int s = x * 3, d = (y * w + x) * 3; raw[d] = row[s + 2]; raw[d + 1] = row[s + 1]; raw[d + 2] = row[s]; }
            }
        }
        finally { rgb.UnlockBits(data); }
        int id = pdf.AddStream("/Type /XObject /Subtype /Image /Width " + w + " /Height " + h + " /ColorSpace /DeviceRGB /BitsPerComponent 8", raw, true);
        return (id, w, h);
    }

    static string F(double v) => v.ToString("0.###", CultureInfo.InvariantCulture);
    static (double, double, double) Rgb(string hex) => (Convert.ToInt32(hex[..2], 16) / 255d, Convert.ToInt32(hex[2..4], 16) / 255d, Convert.ToInt32(hex[4..6], 16) / 255d);
    // Helvetica average widths (per 1000 em): enough to wrap and clip text sensibly.
    static double Width(string s, double size, string family = "Helvetica") => family == "Courier" ? s.Length * 600 * size / 1000
        : s.Sum(ch => ch == ' ' ? 260 : char.IsUpper(ch) ? 667 : "il.,:;'|!".Contains(ch) ? 260 : "mwMW".Contains(ch) ? 850 : char.IsDigit(ch) ? 556 : 520) * size / 1000 * (family == "Times" ? .92 : 1);
    static string Clip(string s, double size, double max, string family = "Helvetica")
    {
        if (Width(s, size, family) <= max) return s;
        while (s.Length > 1 && Width(s + "…", size, family) > max) s = s[..^1];
        return s.TrimEnd() + "…";
    }
    static string RoundRect(double x, double y, double w, double h, double r)
    {
        r = Math.Min(r, Math.Min(w, h) / 2); double k = r * .5523;
        return F(x + r) + " " + F(y) + " m " + F(x + w - r) + " " + F(y) + " l " + F(x + w - r + k) + " " + F(y) + " " + F(x + w) + " " + F(y + r - k) + " " + F(x + w) + " " + F(y + r) + " c " +
               F(x + w) + " " + F(y + h - r) + " l " + F(x + w) + " " + F(y + h - r + k) + " " + F(x + w - r + k) + " " + F(y + h) + " " + F(x + w - r) + " " + F(y + h) + " c " +
               F(x + r) + " " + F(y + h) + " l " + F(x + r - k) + " " + F(y + h) + " " + F(x) + " " + F(y + h - r + k) + " " + F(x) + " " + F(y + h - r) + " c " +
               F(x) + " " + F(y + r) + " l " + F(x) + " " + F(y + r - k) + " " + F(x + r - k) + " " + F(y) + " " + F(x + r) + " " + F(y) + " c h";
    }
    static IEnumerable<string> Wrap(string text, double size, double max, string family = "Helvetica")
    {
        foreach (var para in text.Replace("\r", "").Split('\n'))
        {
            var line = "";
            foreach (var word in para.Split(' '))
            {
                string next = line.Length == 0 ? word : line + " " + word;
                if (Width(next, size, family) <= max || line.Length == 0) line = next;
                else { yield return line; line = word; }
            }
            yield return line;
        }
    }

    sealed class PdfWriter
    {
        readonly Stream s;
        readonly List<long> offsets = new() { 0 };
        readonly Dictionary<int, string> pending = new();
        public PdfWriter(Stream output) { s = output; Raw("%PDF-1.4\n%âãÏÓ\n"); }
        void Raw(string text) { var b = Encoding.Latin1.GetBytes(text); s.Write(b, 0, b.Length); }
        public int Reserve() { offsets.Add(-1); return offsets.Count - 1; }
        public void Set(int id, string dict) => pending[id] = dict;
        public int Add(string dict) { int id = Reserve(); Write(id, dict); return id; }
        void Write(int id, string dict) { offsets[id] = s.Position; Raw(id + " 0 obj\n" + dict + "\nendobj\n"); }
        public int AddStream(string dict, byte[] data, bool deflate)
        {
            int id = Reserve();
            byte[] body = data;
            if (deflate)
            {
                using var ms = new MemoryStream();
                using (var z = new ZLibStream(ms, CompressionLevel.Optimal, true)) z.Write(data, 0, data.Length);
                body = ms.ToArray();
            }
            offsets[id] = s.Position;
            Raw(id + " 0 obj\n<< " + dict + " /Length " + body.Length + (deflate ? " /Filter /FlateDecode" : "") + " >>\nstream\n");
            s.Write(body, 0, body.Length);
            Raw("\nendstream\nendobj\n");
            return id;
        }
        public void Finish(int catalog, int info)
        {
            foreach (var (id, dict) in pending) Write(id, dict);
            long xref = s.Position;
            var sb = new StringBuilder("xref\n0 " + offsets.Count + "\n0000000000 65535 f \n");
            for (int i = 1; i < offsets.Count; i++) sb.Append(offsets[i].ToString("D10") + " 00000 n \n");
            sb.Append("trailer\n<< /Size " + offsets.Count + " /Root " + catalog + " 0 R /Info " + info + " 0 R >>\nstartxref\n" + xref + "\n%%EOF\n");
            Raw(sb.ToString());
        }
        /// <summary>Document-info text (title): UTF-16BE with BOM, so any character survives.</summary>
        public static string Unicode(string text) => "<FEFF" + string.Concat((text ?? "").Select(c => ((int)c).ToString("X4"))) + ">";
        /// <summary>PDF literal string in WinAnsi: Latin-1 plus the typographic marks Windows-1252 adds.</summary>
        public static string Str(string text)
        {
            var sb = new StringBuilder("(");
            foreach (char ch in text ?? "")
            {
                char c = ch switch { '—' => '\u0097', '–' => '\u0096', '“' => '\u0093', '”' => '\u0094', '‘' => '\u0091', '’' => '\u0092', '•' => '\u0095', '…' => '\u0085', '€' => '\u0080', '·' => '·', '×' => '×', _ => ch };
                if (c is '(' or ')' or '\\') sb.Append('\\').Append(c);
                else if (c < 32) sb.Append(' ');
                else if (c > 255) sb.Append('?');
                else sb.Append(c);
            }
            return sb.Append(')').ToString();
        }
    }

    // ------------------------------------------------------------------ one tall image
    static D.Drawing2D.GraphicsPath RoundedPath(float x, float y, float w, float h, float r)
    {
        var path = new D.Drawing2D.GraphicsPath();
        path.AddArc(x, y, 2 * r, 2 * r, 180, 90); path.AddArc(x + w - 2 * r, y, 2 * r, 2 * r, 270, 90);
        path.AddArc(x + w - 2 * r, y + h - 2 * r, 2 * r, 2 * r, 0, 90); path.AddArc(x, y + h - 2 * r, 2 * r, 2 * r, 90, 90);
        path.CloseFigure();
        return path;
    }

    public static D.Bitmap Stitch(IReadOnlyList<ExportPage> pages, ExportOptions o)
    {
        if (cur == null) { cur = o.T.Normalized(); try { return Stitch(pages, o); } finally { cur = null; } }
        const int pad = 32, gap = 40, headH = 44, metaH = 26;
        var sizes = pages.Select(p => Size(p.Item)).ToList();
        int width = Math.Min(Math.Max(sizes.Max(s => s.w), 640), 2400);
        var heights = new List<int>();
        using var measure = D.Graphics.FromHwnd(IntPtr.Zero);
        using var titleFont = new D.Font(Tp.HeadingFont, 15f, D.FontStyle.Bold, D.GraphicsUnit.Pixel);
        using var noteFont = new D.Font(Tp.BodyFont, 14f, D.GraphicsUnit.Pixel);
        using var metaFont = new D.Font(Tp.BodyFont, 12f, D.GraphicsUnit.Pixel);
        using var coverFont = new D.Font(Tp.HeadingFont, 30f, D.FontStyle.Bold, D.GraphicsUnit.Pixel);
        bool filledCover = o.Cover && Tp.Cover is "band" or "full";
        int total = pad, coverH = o.Cover ? (filledCover ? 150 : 96) : 0;
        total += coverH;
        for (int i = 0; i < pages.Count; i++)
        {
            var (w, h) = sizes[i];
            int ih = w > width ? (int)Math.Round(h * (double)width / w) : h;
            bool head = o.Numbers || (o.Captions && pages[i].Title.Trim().Length > 0);
            int noteH = pages[i].Note.Trim().Length > 0 ? (int)Math.Ceiling(measure.MeasureString(pages[i].Note.Trim(), noteFont, width).Height) + 10 : 0;
            int block = (head ? headH : 0) + ih + (o.Meta ? metaH : 0) + noteH;
            heights.Add(block);
            total += block + (i < pages.Count - 1 ? gap : 0);
        }
        total += pad;
        var bmp = new D.Bitmap(width + 2 * pad, total, DI.PixelFormat.Format32bppArgb);
        using var g = D.Graphics.FromImage(bmp);
        D.Color Hex(string h) => D.ColorTranslator.FromHtml("#" + h);
        g.Clear(Hex(Tp.Page));
        g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.ClearTypeGridFit;
        g.InterpolationMode = D.Drawing2D.InterpolationMode.HighQualityBicubic;
        var accent = D.ColorTranslator.FromHtml("#" + o.Accent);
        using var accentBrush = new D.SolidBrush(accent);
        using var ink = new D.SolidBrush(Hex(Tp.Ink));
        using var muted = new D.SolidBrush(Hex(Tp.Faint));
        using var noteBrush = new D.SolidBrush(Hex(Tp.Ink));
        using var border = new D.Pen(Hex(Tp.Frame == "none" ? Tp.Page : Tp.FrameHex(o.Accent)), Tp.Frame == "accent" ? 2 : 1);
        using var rule = new D.Pen(Hex(Tp.Rule));
        using var coverInk = new D.SolidBrush(Hex(Tp.CoverInk));
        using var white = new D.SolidBrush(D.Color.White);
        int y = pad;
        if (o.Cover)
        {
            if (filledCover)
            {
                using var fill = new D.SolidBrush(Hex(Tp.CoverFillHex(o.Accent)));
                g.FillRectangle(fill, 0, 0, width + 2 * pad, pad + coverH - 24);
                g.DrawString(o.Title, coverFont, coverInk, pad + 4, y + 22);
                g.DrawString(Subtitle(pages.Count), metaFont, coverInk, pad + 6, y + 70);
            }
            else if (Tp.Cover == "minimal")
            {
                var center = new D.StringFormat { Alignment = D.StringAlignment.Center };
                g.DrawString(o.Title, coverFont, ink, new D.RectangleF(pad, y, width, 44), center);
                g.FillRectangle(accentBrush, pad + width / 2f - 25, y + 48, 50, 2);
                g.DrawString(Subtitle(pages.Count), metaFont, muted, new D.RectangleF(pad, y + 56, width, 20), center);
            }
            else
            {
                g.FillRectangle(accentBrush, pad, y + 6, 5, 64);
                g.DrawString(o.Title, coverFont, ink, pad + 18, y);
                g.DrawString(Subtitle(pages.Count), metaFont, muted, pad + 20, y + 46);
            }
            y += coverH;
        }
        for (int i = 0; i < pages.Count; i++)
        {
            var p = pages[i];
            string title = o.Captions ? p.Title.Trim() : "";
            if (o.Numbers || title.Length > 0)
            {
                float x = pad;
                if (o.Numbers && Tp.Steps == "pill")
                {
                    string label = StepLabel(i);
                    var size = g.MeasureString(label, titleFont);
                    using (var path = RoundedPath(x, y + 6, size.Width + 16, 26, 13)) g.FillPath(accentBrush, path);
                    g.DrawString(label, titleFont, white, x + 8, y + 9);
                    x += size.Width + 28;
                }
                else if (o.Numbers && Tp.Steps == "number")
                {
                    using var big = new D.Font(Tp.HeadingFont, 24f, D.FontStyle.Bold, D.GraphicsUnit.Pixel);
                    string n2 = (i + 1).ToString("00");
                    g.DrawString(n2, big, accentBrush, x, y + 2);
                    x += g.MeasureString(n2, big).Width + 8;
                }
                else if (o.Numbers) { string step = StepLabel(i) + "   "; g.DrawString(step, titleFont, accentBrush, x, y + 8); x += g.MeasureString(step, titleFont).Width; }
                if (title.Length > 0) g.DrawString(title, titleFont, ink, new D.RectangleF(x, y + 8, width - (x - pad), 24), new D.StringFormat { Trimming = D.StringTrimming.EllipsisCharacter, FormatFlags = D.StringFormatFlags.NoWrap });
                g.DrawLine(rule, pad, y + headH - 6, pad + width, y + headH - 6);
                y += headH;
            }
            using (var img = Store.LoadBitmap(p.Item.Path))
            {
                int w = img.Width, h = img.Height;
                if (w > width) { h = (int)Math.Round(h * (double)width / w); w = width; }
                int x = pad + (width - w) / 2;
                g.DrawImage(img, new D.Rectangle(x, y, w, h));
                if (Tp.Frame != "none") g.DrawRectangle(border, x - 1, y - 1, w + 1, h + 1);
                y += h;
            }
            if (o.Meta) { g.DrawString(MetaLine(p), metaFont, muted, pad, y + 6); y += metaH; }
            string note = p.Note.Trim();
            if (note.Length > 0) { var r = new D.RectangleF(pad, y + 6, width, 4000); var sz = g.MeasureString(note, noteFont, width); g.DrawString(note, noteFont, noteBrush, r); y += (int)Math.Ceiling(sz.Height) + 10; }
            y += gap;
        }
        return bmp;
    }
}
