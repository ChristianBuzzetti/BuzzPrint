using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;

namespace BuzzPrint;

/// <summary>
/// Document templates: pick one for exports, duplicate any into an editable copy, edit, rename or delete your own
/// (up to <see cref="DocTemplates.MaxCustom"/>), or import a design from a Word, PowerPoint, PDF or image file.
/// </summary>
public sealed class TemplatesWindow : Window
{
    readonly string appAccent;
    readonly WrapPanel grid = new();
    readonly TextBlock count = new() { VerticalAlignment = VerticalAlignment.Center };
    readonly Button edit = new() { Content = L.T("Editar"), MinWidth = 100, Margin = new Thickness(8, 0, 0, 0) };
    readonly Button delete = new() { Content = L.T("Eliminar"), MinWidth = 100, Margin = new Thickness(8, 0, 0, 0) };
    readonly Button duplicate = new() { Content = L.T("Duplicar"), MinWidth = 100, Margin = new Thickness(8, 0, 0, 0) };
    readonly Button import = new() { Content = L.T("Importar desde archivo…"), MinWidth = 170, Margin = new Thickness(8, 0, 0, 0) };
    string selectedId;
    string? chosen;

    TemplatesWindow(Window? owner, string currentId, string appAccent)
    {
        this.appAccent = appAccent;
        selectedId = currentId;
        SetResourceReference(StyleProperty, typeof(Window));
        TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        Title = L.T("Plantillas de documento") + " · BuzzPrint";
        Width = 1060; Height = 760; MinWidth = 760; MinHeight = 520; ShowInTaskbar = false;
        if (owner is { IsVisible: true }) { Owner = owner; WindowStartupLocation = WindowStartupLocation.CenterOwner; } else WindowStartupLocation = WindowStartupLocation.CenterScreen;

        var root = new DockPanel { Margin = new Thickness(24, 20, 24, 18) };
        var head = new StackPanel();
        var h = new TextBlock { Text = L.T("Plantillas de documento"), FontWeight = FontWeights.SemiBold }; h.SetResourceReference(TextBlock.FontSizeProperty, "TitleFontSize"); head.Children.Add(h);
        var p = new TextBlock { Text = L.T("El diseño de los PowerPoint, Word, PDF e imágenes que exportás. Elegí una, duplicala para ajustarla a tu gusto o importá el diseño de un archivo tuyo."), TextWrapping = TextWrapping.Wrap, Margin = new Thickness(0, 4, 0, 14) };
        p.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        head.Children.Add(p);
        var top = BobertoMark.WithBadge(head, 48); DockPanel.SetDock(top, Dock.Top); root.Children.Add(top);

        var foot = new DockPanel { Margin = new Thickness(0, 14, 0, 0) };
        var use = new Button { Content = L.T("Usar esta plantilla"), IsDefault = true, MinWidth = 160, Margin = new Thickness(8, 0, 0, 0), FontWeight = FontWeights.SemiBold };
        use.SetResourceReference(BackgroundProperty, "Accent"); use.SetResourceReference(ForegroundProperty, "OnAccent"); use.SetResourceReference(BorderBrushProperty, "Accent");
        use.Click += (_, _) => { chosen = selectedId; Close(); };
        var close = new Button { Content = L.T("Cerrar"), IsCancel = true, MinWidth = 100, Margin = new Thickness(8, 0, 0, 0) };
        close.Click += (_, _) => Close();
        foreach (var b in new[] { use, close }) { DockPanel.SetDock(b, Dock.Right); foot.Children.Add(b); }
        foreach (var b in new[] { import, duplicate, edit, delete }) { DockPanel.SetDock(b, Dock.Right); foot.Children.Add(b); }
        count.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        foot.Children.Add(count);
        DockPanel.SetDock(foot, Dock.Bottom); root.Children.Add(foot);

        root.Children.Add(new ScrollViewer { Content = grid, VerticalScrollBarVisibility = ScrollBarVisibility.Auto });
        Content = root;

        duplicate.Click += (_, _) => Duplicate();
        edit.Click += (_, _) => Edit();
        delete.Click += (_, _) => Delete();
        import.Click += async (_, _) => await ImportAsync();
#if !PREVIEW
        import.Visibility = Visibility.Collapsed;
#endif
        PreviewKeyDown += (_, e) => { if (e.Key == Key.Escape) Close(); };
        Render();
    }

    void Render()
    {
        grid.Children.Clear();
        var all = DocTemplates.All();
        if (all.All(t => t.Id != selectedId)) selectedId = all[0].Id;
        foreach (var t in all)
        {
            var tpl = t;
            bool on = t.Id == selectedId;
            var card = new Border { Margin = new Thickness(0, 0, 14, 14), Cursor = Cursors.Hand, Width = 470, Padding = new Thickness(14), BorderThickness = new Thickness(on ? 2 : 1) };
            card.SetResourceReference(Border.CornerRadiusProperty, "PanelRadius");
            card.SetResourceReference(Border.BackgroundProperty, on ? "AccentWeak" : "SurfaceRaised");
            card.SetResourceReference(Border.BorderBrushProperty, on ? "Accent" : "FaintBorder");
            var body = new StackPanel();
            body.Children.Add(TemplatePreview.Build(t, appAccent, 210));
            var title = new DockPanel { Margin = new Thickness(0, 12, 0, 0) };
            var tag = new TextBlock { Text = t.BuiltIn ? L.T("INCLUIDA") : L.T("PROPIA"), FontSize = 10, FontWeight = FontWeights.SemiBold, VerticalAlignment = VerticalAlignment.Center };
            tag.SetResourceReference(TextBlock.ForegroundProperty, t.BuiltIn ? "Muted" : "Accent");
            DockPanel.SetDock(tag, Dock.Right); title.Children.Add(tag);
            title.Children.Add(new TextBlock { Text = t.BuiltIn ? L.T(t.Name) : t.Name, FontWeight = FontWeights.SemiBold, TextTrimming = TextTrimming.CharacterEllipsis });
            body.Children.Add(title);
            var desc = new TextBlock { Text = t.BuiltIn ? L.T(t.Description) : t.Description, TextWrapping = TextWrapping.Wrap, Margin = new Thickness(0, 2, 0, 0) };
            desc.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
            desc.SetResourceReference(TextBlock.FontSizeProperty, "SmallFontSize");
            body.Children.Add(desc);
            card.Child = body;
            card.MouseLeftButtonDown += (_, e) =>
            {
                if (e.ClickCount == 2) { chosen = tpl.Id; Close(); return; }
                selectedId = tpl.Id; Render();
            };
            grid.Children.Add(card);
        }
        var sel = DocTemplates.Get(selectedId);
        int custom = all.Count(t => !t.BuiltIn);
        edit.IsEnabled = delete.IsEnabled = !sel.BuiltIn;
        duplicate.IsEnabled = import.IsEnabled = custom < DocTemplates.MaxCustom;
        count.Text = L.F("{0} de {1} plantillas propias", custom, DocTemplates.MaxCustom);
    }

    void Duplicate()
    {
        var copy = DocTemplates.Get(selectedId).Copy();
        copy.Name = (DocTemplates.Get(selectedId).BuiltIn ? L.T(copy.Name) : copy.Name) + " " + L.T("(copia)");
        if (copy.BuiltIn) copy.Description = L.T(copy.Description);
        copy.Id = "";
        var edited = TemplateEditWindow.Edit(this, copy, appAccent, L.T("Nueva plantilla"), L.T("Ajustá colores, tipografías y estilo. Se guarda como plantilla propia."));
        if (edited == null) return;
        Save(edited);
    }

    void Edit()
    {
        var t = DocTemplates.Get(selectedId);
        if (t.BuiltIn) return;
        var edited = TemplateEditWindow.Edit(this, t, appAccent, L.T("Editar plantilla"), L.T("Los cambios se aplican a los próximos exports con esta plantilla."));
        if (edited != null) Save(edited);
    }

    void Save(DocTemplate t)
    {
        try { selectedId = DocTemplates.Save(t).Id; Render(); }
        catch (Exception ex) when (ex is InvalidOperationException or System.IO.IOException or UnauthorizedAccessException) { PromptWindow.Info(this, L.T("Plantillas de documento"), ex.Message); }
    }

    void Delete()
    {
        var t = DocTemplates.Get(selectedId);
        if (t.BuiltIn) return;
        if (!PromptWindow.Confirm(this, L.T("Eliminar plantilla"), L.F("Se elimina «{0}». Los documentos que ya exportaste no cambian.", t.Name), L.T("Eliminar"))) return;
        DocTemplates.Delete(t.Id);
        selectedId = DocTemplates.BuiltIns[0].Id;
        Render();
    }

    async Task ImportAsync()
    {
#if PREVIEW
        var dlg = new Microsoft.Win32.OpenFileDialog { Title = L.T("Importar el diseño de un archivo"), Filter = L.T("Documentos e imágenes") + "|*.docx;*.pptx;*.pdf;*.png;*.jpg;*.jpeg|Word|*.docx|PowerPoint|*.pptx|PDF|*.pdf|" + L.T("Imágenes") + "|*.png;*.jpg;*.jpeg" };
        if (dlg.ShowDialog(this) != true) return;
        string file = dlg.FileName;
        string ext = System.IO.Path.GetExtension(file).ToLowerInvariant();
        bool aiReady = Store.Settings.AiEnabled && Ai.AiConnections.Active(Store.Settings) != null;
        if (!aiReady && ext is not (".docx" or ".pptx"))
        {
            PromptWindow.Info(this, L.T("Importar el diseño de un archivo"), L.T("Para leer el diseño de un PDF o una imagen hace falta una conexión de IA. Configurala en IA → Configurar IA. Los Word y PowerPoint se pueden importar sin IA."));
            return;
        }
        if (aiReady && !PromptWindow.Confirm(this, L.T("Importar el diseño de un archivo"),
                L.F("BuzzPrint lee los colores y tipografías del archivo y le pide a {0} que arme la plantilla. Se envía la primera página como imagen (o, en Word y PowerPoint, el tema del documento), no el resto del contenido.", Ai.AiConnections.DisplayName(Ai.AiConnections.Active(Store.Settings)!)),
                L.T("Importar"))) return;
        DocTemplate result;
        try
        {
            using var busy = AiBusy.Show(this, L.T("Interpretando el diseño del archivo…"));
            result = await Ai.AiTemplate.InterpretAsync(file, aiReady, busy.Token);
        }
        catch (OperationCanceledException) { PromptWindow.Info(this, L.T("Importar el diseño de un archivo"), AiBusy.Stopped(this, L.T("Importar el diseño de un archivo"))); return; }
        catch (Exception ex) when (ex is Ai.AiException or System.IO.IOException or System.IO.InvalidDataException or UnauthorizedAccessException or System.Xml.XmlException)
        {
            PromptWindow.Info(this, L.T("Importar el diseño de un archivo"), ex.Message);
            return;
        }
        var edited = TemplateEditWindow.Edit(this, result, appAccent, L.T("Revisar la plantilla importada"), L.T("Esto es lo que se interpretó del archivo. Corregí lo que haga falta antes de guardarla."));
        if (edited != null) Save(edited);
#else
        await Task.CompletedTask;
#endif
    }

    /// <summary>Opens the manager; the template the person chose with "Use", or null.</summary>
    public static string? Pick(Window? owner, string currentId, string appAccent)
    {
        var w = new TemplatesWindow(owner, currentId, appAccent);
        w.ShowDialog();
        return w.chosen;
    }

    /// <summary>For the screenshot run.</summary>
    internal static Window ForShot(Window owner, string currentId, string appAccent) => new TemplatesWindow(owner, currentId, appAccent);
}
