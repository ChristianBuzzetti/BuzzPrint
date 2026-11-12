using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using D = System.Drawing;
using WRect = System.Windows.Shapes.Rectangle;

namespace BuzzPrint;

/// <summary>
/// Annotation editor: arrows, lines, shapes, highlighter, pen, text, numbered steps, pixelate and crop, with a tray of
/// captures. Annotations are drawn with GDI+ (Shapes.cs) so what you see is exactly what is copied and saved.
/// </summary>
public partial class EditorWindow : Window
{
    static EditorWindow? instance;

    sealed class TrayCard { public CaptureItem Item { get; init; } = null!; public BitmapSource? Thumb { get; init; } public string Title { get; init; } = ""; }

    CaptureItem? item;
    D.Bitmap? baseImg, committed;
    List<Mark> shapes = [];
    readonly Stack<(D.Bitmap Base, List<Mark> Shapes)> undo = new(), redo = new();
    bool dirty, loadingTray;

    Tool tool = Tool.Arrow;
    D.Color color = D.Color.FromArgb(230, 30, 40);
    float width = 4, fontSize = 22;
    double zoom = 1;

    Mark? current, selected;
    D.PointF dragStart;
    int dragHandle = -1;
    bool dragging;
    D.RectangleF cropRect;
    TextBox? textEdit;
    Mark? textShape;
    readonly Dictionary<Tool, Button> toolButtons = [];
    readonly List<(Button Button, D.Color Color)> swatchButtons = [];

    static readonly (Tool Tool, string Glyph, string Name, string Key)[] ToolDefs =
    [
        (Tool.Select, "⬚", L.T("Seleccionar y mover"), "V"), (Tool.Arrow, "➜", L.T("Flecha"), "A"), (Tool.Line, "╱", L.T("Línea"), "L"),
        (Tool.Rect, "▭", L.T("Rectángulo"), "R"), (Tool.Ellipse, "◯", L.T("Elipse"), "E"), (Tool.Highlight, "▮", L.T("Resaltador"), "H"),
        (Tool.Pen, "✎", L.T("Lápiz"), "P"), (Tool.Text, "T", L.T("Texto"), "T"), (Tool.Step, "①", L.T("Paso numerado"), "N"),
        (Tool.Blur, "▦", L.T("Pixelar / ocultar datos"), "B"), (Tool.Crop, "⛶", L.T("Recortar"), "C"),
    ];

    public static void Open(CaptureItem? it)
    {
        if (instance == null) { instance = new EditorWindow(); instance.Show(); }
        if (instance.WindowState == WindowState.Minimized) instance.WindowState = WindowState.Normal;
        instance.Activate();
        if (it != null) instance.LoadItem(it);
    }

    internal static EditorWindow? Instance => instance;

    public EditorWindow()
    {
        InitializeComponent();
        BrandMark.Animated = App.Current.Appearance.BobertoMotion && SystemParameters.ClientAreaAnimation;
        foreach (var def in ToolDefs)
        {
            var b = new Button { Content = def.Glyph, ToolTip = def.Name + " · " + def.Key };
            b.SetResourceReference(StyleProperty, "ToolButton");
            var t = def.Tool;
            b.Click += (_, _) => SetTool(t);
            if (def.Tool == Tool.Text) { b.FontFamily = new FontFamily("Segoe UI"); b.FontWeight = FontWeights.Bold; }
            toolButtons[def.Tool] = b;
            Tools.Children.Add(b);
        }
        foreach (var c in new[] { D.Color.FromArgb(230, 30, 40), D.Color.FromArgb(255, 140, 0), D.Color.FromArgb(255, 214, 0), D.Color.FromArgb(30, 170, 60), D.Color.FromArgb(20, 110, 230), D.Color.FromArgb(150, 60, 200), D.Color.Black, D.Color.White })
            AddSwatch(c);
        var custom = new Button { Width = 24, Height = 24, Margin = new Thickness(1), Padding = new Thickness(0), ToolTip = L.T("Otro color…"), MinHeight = 0 };
        var hues = new LinearGradientBrush { StartPoint = new Point(0, 0), EndPoint = new Point(1, 1) };
        foreach (var (o, h) in new[] { (0d, "#FF3B3B"), (.25, "#FFC83B"), (.5, "#3BFF6E"), (.75, "#3BC8FF"), (1d, "#C83BFF") }) hues.GradientStops.Add(new GradientStop((Color)ColorConverter.ConvertFromString(h), o));
        custom.Content = new Border { Width = 14, Height = 14, Background = hues };
        custom.Click += (_, _) =>
        {
            using var d = new System.Windows.Forms.ColorDialog { Color = color, FullOpen = true };
            if (d.ShowDialog() == System.Windows.Forms.DialogResult.OK) SetColor(d.Color);
        };
        Swatches.Children.Add(custom);
        foreach (var v in new[] { 1, 2, 3, 4, 6, 8, 12 }) WidthBox.Items.Add(new ComboBoxItem { Content = v + " px", Tag = v });
        foreach (var v in new[] { 12, 14, 16, 18, 22, 28, 36, 48 }) FontBox.Items.Add(new ComboBoxItem { Content = v + " px", Tag = v });
        WidthBox.SelectedIndex = 3; FontBox.SelectedIndex = 4;

        Store.Changed += StoreChanged;
        Closing += (_, e) => { if (!ConfirmDiscard()) e.Cancel = true; };
        Closed += (_, _) => { Store.Changed -= StoreChanged; instance = null; };
        PreviewKeyDown += OnKey;
        RefreshTray();
        SetTool(Tool.Arrow);
        UpdateSwatches();
        UpdateInfo();
    }

    void StoreChanged(object? s, EventArgs e) => Dispatcher.BeginInvoke(RefreshTray);

    // ------------------------------------------------------------------ toolbar
    void AddSwatch(D.Color c)
    {
        var b = new Button { Width = 24, Height = 24, Margin = new Thickness(1), Padding = new Thickness(0), MinHeight = 0, ToolTip = "#" + c.R.ToString("X2") + c.G.ToString("X2") + c.B.ToString("X2") };
        b.Content = new Border { Width = 14, Height = 14, Background = new SolidColorBrush(Color.FromRgb(c.R, c.G, c.B)), BorderBrush = new SolidColorBrush(Color.FromArgb(60, 128, 128, 128)), BorderThickness = new Thickness(1) };
        b.Click += (_, _) => SetColor(c);
        swatchButtons.Add((b, c));
        Swatches.Children.Add(b);
    }

    void UpdateSwatches()
    {
        foreach (var (b, c) in swatchButtons)
        {
            bool on = c.ToArgb() == color.ToArgb();
            b.SetResourceReference(BorderBrushProperty, on ? "Accent" : "FaintBorder");
            b.SetResourceReference(BackgroundProperty, on ? "AccentTint" : "SurfaceRaised");
        }
    }

    void SetColor(D.Color c)
    {
        color = c;
        UpdateSwatches();
        if (selected != null) { PushUndo(); selected.Color = c; Changed(); }
    }

    void SetTool(Tool t)
    {
        CommitText();
        tool = t;
        foreach (var (k, b) in toolButtons)
        {
            bool on = k == t;
            b.SetResourceReference(BackgroundProperty, on ? "SurfaceRaised" : "Transparent");
            b.SetResourceReference(BorderBrushProperty, on ? "AccentRing" : "Transparent");
            b.SetResourceReference(ForegroundProperty, on ? "Accent" : "Muted");
            if (!on) b.Background = Brushes.Transparent;
        }
        if (t != Tool.Select) selected = null;
        if (t == Tool.Highlight && color.ToArgb() == D.Color.FromArgb(230, 30, 40).ToArgb()) SetColor(D.Color.FromArgb(255, 214, 0));
        Overlay.Cursor = t == Tool.Select ? Cursors.Arrow : t == Tool.Text ? Cursors.IBeam : Cursors.Cross;
        var def = ToolDefs.First(d => d.Tool == t);
        ToolText.Text = def.Name.ToUpperInvariant() + "  ·  " + def.Key;
        DrawOverlay();
    }

    void WidthChanged(object sender, SelectionChangedEventArgs e)
    {
        if (WidthBox.SelectedItem is not ComboBoxItem ci) return;
        width = (int)ci.Tag;
        if (selected != null && selected.Kind != Tool.Text && selected.Kind != Tool.Step) { PushUndo(); selected.Width = width; Changed(); }
    }

    void FontChanged(object sender, SelectionChangedEventArgs e)
    {
        if (FontBox.SelectedItem is not ComboBoxItem ci) return;
        fontSize = (int)ci.Tag;
        if (selected != null && (selected.Kind == Tool.Text || selected.Kind == Tool.Step)) { PushUndo(); selected.FontSize = fontSize; Changed(); }
    }

    // ------------------------------------------------------------------ tray
    void RefreshTray()
    {
        loadingTray = true;
        var cards = Store.All().Take(60).Select(it => new TrayCard { Item = it, Thumb = Imaging.Thumb(it.Path, 260), Title = it.Display + (string.IsNullOrEmpty(it.Source) ? "" : " · " + it.Source) }).ToList();
        Tray.ItemsSource = cards;
        Tray.SelectedItem = cards.FirstOrDefault(c => item != null && c.Item.Id == item.Id);
        loadingTray = false;
    }

    void TraySelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (loadingTray || Tray.SelectedItem is not TrayCard c) return;
        if (item != null && c.Item.Id == item.Id) return;
        LoadItem(c.Item);
        if (item == null || item.Id != c.Item.Id) { loadingTray = true; Tray.SelectedItem = ((IEnumerable<TrayCard>)Tray.ItemsSource).FirstOrDefault(x => item != null && x.Item.Id == item.Id); loadingTray = false; }
    }

    void TrayRightClick(object sender, MouseButtonEventArgs e)
    {
        var d = e.OriginalSource as DependencyObject;
        while (d != null && d is not ListBoxItem) d = VisualTreeHelper.GetParent(d);
        if ((d as ListBoxItem)?.DataContext is TrayCard c && Store.CopyToClipboard(c.Item)) StatusText.Text = L.T("Copiada al portapapeles · ") + c.Item.Display;
        e.Handled = true;
    }

    // ------------------------------------------------------------------ document
    public void LoadItem(CaptureItem it)
    {
        if (item != null && it.Id == item.Id) return;
        if (!ConfirmDiscard()) return;
        CommitText();
        D.Bitmap b;
        var list = new List<Mark>();
        try
        {
            if (File.Exists(it.OrigPath))
            {
                b = Store.LoadBitmap(it.OrigPath);
                var meta = Store.ReadMeta(it);
                if (meta != null && meta.TryGetValue("shapes", out var o) && o is List<object> arr)
                    foreach (var d in arr) try { list.Add(Mark.FromDict((Dictionary<string, object>)d)); } catch { }
            }
            else b = Store.LoadBitmap(it.Path);
        }
        catch (Exception ex) { PromptWindow.Info(this, L.T("No se pudo abrir"), ex.Message); return; }

        baseImg = b; item = it; shapes = list;
        undo.Clear(); redo.Clear(); selected = null; current = null; dirty = false;
        DocTitle.Text = string.IsNullOrEmpty(it.Title) ? (string.IsNullOrEmpty(it.Source) ? L.T("Captura") : it.Source) : it.Title;
        Title = DocTitle.Text + " · Editor · BuzzPrint";
        RenderCommitted();
        Dispatcher.BeginInvoke(FitZoom, System.Windows.Threading.DispatcherPriority.Loaded);
        RefreshTray();
        UpdateInfo();
    }

    bool ConfirmDiscard()
    {
        CommitText();
        if (!dirty || item == null) return true;
        var r = MessageBox.Show(this, L.T("¿Guardar los cambios de la captura actual?"), "BuzzPrint", MessageBoxButton.YesNoCancel, MessageBoxImage.Question);
        if (r == MessageBoxResult.Cancel) return false;
        if (r == MessageBoxResult.Yes) Save();
        dirty = false;
        return true;
    }

    D.Bitmap RenderFinal() => Mark.Render(baseImg!, shapes);

    void RenderCommitted()
    {
        if (baseImg == null) return;
        committed?.Dispose();
        committed = RenderFinal();
        Show(committed);
        DrawOverlay();
    }

    void Show(D.Bitmap bmp)
    {
        View.Source = Imaging.ToSource(bmp);
        View.Width = Overlay.Width = Shadow.Width = bmp.Width;
        View.Height = Overlay.Height = Shadow.Height = bmp.Height;
    }

    void RenderPreview()
    {
        if (committed == null || baseImg == null) return;
        if (current == null) { Show(committed); return; }
        using var tmp = (D.Bitmap)committed.Clone();
        using (var g = D.Graphics.FromImage(tmp)) current.Draw(g, baseImg);
        Show(tmp);
    }

    void CopyClick(object sender, RoutedEventArgs e) => CopyAll();
    void CopyAll()
    {
        if (baseImg == null) return;
        CommitText();
        using (var r = RenderFinal()) Store.CopyToClipboard(r);
        StatusText.Text = L.T("Copiada al portapapeles.");
    }

    void SaveClick(object sender, RoutedEventArgs e) => Save();
    void Save()
    {
        if (baseImg == null || item == null) return;
        CommitText();
        try
        {
            // First save of an untouched capture: keep it as it was and save the annotated version as a new capture.
            if (Store.Settings.EditKeepOriginal && !File.Exists(item.OrigPath)) { SaveAsCopy(); return; }
            baseImg.Save(item.OrigPath, D.Imaging.ImageFormat.Png);
            var meta = Store.ReadMeta(item) ?? new Dictionary<string, object>();
            meta["shapes"] = shapes.Select(s => (object)s.ToDict()).ToList();
            Store.WriteMeta(item, meta);
            using (var r = RenderFinal()) r.Save(item.Path, D.Imaging.ImageFormat.Png);
            Imaging.Forget(item.Path);
            dirty = false;
            StatusText.Text = L.T("Guardada · ") + item.Path;
            Store.RaiseChanged();
            UpdateInfo();
        }
        catch (Exception ex) { PromptWindow.Info(this, L.T("No se pudo guardar"), ex.Message); }
    }

    /// <summary>Saves the annotated image as a new library capture "<name>_editada" (annotations stay editable on the
    /// copy) and switches the editor to it; the original capture is not touched.</summary>
    void SaveAsCopy()
    {
        var original = item!;
        CaptureItem copy;
        using (var final = RenderFinal()) copy = Store.Add(final, original.Source);
        baseImg!.Save(copy.OrigPath, D.Imaging.ImageFormat.Png);
        var meta = Store.ReadMeta(copy) ?? new Dictionary<string, object>();
        meta["title"] = original.Display + (L.Es ? "_editada" : "_edited");
        meta["editedFrom"] = original.Id;
        meta["shapes"] = shapes.Select(s => (object)s.ToDict()).ToList();
        if (!string.IsNullOrEmpty(original.Section)) meta["section"] = original.Section;
        Store.WriteMeta(copy, meta);
        Store.RaiseChanged();
        item = Store.All().FirstOrDefault(x => x.Id == copy.Id) ?? copy;
        dirty = false;
        DocTitle.Text = item.Display;
        Title = DocTitle.Text + " · Editor · BuzzPrint";
        StatusText.Text = L.T("Guardada como copia · la original quedó intacta · ") + item.Display;
        RefreshTray();
        UpdateInfo();
    }

    void ExportClick(object sender, RoutedEventArgs e)
    {
        if (baseImg == null) return;
        CommitText();
        var d = new Microsoft.Win32.SaveFileDialog { Filter = "PNG|*.png|JPEG|*.jpg|BMP|*.bmp", FileName = item?.Id ?? L.T("captura") };
        if (d.ShowDialog(this) != true) return;
        var fmt = d.FilterIndex == 2 ? D.Imaging.ImageFormat.Jpeg : d.FilterIndex == 3 ? D.Imaging.ImageFormat.Bmp : D.Imaging.ImageFormat.Png;
        using (var r = RenderFinal()) r.Save(d.FileName, fmt);
        StatusText.Text = L.T("Exportada · ") + d.FileName;
    }

    void PushUndo()
    {
        undo.Push((baseImg!, shapes.Select(s => s.Clone()).ToList()));
        redo.Clear();
    }

    void UndoClick(object sender, RoutedEventArgs e) => Undo();
    void RedoClick(object sender, RoutedEventArgs e) => Redo();
    void Undo()
    {
        CommitText();
        if (undo.Count == 0) return;
        redo.Push((baseImg!, shapes));
        Restore(undo.Pop());
    }
    void Redo()
    {
        if (redo.Count == 0) return;
        undo.Push((baseImg!, shapes));
        Restore(redo.Pop());
    }
    void Restore((D.Bitmap Base, List<Mark> Shapes) s)
    {
        baseImg = s.Base;
        shapes = s.Shapes.Select(x => x.Clone()).ToList();
        selected = null;
        dirty = true;
        RenderCommitted();
        UpdateInfo();
    }

    void Changed()
    {
        dirty = true;
        RenderCommitted();
        UpdateInfo();
    }

    void UpdateInfo()
    {
        DirtyChip.Visibility = dirty ? Visibility.Visible : Visibility.Collapsed;
        ZoomText.Text = (int)Math.Round(zoom * 100) + "%";
        if (baseImg == null || item == null) { StatusText.Text = L.T("Elegí una captura de la bandeja"); DocTitle.Text = L.T("Sin captura"); DocMeta.Text = ""; return; }
        DocMeta.Text = item.Created.ToString("dd/MM/yyyy HH:mm:ss") + "  ·  " + baseImg.Width + "×" + baseImg.Height + "  ·  " + item.Id + ".png";
        StatusText.Text = baseImg.Width + " × " + baseImg.Height + " px · " + shapes.Count + (shapes.Count == 1 ? L.T(" anotación") : L.T(" anotaciones")) + (dirty ? L.T(" · sin guardar") : L.T(" · guardada"));
    }

    // ------------------------------------------------------------------ zoom
    void FitClick(object sender, RoutedEventArgs e) => FitZoom();
    void ActualClick(object sender, RoutedEventArgs e) => SetZoom(1);
    void ZoomInClick(object sender, RoutedEventArgs e) => SetZoom(zoom * 1.25);
    void ZoomOutClick(object sender, RoutedEventArgs e) => SetZoom(zoom / 1.25);
    void ScrollerSizeChanged(object sender, SizeChangedEventArgs e) { if (fitMode) FitZoom(); }
    bool fitMode = true;

    void FitZoom()
    {
        if (baseImg == null) return;
        double zx = (Scroller.ViewportWidth - 64) / baseImg.Width, zy = (Scroller.ViewportHeight - 64) / baseImg.Height;
        if (double.IsNaN(zx) || zx <= 0) return;
        SetZoom(Math.Max(0.05, Math.Min(1, Math.Min(zx, zy))));
        fitMode = true;
    }

    void SetZoom(double z)
    {
        fitMode = false;
        zoom = Math.Clamp(z, 0.05, 8);
        Zoom.ScaleX = Zoom.ScaleY = zoom;
        RenderOptions.SetBitmapScalingMode(View, zoom >= 1 ? BitmapScalingMode.NearestNeighbor : BitmapScalingMode.HighQuality);
        DrawOverlay();
        UpdateInfo();
    }

    void ScrollerWheel(object sender, MouseWheelEventArgs e)
    {
        if ((Keyboard.Modifiers & ModifierKeys.Control) == 0) return;
        SetZoom(e.Delta > 0 ? zoom * 1.15 : zoom / 1.15);
        e.Handled = true;
    }

    // ------------------------------------------------------------------ keyboard
    void OnKey(object sender, KeyEventArgs e)
    {
        if (textEdit != null)
        {
            if (e.Key == Key.Escape) { CancelText(); e.Handled = true; }
            else if (e.Key == Key.Enter && (Keyboard.Modifiers & ModifierKeys.Shift) == 0) { CommitText(); e.Handled = true; }
            return;
        }
        if (e.OriginalSource is TextBox || e.OriginalSource is ComboBox || e.OriginalSource is ComboBoxItem) return;
        bool ctrl = (Keyboard.Modifiers & ModifierKeys.Control) != 0;
        e.Handled = true;
        if (ctrl)
        {
            switch (e.Key)
            {
                case Key.C: CopyAll(); return;
                case Key.S: Save(); return;
                case Key.Z: Undo(); return;
                case Key.Y: Redo(); return;
                case Key.D0: case Key.NumPad0: FitZoom(); return;
                case Key.D1: case Key.NumPad1: SetZoom(1); return;
                case Key.OemPlus: case Key.Add: SetZoom(zoom * 1.25); return;
                case Key.OemMinus: case Key.Subtract: SetZoom(zoom / 1.25); return;
            }
            e.Handled = false; return;
        }
        switch (e.Key)
        {
            case Key.Delete:
                if (selected != null) { PushUndo(); shapes.Remove(selected); selected = null; Changed(); }
                return;
            case Key.Escape: selected = null; current = null; dragging = false; RenderPreview(); DrawOverlay(); return;
        }
        var def = ToolDefs.FirstOrDefault(d => d.Key == e.Key.ToString());
        if (def.Name != null) { SetTool(def.Tool); return; }
        e.Handled = false;
    }

    void PersonalizeClick(object sender, RoutedEventArgs e) => App.Current.OpenPersonalization(this);

    // ------------------------------------------------------------------ text editing
    void BeginText(Mark s, bool isNew)
    {
        CommitText();
        textShape = s;
        if (!isNew) { PushUndo(); shapes.Remove(s); RenderCommitted(); }
        var c = s.Color;
        textEdit = new TextBox
        {
            Text = s.Text, AcceptsReturn = true, MinWidth = 220, Padding = new Thickness(6, 3, 6, 3),
            FontFamily = new FontFamily("Segoe UI"), FontWeight = FontWeights.Bold, FontSize = s.FontSize,
            Foreground = new SolidColorBrush(Color.FromRgb(c.R, c.G, c.B)), Background = new SolidColorBrush(Color.FromArgb(235, 255, 255, 255)),
            BorderBrush = new SolidColorBrush(Color.FromRgb(c.R, c.G, c.B)), BorderThickness = new Thickness(2)
        };
        Canvas.SetLeft(textEdit, s.P0.X); Canvas.SetTop(textEdit, s.P0.Y);
        textEdit.LostKeyboardFocus += (_, _) => Dispatcher.BeginInvoke(CommitText);
        Overlay.Children.Add(textEdit);
        textEdit.Focus();
        textEdit.SelectAll();
    }

    void CommitText()
    {
        if (textEdit == null) return;
        var tb = textEdit; textEdit = null;
        var s = textShape!; textShape = null;
        string t = tb.Text.TrimEnd();
        Overlay.Children.Remove(tb);
        if (t.Length > 0) { s.Text = t; shapes.Add(s); selected = tool == Tool.Select ? s : null; }
        Changed();
    }

    void CancelText()
    {
        if (textEdit == null) return;
        Overlay.Children.Remove(textEdit);
        textEdit = null; textShape = null;
        DrawOverlay();
    }

    // ------------------------------------------------------------------ mouse
    const double HandleSize = 9;
    D.PointF Pos(MouseEventArgs e) { var p = e.GetPosition(Overlay); return new D.PointF((float)p.X, (float)p.Y); }

    int HitHandle(Mark? s, D.PointF p)
    {
        if (s == null || !s.IsTwoPoint) return -1;
        float tol = (float)(HandleSize / zoom);
        if (Math.Abs(p.X - s.P0.X) <= tol && Math.Abs(p.Y - s.P0.Y) <= tol) return 0;
        if (Math.Abs(p.X - s.P1.X) <= tol && Math.Abs(p.Y - s.P1.Y) <= tol) return 1;
        return -1;
    }

    Mark? HitShape(D.PointF p)
    {
        for (int i = shapes.Count - 1; i >= 0; i--) if (shapes[i].Hit(p, (float)(5 / zoom))) return shapes[i];
        return null;
    }

    void OverlayDown(object sender, MouseButtonEventArgs e)
    {
        if (baseImg == null || e.OriginalSource is TextBox || IsInside<TextBox>(e.OriginalSource)) return;
        if (textEdit != null) { CommitText(); return; }
        var p = Pos(e);
        dragStart = p;
        dragging = true;
        Overlay.CaptureMouse();
        switch (tool)
        {
            case Tool.Select:
            {
                int h = HitHandle(selected, p);
                if (h >= 0) { dragHandle = h; PushUndo(); break; }
                selected = HitShape(p);
                dragHandle = -1;
                if (selected != null)
                {
                    if (e.ClickCount == 2 && selected.Kind == Tool.Text) { dragging = false; Overlay.ReleaseMouseCapture(); BeginText(selected, false); return; }
                    PushUndo();
                }
                else { dragging = false; Overlay.ReleaseMouseCapture(); }
                DrawOverlay();
                break;
            }
            case Tool.Text:
            {
                dragging = false; Overlay.ReleaseMouseCapture();
                var hit = HitShape(p);
                if (hit != null && hit.Kind == Tool.Text) { BeginText(hit, false); break; }
                PushUndo();
                BeginText(new Mark { Kind = Tool.Text, Color = color, FontSize = fontSize, Pts = { p } }, true);
                e.Handled = true;
                break;
            }
            case Tool.Step:
            {
                dragging = false; Overlay.ReleaseMouseCapture();
                PushUndo();
                int n = shapes.Where(s => s.Kind == Tool.Step).Select(s => s.Number).DefaultIfEmpty(0).Max() + 1;
                shapes.Add(new Mark { Kind = Tool.Step, Color = color, FontSize = fontSize, Number = n, Pts = { p } });
                Changed();
                break;
            }
            case Tool.Crop:
                cropRect = new D.RectangleF(p, D.SizeF.Empty);
                break;
            default:
                current = new Mark { Kind = tool, Color = color, Width = width, Pts = { p, p } };
                if (tool == Tool.Pen) current.Pts.RemoveAt(1);
                break;
        }
    }

    static bool IsInside<T>(object source) where T : DependencyObject
    {
        var d = source as DependencyObject;
        while (d != null) { if (d is T) return true; d = d is Visual ? VisualTreeHelper.GetParent(d) : LogicalTreeHelper.GetParent(d); }
        return false;
    }

    void OverlayMove(object sender, MouseEventArgs e)
    {
        if (baseImg == null) return;
        var p = Pos(e);
        if (!dragging)
        {
            if (tool == Tool.Select) Overlay.Cursor = HitHandle(selected, p) >= 0 ? Cursors.SizeAll : HitShape(p) != null ? Cursors.Hand : Cursors.Arrow;
            return;
        }
        if (tool == Tool.Select && selected != null)
        {
            if (dragHandle == 0) selected.P0 = p;
            else if (dragHandle == 1) selected.P1 = p;
            else { selected.Offset(p.X - dragStart.X, p.Y - dragStart.Y); dragStart = p; }
            dirty = true;
            RenderCommitted();
            return;
        }
        if (tool == Tool.Crop)
        {
            cropRect = D.RectangleF.FromLTRB(Math.Min(dragStart.X, p.X), Math.Min(dragStart.Y, p.Y), Math.Max(dragStart.X, p.X), Math.Max(dragStart.Y, p.Y));
            DrawOverlay();
            return;
        }
        if (current != null)
        {
            if (current.Kind == Tool.Pen) current.Pts.Add(p);
            else current.P1 = (Keyboard.Modifiers & ModifierKeys.Shift) != 0 ? Constrain(current, p) : p;
            RenderPreview();
        }
    }

    static D.PointF Constrain(Mark s, D.PointF p)
    {
        float dx = p.X - s.P0.X, dy = p.Y - s.P0.Y;
        if (s.Kind == Tool.Arrow || s.Kind == Tool.Line)
        {
            double ang = Math.Round(Math.Atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4);
            double len = Math.Sqrt(dx * dx + dy * dy);
            return new D.PointF(s.P0.X + (float)(Math.Cos(ang) * len), s.P0.Y + (float)(Math.Sin(ang) * len));
        }
        float m = Math.Max(Math.Abs(dx), Math.Abs(dy));
        return new D.PointF(s.P0.X + Math.Sign(dx) * m, s.P0.Y + Math.Sign(dy) * m);
    }

    void OverlayUp(object sender, MouseButtonEventArgs e)
    {
        Overlay.ReleaseMouseCapture();
        if (!dragging) return;
        dragging = false;
        if (tool == Tool.Select) { if (selected != null) Changed(); return; }
        if (tool == Tool.Crop)
        {
            var r = D.Rectangle.Round(cropRect);
            r.Intersect(new D.Rectangle(D.Point.Empty, baseImg!.Size));
            cropRect = D.RectangleF.Empty;
            if (r.Width > 4 && r.Height > 4)
            {
                PushUndo();
                baseImg = baseImg.Clone(r, D.Imaging.PixelFormat.Format32bppArgb);
                foreach (var s in shapes) s.Offset(-r.X, -r.Y);
                Changed();
                Dispatcher.BeginInvoke(FitZoom, System.Windows.Threading.DispatcherPriority.Loaded);
                SetTool(Tool.Select);
            }
            DrawOverlay();
            return;
        }
        if (current == null) return;
        var b = current.Bounds();
        bool tiny = current.Kind == Tool.Pen ? current.Pts.Count < 2 : (b.Width < 3 && b.Height < 3);
        var done = current;
        current = null;
        if (!tiny) { PushUndo(); shapes.Add(done); Changed(); }
        else RenderPreview();
    }

    /// <summary>--shot only: a representative set of annotations on the open capture.</summary>
    internal void Discard() => dirty = false;
    internal void SaveForShot() => Save();
    internal CaptureItem? Current => item;

    internal void DemoAnnotate()
    {
        if (baseImg == null) return;
        float w = baseImg.Width, h = baseImg.Height;
        D.PointF P(double x, double y) => new((float)(w * x), (float)(h * y));
        shapes.Add(new Mark { Kind = Tool.Highlight, Color = D.Color.FromArgb(255, 214, 0), Pts = { P(.05, .30), P(.45, .36) } });
        shapes.Add(new Mark { Kind = Tool.Rect, Color = D.Color.FromArgb(230, 30, 40), Width = 4, Pts = { P(.55, .25), P(.92, .55) } });
        shapes.Add(new Mark { Kind = Tool.Arrow, Color = D.Color.FromArgb(230, 30, 40), Width = 5, Pts = { P(.40, .70), P(.54, .50) } });
        shapes.Add(new Mark { Kind = Tool.Step, Color = D.Color.FromArgb(230, 30, 40), FontSize = 26, Number = 1, Pts = { P(.55, .25) } });
        shapes.Add(new Mark { Kind = Tool.Text, Color = D.Color.FromArgb(230, 30, 40), FontSize = 24, Text = L.T("Revisar este valor"), Pts = { P(.20, .72) } });
        shapes.Add(new Mark { Kind = Tool.Blur, Color = D.Color.Black, Width = 4, Pts = { P(.05, .80), P(.30, .92) } });
        selected = shapes[1];
        SetTool(Tool.Rect);
        selected = shapes[1];
        Changed();
    }

    // ------------------------------------------------------------------ overlay (selection, handles, crop)
    void DrawOverlay()
    {
        var keep = textEdit;
        Overlay.Children.Clear();
        if (keep != null) Overlay.Children.Add(keep);
        if (baseImg == null) return;
        double px = 1 / zoom;
        var accent = (Brush)Application.Current.Resources["Accent"];
        if (selected != null && shapes.Contains(selected))
        {
            var r = selected.Bounds();
            var box = new WRect { Width = r.Width + 8 * px, Height = r.Height + 8 * px, Stroke = accent, StrokeThickness = px, StrokeDashArray = [4, 3], IsHitTestVisible = false };
            Canvas.SetLeft(box, r.X - 4 * px); Canvas.SetTop(box, r.Y - 4 * px);
            Overlay.Children.Add(box);
            if (selected.IsTwoPoint)
                foreach (var pt in new[] { selected.P0, selected.P1 })
                {
                    var h = new WRect { Width = HandleSize * px, Height = HandleSize * px, Fill = Brushes.White, Stroke = accent, StrokeThickness = px, IsHitTestVisible = false };
                    Canvas.SetLeft(h, pt.X - HandleSize * px / 2); Canvas.SetTop(h, pt.Y - HandleSize * px / 2);
                    Overlay.Children.Add(h);
                }
        }
        if (tool == Tool.Crop && !cropRect.IsEmpty)
        {
            var all = new RectangleGeometry(new Rect(0, 0, baseImg.Width, baseImg.Height));
            var hole = new RectangleGeometry(new Rect(cropRect.X, cropRect.Y, cropRect.Width, cropRect.Height));
            Overlay.Children.Add(new System.Windows.Shapes.Path { Data = new CombinedGeometry(GeometryCombineMode.Exclude, all, hole), Fill = new SolidColorBrush(Color.FromArgb(150, 0, 0, 0)), IsHitTestVisible = false });
            var edge = new WRect { Width = cropRect.Width, Height = cropRect.Height, Stroke = accent, StrokeThickness = 1.5 * px, StrokeDashArray = [5, 3], IsHitTestVisible = false };
            Canvas.SetLeft(edge, cropRect.X); Canvas.SetTop(edge, cropRect.Y);
            Overlay.Children.Add(edge);
            var label = new Border { Background = (Brush)Application.Current.Resources["Header"], Padding = new Thickness(6, 2, 6, 2), IsHitTestVisible = false, LayoutTransform = new ScaleTransform(px, px) };
            var lt = new TextBlock { Text = (int)cropRect.Width + " × " + (int)cropRect.Height, Foreground = accent };
            lt.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont");
            label.Child = lt;
            Canvas.SetLeft(label, cropRect.X); Canvas.SetTop(label, cropRect.Y - 24 * px);
            Overlay.Children.Add(label);
        }
    }
}
