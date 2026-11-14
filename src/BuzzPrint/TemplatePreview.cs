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
        return c;
    }

    static UIElement Step(DocTemplate t, string accent, double w, double h)
    {
        var c = new Canvas { Width = w, Height = h };
        double s = w / 160;
        void Add(UIElement e, double x, double y) { Canvas.SetLeft(e, x * s); Canvas.SetTop(e, y * s); c.Children.Add(e); }
        void Rect(double x, double y, double rw, double rh, string color) => Add(new Rectangle { Width = rw * s, Height = rh * s, Fill = B(color) }, x, y);
        if (t.TopRule) Rect(0, 0, 160, 2, accent);
        double x = 10;
        switch (t.Steps)
        {
            case "pill":
                var pill = new Border { Background = B(accent), CornerRadius = new CornerRadius(5 * s), Padding = new Thickness(4 * s, 0.5 * s, 4 * s, 1 * s), Child = Text(L.T("Paso") + " 1", t.HeadingFont, 5.5 * s, "FFFFFF", true) };
                Add(pill, x, 8); x += 30;
                break;
            case "number":
                Add(Text("01", t.HeadingFont, 10 * s, accent, true), x, 5); x += 17;
                break;
            default:
                Add(Text(L.T("Paso") + " 1", t.HeadingFont, 6.5 * s, accent, true), x, 8); x += 24;
                break;
        }
        Add(Text(L.T("Abrir la consola"), t.HeadingFont, 6.5 * s, t.Ink, true), x, 8);
        Rect(10, 19, 140, .6, t.Rule);
        // The capture: a light mock screen with the template's frame.
        var shot = new Border { Width = 120 * s, Height = 58 * s, Background = B("EEF1F5") };
        if (t.Frame != "none") { shot.BorderBrush = B(t.FrameHex(accent)); shot.BorderThickness = new Thickness(t.Frame == "accent" ? 1.4 : .8); }
        var mock = new Canvas();
        void M(double mx, double my, double mw, double mh, string color) { var r = new Rectangle { Width = mw * s, Height = mh * s, Fill = B(color) }; Canvas.SetLeft(r, mx * s); Canvas.SetTop(r, my * s); mock.Children.Add(r); }
        M(0, 0, 120, 8, "D6DCE5"); M(4, 12, 30, 40, "DFE4EC"); M(38, 12, 76, 5, "C9D1DC"); M(38, 21, 60, 3, "D6DCE5"); M(38, 27, 70, 3, "D6DCE5"); M(38, 33, 22, 7, accent);
        shot.Child = mock;
        Add(shot, 20, 24);
        Rect(20, 88, 90, 2, t.Rule);
        return c;
    }
}
