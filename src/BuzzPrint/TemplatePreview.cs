using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Shapes;

namespace BuzzPrint;

/// <summary>A miniature of a document template: its cover and a step page side by side, drawn with its colors,
/// fonts, cover style, step style and frame. Used in the template list, the editor and the export window.</summary>
public static class TemplatePreview
{
    static SolidColorBrush B(string hex) { var b = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#" + hex)); b.Freeze(); return b; }

    public static FrameworkElement Build(DocTemplate t, string appAccent, double pageWidth)
    {
        t = t.Normalized();
        string accent = t.AccentHex(appAccent);
        double w = pageWidth, h = Math.Round(pageWidth * .62);
        var row = new StackPanel { Orientation = Orientation.Horizontal };
        row.Children.Add(Page(Cover(t, accent, w, h), t, w, h));
        row.Children.Add(new Border { Width = Math.Max(4, w * .05) });
        row.Children.Add(Page(Step(t, accent, w, h), t, w, h));
        return row;
    }

    static Border Page(UIElement content, DocTemplate t, double w, double h)
    {
        var page = new Border { Width = w, Height = h, Background = B(t.Page), BorderBrush = new SolidColorBrush(Color.FromArgb(40, 0, 0, 0)), BorderThickness = new Thickness(1), CornerRadius = new CornerRadius(3), ClipToBounds = true, Child = content };
        page.Effect = new System.Windows.Media.Effects.DropShadowEffect { BlurRadius = 6, ShadowDepth = 1.5, Direction = 270, Opacity = .25 };
        return page;
    }

    static TextBlock Text(string s, string font, double size, string color, bool bold = false, TextAlignment align = TextAlignment.Left) =>
        new() { Text = s, FontFamily = new FontFamily(font), FontSize = size, Foreground = B(color), FontWeight = bold ? FontWeights.Bold : FontWeights.Normal, TextAlignment = align, TextTrimming = TextTrimming.CharacterEllipsis };

    static UIElement Cover(DocTemplate t, string accent, double w, double h)
    {
        var c = new Canvas { Width = w, Height = h };
        double s = w / 160;   // the drawings are laid out for a 160 px page
        string fill = t.CoverFillHex(accent);
        string title = L.T("Título del documento"), sub = L.T("12 capturas · hoy");
        void Add(UIElement e, double x, double y) { Canvas.SetLeft(e, x * s); Canvas.SetTop(e, y * s); c.Children.Add(e); }
        void Rect(double x, double y, double rw, double rh, string color) => Add(new Rectangle { Width = rw * s, Height = rh * s, Fill = B(color) }, x, y);
        switch (t.Cover)
        {
            case "hero":
                Rect(74, 0, 86, 100, fill);
                var ms = MockShot(70, 44, accent, s); ms.BorderBrush = Brushes.White; ms.BorderThickness = new Thickness(.8); Add(ms, 82, 28);
                Rect(72, 0, 2, 100, accent);
                Rect(10, 34, 12, 1.4, accent);
                var ht = Text(title, t.HeadingFont, 9.5 * s, t.Ink, true); ht.Width = 58 * s; ht.TextWrapping = TextWrapping.Wrap; ht.TextTrimming = TextTrimming.None; Add(ht, 10, 37);
                Add(Text(sub, t.BodyFont, 5.2 * s, t.Muted), 10, 62);
                break;
            case "band":
                Rect(0, 0, 160, 52, fill);
                Add(Text(title, t.HeadingFont, 11 * s, t.CoverInk, true), 12, 20);
                Add(Text(sub, t.BodyFont, 6 * s, t.CoverInk), 12, 36);
                for (int i = 0; i < 3; i++) Rect(12, 64 + i * 7, 100 - i * 20, 2.5, t.Rule);
                break;
            case "full":
                Rect(0, 0, 160, 100, fill);
                Rect(12, 30, 18, 1.5, fill == accent ? t.CoverInk : accent);
                Add(Text(title, t.HeadingFont, 11 * s, t.CoverInk, true), 12, 34);
                Add(Text(sub, t.BodyFont, 6 * s, t.CoverInk), 12, 50);
                break;
            case "minimal":
                var tt = Text(title, t.HeadingFont, 10.5 * s, t.Ink, true, TextAlignment.Center); tt.Width = 160 * s; Add(tt, 0, 30);
                Rect(72, 47, 16, 1.2, accent);
                var st = Text(sub, t.BodyFont, 6 * s, t.Muted, false, TextAlignment.Center); st.Width = 160 * s; Add(st, 0, 51);
                break;
            default:
                Rect(12, 30, 2.5, 28, accent);
                Add(Text(title, t.HeadingFont, 11 * s, t.Ink, true), 19, 30);
                Add(Text(sub, t.BodyFont, 6 * s, t.Muted), 19, 47);
                break;
        }
        if (t.Footer.Length > 0) Add(Text(t.Footer, t.BodyFont, 4.5 * s, t.Cover == "full" ? t.CoverInk : t.Faint), 12, 88);
        if (t.Org.Length > 0) { var org = Text(t.Org, t.HeadingFont, 4.6 * s, t.Cover == "full" ? t.CoverInk : t.Muted, true, t.Cover == "hero" ? TextAlignment.Left : TextAlignment.Right); org.Width = (t.Cover == "hero" ? 58 : 76) * s; Add(org, t.Cover == "hero" ? 10 : 72, 86); }
        if (Logo(t, 10 * s) is { } logo) { var box = new Border { Background = t.Cover is "band" or "full" or "hero" ? Brushes.White : null, Padding = new Thickness(1.5 * s), CornerRadius = new CornerRadius(1.5 * s), Child = logo }; Add(box, t.Cover == "minimal" ? 70 : 8, 6); }
        return c;
    }

    static Image? Logo(DocTemplate t, double height)
    {
        if (t.LogoPath is not string path) return null;
        try
        {
            var bmp = new System.Windows.Media.Imaging.BitmapImage();
            bmp.BeginInit(); bmp.CacheOption = System.Windows.Media.Imaging.BitmapCacheOption.OnLoad; bmp.UriSource = new Uri(path); bmp.EndInit(); bmp.Freeze();
            return new Image { Source = bmp, Height = height, Stretch = Stretch.Uniform };
        }
        catch (Exception ex) when (ex is System.IO.IOException or NotSupportedException or UriFormatException) { return null; }
    }

    /// <summary>A light mock screen, used for captures in the miniature.</summary>
    static Border MockShot(double w, double h, string accent, double s)
    {
        var shot = new Border { Width = w * s, Height = h * s, Background = B("E8ECF2"), ClipToBounds = true };
        var mock = new Canvas();
        void M(double mx, double my, double mw, double mh, string color) { var r = new Rectangle { Width = mw * s, Height = mh * s, Fill = B(color) }; Canvas.SetLeft(r, mx * s); Canvas.SetTop(r, my * s); mock.Children.Add(r); }
        M(0, 0, w, 7, "CDD5E0"); M(4, 11, w * .28, h - 20, "D9DFE8"); M(w * .34, 11, w * .6, 5, "C2CBD8"); M(w * .34, 20, w * .5, 3, "D0D7E2"); M(w * .34, 26, w * .55, 3, "D0D7E2"); M(w * .34, 33, w * .2, 6, accent);
        shot.Child = mock;
        return shot;
    }

    static UIElement Step(DocTemplate t, string accent, double w, double h)
    {
        var c = new Canvas { Width = w, Height = h };
        double s = w / 160;
        string brand = t.CoverFillHex(accent);
        bool band = t.Header == "band", bar = t.FooterStyle == "bar", split = t.Layout == "split";
        void Add(UIElement e, double x, double y) { Canvas.SetLeft(e, x * s); Canvas.SetTop(e, y * s); c.Children.Add(e); }
        void Rect(double x, double y, double rw, double rh, string color) => Add(new Rectangle { Width = rw * s, Height = rh * s, Fill = B(color) }, x, y);
        if (t.TopRule && !band) Rect(0, 0, 160, 2, accent);
        double top;
        if (band) { Rect(0, 0, 160, 16, brand); top = 21; }
        else top = 22;
        // Step mark and title (in the band, or over a rule).
        double hy = band ? 4.5 : 8, x = 10;
        string stepInk = band ? "FFFFFF" : accent, titleInk = band ? "FFFFFF" : t.Ink;
        switch (t.Steps)
        {
            case "pill":
                Add(new Border { Background = B(band ? "FFFFFF" : accent), CornerRadius = new CornerRadius(5 * s), Padding = new Thickness(4 * s, 0.5 * s, 4 * s, 1 * s), Child = Text(L.T("Paso") + " 1", t.HeadingFont, 5.5 * s, band ? brand : "FFFFFF", true) }, x, hy);
                x += 30; break;
            case "number": Add(Text("01", t.HeadingFont, 10 * s, stepInk, true), x, hy - 3); x += 17; break;
            default: Add(Text(L.T("Paso") + " 1", t.HeadingFont, 6.5 * s, stepInk, true), x, hy); x += 24; break;
        }
        if (!split || band) Add(Text(L.T("Abrir la consola"), t.HeadingFont, 6.5 * s, titleInk, true), x, hy);
        if (!band && !split) Rect(10, 19, 140, .6, t.Rule);
        if (Logo(t, (band ? 7 : 6) * s) is { } logo) Add(new Border { Background = band ? Brushes.White : null, Padding = new Thickness(1 * s), Child = logo }, 128, band ? 3.5 : 5);
        double bottom = bar ? 90 : 92;
        if (bar) { Rect(0, 93, 160, 7, brand); Add(Text((t.Footer.Length > 0 ? t.Footer : t.Org).Length > 0 ? (t.Footer.Length > 0 ? t.Footer : t.Org) : "", t.HeadingFont, 3.8 * s, "FFFFFF", true), 10, 94.2); Add(Text("1 / 6", t.HeadingFont, 3.8 * s, "FFFFFF", true), 140, 94.2); }
        var shot = MockShot(split ? 96 : 120, split ? bottom - top - 4 : bottom - top - 12, accent, s);
        if (t.Frame != "none") { shot.BorderBrush = B(t.FrameHex(accent)); shot.BorderThickness = new Thickness(t.Frame == "accent" ? 1.4 : .8); }
        if (split)
        {
            if (!band)
            {
                var title = Text(L.T("Abrir la consola"), t.HeadingFont, 7 * s, t.Ink, true); title.Width = 40 * s; title.TextWrapping = TextWrapping.Wrap; title.TextTrimming = TextTrimming.None;
                Add(title, 10, 20); Rect(10, 40, 8, 1.2, accent);
            }
            for (int i = 0; i < 4; i++) Rect(10, (band ? 26 : 46) + i * 6, 38 - (i % 2) * 8, 2, t.Rule);
            Add(shot, 54, top + 2);
        }
        else
        {
            Add(shot, 20, top + 2);
            Rect(20, bottom - 6, 90, 2, t.Rule);
        }
        return c;
    }

}
