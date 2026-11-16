using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using BuzzPrint.Ai;

namespace BuzzPrint;

/// <summary>Common chrome for the AI windows: title, muted purpose line, content, footer.</summary>
public abstract class AiWindowBase : Window
{
    protected readonly DockPanel Root = new() { Margin = new Thickness(24, 20, 24, 18) };
    protected readonly DockPanel Foot = new() { Margin = new Thickness(0, 14, 0, 0), LastChildFill = false };

    protected AiWindowBase(Window? owner, string title, string purpose, double width, double height)
    {
        SetResourceReference(StyleProperty, typeof(Window));
        TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        Title = title + " · " + AppInfo.Name;
        Width = width; Height = height; MinWidth = 520; MinHeight = 380;
        ShowInTaskbar = false;
        if (owner != null && owner.IsVisible) { Owner = owner; WindowStartupLocation = WindowStartupLocation.CenterOwner; }
        else WindowStartupLocation = WindowStartupLocation.CenterScreen;
        var head = new StackPanel();
        DockPanel.SetDock(head, Dock.Top);
        var t = new TextBlock { Text = title, FontWeight = FontWeights.SemiBold };
        t.SetResourceReference(TextBlock.FontSizeProperty, "TitleFontSize");
        head.Children.Add(t);
        var p = Muted(purpose); p.Margin = new Thickness(0, 4, 0, 14);
        head.Children.Add(p);
        var top = BobertoMark.WithBadge(head, 48);
        DockPanel.SetDock(top, Dock.Top);
        Root.Children.Add(top);
        DockPanel.SetDock(Foot, Dock.Bottom);
        Root.Children.Add(Foot);
        Content = Root;
        PreviewKeyDown += (_, e) => { if (e.Key == Key.Escape) Close(); };
    }

    protected Button FootButton(string text, bool primary, RoutedEventHandler click)
    {
        var b = new Button { Content = text, MinWidth = 110, Margin = new Thickness(8, 0, 0, 0) };
        if (primary)
        {
            b.FontWeight = FontWeights.SemiBold; b.IsDefault = true;
            b.SetResourceReference(BackgroundProperty, "Accent"); b.SetResourceReference(ForegroundProperty, "OnAccent"); b.SetResourceReference(BorderBrushProperty, "Accent");
        }
        b.Click += click;
        DockPanel.SetDock(b, Dock.Right);
        Foot.Children.Add(b);
        return b;
    }

    protected static Border Card(UIElement child, Thickness? padding = null)
    {
        var b = new Border { Child = child, Padding = padding ?? new Thickness(14), BorderThickness = new Thickness(1) };
        b.SetResourceReference(Border.BackgroundProperty, "Surface");
        b.SetResourceReference(Border.BorderBrushProperty, "FaintBorder");
        b.SetResourceReference(Border.CornerRadiusProperty, "PanelRadius");
        return b;
    }

    protected static TextBlock Muted(string text)
    {
        var t = new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap };
        t.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        t.SetResourceReference(TextBlock.FontSizeProperty, "SmallFontSize");
        return t;
    }

    protected static Image Thumb(CaptureItem it, double maxHeight)
    {
        var img = new Image { Stretch = Stretch.Uniform, MaxHeight = maxHeight };
        RenderOptions.SetBitmapScalingMode(img, BitmapScalingMode.HighQuality);
        try
        {
            var bi = new BitmapImage();
            bi.BeginInit(); bi.CacheOption = BitmapCacheOption.OnLoad; bi.UriSource = new Uri(it.Path); bi.DecodePixelWidth = 900; bi.EndInit(); bi.Freeze();
            img.Source = bi;
        }
        catch (Exception ex) when (ex is System.IO.IOException or NotSupportedException or UriFormatException) { }
        return img;
    }
}

/// <summary>A text result (incident report…) with Copy.</summary>
public sealed class AiTextWindow : AiWindowBase
{
    AiTextWindow(Window? owner, string title, string purpose, string text) : base(owner, title, purpose, 980, 680)
    {
        var box = new TextBox { Text = text, AcceptsReturn = true, TextWrapping = TextWrapping.Wrap, VerticalScrollBarVisibility = ScrollBarVisibility.Auto, VerticalContentAlignment = VerticalAlignment.Top };
        box.SetResourceReference(TextBox.FontFamilyProperty, "UiFont");
        var copied = Muted("");
        copied.VerticalAlignment = VerticalAlignment.Center;
        copied.TextTrimming = TextTrimming.CharacterEllipsis;
        void Done(string message) => copied.Text = message;
        string Accent() => ExportWindow.AccentHex();
        FootButton(L.T("Cerrar"), false, (_, _) => Close()).IsCancel = true;
        FootButton(L.T("Copiar"), true, (_, _) => { try { Clipboard.SetText(box.Text); Done(L.T("Texto copiado al portapapeles.")); } catch (System.Runtime.InteropServices.COMException ex) { Done(ex.Message); } });
        // Designed version: pastes formatted (headings, severity pill, log lines) into Outlook, Teams, Jira or Word.
        FootButton(L.T("Copiar con formato"), false, (_, _) =>
        {
            try { ReportFormat.CopyHtml(ReportFormat.Html(title, box.Text, Accent()), box.Text); Done(L.T("Copiado con formato: pegalo en un mail, Teams, Jira o Word.")); }
            catch (System.Runtime.InteropServices.COMException ex) { Done(ex.Message); }
        });
        FootButton(L.T("Guardar imagen"), false, (_, _) =>
        {
            var dlg = new Microsoft.Win32.SaveFileDialog { Filter = "PNG|*.png", FileName = SafeName(title) + ".png" };
            if (dlg.ShowDialog(this) != true) return;
            try
            {
                var png = ReportFormat.Image(title, box.Text, (Color)ColorConverter.ConvertFromString("#" + Accent()));
                var enc = new PngBitmapEncoder(); enc.Frames.Add(BitmapFrame.Create(png));
                using (var f = System.IO.File.Create(dlg.FileName)) enc.Save(f);
                Done(L.T("Imagen guardada: ") + System.IO.Path.GetFileName(dlg.FileName));
            }
            catch (Exception ex) when (ex is System.IO.IOException or UnauthorizedAccessException) { Done(ex.Message); }
        });
        FootButton(L.T("Guardar texto"), false, (_, _) =>
        {
            var dlg = new Microsoft.Win32.SaveFileDialog { Filter = L.T("Texto") + "|*.txt|Markdown|*.md|HTML|*.html", FileName = SafeName(title) + ".txt" };
            if (dlg.ShowDialog(this) != true) return;
            try
            {
                string ext = System.IO.Path.GetExtension(dlg.FileName).ToLowerInvariant();
                string content = ext switch
                {
                    ".md" => ReportFormat.Markdown(title, box.Text),
                    ".html" or ".htm" => ReportFormat.FullHtmlPage(title, ReportFormat.Html(title, box.Text, Accent())),
                    _ => box.Text,
                };
                System.IO.File.WriteAllText(dlg.FileName, content, new System.Text.UTF8Encoding(false));
                Done(L.T("Archivo guardado: ") + System.IO.Path.GetFileName(dlg.FileName));
            }
            catch (Exception ex) when (ex is System.IO.IOException or UnauthorizedAccessException) { Done(ex.Message); }
        });
        DockPanel.SetDock(copied, Dock.Left);
        Foot.LastChildFill = true;
        Foot.Children.Add(copied);
        Root.Children.Add(box);
    }

    static string SafeName(string title)
    {
        var bad = System.IO.Path.GetInvalidFileNameChars();
        return new string(title.Select(c => bad.Contains(c) ? '_' : c).ToArray()) + " " + DateTime.Now.ToString("yyyy-MM-dd HHmm");
    }

    public static void Show(Window? owner, string title, string purpose, string text) => new AiTextWindow(owner, title, purpose, text).ShowDialog();
    public static Window ForShot(Window owner, string title, string purpose, string text) => new AiTextWindow(owner, title, purpose, text);
}

/// <summary>Questions and answers about one capture, the capture on the left.</summary>
public sealed class AiAskWindow : AiWindowBase
{
    readonly AiConversation conversation;
    readonly StackPanel transcript = new();
    readonly ScrollViewer scroller;
    readonly TextBox input = new() { AcceptsReturn = false, MinHeight = 38 };
    readonly Button send;

    AiAskWindow(Window? owner, CaptureItem it) : base(owner, L.T("Preguntar sobre la captura"),
        L.F("Cada pregunta envía la captura y su texto a {0}. Preguntá qué significa un error, qué revisar o cómo seguir.", AiConnections.Active(Store.Settings)?.DisplayName() ?? "IA"), 1080, 720)
    {
        conversation = new AiConversation(it);
        var grid = new Grid();
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(0.9, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1.1, GridUnitType.Star) });
        var left = Card(new DockPanel { Children = { Thumb(it, 2000) } }, new Thickness(10));
        left.Margin = new Thickness(0, 0, 14, 0);
        grid.Children.Add(left);

        var right = new DockPanel();
        Grid.SetColumn(right, 1);
        var ask = new DockPanel { Margin = new Thickness(0, 10, 0, 0) };
        DockPanel.SetDock(ask, Dock.Bottom);
        send = new Button { Content = L.T("Preguntar"), MinWidth = 100, Margin = new Thickness(8, 0, 0, 0), FontWeight = FontWeights.SemiBold, IsDefault = true };
        send.SetResourceReference(BackgroundProperty, "Accent"); send.SetResourceReference(ForegroundProperty, "OnAccent"); send.SetResourceReference(BorderBrushProperty, "Accent");
        DockPanel.SetDock(send, Dock.Right);
        ask.Children.Add(send); ask.Children.Add(input);
        right.Children.Add(ask);
        var chips = new WrapPanel { Margin = new Thickness(0, 0, 0, 8) };
        DockPanel.SetDock(chips, Dock.Top);
        foreach (var q in new[] { "¿Qué muestra esta pantalla?", "¿Qué significa este error y cómo lo soluciono?", "¿Qué debería revisar ahora?" })
        {
            var chip = new Button { Content = L.T(q), Style = (Style)FindResource("SessionChip"), Margin = new Thickness(0, 0, 6, 6) };
            chip.Click += async (_, _) => { input.Text = L.T(q); await SendAsync(); };
            chips.Children.Add(chip);
        }
        right.Children.Add(chips);
        scroller = new ScrollViewer { Content = transcript, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        right.Children.Add(Card(scroller, new Thickness(14)));
        grid.Children.Add(right);
        Root.Children.Add(grid);
        FootButton(L.T("Cerrar"), false, (_, _) => Close()).IsCancel = true;
        send.Click += async (_, _) => await SendAsync();
        Loaded += (_, _) => input.Focus();
    }

    async Task SendAsync()
    {
        string q = input.Text.Trim();
        if (q.Length == 0 || !send.IsEnabled) return;
        input.Text = "";
        Add(q, true);
        var thinking = Muted(L.T("Pensando…")); thinking.Margin = new Thickness(0, 0, 0, 12);
        transcript.Children.Add(thinking); scroller.ScrollToEnd();
        send.IsEnabled = false;
        try { string a; using (var busy = AiBusy.Show(this, L.T("Mirando la captura para responder…"))) a = await conversation.AskAsync(q, busy.Token); transcript.Children.Remove(thinking); Add(a, false); }
        catch (OperationCanceledException) { transcript.Children.Remove(thinking); var note = Muted(AiBusy.Stopped(this, Title)); note.Margin = new Thickness(0, 0, 0, 12); transcript.Children.Add(note); }
        catch (AiException ex) { transcript.Children.Remove(thinking); var err = Muted(ex.Message); err.SetResourceReference(TextBlock.ForegroundProperty, "ErrorInk"); err.Margin = new Thickness(0, 0, 0, 12); transcript.Children.Add(err); }
        finally { send.IsEnabled = true; input.Focus(); scroller.ScrollToEnd(); }
    }

    void Add(string text, bool question)
    {
        var box = new TextBox { Text = text, IsReadOnly = true, BorderThickness = new Thickness(question ? 1 : 0), TextWrapping = TextWrapping.Wrap, Margin = new Thickness(question ? 60 : 0, 0, 0, 12), Padding = new Thickness(10, 8, 10, 8) };
        box.SetResourceReference(BackgroundProperty, question ? "AccentWeak" : "Surface");
        box.SetResourceReference(BorderBrushProperty, "AccentCardRing");
        if (!question) box.Padding = new Thickness(0);
        transcript.Children.Add(box);
    }

    public static void Open(Window? owner, CaptureItem it) => new AiAskWindow(owner, it).Show();
}

/// <summary>A reviewable list: each row has a checkbox, a main line and a muted line; returns the checked rows.</summary>
public sealed class AiReviewWindow : AiWindowBase
{
    readonly List<CheckBox> checks = [];
    bool accepted;

    AiReviewWindow(Window? owner, string title, string purpose, IReadOnlyList<(string main, string sub)> rows, string apply, CaptureItem? preview)
        : base(owner, title, purpose, preview != null ? 1040 : 720, 640)
    {
        var stack = new StackPanel();
        foreach (var (main, sub) in rows)
        {
            var line = new StackPanel();
            line.Children.Add(new TextBlock { Text = main, FontWeight = FontWeights.SemiBold, TextWrapping = TextWrapping.Wrap });
            if (sub.Length > 0) { var m = Muted(sub); m.Margin = new Thickness(0, 2, 0, 0); line.Children.Add(m); }
            var cb = new CheckBox { IsChecked = true, Content = line, Margin = new Thickness(0, 0, 0, 10) };
            checks.Add(cb);
            stack.Children.Add(cb);
        }
        var list = Card(new ScrollViewer { Content = stack, VerticalScrollBarVisibility = ScrollBarVisibility.Auto });
        if (preview != null)
        {
            var grid = new Grid();
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1.2, GridUnitType.Star) });
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            var img = Card(new DockPanel { Children = { Thumb(preview, 2000) } }, new Thickness(10));
            img.Margin = new Thickness(0, 0, 14, 0);
            grid.Children.Add(img);
            Grid.SetColumn(list, 1);
            grid.Children.Add(list);
            Root.Children.Add(grid);
        }
        else Root.Children.Add(list);
        FootButton(apply, true, (_, _) => { accepted = true; Close(); });
        FootButton(L.T("Cancelar"), false, (_, _) => Close()).IsCancel = true;
        var all = new Button { Content = L.T("Marcar o desmarcar todo"), Style = (Style)FindResource("QuietButton") };
        all.Click += (_, _) => { bool on = checks.Any(c => c.IsChecked != true); foreach (var c in checks) c.IsChecked = on; };
        DockPanel.SetDock(all, Dock.Left);
        Foot.Children.Add(all);
    }

    /// <summary>Indexes of the rows left checked, or null if cancelled.</summary>
    public static List<int>? Ask(Window? owner, string title, string purpose, IReadOnlyList<(string main, string sub)> rows, string apply, CaptureItem? preview = null)
    {
        var w = new AiReviewWindow(owner, title, purpose, rows, apply, preview);
        w.ShowDialog();
        return w.accepted ? w.checks.Select((c, i) => (c, i)).Where(x => x.c.IsChecked == true).Select(x => x.i).ToList() : null;
    }
}
