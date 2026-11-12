using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Threading;

namespace BuzzPrint;

/// <summary>How the BuzzPrint mark looks (Personalize → Effects → Logo). Tile "accent" follows the palette, "none" drops it.</summary>
public sealed record MarkStyle(string Shape = "hex", string Head = "#0E1015", string Eyes = "oval", string Tile = "accent")
{
    public static readonly string[] Shapes = { "hex", "circle", "squircle", "diamond", "ghost" };
    public static readonly string[] EyeStyles = { "oval", "round", "happy", "line", "pixel" };
    public static readonly string[] HeadColors = { "#0E1015", "#F4F4F5", "#2684FF", "#10B981", "#F59E0B", "#8B5CF6", "#F43F5E" };
    public static readonly string[] TileColors = { "accent", "#F43F5E", "#0E1015", "#F4F4F5", "#2684FF", "#10B981", "#F59E0B", "#8B5CF6", "none" };

    public static MarkStyle FromSettings(Settings s) => new MarkStyle(
        Shapes.Contains(s.MarkShape) ? s.MarkShape : "hex",
        IsHex(s.MarkHead) ? s.MarkHead : "#0E1015",
        EyeStyles.Contains(s.MarkEyes) ? s.MarkEyes : "oval",
        s.MarkTile is "accent" or "none" || IsHex(s.MarkTile) ? s.MarkTile : "accent");
    public void Save(Settings s) { s.MarkShape = Shape; s.MarkHead = Head; s.MarkEyes = Eyes; s.MarkTile = Tile; }
    static bool IsHex(string? v) => v is { Length: 7 } && v[0] == '#' && v.Skip(1).All(Uri.IsHexDigit);
    public static Color ToColor(string hex) => (Color)ColorConverter.ConvertFromString(hex);
}

/// <summary>
/// The BuzzPrint mark: Boberto's head with two eyes on a tile. The same drawing is the app icon (<see cref="Draw"/>);
/// in the header it is alive: the eyes follow the cursor anywhere on screen, it blinks every few seconds and, when
/// clicked, blinks both eyes, alternates them or double-blinks. With animations off it stays still.
/// </summary>
public sealed class BobertoMark : FrameworkElement
{
    public static readonly DependencyProperty TileProperty = DependencyProperty.Register(nameof(Tile), typeof(Brush), typeof(BobertoMark),
        new FrameworkPropertyMetadata(new SolidColorBrush(Color.FromRgb(0xF4, 0x3F, 0x5E)), FrameworkPropertyMetadataOptions.AffectsRender));
    /// <summary>Tile brush used when the style's tile is "accent" (bind it to the Accent resource).</summary>
    public Brush Tile { get => (Brush)GetValue(TileProperty); set => SetValue(TileProperty, value); }

    /// <summary>The style every live mark uses; set it to restyle all of them at once.</summary>
    public static MarkStyle Current { get => current; set { current = value; CurrentChanged?.Invoke(); } }
    static MarkStyle current = new();
    public static event Action? CurrentChanged;
    /// <summary>A fixed style for previews (option cards); null = <see cref="Current"/>.</summary>
    public MarkStyle? Style { get => style; set { style = value; InvalidateVisual(); } }
    MarkStyle? style;

    /// <summary>Follow the cursor and blink. Off = still mark (reduced motion).</summary>
    public bool Animated { get => animated; set { animated = value; if (!value) { eye = target = default; blinkL = blinkR = 1; InvalidateVisual(); } Sync(); } }

    readonly DispatcherTimer timer = new(DispatcherPriority.Render) { Interval = TimeSpan.FromMilliseconds(33) };
    readonly Random random = new();
    bool animated = true;
    Vector eye, target;               // eye offset in 64-unit space
    double blinkL = 1, blinkR = 1;    // 1 open, 0 closed
    DateTime nextBlink = DateTime.Now.AddSeconds(2), blinkStart = DateTime.MinValue;
    enum Blink { Both, Alternate, Double }
    Blink blink;
    double blinkUnit = 150;

    public BobertoMark()
    {
        Cursor = Cursors.Hand;
        timer.Tick += (_, _) => Step();
        IsVisibleChanged += (_, _) => Sync();
        Loaded += (_, _) => { CurrentChanged += InvalidateVisual; Sync(); };
        Unloaded += (_, _) => { CurrentChanged -= InvalidateVisual; timer.Stop(); };
        // Click: both eyes, one after the other, or a quick double blink.
        MouseLeftButtonDown += (_, _) => { if (animated) Start((Blink)random.Next(3), 170); };
    }

    void Start(Blink kind, double unit) { blink = kind; blinkUnit = unit; blinkStart = DateTime.Now; }

    void Sync()
    {
        bool run = animated && IsVisible && IsLoaded;
        if (run && !timer.IsEnabled) timer.Start(); else if (!run && timer.IsEnabled) timer.Stop();
    }

    [StructLayout(LayoutKind.Sequential)] struct POINT { public int X, Y; }
    [DllImport("user32.dll")] static extern bool GetCursorPos(out POINT p);

    static double Open(double t) => t < 0 || t >= 1 ? 1 : Math.Abs(1 - 2 * t);   // closes, then reopens

    void Step()
    {
        var window = Window.GetWindow(this);
        if (window == null || window.WindowState == WindowState.Minimized) return;
        // Eyes: look toward the cursor, wherever it is on screen; the reach eases off with distance.
        if (GetCursorPos(out var p) && PresentationSource.FromVisual(this) != null)
        {
            Point centre = PointToScreen(new Point(ActualWidth / 2, ActualHeight * .48));
            var d = new Vector(p.X - centre.X, p.Y - centre.Y);
            double len = d.Length;
            target = len < 1 ? default : d / len * Math.Min(1, len / 220);
            target = new Vector(target.X * 3.4, target.Y * 2.6);
        }
        eye += (target - eye) * .22;

        var now = DateTime.Now;
        if (blinkStart == DateTime.MinValue && now >= nextBlink) Start(random.NextDouble() < .15 ? Blink.Double : Blink.Both, 150);
        if (blinkStart != DateTime.MinValue)
        {
            double t = (now - blinkStart).TotalMilliseconds / blinkUnit;
            double length;
            switch (blink)
            {
                case Blink.Alternate: blinkL = Open(t); blinkR = Open(t - .6); length = 1.6; break;
                case Blink.Double: blinkL = blinkR = Math.Min(Open(t), Open(t - 1.3)); length = 2.3; break;
                default: blinkL = blinkR = Open(t); length = 1; break;
            }
            blinkL = Math.Max(.08, blinkL); blinkR = Math.Max(.08, blinkR);
            if (t >= length)
            {
                blinkStart = DateTime.MinValue; blinkL = blinkR = 1;
                nextBlink = now.AddMilliseconds(2500 + random.Next(3500));
            }
        }
        InvalidateVisual();
    }

    protected override void OnRender(DrawingContext dc)
    {
        double s = Math.Min(ActualWidth, ActualHeight);
        if (s <= 0) return;
        dc.PushTransform(new TranslateTransform((ActualWidth - s) / 2, (ActualHeight - s) / 2));
        Draw(dc, s, style ?? current, Tile, eye, blinkL, blinkR);
        dc.Pop();
    }

    /// <summary>Draws the mark in a square of <paramref name="size"/> px. Eye offset is in 64-unit space.</summary>
    public static void Draw(DrawingContext g, double size, MarkStyle st, Brush accent, Vector eye = default, double blinkLeft = 1, double blinkRight = 1)
    {
        bool tiny = size <= 40;
        g.PushTransform(new ScaleTransform(size / 64, size / 64));
        if (st.Tile != "none") g.DrawRoundedRectangle(st.Tile == "accent" ? accent : new SolidColorBrush(MarkStyle.ToColor(st.Tile)), null, new Rect(2, 2, 60, 60), 14, 14);
        bool bare = st.Tile == "none";
        if (bare) g.PushTransform(new ScaleTransform(1.25, 1.25, 32, 32));   // no tile: the head fills the square

        var headColor = MarkStyle.ToColor(st.Head);
        var ink = new SolidColorBrush(headColor);
        var round = new Pen(ink, 7) { LineJoin = PenLineJoin.Round };
        double cy = 31;
        switch (st.Shape)
        {
            case "circle": g.DrawEllipse(ink, null, new Point(32, 32), 19.5, 19.5); cy = 32; break;
            case "squircle": g.DrawRoundedRectangle(ink, null, new Rect(13, 13, 38, 38), 13, 13); cy = 32; break;
            case "diamond": g.DrawGeometry(ink, round, Poly((32, 12), (52, 32), (32, 52), (12, 32))); cy = 32; break;
            case "ghost":
                var ghost = Geometry.Parse("M14.5 32 A17.5 17.5 0 0 1 49.5 32 V49 Q46.6 52.5 43.7 49 Q40.8 45.5 37.9 49 Q35 52.5 32 49 Q29 45.5 26.1 49 Q23.2 52.5 20.3 49 Q17.4 45.5 14.5 49 Z");
                g.DrawGeometry(ink, null, ghost); cy = 31; break;
            default: g.DrawGeometry(ink, round, Poly((32, 13), (48.5, 22.5), (48.5, 41.5), (32, 51), (15.5, 41.5), (15.5, 22.5))); break;
        }

        // Eyes contrast with the head: white on dark heads, ink on light ones.
        double lum = (.2126 * headColor.R + .7152 * headColor.G + .0722 * headColor.B) / 255;
        var eyeBrush = new SolidColorBrush(lum > .62 ? Color.FromRgb(0x0E, 0x10, 0x15) : Color.FromRgb(0xF4, 0xF4, 0xF5));
        double gap = tiny ? 7 : 6.5;
        DrawEye(g, st.Eyes, eyeBrush, new Point(32 - gap + eye.X, cy + eye.Y), blinkLeft, tiny);
        DrawEye(g, st.Eyes, eyeBrush, new Point(32 + gap + eye.X, cy + eye.Y), blinkRight, tiny);
        if (bare) g.Pop();
        g.Pop();
    }

    static void DrawEye(DrawingContext g, string kind, Brush b, Point c, double open, bool tiny)
    {
        double k = tiny ? 1.2 : 1;
        switch (kind)
        {
            case "round": g.DrawEllipse(b, null, c, 4.6 * k, 4.6 * k * open); break;
            case "line": g.DrawRoundedRectangle(b, null, new Rect(c.X - 4.6 * k, c.Y - 1.6 * k * open, 9.2 * k, 3.2 * k * open), 1.6 * k, 1.6 * k * open); break;
            case "pixel": { double h = 7 * k * open; g.DrawRoundedRectangle(b, null, new Rect(c.X - 3.5 * k, c.Y - h / 2, 7 * k, h), 1.2, 1.2); break; }
            case "happy":
            {
                // ∩ arc: a smiling, squinting eye; blinking flattens it.
                var arc = new StreamGeometry();
                using (var ctx = arc.Open()) { ctx.BeginFigure(new Point(c.X - 4.2 * k, c.Y + 2 * k), false, false); ctx.QuadraticBezierTo(new Point(c.X, c.Y - 5.5 * k * open), new Point(c.X + 4.2 * k, c.Y + 2 * k), true, true); }
                g.DrawGeometry(null, new Pen(b, 3.2 * k) { StartLineCap = PenLineCap.Round, EndLineCap = PenLineCap.Round }, arc);
                break;
            }
            default: g.DrawEllipse(b, null, c, 3.6 * k, 5.4 * k * open); break;
        }
    }

    static Geometry Poly(params (double x, double y)[] pts)
    {
        var geo = new StreamGeometry();
        using (var c = geo.Open())
        {
            c.BeginFigure(new Point(pts[0].x, pts[0].y), true, true);
            c.PolyLineTo(pts.Skip(1).Select(p => new Point(p.x, p.y)).ToList(), true, true);
        }
        geo.Freeze();
        return geo;
    }

    /// <summary>The current mark as a bitmap (window icon, tray).</summary>
    public static System.Windows.Media.Imaging.BitmapSource Render(int size, Brush accent)
    {
        var visual = new DrawingVisual();
        using (var g = visual.RenderOpen()) Draw(g, size, current, accent);
        var bmp = new System.Windows.Media.Imaging.RenderTargetBitmap(size, size, 96, 96, PixelFormats.Pbgra32);
        bmp.Render(visual);
        bmp.Freeze();
        return bmp;
    }
}
