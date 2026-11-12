using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Shapes;
using System.Windows.Threading;
using D = System.Drawing;

namespace BuzzPrint;

/// <summary>Floating DBMON-style recording control: pulsing REC dot, clock, source, audio level and Stop / Discard.
/// It and the dashed frame around the recorded area are excluded from screen capture, so neither shows in the video.</summary>
public sealed class RecordingBar : Window
{
    [DllImport("user32.dll")] static extern bool SetWindowDisplayAffinity(IntPtr hwnd, uint affinity);
    [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hwnd, out Native.RECT rect);
    [DllImport("user32.dll")] static extern int GetWindowLong(IntPtr hwnd, int index);
    [DllImport("user32.dll")] static extern int SetWindowLong(IntPtr hwnd, int index, int value);
    const uint WDA_EXCLUDEFROMCAPTURE = 0x11, SWP_NOSIZE = 1, SWP_NOACTIVATE = 0x10, SWP_NOZORDER = 4;
    static readonly IntPtr HWND_TOPMOST = new(-1);

    readonly RecordKind kind;
    readonly D.Rectangle area;
    readonly TextBlock clock = new(), caption = new(), recLabel = new();
    readonly Border meter = new() { Width = 0, Height = 4, HorizontalAlignment = HorizontalAlignment.Left };
    readonly Ellipse dot = new() { Width = 9, Height = 9, Margin = new Thickness(0, 0, 7, 0), VerticalAlignment = VerticalAlignment.Center };
    readonly Button stop = new(), discard = new();
    readonly DispatcherTimer tick = new() { Interval = TimeSpan.FromMilliseconds(100) };
    readonly StackPanel audioIcons = new() { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
    Window? frame;
    Recorder? recorder;
    TaskCompletionSource<bool>? countdown;

    public RecordingBar(RecordKind kind, D.Rectangle area, string source)
    {
        this.kind = kind; this.area = area;
        WindowStyle = WindowStyle.None; ResizeMode = ResizeMode.NoResize; AllowsTransparency = true; Background = Brushes.Transparent;
        Topmost = true; ShowInTaskbar = false; SizeToContent = SizeToContent.WidthAndHeight; Left = -10000; Top = -10000; ShowActivated = true;
        UseLayoutRounding = true; TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        SetResourceReference(FontFamilyProperty, "UiFont");

        var card = new Border { BorderThickness = new Thickness(1), Padding = new Thickness(12, 8, 8, 8) };
        card.SetResourceReference(Border.BackgroundProperty, "Header");
        card.SetResourceReference(Border.BorderBrushProperty, "AccentCardRing");
        card.SetResourceReference(Border.CornerRadiusProperty, "ControlRadius");
        card.Effect = new System.Windows.Media.Effects.DropShadowEffect { BlurRadius = 18, ShadowDepth = 3, Opacity = .45, Color = Colors.Black };
        var outer = new Grid { Margin = new Thickness(14) };
        var rule = new Border { Height = 2, VerticalAlignment = VerticalAlignment.Top };
        rule.SetResourceReference(Border.BackgroundProperty, "Accent");
        outer.Children.Add(card); outer.Children.Add(rule);

        var row = new StackPanel { Orientation = Orientation.Horizontal };
        dot.Fill = new SolidColorBrush(Color.FromRgb(0xF4, 0x3F, 0x5E));
        row.Children.Add(dot);
        recLabel.Text = "REC"; recLabel.FontWeight = FontWeights.Bold; recLabel.FontSize = 11; recLabel.VerticalAlignment = VerticalAlignment.Center; recLabel.Margin = new Thickness(0, 0, 12, 0);
        recLabel.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont"); recLabel.SetResourceReference(TextBlock.ForegroundProperty, "Accent");
        row.Children.Add(recLabel);
        clock.Text = "00:00"; clock.FontSize = 16; clock.FontWeight = FontWeights.SemiBold; clock.MinWidth = 58; clock.VerticalAlignment = VerticalAlignment.Center;
        clock.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont"); clock.SetResourceReference(TextBlock.ForegroundProperty, "Ink");
        row.Children.Add(clock);
        var sep = new Border { Width = 1, Height = 22, Margin = new Thickness(10, 0, 10, 0) }; sep.SetResourceReference(Border.BackgroundProperty, "FaintBorder");
        row.Children.Add(sep);
        var info = new StackPanel { VerticalAlignment = VerticalAlignment.Center, Width = 170 };
        caption.Text = (kind == RecordKind.Video ? area.Width + "×" + area.Height + " · " : "") + source;
        caption.TextTrimming = TextTrimming.CharacterEllipsis; caption.FontSize = 11;
        caption.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        info.Children.Add(caption);
        var meterTrack = new Grid { Height = 4, Margin = new Thickness(0, 5, 0, 0) };
        var track = new Border(); track.SetResourceReference(Border.BackgroundProperty, "Canvas");
        meter.SetResourceReference(Border.BackgroundProperty, "Accent");
        meterTrack.Children.Add(track); meterTrack.Children.Add(meter);
        var meterRow = new DockPanel();
        DockPanel.SetDock(audioIcons, Dock.Left); meterRow.Children.Add(audioIcons); meterRow.Children.Add(meterTrack);
        info.Children.Add(meterRow);
        row.Children.Add(info);

        stop.Margin = new Thickness(12, 0, 4, 0);
        stop.SetResourceReference(BackgroundProperty, "Accent"); stop.SetResourceReference(ForegroundProperty, "OnAccent"); stop.SetResourceReference(BorderBrushProperty, "Accent");
        stop.Content = Glyph("", L.T("Detener"));
        stop.ToolTip = L.T("Detener y guardar") + " · " + Store.Settings.HotkeyRecord;
        stop.Click += (_, _) => { if (countdown != null) { countdown.TrySetResult(true); return; } App.Current.StopRecording(true); };
        row.Children.Add(stop);
        discard.Padding = new Thickness(9, 0, 9, 0);
        discard.Content = new TextBlock { Text = "", FontFamily = new FontFamily("Segoe Fluent Icons, Segoe MDL2 Assets"), FontSize = 12, VerticalAlignment = VerticalAlignment.Center };
        discard.ToolTip = L.T("Descartar la grabación");
        discard.Click += (_, _) => { if (countdown != null) { countdown.TrySetResult(false); return; } App.Current.StopRecording(false); };
        row.Children.Add(discard);
        card.Child = row;
        Content = outer;

        // Drag from anywhere but the buttons.
        MouseLeftButtonDown += (_, e) => { if (e.OriginalSource is not DependencyObject d || !IsInButton(d)) try { DragMove(); } catch (InvalidOperationException) { } };
        KeyDown += (_, e) => { if (e.Key == Key.Escape && countdown != null) countdown.TrySetResult(false); };
        SourceInitialized += (_, _) =>
        {
            var h = new WindowInteropHelper(this).Handle;
            // The visual check photographs the bar itself, so only there it stays capturable.
            if (!App.IsShot) SetWindowDisplayAffinity(h, WDA_EXCLUDEFROMCAPTURE);
        };
        ContentRendered += (_, _) => Place();
        tick.Tick += (_, _) => Tick();
        Closed += (_, _) => { tick.Stop(); frame?.Close(); };
        if (kind == RecordKind.Video) ShowFrame();
    }

    static bool IsInButton(DependencyObject d)
    {
        while (d != null) { if (d is Button) return true; d = d is Visual or System.Windows.Media.Media3D.Visual3D ? VisualTreeHelper.GetParent(d) : LogicalTreeHelper.GetParent(d); }
        return false;
    }

    static StackPanel Glyph(string glyph, string text)
    {
        var p = new StackPanel { Orientation = Orientation.Horizontal };
        p.Children.Add(new TextBlock { Text = glyph, FontFamily = new FontFamily("Segoe Fluent Icons, Segoe MDL2 Assets"), FontSize = 11, Margin = new Thickness(0, 0, 7, 0), VerticalAlignment = VerticalAlignment.Center });
        p.Children.Add(new TextBlock { Text = text, FontWeight = FontWeights.SemiBold, VerticalAlignment = VerticalAlignment.Center });
        return p;
    }

    /// <summary>Below the recorded area, else above, else inside its bottom edge (it is excluded from the video anyway).
    /// Audio-only: top center of the screen under the cursor.</summary>
    void Place()
    {
        var h = new WindowInteropHelper(this).Handle;
        GetWindowRect(h, out var r);
        int w = r.Right - r.Left, ht = r.Bottom - r.Top;
        var screen = kind == RecordKind.Video ? System.Windows.Forms.Screen.FromRectangle(area) : System.Windows.Forms.Screen.FromPoint(System.Windows.Forms.Cursor.Position);
        var wa = screen.WorkingArea;
        int x, y;
        if (kind == RecordKind.Audio) { x = wa.X + (wa.Width - w) / 2; y = wa.Y + 8; }
        else
        {
            x = area.X + area.Width / 2 - w / 2;
            if (area.Bottom + ht <= wa.Bottom) y = area.Bottom;
            else if (area.Top - ht >= wa.Top) y = area.Top - ht;
            else y = area.Bottom - ht - 12;
        }
        x = Math.Max(wa.X, Math.Min(x, wa.Right - w));
        y = Math.Max(wa.Y, Math.Min(y, wa.Bottom - ht));
        SetWindowPos(h, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE);
    }

    void ShowFrame()
    {
        var rect = new Rectangle { StrokeThickness = 2, StrokeDashArray = new DoubleCollection { 4, 3 } };
        rect.SetResourceReference(Shape.StrokeProperty, "Accent");
        frame = new Window
        {
            WindowStyle = WindowStyle.None, ResizeMode = ResizeMode.NoResize, AllowsTransparency = true, Background = Brushes.Transparent,
            Topmost = true, ShowInTaskbar = false, ShowActivated = false, Left = -10000, Top = -10000, Width = 10, Height = 10, Content = rect
        };
        frame.SourceInitialized += (_, _) =>
        {
            var h = new WindowInteropHelper(frame).Handle;
            SetWindowDisplayAffinity(h, WDA_EXCLUDEFROMCAPTURE);
            // Click-through and never activated: the frame must not get in the way of what is being recorded.
            SetWindowLong(h, -20, GetWindowLong(h, -20) | 0x20 | 0x80000 | 0x80 | 0x08000000);
            const int pad = 3;
            SetWindowPos(h, HWND_TOPMOST, area.X - pad, area.Y - pad, area.Width + pad * 2, area.Height + pad * 2, SWP_NOACTIVATE);
        };
        frame.Show();
    }

    /// <summary>3-2-1 before a video starts. Stop starts at once, ✕ or Esc cancels. Returns false when cancelled.</summary>
    public async Task<bool> CountdownAsync(int seconds)
    {
        countdown = new TaskCompletionSource<bool>();
        recLabel.Text = L.T("EN");
        stop.Content = Glyph("", L.T("Empezar"));
        Activate();
        for (int i = seconds; i > 0 && !countdown.Task.IsCompleted; i--)
        {
            clock.Text = i.ToString();
            await Task.WhenAny(countdown.Task, Task.Delay(1000));
        }
        bool go = !countdown.Task.IsCompleted || countdown.Task.Result;
        countdown = null;
        recLabel.Text = "REC";
        stop.Content = Glyph("", L.T("Detener"));
        clock.Text = "00:00";
        return go;
    }

    public void Attach(Recorder rec)
    {
        recorder = rec;
        audioIcons.Children.Clear();
        void Icon(string glyph, string tip)
        {
            var t = new TextBlock { Text = glyph, FontFamily = new FontFamily("Segoe Fluent Icons, Segoe MDL2 Assets"), FontSize = 10, Margin = new Thickness(0, 0, 5, 0), ToolTip = tip, VerticalAlignment = VerticalAlignment.Center };
            t.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
            audioIcons.Children.Add(t);
        }
        if (rec.WithSystem) Icon("", L.T("Audio del sistema"));
        if (rec.WithMic) Icon("", L.T("Micrófono"));
        if (!rec.WithSystem && !rec.WithMic) Icon("", L.T("Sin audio"));
        if (SystemParameters.ClientAreaAnimation)
            dot.BeginAnimation(OpacityProperty, new DoubleAnimation(1, .25, TimeSpan.FromSeconds(.6)) { AutoReverse = true, RepeatBehavior = RepeatBehavior.Forever });
        tick.Start();
    }

    void Tick()
    {
        if (recorder == null) return;
        clock.Text = Recorder.Clock(recorder.Elapsed);
        meter.Width = Math.Round(Math.Sqrt(recorder.Level) * 120);
    }

    public void ShowSaving()
    {
        tick.Stop();
        dot.BeginAnimation(OpacityProperty, null);
        recLabel.Text = L.T("GUARDANDO");
        stop.IsEnabled = discard.IsEnabled = false;
        frame?.Close(); frame = null;
    }
}
