using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Input;
using System.Windows.Media;

namespace BuzzPrint;

/// <summary>
/// Popup near the cursor with the latest captures, as a Neo Mirai card. 1-9 / click = copy (and paste into the
/// window that was active), Shift = copy only, E / right click = edit, Esc closes.
/// </summary>
public sealed class QuickPickWindow : Window
{
    static QuickPickWindow? open;
    readonly IntPtr previous;
    readonly List<CaptureItem> items;
    readonly List<Border> cells = [];
    int index;
    bool closing;
    const int Cols = 4;

    public static void Toggle(IntPtr previousWindow)
    {
        if (open != null) { open.SafeClose(); return; }
        open = new QuickPickWindow(previousWindow, Store.All().Take(Math.Clamp(Store.Settings.QuickPickCount, 1, 20)).ToList());
        open.Show();
        open.Activate();
    }

    internal static QuickPickWindow ForShot() => new(IntPtr.Zero, Store.All().Take(8).ToList());

    QuickPickWindow(IntPtr prev, List<CaptureItem> list)
    {
        previous = prev; items = list;
        SetResourceReference(StyleProperty, typeof(Window));
        TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        WindowStyle = WindowStyle.None; ResizeMode = ResizeMode.NoResize; AllowsTransparency = true; Background = Brushes.Transparent;
        Topmost = true; ShowInTaskbar = false; SizeToContent = SizeToContent.WidthAndHeight; Title = L.T("BuzzPrint · Menú rápido");

        var card = new ContentControl { Margin = new Thickness(14, 10, 14, 22) };
        card.SetResourceReference(StyleProperty, "NeoCard");
        var root = new DockPanel { Margin = new Thickness(16, 14, 16, 12) };
        card.Content = root;

        var head = new DockPanel { Margin = new Thickness(0, 0, 0, 10) };
        var count = Mono(items.Count + (items.Count == 1 ? L.T(" CAPTURA") : L.T(" CAPTURAS")), "Muted");
        DockPanel.SetDock(count, Dock.Right); head.Children.Add(count);
        var title = new StackPanel { Orientation = Orientation.Horizontal };
        var img = new BobertoMark { Width = 22, Height = 22, Margin = new Thickness(0, 0, 8, 0), Animated = App.Current.Appearance.BobertoMotion && SystemParameters.ClientAreaAnimation };
        img.SetResourceReference(BobertoMark.TileProperty, "Accent");
        title.Children.Add(img);
        var t = Mono(L.T("◤ CAPTURAS RECIENTES"), "Accent"); t.FontWeight = FontWeights.Bold; t.FontSize = 12; title.Children.Add(t);
        head.Children.Add(title);
        DockPanel.SetDock(head, Dock.Top); root.Children.Add(head);
        var rule = new Border { Height = 1.5, Margin = new Thickness(0, 0, 0, 12) }; rule.SetResourceReference(Border.BackgroundProperty, "AccentRule");
        DockPanel.SetDock(rule, Dock.Top); root.Children.Add(rule);

        var foot = new Border { BorderThickness = new Thickness(0, 1, 0, 0), Padding = new Thickness(0, 10, 0, 0), Margin = new Thickness(0, 6, 0, 0) };
        foot.SetResourceReference(Border.BorderBrushProperty, "FaintBorder");
        foot.Child = Mono(Store.Settings.PasteOnPick ? L.T("1-9 / CLIC  COPIAR Y PEGAR   ·   SHIFT  SOLO COPIAR   ·   E  EDITAR   ·   ESC") : L.T("1-9 / CLIC  COPIAR   ·   E  EDITAR   ·   ESC"), "Muted");
        DockPanel.SetDock(foot, Dock.Bottom); root.Children.Add(foot);

        if (items.Count == 0)
        {
            var empty = new TextBlock { Text = L.T("Todavía no hay capturas. Apretá PrtSc para tomar una."), Margin = new Thickness(0, 20, 0, 20), Width = 420 };
            empty.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
            root.Children.Add(empty);
        }
        else
        {
            var grid = new UniformGrid { Columns = Math.Min(Cols, items.Count) };
            for (int i = 0; i < items.Count; i++) grid.Children.Add(Cell(i));
            root.Children.Add(grid);
        }
        Content = card;

        Loaded += (_, _) =>
        {
            // Near the cursor, kept inside the work area of that screen.
            var src = PresentationSource.FromVisual(this);
            double sx = src?.CompositionTarget?.TransformFromDevice.M11 ?? 1, sy = src?.CompositionTarget?.TransformFromDevice.M22 ?? 1;
            var cur = System.Windows.Forms.Cursor.Position;
            var wa = System.Windows.Forms.Screen.FromPoint(cur).WorkingArea;
            double x = cur.X * sx - ActualWidth / 2, y = cur.Y * sy - 30;
            Left = Math.Max(wa.Left * sx, Math.Min(x, wa.Right * sx - ActualWidth));
            Top = Math.Max(wa.Top * sy, Math.Min(y, wa.Bottom * sy - ActualHeight));
            Highlight();
            Focus();
        };
        Deactivated += (_, _) => SafeClose();
        Closed += (_, _) => { if (open == this) open = null; };
        PreviewKeyDown += OnKey;
    }

    static TextBlock Mono(string text, string brush)
    {
        var tb = new TextBlock { Text = text, VerticalAlignment = VerticalAlignment.Center };
        tb.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont");
        tb.SetResourceReference(TextBlock.FontSizeProperty, "SmallFontSize");
        tb.SetResourceReference(TextBlock.ForegroundProperty, brush);
        return tb;
    }

    Border Cell(int i)
    {
        var it = items[i];
        var stack = new StackPanel { Width = 196 };
        var pic = new Grid { Height = 118, Margin = new Thickness(6, 6, 6, 0) };
        pic.SetResourceReference(Panel.BackgroundProperty, "Canvas");
        pic.Children.Add(new System.Windows.Controls.Image { Source = Imaging.Thumb(it.Path, 400), Stretch = Stretch.Uniform, Margin = new Thickness(5) });
        if (i < 10)
        {
            var badge = new Border { Width = 20, Height = 20, HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top, Margin = new Thickness(5), BorderThickness = new Thickness(1) };
            badge.SetResourceReference(Border.BackgroundProperty, "Header");
            badge.SetResourceReference(Border.BorderBrushProperty, "AccentRing");
            badge.SetResourceReference(Border.CornerRadiusProperty, "ControlRadius");
            var n = Mono(((i + 1) % 10).ToString(), "Accent"); n.HorizontalAlignment = HorizontalAlignment.Center; n.FontWeight = FontWeights.Bold;
            badge.Child = n;
            badge.Tag = "badge";
            pic.Children.Add(badge);
        }
        stack.Children.Add(pic);
        var name = new TextBlock { Text = string.IsNullOrEmpty(it.Title) ? (string.IsNullOrEmpty(it.Source) ? L.T("Captura") : it.Source) : it.Title, FontWeight = FontWeights.SemiBold, Margin = new Thickness(10, 8, 10, 0), TextTrimming = TextTrimming.CharacterEllipsis };
        name.SetResourceReference(TextBlock.ForegroundProperty, "Ink");
        stack.Children.Add(name);
        var meta = Mono(it.Created.ToString("dd/MM HH:mm") + (it.Width > 0 ? " · " + it.Width + "×" + it.Height : ""), "Muted");
        meta.Margin = new Thickness(10, 2, 10, 10);
        stack.Children.Add(meta);

        var frame = new Border { Child = stack, BorderThickness = new Thickness(1), Margin = new Thickness(0, 0, 10, 10), Cursor = Cursors.Hand };
        frame.SetResourceReference(Border.CornerRadiusProperty, "ControlRadius");
        frame.MouseEnter += (_, _) => { index = i; Highlight(); };
        frame.MouseLeftButtonUp += (_, _) => Pick(i, (Keyboard.Modifiers & ModifierKeys.Shift) == 0);
        frame.MouseRightButtonUp += (_, _) => Edit(i);
        cells.Add(frame);
        return frame;
    }

    void Highlight()
    {
        for (int i = 0; i < cells.Count; i++)
        {
            bool on = i == index;
            cells[i].SetResourceReference(Border.BackgroundProperty, on ? "AccentTint" : "Surface");
            cells[i].SetResourceReference(Border.BorderBrushProperty, on ? "AccentRing" : "FaintBorder");
            cells[i].BorderThickness = new Thickness(on ? 3 : 1, 1, 1, 1);
        }
    }

    void OnKey(object sender, KeyEventArgs e)
    {
        e.Handled = true;
        if (e.Key == Key.Escape) { SafeClose(); return; }
        int n = e.Key is >= Key.D0 and <= Key.D9 ? e.Key - Key.D0 : e.Key is >= Key.NumPad0 and <= Key.NumPad9 ? e.Key - Key.NumPad0 : -1;
        bool shift = (Keyboard.Modifiers & ModifierKeys.Shift) != 0;
        if (n >= 0) { int i = n == 0 ? 9 : n - 1; if (i < items.Count) Pick(i, !shift); return; }
        if (items.Count == 0) return;
        switch (e.Key)
        {
            case Key.Right: index = Math.Min(items.Count - 1, index + 1); break;
            case Key.Left: index = Math.Max(0, index - 1); break;
            case Key.Down: index = Math.Min(items.Count - 1, index + Cols); break;
            case Key.Up: index = Math.Max(0, index - Cols); break;
            case Key.Enter: Pick(index, !shift); return;
            case Key.E: Edit(index); return;
            default: e.Handled = false; return;
        }
        Highlight();
    }

    void SafeClose()
    {
        if (closing) return;
        closing = true;
        Dispatcher.BeginInvoke(() => { try { Close(); } catch (InvalidOperationException) { } });
    }

    void Edit(int i) { var it = items[i]; SafeClose(); EditorWindow.Open(it); }

    void Pick(int i, bool paste)
    {
        var it = items[i];
        if (!Store.CopyToClipboard(it)) return;
        Hide();
        SafeClose();
        if (paste && Store.Settings.PasteOnPick && previous != IntPtr.Zero)
        {
            Native.SetForegroundWindow(previous);
            Thread.Sleep(120);
            System.Windows.Forms.SendKeys.SendWait("^v");
        }
    }
}
