using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Documents;
using System.Windows.Input;
using System.Windows.Media;

namespace BuzzPrint;

/// <summary>
/// Export several captures, in an order the person sets, to PowerPoint, Word, PDF or one tall image. Each row has a
/// title and a note that go into the document (and back to the library); rows reorder by dragging the grip, with
/// Alt+↑/↓, or with the sort buttons. Options are remembered.
/// </summary>
public sealed class ExportWindow : Window
{
    readonly List<ExportPage> pages;
    readonly StackPanel rows = new();
    readonly TextBox titleBox;
    readonly Button exportButton;
    readonly TextBlock countText, statusText;
    readonly Dictionary<ExportFormat, RadioButton> formats = new();
    readonly CheckBox cover, numbers, captions, meta, onePerPage, landscape, open, saveText;
    readonly int skipped;
    ExportFormat format;
    bool busy;

    static readonly (ExportFormat f, string glyph, string name, string desc)[] Formats =
    {
        (ExportFormat.Pptx, "", "PowerPoint", "Una captura por diapositiva, 16:9"),
        (ExportFormat.Docx, "", "Word", "Documento con pasos navegables"),
        (ExportFormat.Pdf, "", "PDF", "Para compartir o imprimir"),
        (ExportFormat.Png, "", "Imagen única", "Todas apiladas en un PNG"),
    };

    ExportWindow(IEnumerable<CaptureItem> items, int skippedRecordings)
    {
        skipped = skippedRecordings;
        pages = items.Select(it => new ExportPage { Item = it, Title = string.IsNullOrEmpty(it.Title) ? it.Source : it.Title, Note = it.Note ?? "" }).ToList();
        var st = Store.Settings;
        format = st.ExportFormat switch { "docx" => ExportFormat.Docx, "pdf" => ExportFormat.Pdf, "png" => ExportFormat.Png, _ => ExportFormat.Pptx };

        SetResourceReference(StyleProperty, typeof(Window));
        TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        Title = L.T("Exportar capturas") + " · BuzzPrint";
        Width = 1060; Height = 720; MinWidth = 820; MinHeight = 520;
        WindowStartupLocation = WindowStartupLocation.CenterOwner; ShowInTaskbar = false;

        var root = new DockPanel { Margin = new Thickness(24, 20, 24, 18) };
        // Header
        var head = new StackPanel();
        var h = new TextBlock { FontWeight = FontWeights.SemiBold };
        var mark = new Run("◤ "); mark.SetResourceReference(TextElement.ForegroundProperty, "Accent");
        h.Inlines.Add(mark); h.Inlines.Add(new Run(L.T("Exportar capturas")));
        h.SetResourceReference(TextBlock.FontSizeProperty, "SectionFontSize");
        head.Children.Add(h);
        head.Children.Add(Muted(L.T("Ordená, poné títulos y notas, y elegí el formato. El orden de la lista es el del documento."), new Thickness(0, 6, 0, 14)));
        DockPanel.SetDock(head, Dock.Top); root.Children.Add(head);

        // Footer
        var foot = new DockPanel { Margin = new Thickness(0, 14, 0, 0) };
        exportButton = new Button { IsDefault = true, MinWidth = 150, FontWeight = FontWeights.SemiBold, Margin = new Thickness(8, 0, 0, 0) };
        exportButton.SetResourceReference(BackgroundProperty, "Accent"); exportButton.SetResourceReference(ForegroundProperty, "OnAccent"); exportButton.SetResourceReference(BorderBrushProperty, "Accent");
        exportButton.Click += async (_, _) => await ExportAsync();
        var cancel = new Button { Content = L.T("Cancelar"), IsCancel = true, MinWidth = 96 };
        var right = new StackPanel { Orientation = Orientation.Horizontal };
        right.Children.Add(cancel); right.Children.Add(exportButton);
        DockPanel.SetDock(right, Dock.Right); foot.Children.Add(right);
        statusText = Muted("", new Thickness(0)); statusText.VerticalAlignment = VerticalAlignment.Center; statusText.TextTrimming = TextTrimming.CharacterEllipsis;
        foot.Children.Add(statusText);
        DockPanel.SetDock(foot, Dock.Bottom); root.Children.Add(foot);

        // Options column
        var opts = new StackPanel();
        opts.Children.Add(Section(L.T("FORMATO"), 0));
        foreach (var (f, glyph, name, desc) in Formats)
        {
            var rb = new RadioButton { GroupName = "fmt", IsChecked = f == format, Margin = new Thickness(0, 0, 0, 6), Cursor = Cursors.Hand, Tag = f };
            rb.Template = CardTemplate();
            var card = new DockPanel();
            var g = new TextBlock { Text = glyph, FontFamily = new FontFamily("Segoe Fluent Icons, Segoe MDL2 Assets"), FontSize = 18, Width = 30, VerticalAlignment = VerticalAlignment.Center };
            g.SetResourceReference(TextBlock.ForegroundProperty, "Accent");
            card.Children.Add(g);
            var txt = new StackPanel();
            txt.Children.Add(new TextBlock { Text = L.T(name), FontWeight = FontWeights.SemiBold });
            txt.Children.Add(Muted(L.T(desc), new Thickness(0, 1, 0, 0)));
            card.Children.Add(txt);
            rb.Content = card;
            rb.Checked += (_, _) => { format = f; Sync(); };
            formats[f] = rb;
            opts.Children.Add(rb);
        }
        opts.Children.Add(Section(L.T("DOCUMENTO"), 18));
        titleBox = new TextBox { Text = L.T("Capturas") + " — " + DateTime.Now.ToString("d"), Margin = new Thickness(0, 0, 0, 4) };
        opts.Children.Add(titleBox);
        opts.Children.Add(Muted(L.T("Va en la portada y en el nombre del archivo."), new Thickness(0, 0, 0, 6)));
        opts.Children.Add(Section(L.T("INCLUIR"), 12));
        cover = Check(L.T("Portada con título y fecha"), st.ExportCover);
        numbers = Check(L.T("Numerar pasos (Paso 1, Paso 2…)"), st.ExportNumbers);
        captions = Check(L.T("Título de cada captura"), st.ExportCaptions);
        meta = Check(L.T("Origen, fecha y tamaño"), st.ExportMeta);
        onePerPage = Check(L.T("Una captura por página"), st.ExportOnePerPage);
        landscape = Check(L.T("Páginas horizontales"), st.ExportLandscape);
        foreach (var c in new[] { cover, numbers, captions, meta, onePerPage, landscape }) opts.Children.Add(c);
        opts.Children.Add(Section(L.T("AL TERMINAR"), 12));
        open = Check(L.T("Abrir el archivo"), st.ExportOpen);
        saveText = Check(L.T("Guardar títulos y notas en la biblioteca"), st.ExportSaveText);
        opts.Children.Add(open); opts.Children.Add(saveText);
        var optsScroll = new ScrollViewer { Content = opts, VerticalScrollBarVisibility = ScrollBarVisibility.Auto, Width = 320, Margin = new Thickness(20, 0, 0, 0), Padding = new Thickness(0, 0, 8, 0) };
        DockPanel.SetDock(optsScroll, Dock.Right); root.Children.Add(optsScroll);

        // List column: sort bar + rows
        var left = new DockPanel();
        var bar = new DockPanel { Margin = new Thickness(0, 0, 0, 8) };
        countText = new TextBlock { VerticalAlignment = VerticalAlignment.Center, FontWeight = FontWeights.SemiBold };
        var sorts = new StackPanel { Orientation = Orientation.Horizontal };
        sorts.Children.Add(Quiet(L.T("Más antiguas primero"), () => Sort(true)));
        sorts.Children.Add(Quiet(L.T("Más nuevas primero"), () => Sort(false)));
        sorts.Children.Add(Quiet(L.T("Invertir"), () => { pages.Reverse(); Render(); }));
        DockPanel.SetDock(sorts, Dock.Right); bar.Children.Add(sorts);
        bar.Children.Add(countText);
        DockPanel.SetDock(bar, Dock.Top); left.Children.Add(bar);
        var listFrame = new Border { BorderThickness = new Thickness(1), Padding = new Thickness(0) };
        listFrame.SetResourceReference(Border.BorderBrushProperty, "FaintBorder");
        listFrame.SetResourceReference(Border.BackgroundProperty, "Canvas");
        listFrame.SetResourceReference(Border.CornerRadiusProperty, "ControlRadius");
        listFrame.Child = new ScrollViewer { Content = rows, VerticalScrollBarVisibility = ScrollBarVisibility.Auto, Padding = new Thickness(8) };
        left.Children.Add(listFrame);
        root.Children.Add(left);

        Content = root;
        PreviewKeyDown += OnKey;
        Loaded += (_, _) => { Render(); Sync(); };
    }

    /// <summary>Opens the export window for the given captures (recordings are left out and counted).</summary>
    /// <param name="keepOrder">true when the person picked the captures one by one (Ctrl+clic): that order is kept;
    /// otherwise they are listed chronologically, the order things happened.</param>
    public static void Open(Window owner, IReadOnlyList<CaptureItem> items, bool keepOrder)
    {
        var images = items.Where(i => !i.IsRecording && File.Exists(i.Path)).ToList();
        int skipped = items.Count - images.Count;
        if (images.Count == 0)
        {
            PromptWindow.Info(owner, L.T("Exportar capturas"), L.T("Elegí una o más capturas en la biblioteca (Ctrl+clic o Shift+clic). Las grabaciones de video y audio no se exportan."));
            return;
        }
        if (!keepOrder) images = images.OrderBy(i => i.Created).ToList();
        new ExportWindow(images, skipped) { Owner = owner }.ShowDialog();
    }
    // ------------------------------------------------------------------ rows
    void Render()
    {
        rows.Children.Clear();
        for (int i = 0; i < pages.Count; i++) rows.Children.Add(Row(i));
        countText.Text = L.Count(pages.Count, "captura", "capturas") + (skipped > 0 ? "  ·  " + L.F("{0} sin incluir (grabaciones)", skipped) : "");
        Sync();
    }

    FrameworkElement Row(int index)
    {
        var p = pages[index];
        var border = new Border { BorderThickness = new Thickness(1), Margin = new Thickness(0, 0, 0, 8), Padding = new Thickness(8), AllowDrop = true, Tag = index };
        border.SetResourceReference(Border.BackgroundProperty, "Surface");
        border.SetResourceReference(Border.BorderBrushProperty, "FaintBorder");
        border.SetResourceReference(Border.CornerRadiusProperty, "ControlRadius");
        var grid = new Grid();
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

        // Grip + step number: drag handle.
        var grip = new StackPanel { Width = 38, VerticalAlignment = VerticalAlignment.Center, Cursor = Cursors.SizeAll, Background = Brushes.Transparent, ToolTip = L.T("Arrastrá para reordenar · Alt+↑/↓") };
        var num = new TextBlock { Text = (index + 1).ToString("00"), FontWeight = FontWeights.Bold, HorizontalAlignment = HorizontalAlignment.Center, FontSize = 15 };
        num.SetResourceReference(TextBlock.ForegroundProperty, "Accent"); num.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont");
        var dots = new TextBlock { Text = "", FontFamily = new FontFamily("Segoe Fluent Icons, Segoe MDL2 Assets"), HorizontalAlignment = HorizontalAlignment.Center, Margin = new Thickness(0, 4, 0, 0) };
        dots.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        grip.Children.Add(num); grip.Children.Add(dots);
        Point start = default;
        grip.PreviewMouseLeftButtonDown += (_, e) => start = e.GetPosition(this);
        grip.PreviewMouseMove += (_, e) =>
        {
            if (e.LeftButton != MouseButtonState.Pressed) return;
            var pos = e.GetPosition(this);
            if (Math.Abs(pos.Y - start.Y) < 5 && Math.Abs(pos.X - start.X) < 5) return;
            border.Opacity = .5;
            DragDrop.DoDragDrop(grip, new DataObject("buzzprint-export-row", index), DragDropEffects.Move);
            border.Opacity = 1;
        };
        border.DragOver += (_, e) => { e.Effects = e.Data.GetDataPresent("buzzprint-export-row") ? DragDropEffects.Move : DragDropEffects.None; border.SetResourceReference(Border.BorderBrushProperty, e.Effects == DragDropEffects.Move ? "Accent" : "FaintBorder"); e.Handled = true; };
        border.DragLeave += (_, _) => border.SetResourceReference(Border.BorderBrushProperty, "FaintBorder");
        border.Drop += (_, e) =>
        {
            if (e.Data.GetData("buzzprint-export-row") is int from && from != index) Move(from, index);
            else border.SetResourceReference(Border.BorderBrushProperty, "FaintBorder");
            e.Handled = true;
        };
        grid.Children.Add(grip);

        var thumbFrame = new Border { Width = 150, Height = 86, Margin = new Thickness(4, 0, 12, 0), BorderThickness = new Thickness(1) };
        thumbFrame.SetResourceReference(Border.BackgroundProperty, "Canvas");
        thumbFrame.SetResourceReference(Border.BorderBrushProperty, "FaintBorder");
        thumbFrame.Child = new Image { Source = Imaging.Thumb(p.Item.Path, 300), Stretch = Stretch.Uniform, Margin = new Thickness(3) };
        RenderOptions.SetBitmapScalingMode(thumbFrame.Child, BitmapScalingMode.HighQuality);
        Grid.SetColumn(thumbFrame, 1); grid.Children.Add(thumbFrame);

        var fields = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
        var title = new TextBox { Text = p.Title, FontWeight = FontWeights.SemiBold, Padding = new Thickness(8, 5, 8, 5) };
        title.TextChanged += (_, _) => p.Title = title.Text;
        fields.Children.Add(Hinted(title, L.T("Título de la captura")));
        var note = new TextBox { Text = p.Note, AcceptsReturn = true, TextWrapping = TextWrapping.Wrap, MaxHeight = 90, VerticalScrollBarVisibility = ScrollBarVisibility.Auto, Margin = new Thickness(0, 6, 0, 0), Padding = new Thickness(8, 5, 8, 5) };
        note.TextChanged += (_, _) => p.Note = note.Text;
        fields.Children.Add(Hinted(note, L.T("Nota o instrucción (opcional): va debajo de la imagen")));
        var metaLine = Muted(p.Item.Created.ToString("g") + (p.Item.Width > 0 ? "  ·  " + p.Item.Width + "×" + p.Item.Height : "") + (string.IsNullOrEmpty(p.Item.Source) ? "" : "  ·  " + p.Item.Source), new Thickness(2, 5, 0, 0));
        metaLine.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont");
        metaLine.TextTrimming = TextTrimming.CharacterEllipsis;
        fields.Children.Add(metaLine);
        Grid.SetColumn(fields, 2); grid.Children.Add(fields);

        var actions = new StackPanel { VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(8, 0, 0, 0) };
        actions.Children.Add(RowButton("", L.T("Subir · Alt+↑"), () => Move(index, index - 1), index > 0));
        actions.Children.Add(RowButton("", L.T("Bajar · Alt+↓"), () => Move(index, index + 1), index < pages.Count - 1));
        actions.Children.Add(RowButton("", L.T("Quitar de la exportación (no la borra)"), () => { pages.RemoveAt(index); Render(); }, pages.Count > 1));
        Grid.SetColumn(actions, 3); grid.Children.Add(actions);
        border.Child = grid;
        return border;
    }

    void Move(int from, int to, bool focus = false)
    {
        if (to < 0 || to >= pages.Count || from == to) return;
        var p = pages[from];
        pages.RemoveAt(from); pages.Insert(to, p);
        Render();
        if (focus && rows.Children[to] is Border b) FocusFirstBox(b, keyboardSource);
        ((FrameworkElement)rows.Children[to]).BringIntoView();
    }
    int keyboardSource;
    static void FocusFirstBox(DependencyObject root, int which)
    {
        int seen = 0;
        void Walk(DependencyObject d)
        {
            for (int i = 0; i < VisualTreeHelper.GetChildrenCount(d); i++)
            {
                var c = VisualTreeHelper.GetChild(d, i);
                if (c is TextBox t) { if (seen++ == which) { t.Focus(); return; } }
                else Walk(c);
            }
        }
        root.Dispatcher.BeginInvoke(() => Walk(root), System.Windows.Threading.DispatcherPriority.Loaded);
    }

    void Sort(bool oldestFirst)
    {
        var sorted = oldestFirst ? pages.OrderBy(p => p.Item.Created).ToList() : pages.OrderByDescending(p => p.Item.Created).ToList();
        pages.Clear(); pages.AddRange(sorted); Render();
    }

    void OnKey(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Escape && !busy) { Close(); e.Handled = true; return; }
        if (Keyboard.Modifiers != ModifierKeys.Alt || e.SystemKey is not (Key.Up or Key.Down)) return;
        // Alt+↑/↓ moves the row that has the keyboard focus.
        for (int i = 0; i < rows.Children.Count; i++)
        {
            if (rows.Children[i] is not FrameworkElement row || !row.IsKeyboardFocusWithin) continue;
            keyboardSource = Keyboard.FocusedElement is TextBox tb && tb.AcceptsReturn ? 1 : 0;
            Move(i, e.SystemKey == Key.Up ? i - 1 : i + 1, true);
            e.Handled = true;
            return;
        }
    }

    void Sync()
    {
        if (exportButton == null) return;
        bool office = format is ExportFormat.Docx or ExportFormat.Pdf;
        onePerPage.IsEnabled = format == ExportFormat.Docx;
        landscape.IsEnabled = office;
        onePerPage.Opacity = onePerPage.IsEnabled ? 1 : .45;
        landscape.Opacity = landscape.IsEnabled ? 1 : .45;
        string name = format switch { ExportFormat.Pptx => "PowerPoint", ExportFormat.Docx => "Word", ExportFormat.Pdf => "PDF", _ => L.T("imagen") };
        exportButton.Content = L.F("Exportar a {0}", name);
        exportButton.IsEnabled = !busy && pages.Count > 0;
    }

    // ------------------------------------------------------------------ export
    async Task ExportAsync()
    {
        if (busy || pages.Count == 0) return;
        var st = Store.Settings;
        st.ExportFormat = format switch { ExportFormat.Docx => "docx", ExportFormat.Pdf => "pdf", ExportFormat.Png => "png", _ => "pptx" };
        st.ExportCover = cover.IsChecked == true; st.ExportNumbers = numbers.IsChecked == true; st.ExportCaptions = captions.IsChecked == true;
        st.ExportMeta = meta.IsChecked == true; st.ExportOnePerPage = onePerPage.IsChecked == true; st.ExportLandscape = landscape.IsChecked == true;
        st.ExportOpen = open.IsChecked == true; st.ExportSaveText = saveText.IsChecked == true;
        string folder = Directory.Exists(st.ExportFolder) ? st.ExportFolder : Environment.GetFolderPath(Environment.SpecialFolder.MyDocuments);
        string docTitle = titleBox.Text.Trim().Length > 0 ? titleBox.Text.Trim() : L.T("Capturas");
        string ext = Exporter.Extension(format);
        var dlg = new Microsoft.Win32.SaveFileDialog
        {
            Title = L.T("Exportar capturas"), InitialDirectory = folder, FileName = SafeName(docTitle) + ext, DefaultExt = ext, AddExtension = true,
            Filter = format switch { ExportFormat.Pptx => L.T("Presentación de PowerPoint") + " (*.pptx)|*.pptx", ExportFormat.Docx => L.T("Documento de Word") + " (*.docx)|*.docx", ExportFormat.Pdf => "PDF (*.pdf)|*.pdf", _ => L.T("Imagen PNG") + " (*.png)|*.png" }
        };
        if (dlg.ShowDialog(this) != true) return;
        st.ExportFolder = Path.GetDirectoryName(dlg.FileName) ?? "";
        Store.SaveSettings();

        var options = new ExportOptions
        {
            Title = docTitle, Cover = st.ExportCover, Numbers = st.ExportNumbers, Captions = st.ExportCaptions, Meta = st.ExportMeta,
            OnePerPage = st.ExportOnePerPage, Landscape = st.ExportLandscape, Accent = AccentHex()
        };
        var snapshot = pages.Select(p => new ExportPage { Item = p.Item, Title = p.Title, Note = p.Note }).ToList();
        busy = true; Sync();
        statusText.Text = L.T("Exportando…");
        Cursor = Cursors.Wait;
        string path = dlg.FileName;
        var fmt = format;
        try
        {
            await Task.Run(() => Exporter.Write(path, fmt, snapshot, options));
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or InvalidOperationException or ArgumentException or System.Runtime.InteropServices.ExternalException)
        {
            busy = false; Cursor = null; Sync();
            statusText.Text = "";
            PromptWindow.Info(this, L.T("No se pudo exportar"), ex is IOException ? L.T("El archivo está abierto en otro programa o no se puede escribir. Cerralo y probá de nuevo.") + "\n\n" + ex.Message : ex.Message);
            return;
        }
        if (st.ExportSaveText) SaveTextBack();
        Cursor = null;
        if (st.ExportOpen) try { Process.Start(new ProcessStartInfo(path) { UseShellExecute = true }); } catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException) { }
        else try { Process.Start("explorer.exe", "/select,\"" + path + "\""); } catch (System.ComponentModel.Win32Exception) { }
        LastExport = path;
        Close();
    }

    public static string? LastExport { get; private set; }
    internal static ExportWindow ForShot(Window owner, IReadOnlyList<CaptureItem> items) => new(items.OrderBy(i => i.Created), 0) { Owner = owner };

    void SaveTextBack()
    {
        bool changed = false;
        foreach (var p in pages)
        {
            string current = string.IsNullOrEmpty(p.Item.Title) ? p.Item.Source : p.Item.Title;
            if (p.Title.Trim() != current.Trim() && p.Title.Trim().Length > 0) { Store.UpdateMeta(p.Item, "title", p.Title.Trim()); changed = true; }
            if (p.Note.Trim() != (p.Item.Note ?? "").Trim()) { Store.UpdateMeta(p.Item, "note", p.Note.Trim()); changed = true; }
        }
        if (changed) Store.RaiseChanged();
    }

    internal static string AccentHex()
    {
        if (Application.Current.TryFindResource("Accent") is SolidColorBrush b) return b.Color.R.ToString("X2") + b.Color.G.ToString("X2") + b.Color.B.ToString("X2");
        return "F43F5E";
    }
    static string SafeName(string s)
    {
        foreach (var c in Path.GetInvalidFileNameChars()) s = s.Replace(c, '-');
        s = s.Trim().TrimEnd('.');
        return s.Length > 80 ? s[..80] : s.Length == 0 ? "BuzzPrint" : s;
    }

    // ------------------------------------------------------------------ small UI helpers
    static TextBlock Muted(string text, Thickness margin)
    {
        var t = new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap, Margin = margin };
        t.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        t.SetResourceReference(TextBlock.FontSizeProperty, "SmallFontSize");
        return t;
    }
    static FrameworkElement Section(string text, double top)
    {
        var sp = new StackPanel { Orientation = Orientation.Horizontal, Margin = new Thickness(0, top, 0, 8) };
        var m = new TextBlock { Text = "▚", FontSize = 10, Margin = new Thickness(0, 0, 7, 0), VerticalAlignment = VerticalAlignment.Center };
        m.SetResourceReference(TextBlock.ForegroundProperty, "Accent");
        var t = new TextBlock { Text = text };
        t.SetResourceReference(StyleProperty, "SectionLabel");
        sp.Children.Add(m); sp.Children.Add(t);
        return sp;
    }
    static CheckBox Check(string text, bool value) => new() { Content = text, IsChecked = value, Margin = new Thickness(0, 4, 0, 4) };
    static Button Quiet(string text, Action click)
    {
        var b = new Button { Content = text };
        b.SetResourceReference(StyleProperty, "QuietButton");
        b.Click += (_, _) => click();
        return b;
    }
    static Button RowButton(string glyph, string tip, Action click, bool enabled)
    {
        var b = new Button { Content = glyph, ToolTip = tip, IsEnabled = enabled, Width = 28, Height = 26, FontSize = 12, Opacity = enabled ? 1 : .35 };
        b.SetResourceReference(StyleProperty, "IconButton");
        b.Width = 28; b.Height = 26; b.FontSize = 12;
        b.Click += (_, _) => click();
        return b;
    }
    static FrameworkElement Hinted(TextBox box, string hint)
    {
        var g = new Grid();
        var t = new TextBlock { Text = hint, IsHitTestVisible = false, Margin = new Thickness(10, 6, 10, 0), TextTrimming = TextTrimming.CharacterEllipsis, FontWeight = FontWeights.Normal };
        t.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        t.Opacity = .7;
        void Update() => t.Visibility = box.Text.Length == 0 ? Visibility.Visible : Visibility.Collapsed;
        box.TextChanged += (_, _) => Update();
        Update();
        g.Children.Add(box); g.Children.Add(t);
        return g;
    }
    static ControlTemplate CardTemplate()
    {
        // Radio as a selectable card: accent ring and tint when checked.
        var border = new FrameworkElementFactory(typeof(Border), "Card");
        border.SetValue(Border.BorderThicknessProperty, new Thickness(1));
        border.SetValue(Border.PaddingProperty, new Thickness(10, 8, 10, 8));
        border.SetResourceReference(Border.BackgroundProperty, "Surface");
        border.SetResourceReference(Border.BorderBrushProperty, "FaintBorder");
        border.SetResourceReference(Border.CornerRadiusProperty, "ControlRadius");
        border.AppendChild(new FrameworkElementFactory(typeof(ContentPresenter)));
        var t = new ControlTemplate(typeof(RadioButton)) { VisualTree = border };
        var on = new Trigger { Property = ToggleButton.IsCheckedProperty, Value = true };
        on.Setters.Add(new Setter(Border.BorderBrushProperty, new DynamicResourceExtension("Accent"), "Card"));
        on.Setters.Add(new Setter(Border.BackgroundProperty, new DynamicResourceExtension("AccentWeak"), "Card"));
        var hover = new Trigger { Property = IsMouseOverProperty, Value = true };
        hover.Setters.Add(new Setter(Border.BorderBrushProperty, new DynamicResourceExtension("StrongBorder"), "Card"));
        t.Triggers.Add(hover);
        t.Triggers.Add(on);   // added last: checked wins over hover
        return t;
    }
}
