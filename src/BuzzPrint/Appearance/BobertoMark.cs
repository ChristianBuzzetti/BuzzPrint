using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Threading;

namespace BuzzPrint;

/// <summary>How the BuzzPrint mark looks (Personalize → Effects → Logo). Tile "accent" follows the palette, "none" drops it.</summary>
public sealed record MarkStyle(string Shape = "hex", string Head = "#0E1015", string Eyes = "oval", string Tile = "accent")
{
    // Classic shapes plus soft, organic ones (each drawn from its own curve, see Outline).
    public static readonly string[] Shapes = { "hex", "circle", "squircle", "diamond", "ghost", "blob", "pebble", "cloud", "drop", "clover", "flower", "star", "burst", "heart" };
    public static readonly string[] EyeStyles = { "oval", "pill", "round", "happy", "line", "pixel" };
    public static readonly string[] HeadColors = { "#0E1015", "#F4F4F5", "#2684FF", "#10B981", "#F59E0B", "#8B5CF6", "#F43F5E" };
    public static readonly string[] TileColors = { "accent", "#F43F5E", "#0E1015", "#F4F4F5", "#2684FF", "#10B981", "#F59E0B", "#8B5CF6", "none" };

    public static MarkStyle FromSettings(Settings s) => new MarkStyle(
        Shapes.Contains(s.MarkShape) ? s.MarkShape : "hex",
        IsHex(s.MarkHead) ? s.MarkHead : "#0E1015",
        EyeStyles.Contains(s.MarkEyes) ? s.MarkEyes : "oval",
        s.MarkTile is "accent" or "none" || IsHex(s.MarkTile) ? s.MarkTile : "accent");
    public void Save(Settings s) { s.MarkShape = Shape; s.MarkHead = Head; s.MarkEyes = Eyes; s.MarkTile = Tile; }
    public static bool IsHex(string? v) => v is { Length: 7 } && v[0] == '#' && v.Skip(1).All(Uri.IsHexDigit);
    public static Color ToColor(string hex) => (Color)ColorConverter.ConvertFromString(hex);
}

/// <summary>
/// The BuzzPrint mark: Boberto's head with two eyes on a tile. The same drawing is the app icon (<see cref="Draw"/>).
/// Alive marks sway and float gently, bounce under the mouse, squash when they change and morph from one head to the
/// next. Sway, float, bounce and squash are WPF animations on the render transform (composited, smooth); only the
/// morph, a blink or the eyes moving redraw. The header mark's eyes follow the cursor. Animations off = still mark.
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
    public MarkStyle? Style { get => style; set { var before = Effective; style = value; Changed(before); } }
    MarkStyle? style;
    MarkStyle Effective => style ?? current;

    /// <summary>Eyes follow the cursor (header mark). Off for picker tiles, which still blink and move.</summary>
    public bool Follow { get; set; } = true;
    /// <summary>Resting tilt in degrees, so a row of shapes looks hand-placed rather than stamped.</summary>
    public double Tilt { get => tilt; set { tilt = value; rotate.Angle = value; if (animated && IsLoaded) StartIdle(); } }
    double tilt;

    /// <summary>Follow the cursor, blink and move. Off = still mark (reduced motion).</summary>
    public bool Animated
    {
        get => animated;
        set { animated = value; if (!value) { eye = target = default; blinkL = blinkR = 1; StopMotion(); InvalidateVisual(); } else if (IsLoaded) StartIdle(); Sync(); }
    }

    readonly DispatcherTimer timer = new(DispatcherPriority.Render) { Interval = TimeSpan.FromMilliseconds(16) };
    readonly Random random = new();
    readonly RotateTransform rotate = new(), wiggle = new();
    readonly ScaleTransform hover = new(1, 1), squish = new(1, 1);
    readonly TranslateTransform bob = new();
    bool animated = true;
    Vector eye, target;               // eye offset in 64-unit space
    double blinkL = 1, blinkR = 1;    // 1 open, 0 closed
    DateTime nextBlink = DateTime.Now.AddSeconds(2), blinkStart = DateTime.MinValue;
    enum Blink { Both, Alternate, Double }
    Blink blink;
    double blinkUnit = 150;
    Outline? morphFrom; DateTime morphStart = DateTime.MinValue;
    const double MorphMs = 420;
    MarkStyle? lastDrawn;

    public BobertoMark()
    {
        Cursor = Cursors.Hand;
        RenderTransformOrigin = new Point(.5, .55);
        RenderTransform = new TransformGroup { Children = { squish, hover, wiggle, rotate, bob } };
        timer.Tick += (_, _) => Step();
        IsVisibleChanged += (_, _) => Sync();
        Loaded += (_, _) => { CurrentChanged += OnCurrentChanged; if (animated) StartIdle(); Sync(); };
        Unloaded += (_, _) => { CurrentChanged -= OnCurrentChanged; timer.Stop(); StopMotion(); };
        MouseLeftButtonDown += (_, _) => { if (animated) Start((Blink)random.Next(3), 170); };
        MouseEnter += (_, _) => Hover(true);
        MouseLeave += (_, _) => Hover(false);
    }

    // ------------------------------------------------------------------ motion (composited)
    void StartIdle()
    {
        if (!animated) return;
        double phase = random.NextDouble();
        var sway = new DoubleAnimation(tilt - 3.2, tilt + 3.2, TimeSpan.FromSeconds(2.4 + phase))
            { AutoReverse = true, RepeatBehavior = RepeatBehavior.Forever, EasingFunction = new SineEase { EasingMode = EasingMode.EaseInOut } };
        rotate.BeginAnimation(RotateTransform.AngleProperty, sway);
        double lift = Math.Max(1, Math.Min(Width is > 0 ? Width : ActualWidth, Height is > 0 ? Height : ActualHeight) * .035);
        var floatY = new DoubleAnimation(lift * .4, -lift * .6, TimeSpan.FromSeconds(1.7 + phase * .6))
            { AutoReverse = true, RepeatBehavior = RepeatBehavior.Forever, EasingFunction = new SineEase { EasingMode = EasingMode.EaseInOut } };
        bob.BeginAnimation(TranslateTransform.YProperty, floatY);
    }

    void StopMotion()
    {
        rotate.BeginAnimation(RotateTransform.AngleProperty, null); rotate.Angle = tilt;
        bob.BeginAnimation(TranslateTransform.YProperty, null); bob.Y = 0;
        foreach (var dp in new[] { ScaleTransform.ScaleXProperty, ScaleTransform.ScaleYProperty }) { hover.BeginAnimation(dp, null); squish.BeginAnimation(dp, null); }
        hover.ScaleX = hover.ScaleY = squish.ScaleX = squish.ScaleY = 1;
        wiggle.BeginAnimation(RotateTransform.AngleProperty, null); wiggle.Angle = 0;
    }

    /// <summary>Grow with a spring and wiggle when the pointer arrives (also called by picker tiles).</summary>
    public void Hover(bool over)
    {
        if (!animated) return;
        var grow = new DoubleAnimation(over ? 1.14 : 1, TimeSpan.FromMilliseconds(over ? 420 : 300)) { EasingFunction = new ElasticEase { Oscillations = 1, Springiness = 4, EasingMode = EasingMode.EaseOut } };
        hover.BeginAnimation(ScaleTransform.ScaleXProperty, grow);
        hover.BeginAnimation(ScaleTransform.ScaleYProperty, grow);
        if (over)
        {
            var w = new DoubleAnimationUsingKeyFrames { Duration = TimeSpan.FromMilliseconds(520) };
            foreach (var (ms, a) in new[] { (0, 0.0), (90, -10.0), (200, 8.0), (310, -4.5), (420, 2.0), (520, 0.0) })
                w.KeyFrames.Add(new EasingDoubleKeyFrame(a, KeyTime.FromTimeSpan(TimeSpan.FromMilliseconds(ms)), new SineEase { EasingMode = EasingMode.EaseInOut }));
            wiggle.BeginAnimation(RotateTransform.AngleProperty, w);
        }
    }

    /// <summary>A springy squash, as when the shape changes.</summary>
    public void Squish()
    {
        if (!animated) return;
        DoubleAnimationUsingKeyFrames Frames(params double[] v)
        {
            var a = new DoubleAnimationUsingKeyFrames { Duration = TimeSpan.FromMilliseconds(560) };
            double[] at = { 0, 110, 240, 370, 560 };
            for (int i = 0; i < v.Length; i++) a.KeyFrames.Add(new EasingDoubleKeyFrame(v[i], KeyTime.FromTimeSpan(TimeSpan.FromMilliseconds(at[i])), new SineEase { EasingMode = EasingMode.EaseOut }));
            return a;
        }
        squish.BeginAnimation(ScaleTransform.ScaleXProperty, Frames(1, 1.2, .92, 1.04, 1));
        squish.BeginAnimation(ScaleTransform.ScaleYProperty, Frames(1, .82, 1.08, .97, 1));
    }

    void OnCurrentChanged() { if (style == null) Changed(lastDrawn); else InvalidateVisual(); }

    /// <summary>A new style: morph the head from the one on screen and squash; still marks just redraw.</summary>
    void Changed(MarkStyle? before)
    {
        if (animated && IsLoaded && before != null && before.Shape != Effective.Shape)
        {
            morphFrom = Outline.For(before.Shape);
            morphStart = DateTime.Now;
            Squish();
            Sync();
        }
        InvalidateVisual();
    }

    // ------------------------------------------------------------------ eyes, blink, morph (redraw only when needed)
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
        bool dirty = false;
        if (Follow && GetCursorPos(out var p) && PresentationSource.FromVisual(this) != null)
        {
            Point centre = PointToScreen(new Point(ActualWidth / 2, ActualHeight * .48));
            var d = new Vector(p.X - centre.X, p.Y - centre.Y);
            double len = d.Length;
            target = len < 1 ? default : d / len * Math.Min(1, len / 220);
            target = new Vector(target.X * 3.4, target.Y * 2.6);
        }
        var next = eye + (target - eye) * .16;
        if ((next - eye).LengthSquared > 0.0004) { eye = next; dirty = true; }

        var now = DateTime.Now;
        if (blinkStart == DateTime.MinValue && now >= nextBlink) Start(random.NextDouble() < .15 ? Blink.Double : Blink.Both, 150);
        if (blinkStart != DateTime.MinValue)
        {
            dirty = true;
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
        if (morphFrom != null) { dirty = true; if ((now - morphStart).TotalMilliseconds >= MorphMs) morphFrom = null; }
        if (dirty) InvalidateVisual();
    }

    protected override void OnRender(DrawingContext dc)
    {
        double s = Math.Min(ActualWidth, ActualHeight);
        if (s <= 0) return;
        lastDrawn = Effective;
        dc.PushTransform(new TranslateTransform((ActualWidth - s) / 2, (ActualHeight - s) / 2));
        Outline? head = null;
        if (morphFrom != null)
        {
            double t = Math.Min(1, (DateTime.Now - morphStart).TotalMilliseconds / MorphMs);
            double k = 1 + 2.2 * Math.Pow(t - 1, 3) + 1.2 * Math.Pow(t - 1, 2);   // ease out with a jelly overshoot
            head = Outline.Lerp(morphFrom, Outline.For(Effective.Shape), k);
        }
        Draw(dc, s, Effective, Tile, eye, blinkL, blinkR, head);
        dc.Pop();
    }

    // ------------------------------------------------------------------ drawing

    /// <summary>Draws the mark in a square of <paramref name="size"/> px. Eye offset is in 64-unit space.</summary>
    public static void Draw(DrawingContext g, double size, MarkStyle st, Brush accent, Vector eye = default, double blinkLeft = 1, double blinkRight = 1, Outline? headOverride = null)
    {
        bool tiny = size <= 40;
        g.PushTransform(new ScaleTransform(size / 64, size / 64));
        if (st.Tile != "none") g.DrawRoundedRectangle(st.Tile == "accent" ? accent : Frozen(MarkStyle.ToColor(st.Tile)), null, new Rect(2, 2, 60, 60), 14, 14);
        bool bare = st.Tile == "none";
        if (bare) g.PushTransform(new ScaleTransform(1.25, 1.25, 32, 32));   // no tile: the head fills the square

        var headColor = MarkStyle.ToColor(st.Head);
        var outline = headOverride ?? Outline.For(st.Shape);
        g.DrawGeometry(Frozen(headColor), null, outline.Geometry());
        double cy = outline.EyeY;

        // Eyes contrast with the head: white on dark heads, ink on light ones.
        double lum = (.2126 * headColor.R + .7152 * headColor.G + .0722 * headColor.B) / 255;
        var eyeBrush = lum > .62 ? DarkEye : LightEye;
        double gap = tiny ? 7 : 6.5;
        DrawEye(g, st.Eyes, eyeBrush, new Point(32 - gap + eye.X, cy + eye.Y), blinkLeft, tiny);
        DrawEye(g, st.Eyes, eyeBrush, new Point(32 + gap + eye.X, cy + eye.Y), blinkRight, tiny);
        if (bare) g.Pop();
        g.Pop();
    }

    static readonly Dictionary<Color, Brush> brushes = [];   // declared first: the eye brushes below use it
    static readonly Brush DarkEye = Frozen(Color.FromRgb(0x0E, 0x10, 0x15)), LightEye = Frozen(Color.FromRgb(0xF4, 0xF4, 0xF5));
    static Brush Frozen(Color c)
    {
        if (brushes.TryGetValue(c, out var b)) return b;
        var nb = new SolidColorBrush(c); nb.Freeze();
        return brushes[c] = nb;
    }

    static void DrawEye(DrawingContext g, string kind, Brush b, Point c, double open, bool tiny)
    {
        double k = tiny ? 1.2 : 1;
        switch (kind)
        {
            case "round": g.DrawEllipse(b, null, c, 4.6 * k, 4.6 * k * open); break;
            case "pill": { double h = 11.5 * k * open, w = 5.2 * k; g.DrawRoundedRectangle(b, null, new Rect(c.X - w / 2, c.Y - h / 2, w, h), w / 2, Math.Min(w / 2, h / 2)); break; }
            case "line": g.DrawRoundedRectangle(b, null, new Rect(c.X - 4.6 * k, c.Y - 1.6 * k * open, 9.2 * k, 3.2 * k * open), 1.6 * k, 1.6 * k * open); break;
            case "pixel": { double h = 7 * k * open; g.DrawRoundedRectangle(b, null, new Rect(c.X - 3.5 * k, c.Y - h / 2, 7 * k, h), 1.2, 1.2); break; }
            case "happy":
            {
                var arc = new StreamGeometry();
                using (var ctx = arc.Open()) { ctx.BeginFigure(new Point(c.X - 4.2 * k, c.Y + 2 * k), false, false); ctx.QuadraticBezierTo(new Point(c.X, c.Y - 5.5 * k * open), new Point(c.X + 4.2 * k, c.Y + 2 * k), true, true); }
                g.DrawGeometry(null, new Pen(b, 3.2 * k) { StartLineCap = PenLineCap.Round, EndLineCap = PenLineCap.Round }, arc);
                break;
            }
            default: g.DrawEllipse(b, null, c, 3.6 * k, 5.4 * k * open); break;
        }
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

/// <summary>
/// A head as a radial profile: the distance from (32,32) at <see cref="N"/> evenly spaced angles, plus where the eyes
/// sit. Every head has one, so any two can morph by interpolating radii. Profiles are computed once and cached;
/// polar heads come straight from their curve, the others by casting rays against their outline.
/// </summary>
public sealed class Outline
{
    public const int N = 120;
    public double[] R { get; }
    public double EyeY { get; }
    Geometry? geometry;
    Outline(double[] r, double eyeY) { R = r; EyeY = eyeY; }

    static readonly Dictionary<string, Outline> cache = [];
    public static Outline For(string shape)
    {
        if (cache.TryGetValue(shape, out var o)) return o;
        return cache[shape] = Build(shape);
    }

    public static Outline Lerp(Outline a, Outline b, double t)
    {
        var r = new double[N];
        for (int i = 0; i < N; i++) r[i] = Math.Max(0, a.R[i] + (b.R[i] - a.R[i]) * t);
        return new Outline(r, a.EyeY + (b.EyeY - a.EyeY) * t);
    }

    public Geometry Geometry()
    {
        if (geometry != null) return geometry;
        var geo = new StreamGeometry();
        using (var c = geo.Open())
        {
            var pts = new List<Point>(N);
            for (int i = 0; i < N; i++) { double a = i * 2 * Math.PI / N; pts.Add(new Point(32 + Math.Cos(a) * R[i], 32 + Math.Sin(a) * R[i])); }
            c.BeginFigure(pts[0], true, true);
            c.PolyLineTo(pts.Skip(1).ToList(), true, true);
        }
        geo.Freeze();
        return geometry = geo;
    }

    static Outline Polar(Func<double, double> r, double eyeY = 32, double sx = 1, double sy = 1)
    {
        var rr = new double[N];
        for (int i = 0; i < N; i++)
        {
            double a = i * 2 * Math.PI / N, v = r(a);
            double x = Math.Cos(a) * v * sx, y = Math.Sin(a) * v * sy;
            rr[i] = Math.Sqrt(x * x + y * y);
        }
        return new Outline(rr, eyeY);
    }

    static Outline Build(string shape)
    {
        switch (shape)
        {
            case "circle": return Polar(_ => 19.5);
            case "blob": return Polar(a => 19.2 * (1 + .055 * Math.Cos(3 * a + .5) + .035 * Math.Sin(2 * a)));
            case "pebble": return Polar(a => 19 * (1 + .09 * Math.Cos(2 * a + .9) + .045 * Math.Cos(3 * a + 2.1)), 33, 1.08, .92);
            case "clover": return Polar(a => 16.8 + 4.4 * Math.Cos(4 * a + Math.PI / 4));
            case "flower": return Polar(a => 17.2 + 3 * Math.Cos(6 * a));
            case "star": return Polar(a => 13.6 + 9 * Math.Pow((1 + Math.Cos(4 * a)) / 2, 2.4));
            case "burst": return Polar(a => 17.6 + 2.4 * Math.Cos(9 * a));
        }
        Geometry g; double eyeY = 32;
        switch (shape)
        {
            case "squircle": g = new RectangleGeometry(new Rect(13, 13, 38, 38), 13, 13); break;
            case "diamond": g = Rounded(new Point(32, 12), new Point(52, 32), new Point(32, 52), new Point(12, 32)); break;
            case "ghost": eyeY = 31; g = System.Windows.Media.Geometry.Parse("M14.5 32 A17.5 17.5 0 0 1 49.5 32 V49 Q46.6 52.5 43.7 49 Q40.8 45.5 37.9 49 Q35 52.5 32 49 Q29 45.5 26.1 49 Q23.2 52.5 20.3 49 Q17.4 45.5 14.5 49 Z"); break;
            case "cloud":
                eyeY = 35;
                var cloud = new GeometryGroup { FillRule = FillRule.Nonzero };
                cloud.Children.Add(new EllipseGeometry(new Point(23, 37), 10.5, 10.5));
                cloud.Children.Add(new EllipseGeometry(new Point(33, 28.5), 13, 13));
                cloud.Children.Add(new EllipseGeometry(new Point(43, 36), 10, 10));
                cloud.Children.Add(new RectangleGeometry(new Rect(22, 34, 22, 13), 6, 6));
                g = cloud; break;
            case "drop": eyeY = 36; g = System.Windows.Media.Geometry.Parse("M32 11 C39 20 50.5 28.5 50.5 38.5 A18.5 18.5 0 0 1 13.5 38.5 C13.5 28.5 25 20 32 11 Z"); break;
            case "heart": eyeY = 31; g = System.Windows.Media.Geometry.Parse("M32 51 C19 43 11.5 35 12.5 25.5 C13.5 17.5 22.5 14 28.5 18.5 C30.5 20 31.5 21.5 32 23 C32.5 21.5 33.5 20 35.5 18.5 C41.5 14 50.5 17.5 51.5 25.5 C52.5 35 45 43 32 51 Z"); break;
            default: g = Rounded(new Point(32, 13), new Point(48.5, 22.5), new Point(48.5, 41.5), new Point(32, 51), new Point(15.5, 41.5), new Point(15.5, 22.5)); eyeY = 31; break;   // hex
        }
        return Cast(g, eyeY);
    }

    /// <summary>A polygon with soft corners: the fill plus a 7-unit round stroke (the classic mark's look).</summary>
    static Geometry Rounded(params Point[] pts)
    {
        var fig = new PathFigure(pts[0], pts.Skip(1).Select(p => (PathSegment)new LineSegment(p, true)), true);
        var poly = new PathGeometry([fig]);
        var widened = poly.GetWidenedPathGeometry(new Pen(Brushes.Black, 7) { LineJoin = PenLineJoin.Round });
        return System.Windows.Media.Geometry.Combine(poly, widened, GeometryCombineMode.Union, null);
    }

    /// <summary>Radii by ray casting from (32,32) against the flattened outline (farthest hit per angle).</summary>
    static Outline Cast(Geometry g, double eyeY)
    {
        var flat = g.GetFlattenedPathGeometry(0.05, ToleranceType.Absolute);
        var edges = new List<(Point a, Point b)>();
        foreach (var f in flat.Figures)
        {
            var pts = new List<Point> { f.StartPoint };
            foreach (var seg in f.Segments)
                if (seg is PolyLineSegment pl) pts.AddRange(pl.Points); else if (seg is LineSegment ls) pts.Add(ls.Point);
            for (int i = 0; i < pts.Count; i++) edges.Add((pts[i], pts[(i + 1) % pts.Count]));
        }
        var r = new double[N];
        var o = new Point(32, 32);
        for (int i = 0; i < N; i++)
        {
            double a = i * 2 * Math.PI / N, dx = Math.Cos(a), dy = Math.Sin(a), best = 0;
            foreach (var (p, q) in edges)
            {
                double ex = q.X - p.X, ey = q.Y - p.Y, den = dx * ey - dy * ex;
                if (Math.Abs(den) < 1e-9) continue;
                double t = ((p.X - o.X) * ey - (p.Y - o.Y) * ex) / den, u = ((p.X - o.X) * dy - (p.Y - o.Y) * dx) / den;
                if (t > 0 && u >= 0 && u <= 1 && t > best) best = t;
            }
            r[i] = best;
        }
        return new Outline(r, eyeY);
    }
}
