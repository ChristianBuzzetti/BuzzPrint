using System.IO;
using System.Net;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;

namespace BuzzPrint;

/// <summary>
/// A plain-text report ("Heading" line, then its text; blocks separated by blank lines) turned into a designed HTML
/// version for the clipboard (pastes formatted into Outlook, Teams, Jira or Word), a PNG and text files.
/// The HTML and image are light-themed on purpose: they end up in mail and tickets, not in the app.
/// </summary>
public static class ReportFormat
{
    public sealed record Section(string Heading, List<string> Lines);

    public static List<Section> Parse(string text)
    {
        var sections = new List<Section>();
        var blocks = text.Replace("\r\n", "\n").Split("\n\n", StringSplitOptions.RemoveEmptyEntries);
        foreach (var block in blocks)
        {
            var lines = block.Split('\n').Select(l => l.TrimEnd()).Where(l => l.Trim().Length > 0).ToList();
            if (lines.Count == 0) continue;
            string first = lines[0].Trim().TrimEnd(':');
            bool heading = lines.Count > 1 && first.Length <= 48 && !first.EndsWith('.');
            if (heading) sections.Add(new Section(first, lines.Skip(1).ToList()));
            else sections.Add(new Section("", lines));
        }
        return sections;
    }

    static bool IsSeverity(string h) => h.Contains("severity", StringComparison.OrdinalIgnoreCase) || h.Contains("severidad", StringComparison.OrdinalIgnoreCase) || h.Contains("prioridad", StringComparison.OrdinalIgnoreCase);
    static string SeverityColor(string line)
    {
        string l = line.ToLowerInvariant();
        if (l.StartsWith("crit")) return "#B91C1C";
        if (l.StartsWith("high") || l.StartsWith("alta")) return "#DC2626";
        if (l.StartsWith("med")) return "#D97706";
        if (l.StartsWith("low") || l.StartsWith("baja")) return "#059669";
        return "#6B7280";
    }
    static bool IsBullet(string l) => l.TrimStart().StartsWith("- ") || l.TrimStart().StartsWith("• ") || l.TrimStart().StartsWith("* ");
    static bool IsNumbered(string l) { var t = l.TrimStart(); int i = 0; while (i < t.Length && char.IsDigit(t[i])) i++; return i > 0 && i < t.Length - 1 && (t[i] == '.' || t[i] == ')') && t[i + 1] == ' '; }
    static bool IsQuote(string l) => l.TrimStart().StartsWith('"') || l.TrimStart().StartsWith('`');
    static string Strip(string l)
    {
        var t = l.TrimStart();
        if (IsBullet(t)) return t[2..];
        return t;
    }

    // ------------------------------------------------------------------ HTML
    public static string Html(string title, string text, string accentHex)
    {
        string accent = "#" + accentHex.TrimStart('#');
        string E(string s) => WebUtility.HtmlEncode(s);
        var sb = new StringBuilder();
        sb.Append($"<div style=\"font-family:'Segoe UI',Arial,sans-serif;font-size:14px;line-height:1.5;color:#1F2328;max-width:780px;background:#FFFFFF;border:1px solid #E5E7EB;border-top:4px solid {accent};border-radius:8px;padding:20px 24px\">");
        sb.Append($"<div style=\"font-size:11px;font-weight:700;letter-spacing:.08em;text-transform:uppercase;color:{accent}\">{E(title)}</div>");
        var sections = Parse(text);
        var summary = sections.FirstOrDefault();
        if (summary != null && summary.Lines.Count > 0)
            sb.Append($"<div style=\"font-size:20px;font-weight:700;margin:6px 0 2px;color:#111827\">{E(Strip(summary.Lines[0]))}</div>");
        sb.Append($"<div style=\"font-size:12px;color:#6B7280;margin-bottom:6px\">{E(DateTime.Now.ToString("f", L.Culture))} · BuzzPrint</div>");
        foreach (var s in sections)
        {
            bool isSummary = s == summary && s.Lines.Count == 1;
            if (isSummary) continue;
            sb.Append("<div style=\"margin-top:16px\">");
            if (s.Heading.Length > 0)
                sb.Append($"<div style=\"font-size:11px;font-weight:700;letter-spacing:.06em;text-transform:uppercase;color:#6B7280;border-bottom:1px solid #EEF0F2;padding-bottom:4px;margin-bottom:6px\">{E(s.Heading)}</div>");
            bool inList = false, inOl = false;
            void CloseLists() { if (inList) { sb.Append("</ul>"); inList = false; } if (inOl) { sb.Append("</ol>"); inOl = false; } }
            for (int i = 0; i < s.Lines.Count; i++)
            {
                string line = s.Lines[i];
                if (i == 0 && IsSeverity(s.Heading))
                {
                    CloseLists();
                    string word = line.Split(' ', '—', '-', ':', ',')[0];
                    string rest = line[word.Length..].TrimStart(' ', '—', '-', ':', ',');
                    sb.Append($"<p style=\"margin:4px 0\"><span style=\"display:inline-block;background:{SeverityColor(line)};color:#FFFFFF;font-weight:700;font-size:12px;border-radius:10px;padding:1px 10px;margin-right:8px\">{E(word)}</span>{E(rest)}</p>");
                }
                else if (IsBullet(line))
                {
                    if (inOl) { sb.Append("</ol>"); inOl = false; }
                    if (!inList) { sb.Append("<ul style=\"margin:4px 0 4px 20px;padding:0\">"); inList = true; }
                    sb.Append($"<li style=\"margin:2px 0\">{E(Strip(line))}</li>");
                }
                else if (IsNumbered(line))
                {
                    if (inList) { sb.Append("</ul>"); inList = false; }
                    if (!inOl) { sb.Append("<ol style=\"margin:4px 0 4px 22px;padding:0\">"); inOl = true; }
                    var t = line.TrimStart(); int k = t.IndexOf(' ');
                    sb.Append($"<li style=\"margin:2px 0\">{E(t[(k + 1)..])}</li>");
                }
                else if (IsQuote(line))
                {
                    CloseLists();
                    sb.Append($"<div style=\"font-family:Consolas,'Cascadia Mono',monospace;font-size:12px;background:#F6F8FA;border:1px solid #EEF0F2;border-radius:6px;padding:6px 10px;margin:4px 0;white-space:pre-wrap;word-break:break-word\">{E(line.Trim())}</div>");
                }
                else { CloseLists(); sb.Append($"<p style=\"margin:4px 0\">{E(line.Trim())}</p>"); }
            }
            CloseLists();
            sb.Append("</div>");
        }
        sb.Append("</div>");
        return sb.ToString();
    }

    /// <summary>Puts the designed HTML (CF_HTML) and the plain text on the clipboard together.</summary>
    public static void CopyHtml(string fragment, string plain)
    {
        const string marker = "<!--StartFragment-->", endMarker = "<!--EndFragment-->";
        string doc = "<html><head><meta charset=\"utf-8\"></head><body>" + marker + fragment + endMarker + "</body></html>";
        const string headerFormat = "Version:0.9\r\nStartHTML:{0:D10}\r\nEndHTML:{1:D10}\r\nStartFragment:{2:D10}\r\nEndFragment:{3:D10}\r\n";
        int headerLength = string.Format(headerFormat, 0, 0, 0, 0).Length;
        var utf8 = Encoding.UTF8;
        int startHtml = headerLength;
        int startFragment = startHtml + utf8.GetByteCount(doc[..(doc.IndexOf(marker, StringComparison.Ordinal) + marker.Length)]);
        int endFragment = startHtml + utf8.GetByteCount(doc[..doc.IndexOf(endMarker, StringComparison.Ordinal)]);
        int endHtml = startHtml + utf8.GetByteCount(doc);
        byte[] bytes = utf8.GetBytes(string.Format(headerFormat, startHtml, endHtml, startFragment, endFragment) + doc + "\0");
        var data = new DataObject();
        data.SetData(DataFormats.Html, new MemoryStream(bytes));
        data.SetData(DataFormats.UnicodeText, plain);
        Clipboard.SetDataObject(data, true);
    }

    public static string FullHtmlPage(string title, string fragment) =>
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>" + WebUtility.HtmlEncode(title) + "</title></head><body style=\"background:#F3F4F6;padding:24px;margin:0\">" + fragment + "</body></html>";

    public static string Markdown(string title, string text)
    {
        var sb = new StringBuilder("# " + title + "\n\n");
        foreach (var s in Parse(text))
        {
            if (s.Heading.Length > 0) sb.Append("## " + s.Heading + "\n\n");
            foreach (var l in s.Lines) sb.Append(IsQuote(l) ? "    " + l.Trim() : l).Append('\n');
            sb.Append('\n');
        }
        return sb.ToString().TrimEnd() + "\n";
    }

    // ------------------------------------------------------------------ image
    /// <summary>The report as a light, designed card (PNG, 2x), the same look as the HTML.</summary>
    public static BitmapSource Image(string title, string text, Color accent)
    {
        const double width = 880;
        var ink = new SolidColorBrush(Color.FromRgb(0x1F, 0x23, 0x28));
        var muted = new SolidColorBrush(Color.FromRgb(0x6B, 0x72, 0x80));
        var ui = new FontFamily("Segoe UI");
        var mono = new FontFamily("Consolas");
        var stack = new StackPanel { Margin = new Thickness(28, 22, 28, 26) };
        TextBlock T(string s, double size, Brush b, FontWeight w, Thickness m, FontFamily? f = null) =>
            new() { Text = s, FontSize = size, Foreground = b, FontWeight = w, Margin = m, TextWrapping = TextWrapping.Wrap, FontFamily = f ?? ui };
        stack.Children.Add(T(title.ToUpperInvariant(), 11.5, new SolidColorBrush(accent), FontWeights.Bold, new Thickness(0)));
        var sections = Parse(text);
        var summary = sections.FirstOrDefault();
        if (summary != null && summary.Lines.Count > 0) stack.Children.Add(T(Strip(summary.Lines[0]), 21, Brushes.Black, FontWeights.Bold, new Thickness(0, 6, 0, 2)));
        stack.Children.Add(T(DateTime.Now.ToString("f", L.Culture) + " · BuzzPrint", 12, muted, FontWeights.Normal, new Thickness(0, 0, 0, 4)));
        foreach (var s in sections)
        {
            if (s == summary && s.Lines.Count == 1) continue;
            if (s.Heading.Length > 0)
            {
                stack.Children.Add(T(s.Heading.ToUpperInvariant(), 11, muted, FontWeights.Bold, new Thickness(0, 16, 0, 4)));
                stack.Children.Add(new Border { Height = 1, Background = new SolidColorBrush(Color.FromRgb(0xEE, 0xF0, 0xF2)), Margin = new Thickness(0, 0, 0, 6) });
            }
            for (int i = 0; i < s.Lines.Count; i++)
            {
                string line = s.Lines[i];
                if (i == 0 && IsSeverity(s.Heading))
                {
                    string word = line.Split(' ', '—', '-', ':', ',')[0];
                    string rest = line[word.Length..].TrimStart(' ', '—', '-', ':', ',');
                    var row = new DockPanel { Margin = new Thickness(0, 2, 0, 2) };
                    var pill = new Border { Background = (Brush)new BrushConverter().ConvertFromString(SeverityColor(line))!, CornerRadius = new CornerRadius(10), Padding = new Thickness(10, 1, 10, 2), Margin = new Thickness(0, 0, 10, 0), VerticalAlignment = VerticalAlignment.Top,
                        Child = T(word, 12, Brushes.White, FontWeights.Bold, new Thickness(0)) };
                    DockPanel.SetDock(pill, Dock.Left);
                    row.Children.Add(pill); row.Children.Add(T(rest, 14, ink, FontWeights.Normal, new Thickness(0)));
                    stack.Children.Add(row);
                }
                else if (IsQuote(line))
                    stack.Children.Add(new Border { Background = new SolidColorBrush(Color.FromRgb(0xF6, 0xF8, 0xFA)), BorderBrush = new SolidColorBrush(Color.FromRgb(0xEE, 0xF0, 0xF2)), BorderThickness = new Thickness(1), CornerRadius = new CornerRadius(6), Padding = new Thickness(10, 6, 10, 6), Margin = new Thickness(0, 3, 0, 3),
                        Child = T(line.Trim(), 12.5, ink, FontWeights.Normal, new Thickness(0), mono) });
                else if (IsBullet(line))
                    stack.Children.Add(T("•  " + Strip(line), 14, ink, FontWeights.Normal, new Thickness(12, 1, 0, 1)));
                else stack.Children.Add(T(line.Trim(), 14, ink, FontWeights.Normal, new Thickness(0, 1, 0, 1)));
            }
        }
        var card = new Border { Width = width, Background = Brushes.White, BorderBrush = new SolidColorBrush(Color.FromRgb(0xE5, 0xE7, 0xEB)), BorderThickness = new Thickness(1, 0, 1, 1), Child = stack };
        var frame = new DockPanel { Background = Brushes.White };
        var bar = new Border { Height = 5, Background = new SolidColorBrush(accent) };
        DockPanel.SetDock(bar, Dock.Top);
        frame.Children.Add(bar); frame.Children.Add(card);
        frame.Measure(new Size(width, double.PositiveInfinity));
        frame.Arrange(new Rect(frame.DesiredSize));
        frame.UpdateLayout();
        const double scale = 2;
        var bmp = new RenderTargetBitmap((int)Math.Ceiling(frame.ActualWidth * scale), (int)Math.Ceiling(frame.ActualHeight * scale), 96 * scale, 96 * scale, PixelFormats.Pbgra32);
        bmp.Render(frame);
        bmp.Freeze();
        return bmp;
    }
}
