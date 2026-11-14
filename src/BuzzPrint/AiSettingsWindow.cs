#if PREVIEW
using System.Diagnostics;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Documents;
using System.Windows.Input;
using System.Windows.Media;
using BuzzPrint.Ai;

namespace BuzzPrint;

/// <summary>
/// IA → Configurar IA…: AI on/off and automatic captions on top; below, the saved connections on the left (one is
/// active, the one every AI feature uses) and the selected connection's editor on the right: provider, model (with
/// the recommended ones and the list the account really offers), API key (stored encrypted) and a connection test.
/// Edits are kept in a working copy until Save.
/// </summary>
public sealed class AiSettingsWindow : Window
{
    readonly List<AiConnection> working;
    readonly Dictionary<string, string> typedKeys = [];   // keys typed in this session, saved on Save
    readonly HashSet<string> deleted = [];
    string activeId;
    AiConnection? current;
    bool loading;

    readonly CheckBox enabled = new() { Content = L.T("Usar IA en BuzzPrint"), Margin = new Thickness(0) };
    readonly CheckBox autoCaption = new() { Content = L.T("Título y resumen automáticos en cada captura nueva"), Margin = new Thickness(0, 10, 0, 0) };
    readonly StackPanel list = new();
    readonly ComboBox provider = new() { Margin = new Thickness(0, 6, 0, 0) };
    readonly TextBlock hint = Muted("");
    readonly ComboBox model = new() { IsEditable = true, Margin = new Thickness(0, 6, 0, 0) };
    readonly Button loadModels = new() { Content = L.T("Ver modelos"), Margin = new Thickness(8, 6, 0, 0), Padding = new Thickness(12, 0, 12, 0) };
    readonly TextBlock modelNote = Muted("");
    readonly TextBox url = new() { Margin = new Thickness(0, 6, 0, 0) };
    readonly TextBox name = new() { Margin = new Thickness(0, 6, 0, 0) };
    readonly PasswordBox key = new() { Margin = new Thickness(0, 6, 0, 0) };
    readonly TextBlock keyState = Muted("");
    readonly TextBlock status = new() { TextWrapping = TextWrapping.Wrap, Margin = new Thickness(0, 12, 0, 0) };
    readonly StackPanel urlRow = new(), keyRow = new(), editor = new();
    readonly Hyperlink keyLink = new(new Run(L.T("Conseguir una API key")));
    readonly Button test = new() { Content = L.T("Probar conexión"), MinWidth = 130 };
    readonly Button makeActive = new() { Content = L.T("Usar esta conexión"), MinWidth = 140 };
    readonly Button remove = new() { Content = L.T("Eliminar"), MinWidth = 90 };
    readonly TextBlock emptyEditor = Muted(L.T("Agregá una conexión para empezar: elegís el proveedor, el modelo y pegás la API key."));

    AiSettingsWindow()
    {
        SetResourceReference(StyleProperty, typeof(Window));
        TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        Title = L.T("Configurar IA") + " · " + AppInfo.Name;
        Width = 880; Height = 660; MinWidth = 760; MinHeight = 560;
        WindowStartupLocation = WindowStartupLocation.CenterOwner; ShowInTaskbar = false;

        var st = Store.Settings;
        AiConnections.Active(st);   // migrates the single-provider settings of the first preview
        working = st.AiConnections.Select(Clone).ToList();
        activeId = st.AiActiveId.Length > 0 ? st.AiActiveId : working.FirstOrDefault()?.Id ?? "";

        var root = new DockPanel { Margin = new Thickness(24, 20, 24, 18) };

        var head = new StackPanel();
        DockPanel.SetDock(head, Dock.Top);
        var title = new TextBlock { Text = L.T("Inteligencia artificial"), FontWeight = FontWeights.SemiBold };
        title.SetResourceReference(TextBlock.FontSizeProperty, "TitleFontSize");
        head.Children.Add(title);
        var intro = Muted(L.T("Conectá uno o más proveedores y elegí cuál usar. Nada se envía solo: cada función te muestra antes qué capturas van y a qué servicio. Con Ollama, nada sale de tu PC."));
        intro.Margin = new Thickness(0, 4, 0, 14);
        head.Children.Add(intro);
        var switches = Card(new Thickness(16, 12, 16, 12));
        var sw = new StackPanel();
        sw.Children.Add(enabled);
        sw.Children.Add(autoCaption);
        var autoNote = Muted(L.T("Cada captura nueva se envía a la conexión activa para escribir su título y resumen. Apagado, lo pedís a mano desde el clic derecho → IA."));
        autoNote.Margin = new Thickness(46, 2, 0, 0);
        sw.Children.Add(autoNote);
        switches.Child = sw;
        head.Children.Add(switches);
        root.Children.Add(head);

        var foot = new DockPanel { Margin = new Thickness(0, 14, 0, 0), LastChildFill = false };
        DockPanel.SetDock(foot, Dock.Bottom);
        var save = new Button { Content = L.T("Guardar"), IsDefault = true, MinWidth = 110, FontWeight = FontWeights.SemiBold, Margin = new Thickness(8, 0, 0, 0) };
        save.SetResourceReference(BackgroundProperty, "Accent"); save.SetResourceReference(ForegroundProperty, "OnAccent"); save.SetResourceReference(BorderBrushProperty, "Accent");
        var cancel = new Button { Content = L.T("Cancelar"), IsCancel = true, MinWidth = 100 };
        DockPanel.SetDock(save, Dock.Right); DockPanel.SetDock(cancel, Dock.Right);
        foot.Children.Add(save); foot.Children.Add(cancel);
        root.Children.Add(foot);

        var body = new Grid { Margin = new Thickness(0, 14, 0, 0) };
        body.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(280) });
        body.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        var left = new DockPanel { Margin = new Thickness(0, 0, 14, 0) };
        var leftHead = new DockPanel { Margin = new Thickness(2, 0, 0, 8) };
        var add = new Button { Content = "+ " + L.T("Agregar"), Padding = new Thickness(12, 0, 12, 0), Margin = new Thickness(0) };
        DockPanel.SetDock(add, Dock.Right);
        leftHead.Children.Add(add);
        var connLabel = SectionLabel(L.T("CONEXIONES"));
        connLabel.VerticalAlignment = VerticalAlignment.Center;
        leftHead.Children.Add(connLabel);
        DockPanel.SetDock(leftHead, Dock.Top);
        left.Children.Add(leftHead);
        left.Children.Add(new ScrollViewer { Content = list, VerticalScrollBarVisibility = ScrollBarVisibility.Auto });
        body.Children.Add(left);

        var right = Card(new Thickness(18, 14, 18, 16));
        Grid.SetColumn(right, 1);
        var rightStack = new StackPanel();
        rightStack.Children.Add(emptyEditor);
        BuildEditor();
        rightStack.Children.Add(editor);
        right.Child = new ScrollViewer { Content = rightStack, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        body.Children.Add(right);
        root.Children.Add(body);
        Content = root;

        enabled.IsChecked = st.AiEnabled;
        autoCaption.IsChecked = st.AiAutoCaption;
        enabled.Click += (_, _) => SyncEnabled();
        add.Click += (_, _) => { var c = new AiConnection { Provider = "anthropic" }; working.Add(c); if (activeId.Length == 0) activeId = c.Id; Select(c); };
        save.Click += (_, _) => { if (Commit()) Close(); };
        PreviewKeyDown += (_, e) => { if (e.Key == Key.Escape) Close(); };
        Select(working.FirstOrDefault(c => c.Id == activeId) ?? working.FirstOrDefault());
        SyncEnabled();
    }

    void BuildEditor()
    {
        provider.ItemsSource = AiProviders.All;
        editor.Children.Add(SectionLabel(L.T("PROVEEDOR")));
        editor.Children.Add(provider);
        hint.Margin = new Thickness(0, 6, 0, 0);
        editor.Children.Add(hint);

        editor.Children.Add(SectionLabel(L.T("MODELO"), 16));
        var modelRow = new DockPanel();
        DockPanel.SetDock(loadModels, Dock.Right);
        modelRow.Children.Add(loadModels);
        modelRow.Children.Add(model);
        editor.Children.Add(modelRow);
        modelNote.Margin = new Thickness(0, 6, 0, 0);
        editor.Children.Add(modelNote);

        urlRow.Children.Add(SectionLabel(L.T("URL DEL SERVICIO (TERMINA EN /v1)"), 16));
        urlRow.Children.Add(url);
        editor.Children.Add(urlRow);

        keyRow.Children.Add(SectionLabel(L.T("API KEY"), 16));
        keyRow.Children.Add(key);
        var keyFoot = new DockPanel { Margin = new Thickness(0, 6, 0, 0) };
        var linkText = new TextBlock(keyLink) { HorizontalAlignment = HorizontalAlignment.Right };
        DockPanel.SetDock(linkText, Dock.Right);
        keyFoot.Children.Add(linkText);
        keyFoot.Children.Add(keyState);
        keyRow.Children.Add(keyFoot);
        var plans = Muted(L.T("ChatGPT Plus, Gemini Advanced o Claude Pro no incluyen la API: la key se saca aparte en la consola del proveedor y se paga por uso (algunas tienen un nivel gratuito)."));
        plans.Margin = new Thickness(0, 6, 0, 0);
        keyRow.Children.Add(plans);
        editor.Children.Add(keyRow);

        editor.Children.Add(SectionLabel(L.T("NOMBRE (OPCIONAL)"), 16));
        editor.Children.Add(name);

        editor.Children.Add(status);
        var actions = new WrapPanel { Margin = new Thickness(0, 14, 0, 0) };
        actions.Children.Add(test); actions.Children.Add(makeActive); actions.Children.Add(remove);
        editor.Children.Add(actions);

        provider.SelectionChanged += (_, _) =>
        {
            if (loading || current == null || provider.SelectedItem is not AiProvider p) return;
            current.Provider = p.Id; current.Model = ""; ShowProvider(); RenderList();
        };
        model.AddHandler(System.Windows.Controls.Primitives.TextBoxBase.TextChangedEvent, new TextChangedEventHandler((_, _) =>
        {
            if (loading || current == null) return;
            current.Model = (model.Text ?? "").Trim(); ShowModelNote(); RenderList();
        }));
        url.TextChanged += (_, _) => { if (!loading && current != null) current.BaseUrl = url.Text.Trim(); };
        name.TextChanged += (_, _) => { if (!loading && current != null) { current.Name = name.Text; RenderList(); } };
        key.PasswordChanged += (_, _) => { if (!loading && current != null) { if (key.Password.Length > 0) typedKeys[current.Id] = key.Password; else typedKeys.Remove(current.Id); } };
        keyLink.Click += (_, _) => { if (current == null) return; var p = AiProviders.Get(current.Provider); if (p.KeyHelpUrl.Length > 0) try { Process.Start(new ProcessStartInfo(p.KeyHelpUrl) { UseShellExecute = true }); } catch (System.ComponentModel.Win32Exception) { } };
        loadModels.Click += async (_, _) => await LoadModelsAsync();
        test.Click += async (_, _) => await TestAsync();
        makeActive.Click += (_, _) => { if (current != null) { activeId = current.Id; RenderList(); ShowActiveButton(); } };
        remove.Click += (_, _) =>
        {
            if (current == null) return;
            deleted.Add(current.Id); typedKeys.Remove(current.Id); working.Remove(current);
            if (activeId == current.Id) activeId = working.FirstOrDefault()?.Id ?? "";
            Select(working.FirstOrDefault());
        };
    }

    void Select(AiConnection? c)
    {
        current = c;
        editor.Visibility = c == null ? Visibility.Collapsed : Visibility.Visible;
        emptyEditor.Visibility = c == null ? Visibility.Visible : Visibility.Collapsed;
        RenderList();
        if (c == null) return;
        loading = true;
        try
        {
            provider.SelectedItem = AiProviders.Get(c.Provider);
            url.Text = c.BaseUrl;
            name.Text = c.Name;
            key.Password = typedKeys.TryGetValue(c.Id, out var typed) ? typed : "";
        }
        finally { loading = false; }
        ShowProvider();
    }

    void ShowProvider()
    {
        if (current == null) return;
        var p = AiProviders.Get(current.Provider);
        loading = true;
        try
        {
            hint.Text = L.T(p.Hint);
            model.ItemsSource = p.Models.Select(m => m.Id).ToList();
            if (current.Model.Length == 0) current.Model = p.SuggestedModel;
            model.Text = current.Model;
            urlRow.Visibility = p.Id == "custom" ? Visibility.Visible : Visibility.Collapsed;
            keyRow.Visibility = p.NeedsKey ? Visibility.Visible : Visibility.Collapsed;
            keyLink.IsEnabled = p.KeyHelpUrl.Length > 0;
            string? stored = AiKeys.Load(current.Id);
            keyState.Text = typedKeys.ContainsKey(current.Id) ? L.T("Se guarda al tocar Guardar.")
                : stored != null ? L.F("Guardada: {0}. Escribí otra para reemplazarla.", AiKeys.Masked(stored)) : L.T("Todavía no hay una key guardada.");
            status.Text = "";
        }
        finally { loading = false; }
        ShowModelNote();
        ShowActiveButton();
        SyncEnabled();
    }

    void ShowModelNote()
    {
        if (current == null) return;
        var p = AiProviders.Get(current.Provider);
        var known = p.Models.FirstOrDefault(m => m.Id == current.Model);
        modelNote.Text = known != null ? L.T(known.Note)
            : p.Models.Count > 0 ? L.T("Modelo elegido a mano. Los recomendados están en la lista.")
            : L.T("Escribí el nombre del modelo o tocá «Ver modelos» para elegir de tu cuenta.");
    }

    void ShowActiveButton()
    {
        bool isActive = current != null && current.Id == activeId;
        makeActive.IsEnabled = !isActive && enabled.IsChecked == true;
        makeActive.Content = isActive ? L.T("Conexión activa") : L.T("Usar esta conexión");
    }

    void RenderList()
    {
        list.Children.Clear();
        if (working.Count == 0) { var none = Muted(L.T("Sin conexiones todavía.")); none.Margin = new Thickness(4, 4, 0, 0); list.Children.Add(none); return; }
        foreach (var c in working)
        {
            var conn = c;
            bool isActive = c.Id == activeId, isCurrent = c == current;
            var row = new Border { Padding = new Thickness(12, 10, 12, 10), Margin = new Thickness(0, 0, 0, 6), BorderThickness = new Thickness(1), Cursor = Cursors.Hand };
            row.SetResourceReference(Border.CornerRadiusProperty, "StripRadius");
            row.SetResourceReference(Border.BackgroundProperty, isCurrent ? "AccentWeak" : "SurfaceRaised");
            row.SetResourceReference(Border.BorderBrushProperty, isCurrent ? "AccentCardRing" : "FaintBorder");
            var stack = new StackPanel();
            var top = new DockPanel();
            if (isActive)
            {
                var badge = new Border { Padding = new Thickness(7, 1, 7, 1), VerticalAlignment = VerticalAlignment.Center };
                badge.SetResourceReference(Border.BackgroundProperty, "Accent");
                badge.SetResourceReference(Border.CornerRadiusProperty, "PillRadius");
                var bt = new TextBlock { Text = L.T("ACTIVA"), FontSize = 9.5, FontWeight = FontWeights.Bold };
                bt.SetResourceReference(TextBlock.ForegroundProperty, "OnAccent");
                badge.Child = bt;
                DockPanel.SetDock(badge, Dock.Right);
                top.Children.Add(badge);
            }
            var nameText = new TextBlock { Text = c.Name.Trim().Length > 0 ? c.Name.Trim() : AiProviders.Get(c.Provider).Name, FontWeight = FontWeights.SemiBold, TextTrimming = TextTrimming.CharacterEllipsis };
            top.Children.Add(nameText);
            stack.Children.Add(top);
            var sub = Muted(AiProviders.ModelFor(c) is { Length: > 0 } m ? m : L.T("sin modelo"));
            sub.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont");
            sub.Margin = new Thickness(0, 3, 0, 0);
            stack.Children.Add(sub);
            row.Child = stack;
            row.MouseLeftButtonUp += (_, _) => Select(conn);
            list.Children.Add(row);
        }
    }

    void SyncEnabled()
    {
        bool on = enabled.IsChecked == true;
        autoCaption.IsEnabled = on;
        foreach (var c in new UIElement[] { provider, model, loadModels, url, key, name, test, remove }) c.IsEnabled = on;
        makeActive.IsEnabled = on && current != null && current.Id != activeId;
    }

    async Task LoadModelsAsync()
    {
        if (current == null) return;
        loadModels.IsEnabled = false;
        SetStatus(L.T("Buscando los modelos de tu cuenta…"), "Muted");
        try
        {
            var ids = await AiClient.ListModelsAsync(current, typedKeys.GetValueOrDefault(current.Id));
            var p = AiProviders.Get(current.Provider);
            var choices = p.Models.Select(m => m.Id).ToList();
            foreach (var id in ids) if (!choices.Contains(id)) choices.Add(id);
            loading = true;
            try { string keep = current.Model; model.ItemsSource = choices; model.Text = keep; }
            finally { loading = false; }
            model.IsDropDownOpen = true;
            SetStatus(L.F("{0} modelos disponibles en tu cuenta. Elegí uno de la lista.", ids.Count), "Muted");
        }
        catch (AiException ex) { SetStatus(ex.Message, "ErrorInk"); }
        finally { loadModels.IsEnabled = enabled.IsChecked == true; }
    }

    async Task TestAsync()
    {
        if (current == null) return;
        test.IsEnabled = false;
        SetStatus(L.F("Probando {0}…", current.DisplayName()), "Muted");
        try
        {
            var client = AiClient.FromConnection(current, typedKeys.GetValueOrDefault(current.Id));
            var answer = await client.CompleteAsync(new AiRequest("You are a connection test. Reply with exactly: OK", "Reply with exactly: OK", [], 50));
            SetStatus(L.T("Conexión correcta. El modelo respondió: ") + (answer.Length > 60 ? answer[..60] + "…" : answer), "Ok");
        }
        catch (AiException ex) { SetStatus(ex.Message, "ErrorInk"); }
        finally { test.IsEnabled = enabled.IsChecked == true; }
    }

    void SetStatus(string text, string brush) { status.Text = text; status.SetResourceReference(TextBlock.ForegroundProperty, brush); }

    /// <summary>Validates and writes everything back: connections, active one, switches, typed keys, removed keys.</summary>
    bool Commit()
    {
        var st = Store.Settings;
        bool on = enabled.IsChecked == true;
        if (on && working.Count == 0) { SetStatus(L.T("Agregá al menos una conexión, o apagá la IA."), "ErrorInk"); return false; }
        foreach (var c in working)
        {
            var p = AiProviders.Get(c.Provider);
            if (AiProviders.ModelFor(c).Length == 0) { Select(c); SetStatus(L.T("Falta elegir el modelo de esta conexión."), "ErrorInk"); return false; }
            if (p.NeedsKey && !typedKeys.ContainsKey(c.Id) && !AiKeys.Has(c.Id)) { Select(c); SetStatus(L.T("Falta la API key de esta conexión."), "ErrorInk"); return false; }
        }
        foreach (var id in deleted) AiKeys.Delete(id);
        foreach (var (id, k) in typedKeys) AiKeys.Save(id, k);
        st.AiConnections = working.Select(Clone).ToList();
        st.AiActiveId = working.Any(c => c.Id == activeId) ? activeId : working.FirstOrDefault()?.Id ?? "";
        st.AiEnabled = on;
        st.AiAutoCaption = autoCaption.IsChecked == true;
        Store.SaveSettings();
        return true;
    }

    static AiConnection Clone(AiConnection c) => new() { Id = c.Id, Name = c.Name, Provider = c.Provider, Model = c.Model, BaseUrl = c.BaseUrl };

    static Border Card(Thickness padding)
    {
        var b = new Border { Padding = padding, BorderThickness = new Thickness(1) };
        b.SetResourceReference(Border.BackgroundProperty, "Surface");
        b.SetResourceReference(Border.BorderBrushProperty, "FaintBorder");
        b.SetResourceReference(Border.CornerRadiusProperty, "PanelRadius");
        return b;
    }

    static TextBlock SectionLabel(string text, double top = 0)
    {
        var t = new TextBlock { Text = text, FontSize = 10, FontWeight = FontWeights.Bold, Margin = new Thickness(0, top, 0, 0) };
        t.SetResourceReference(TextBlock.ForegroundProperty, "LineNumber");
        return t;
    }

    static TextBlock Muted(string text)
    {
        var t = new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap };
        t.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        t.SetResourceReference(TextBlock.FontSizeProperty, "SmallFontSize");
        return t;
    }

    public static void ShowFor(Window? owner)
    {
        var w = new AiSettingsWindow();
        if (owner != null && owner.IsVisible) w.Owner = owner; else w.WindowStartupLocation = WindowStartupLocation.CenterScreen;
        w.ShowDialog();
    }
}
#endif
