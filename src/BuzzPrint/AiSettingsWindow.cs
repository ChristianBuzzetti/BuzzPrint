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
/// IA → Configurar IA…. Top: AI on/off and automatic summaries. Left: saved connections (the one in use carries an
/// accent bar and a check). Right: the selected connection: provider tiles, model (recommended list or the account's
/// own list), API key (encrypted, tied to its provider), optional name, test, use, delete. Changes apply on Save.
/// </summary>
public sealed class AiSettingsWindow : Window
{
    readonly List<AiConnection> working;
    readonly Dictionary<string, string> typedKeys = [];   // connection id → key typed for its current provider
    readonly HashSet<string> deleted = [];
    string activeId;
    AiConnection? current;
    bool loading;

    readonly CheckBox enabled = new() { Margin = new Thickness(0), VerticalAlignment = VerticalAlignment.Center };
    readonly CheckBox autoCaption = new() { Margin = new Thickness(0), VerticalAlignment = VerticalAlignment.Center };
    readonly StackPanel list = new();
    readonly System.Windows.Controls.Primitives.UniformGrid providerTiles = new() { Columns = 3, Margin = new Thickness(-4, 6, -4, 0) };
    readonly TextBlock providerHint = Muted("");
    readonly ComboBox model = new() { IsEditable = true, MinHeight = 38 };
    readonly Button loadModels = new() { MinWidth = 120, Margin = new Thickness(8, 0, 0, 0) };
    readonly TextBlock modelNote = Muted("");
    readonly Button showAll = new() { Style = null, Padding = new Thickness(0), Margin = new Thickness(0, 4, 0, 0), HorizontalAlignment = HorizontalAlignment.Left, Visibility = Visibility.Collapsed, Cursor = Cursors.Hand };
    readonly Dictionary<string, List<string>> listed = [];   // connection id → every chat model its account offers (this window only)
    readonly TextBox url = new() { MinHeight = 38 };
    readonly TextBox name = new() { MinHeight = 38 };
    readonly PasswordBox key = new() { MinHeight = 38 };
    readonly TextBlock keyState = Muted("");
    readonly Button keyLink = new();
    readonly TextBlock status = new() { TextWrapping = TextWrapping.Wrap, VerticalAlignment = VerticalAlignment.Center };
    readonly StackPanel urlBlock = new(), keyBlock = new(), editor = new();
    readonly TextBlock headTitle = new() { FontWeight = FontWeights.SemiBold }, headSub = Muted("");
    readonly Border headBadge = new() { Width = 40, Height = 40, Margin = new Thickness(0, 0, 12, 0) };
    readonly TextBlock headLetter = new() { FontWeight = FontWeights.Bold, FontSize = 17, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
    readonly Button useButton = new() { MinWidth = 150 };
    readonly Button test = new() { MinWidth = 140 };
    readonly Button remove = new() { MinWidth = 100 };
    readonly StackPanel emptyEditor = new() { VerticalAlignment = VerticalAlignment.Center, HorizontalAlignment = HorizontalAlignment.Center };
    readonly Button addButton = new() { HorizontalContentAlignment = HorizontalAlignment.Left, Margin = new Thickness(0, 6, 0, 0), MinHeight = 44 };

    AiSettingsWindow()
    {
        SetResourceReference(StyleProperty, typeof(Window));
        TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        Title = L.T("Configurar IA") + " · " + AppInfo.Name;
        Width = 980; Height = 720; MinWidth = 860; MinHeight = 600;
        WindowStartupLocation = WindowStartupLocation.CenterOwner; ShowInTaskbar = false;

        var st = Store.Settings;
        AiConnections.Active(st);   // migrates the first preview's single-provider settings
        working = st.AiConnections.Select(Clone).ToList();
        activeId = st.AiActiveId.Length > 0 ? st.AiActiveId : working.FirstOrDefault()?.Id ?? "";

        var root = new DockPanel { Margin = new Thickness(28, 24, 28, 20) };

        // ---- header and switches
        var head = new StackPanel();
        DockPanel.SetDock(head, Dock.Top);
        var title = new TextBlock { Text = L.T("Inteligencia artificial"), FontWeight = FontWeights.SemiBold };
        title.SetResourceReference(TextBlock.FontSizeProperty, "TitleFontSize");
        var titleBlock = new StackPanel();
        titleBlock.Children.Add(title);
        head.Children.Add(BobertoMark.WithBadge(titleBlock, 48));
        var intro = Muted(L.T("Conectá uno o más proveedores y elegí cuál usar. Nada se envía solo: cada función te muestra antes qué capturas van y a qué servicio. Con Ollama, nada sale de tu PC."));
        intro.Margin = new Thickness(0, 6, 0, 0);
        titleBlock.Children.Add(intro);
        titleBlock.Margin = new Thickness(0, 0, 0, 16);
        var switches = new StackPanel();
        switches.Children.Add(SwitchRow(enabled, L.T("Usar IA en BuzzPrint"), L.T("Muestra las funciones de IA en la barra lateral, los menús y el editor."), false));
        switches.Children.Add(SwitchRow(autoCaption, L.T("Título y resumen automáticos en cada captura nueva"), L.T("Cada captura nueva se envía a la conexión en uso. Apagado, lo pedís a mano desde el clic derecho → IA."), true));
        head.Children.Add(Card(switches, new Thickness(18, 6, 18, 6)));
        root.Children.Add(head);

        // ---- footer
        var foot = new DockPanel { Margin = new Thickness(0, 16, 0, 0), LastChildFill = false };
        DockPanel.SetDock(foot, Dock.Bottom);
        var save = Primary(L.T("Guardar"));
        save.IsDefault = true; save.MinWidth = 120;
        var cancel = new Button { Content = L.T("Cancelar"), IsCancel = true, MinWidth = 110, Margin = new Thickness(0, 0, 8, 0) };
        DockPanel.SetDock(save, Dock.Right); DockPanel.SetDock(cancel, Dock.Right);
        foot.Children.Add(save); foot.Children.Add(cancel);
        root.Children.Add(foot);

        // ---- body
        var body = new Grid { Margin = new Thickness(0, 18, 0, 0) };
        body.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(300) });
        body.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        var left = new DockPanel { Margin = new Thickness(0, 0, 18, 0) };
        var label = Label(L.T("CONEXIONES"));
        label.Margin = new Thickness(2, 0, 0, 10);
        DockPanel.SetDock(label, Dock.Top);
        left.Children.Add(label);
        addButton.Content = Row("", L.T("Agregar conexión"));
        DockPanel.SetDock(addButton, Dock.Bottom);
        left.Children.Add(addButton);
        left.Children.Add(new ScrollViewer { Content = list, VerticalScrollBarVisibility = ScrollBarVisibility.Auto });
        body.Children.Add(left);

        BuildEditor();
        var emptyIcon = new TextBlock { Text = "", FontFamily = new FontFamily("Segoe Fluent Icons, Segoe MDL2 Assets"), FontSize = 30, HorizontalAlignment = HorizontalAlignment.Center };
        emptyIcon.SetResourceReference(TextBlock.ForegroundProperty, "Accent");
        emptyEditor.Children.Add(emptyIcon);
        var emptyText = Muted(L.T("Agregá una conexión para empezar: elegís el proveedor, el modelo y pegás la API key."));
        emptyText.TextAlignment = TextAlignment.Center; emptyText.MaxWidth = 320; emptyText.Margin = new Thickness(0, 10, 0, 0);
        emptyEditor.Children.Add(emptyText);
        var rightGrid = new Grid();
        rightGrid.Children.Add(new ScrollViewer { Content = editor, VerticalScrollBarVisibility = ScrollBarVisibility.Auto, Padding = new Thickness(0, 0, 6, 0) });
        rightGrid.Children.Add(emptyEditor);
        var right = Card(rightGrid, new Thickness(24, 20, 20, 20));
        Grid.SetColumn(right, 1);
        body.Children.Add(right);
        root.Children.Add(body);
        Content = root;

        enabled.IsChecked = st.AiEnabled;
        autoCaption.IsChecked = st.AiAutoCaption;
        enabled.Click += (_, _) => SyncEnabled();
        addButton.Click += (_, _) => { var c = new AiConnection { Provider = "anthropic" }; working.Add(c); if (activeId.Length == 0) activeId = c.Id; Select(c); };
        save.Click += (_, _) => { if (Commit()) Close(); };
        PreviewKeyDown += (_, e) => { if (e.Key == Key.Escape) Close(); };
        Select(working.FirstOrDefault(c => c.Id == activeId) ?? working.FirstOrDefault());
        SyncEnabled();
    }

    void BuildEditor()
    {
        // Header: provider badge, connection name, use / in-use.
        var header = new DockPanel { Margin = new Thickness(0, 0, 0, 18) };
        headBadge.SetResourceReference(Border.CornerRadiusProperty, "ControlRadius");
        headBadge.SetResourceReference(Border.BackgroundProperty, "AccentWeak");
        headLetter.SetResourceReference(TextBlock.ForegroundProperty, "Accent");
        headBadge.Child = headLetter;
        DockPanel.SetDock(headBadge, Dock.Left);
        header.Children.Add(headBadge);
        DockPanel.SetDock(useButton, Dock.Right);
        useButton.VerticalAlignment = VerticalAlignment.Center;
        header.Children.Add(useButton);
        var names = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
        headTitle.SetResourceReference(TextBlock.FontSizeProperty, "SectionFontSize");
        names.Children.Add(headTitle);
        headSub.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont");
        headSub.Margin = new Thickness(0, 2, 0, 0);
        names.Children.Add(headSub);
        header.Children.Add(names);
        editor.Children.Add(header);

        editor.Children.Add(Label(L.T("PROVEEDOR")));
        editor.Children.Add(providerTiles);
        providerHint.Margin = new Thickness(0, 8, 0, 0);
        editor.Children.Add(providerHint);

        editor.Children.Add(Label(L.T("MODELO"), 20));
        var modelRow = new DockPanel { Margin = new Thickness(0, 6, 0, 0) };
        loadModels.Content = Row("", L.T("Ver modelos"));
        DockPanel.SetDock(loadModels, Dock.Right);
        modelRow.Children.Add(loadModels);
        modelRow.Children.Add(model);
        editor.Children.Add(modelRow);
        modelNote.Margin = new Thickness(0, 6, 0, 0);
        editor.Children.Add(modelNote);
        showAll.SetResourceReference(StyleProperty, "QuietButton");
        editor.Children.Add(showAll);

        urlBlock.Children.Add(Label(L.T("URL DEL SERVICIO (TERMINA EN /v1)"), 20));
        url.Margin = new Thickness(0, 6, 0, 0);
        urlBlock.Children.Add(url);
        editor.Children.Add(urlBlock);

        var keyHead = new DockPanel { Margin = new Thickness(0, 20, 0, 0) };
        keyLink.Style = (Style)FindResource("QuietButton");
        keyLink.Content = Row("", L.T("Conseguir una API key"));
        keyLink.Margin = new Thickness(0);
        DockPanel.SetDock(keyLink, Dock.Right);
        keyHead.Children.Add(keyLink);
        var keyLabel = Label(L.T("API KEY"));
        keyLabel.VerticalAlignment = VerticalAlignment.Center;
        keyHead.Children.Add(keyLabel);
        keyBlock.Children.Add(keyHead);
        key.Margin = new Thickness(0, 6, 0, 0);
        keyBlock.Children.Add(key);
        keyState.Margin = new Thickness(0, 6, 0, 0);
        keyBlock.Children.Add(keyState);
        var plans = Muted(L.T("ChatGPT Plus, Gemini Advanced o Claude Pro no incluyen la API: la key se saca aparte en la consola del proveedor y se paga por uso (algunas tienen un nivel gratuito)."));
        plans.Margin = new Thickness(0, 4, 0, 0);
        keyBlock.Children.Add(plans);
        editor.Children.Add(keyBlock);

        editor.Children.Add(Label(L.T("NOMBRE (OPCIONAL)"), 20));
        name.Margin = new Thickness(0, 6, 0, 0);
        editor.Children.Add(name);

        // Actions: test + status on the left, delete on the right.
        var actions = new DockPanel { Margin = new Thickness(0, 22, 0, 0) };
        test.Content = Row("", L.T("Probar conexión"));
        DockPanel.SetDock(test, Dock.Left);
        actions.Children.Add(test);
        remove.Content = Row("", L.T("Eliminar"));
        remove.SetResourceReference(ForegroundProperty, "ErrorInk");
        DockPanel.SetDock(remove, Dock.Right);
        actions.Children.Add(remove);
        status.Margin = new Thickness(14, 0, 12, 0);
        actions.Children.Add(status);
        editor.Children.Add(actions);

        foreach (var p in AiProviders.All)
        {
            var provider = p;
            var tile = new RadioButton { GroupName = "provider", Margin = new Thickness(4), Cursor = Cursors.Hand, Tag = p.Id, Style = (Style)FindResource("NavItem") };
            var content = new DockPanel { Margin = new Thickness(0, 7, 0, 7) };
            var letter = new Border { Width = 28, Height = 28, Margin = new Thickness(0, 0, 10, 0) };
            letter.SetResourceReference(Border.CornerRadiusProperty, "ControlRadius");
            letter.SetResourceReference(Border.BackgroundProperty, "AlternateRow");
            letter.Child = new TextBlock { Text = p.Letter, FontWeight = FontWeights.Bold, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
            DockPanel.SetDock(letter, Dock.Left);
            content.Children.Add(letter);
            var texts = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
            texts.Children.Add(new TextBlock { Text = L.T(p.Name), FontWeight = FontWeights.SemiBold });
            var tag = Muted(L.T(p.Tag)); tag.TextWrapping = TextWrapping.NoWrap; tag.TextTrimming = TextTrimming.CharacterEllipsis;
            tag.FontWeight = FontWeights.Normal;
            texts.Children.Add(tag);
            content.Children.Add(texts);
            tile.Content = content;
            tile.Checked += (_, _) =>
            {
                if (loading || current == null || current.Provider == provider.Id) return;
                current.Provider = provider.Id; current.Model = "";
                typedKeys.Remove(current.Id);   // a key belongs to the provider it was typed for
                ShowProvider(); RenderList();
            };
            providerTiles.Children.Add(tile);
        }

        model.AddHandler(System.Windows.Controls.Primitives.TextBoxBase.TextChangedEvent, new TextChangedEventHandler((_, _) =>
        {
            if (loading || current == null) return;
            current.Model = (model.Text ?? "").Trim(); ShowModelNote(); ShowHeader(); RenderList();
        }));
        url.TextChanged += (_, _) => { if (!loading && current != null) current.BaseUrl = url.Text.Trim(); };
        name.TextChanged += (_, _) => { if (!loading && current != null) { current.Name = name.Text; ShowHeader(); RenderList(); } };
        key.PasswordChanged += (_, _) => { if (!loading && current != null) { if (key.Password.Length > 0) typedKeys[current.Id] = key.Password; else typedKeys.Remove(current.Id); ShowKeyState(); } };
        keyLink.Click += (_, _) => { if (current == null) return; var p = AiProviders.Get(current.Provider); if (p.KeyHelpUrl.Length > 0) try { Process.Start(new ProcessStartInfo(p.KeyHelpUrl) { UseShellExecute = true }); } catch (System.ComponentModel.Win32Exception) { } };
        loadModels.Click += async (_, _) => await LoadModelsAsync();
        // An empty list would open as a blank sliver under the field: keep it closed until there is something to pick.
        model.DropDownOpened += (_, _) => { if (model.Items.Count == 0) Dispatcher.BeginInvoke(() => model.IsDropDownOpen = false); };
        test.Click += async (_, _) => await TestAsync();
        useButton.Click += (_, _) => { if (current != null) { activeId = current.Id; RenderList(); ShowHeader(); } };
        remove.Click += (_, _) => { if (current != null) Delete(current); };
        showAll.Click += (_, _) =>
        {
            if (current == null || !listed.TryGetValue(current.Id, out var all)) return;
            Offer(all, L.F("Todos los modelos de chat de tu cuenta ({0}).", all.Count), "Muted", open: true);
            showAll.Visibility = Visibility.Collapsed;
        };
    }

    void Delete(AiConnection c)
    {
        deleted.Add(c.Id); typedKeys.Remove(c.Id); listed.Remove(c.Id); working.Remove(c);
        if (activeId == c.Id) activeId = working.FirstOrDefault()?.Id ?? "";
        Select(c == current || current == null ? working.FirstOrDefault() : current);
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
            foreach (RadioButton tile in providerTiles.Children) tile.IsChecked = (string)tile.Tag == p.Id;
            providerHint.Text = L.T(p.Hint);
            model.ItemsSource = p.Models.Select(m => m.Id).ToList();
            if (current.Model.Length == 0) current.Model = p.SuggestedModel;
            model.Text = current.Model;
            urlBlock.Visibility = p.Id == "custom" ? Visibility.Visible : Visibility.Collapsed;
            keyBlock.Visibility = p.NeedsKey ? Visibility.Visible : Visibility.Collapsed;
            keyLink.Visibility = p.KeyHelpUrl.Length > 0 ? Visibility.Visible : Visibility.Collapsed;
            key.Password = typedKeys.TryGetValue(current.Id, out var typed) ? typed : "";
            status.Text = "";
        }
        finally { loading = false; }
        showAll.Visibility = Visibility.Collapsed;
        ShowKeyState(); ShowModelNote(); ShowHeader(); SyncEnabled();
        // Fresh models arrive by themselves: with a key on hand the list is fetched (and curated) quietly.
        if (enabled.IsChecked == true && (!p.NeedsKey || typedKeys.ContainsKey(current.Id) || AiKeys.Has(current.Id, current.Provider)))
            _ = LoadModelsAsync(quiet: true);
    }

    void ShowKeyState()
    {
        if (current == null) return;
        string? stored = AiKeys.Load(current.Id, current.Provider);
        keyState.Text = typedKeys.ContainsKey(current.Id) ? L.T("Se guarda al tocar Guardar.")
            : stored != null ? L.F("Guardada: {0}. Escribí otra para reemplazarla.", AiKeys.Masked(stored)) : L.T("Todavía no hay una key guardada para este proveedor.");
    }

    void ShowHeader()
    {
        if (current == null) return;
        var p = AiProviders.Get(current.Provider);
        headLetter.Text = p.Letter;
        headTitle.Text = current.Name.Trim().Length > 0 ? current.Name.Trim() : L.T(p.Name);
        headSub.Text = AiProviders.ModelFor(current) is { Length: > 0 } m ? m : L.T("sin modelo");
        bool inUse = current.Id == activeId;
        useButton.Content = Row(inUse ? "" : "", inUse ? L.T("En uso") : L.T("Usar esta conexión"));
        useButton.IsEnabled = !inUse && enabled.IsChecked == true;
    }

    void ShowModelNote()
    {
        if (current == null) return;
        var p = AiProviders.Get(current.Provider);
        var known = p.Models.FirstOrDefault(m => m.Id == current.Model);
        modelNote.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        modelNote.Text = known != null ? L.T(known.Note)
            : p.Models.Count > 0 ? L.T("Modelo elegido a mano. Los recomendados están en la lista.")
            : L.T("Escribí el nombre del modelo o tocá «Ver modelos» para elegir de tu cuenta.");
    }

    void RenderList()
    {
        list.Children.Clear();
        if (working.Count == 0) { var none = Muted(L.T("Sin conexiones todavía.")); none.Margin = new Thickness(4, 4, 0, 0); list.Children.Add(none); return; }
        foreach (var c in working)
        {
            var conn = c;
            var p = AiProviders.Get(c.Provider);
            bool inUse = c.Id == activeId, isCurrent = c == current;
            var row = new Border { Padding = new Thickness(12, 10, 12, 10), Margin = new Thickness(0, 0, 0, 8), BorderThickness = new Thickness(1), Cursor = Cursors.Hand };
            row.SetResourceReference(Border.CornerRadiusProperty, "StripRadius");
            row.SetResourceReference(Border.BackgroundProperty, isCurrent ? "AccentWeak" : "SurfaceRaised");
            row.SetResourceReference(Border.BorderBrushProperty, isCurrent ? "AccentCardRing" : "FaintBorder");
            var grid = new DockPanel();
            var letter = new Border { Width = 32, Height = 32, Margin = new Thickness(0, 0, 10, 0) };
            letter.SetResourceReference(Border.CornerRadiusProperty, "ControlRadius");
            letter.SetResourceReference(Border.BackgroundProperty, inUse ? "Accent" : "AlternateRow");
            var lt = new TextBlock { Text = p.Letter, FontWeight = FontWeights.Bold, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
            lt.SetResourceReference(TextBlock.ForegroundProperty, inUse ? "OnAccent" : "Ink");
            letter.Child = lt;
            DockPanel.SetDock(letter, Dock.Left);
            grid.Children.Add(letter);
            var trash = new Button { Content = new TextBlock { Text = "", FontFamily = new FontFamily("Segoe Fluent Icons, Segoe MDL2 Assets"), FontSize = 13 },
                ToolTip = L.T("Eliminar esta conexión"), Width = 30, Height = 30, Padding = new Thickness(0), Margin = new Thickness(8, 0, -4, 0), VerticalAlignment = VerticalAlignment.Center, IsEnabled = enabled.IsChecked == true };
            trash.SetResourceReference(StyleProperty, "QuietButton");
            System.Windows.Automation.AutomationProperties.SetName(trash, L.T("Eliminar esta conexión"));
            trash.Click += (_, e) => { e.Handled = true; Delete(conn); };
            trash.MouseLeftButtonUp += (_, e) => e.Handled = true;
            DockPanel.SetDock(trash, Dock.Right);
            grid.Children.Add(trash);
            var texts = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
            var top = new DockPanel();
            if (inUse)
            {
                var badge = Row("", L.T("En uso"));
                badge.VerticalAlignment = VerticalAlignment.Center;
                badge.SetResourceReference(TextElement.ForegroundProperty, "Accent");
                foreach (TextBlock t in badge.Children) { t.FontSize = 11; t.FontWeight = FontWeights.SemiBold; }
                ((TextBlock)badge.Children[0]).Margin = new Thickness(0, 0, 5, 0);
                DockPanel.SetDock(badge, Dock.Right);
                top.Children.Add(badge);
            }
            top.Children.Add(new TextBlock { Text = c.Name.Trim().Length > 0 ? c.Name.Trim() : L.T(p.Name), FontWeight = FontWeights.SemiBold, TextTrimming = TextTrimming.CharacterEllipsis });
            texts.Children.Add(top);
            var sub = Muted(AiProviders.ModelFor(c) is { Length: > 0 } m ? m : L.T("sin modelo"));
            sub.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont");
            sub.TextWrapping = TextWrapping.NoWrap; sub.TextTrimming = TextTrimming.CharacterEllipsis;
            sub.Margin = new Thickness(0, 2, 0, 0);
            texts.Children.Add(sub);
            grid.Children.Add(texts);
            row.Child = grid;
            row.MouseLeftButtonUp += (_, _) => Select(conn);
            list.Children.Add(row);
        }
    }

    void SyncEnabled()
    {
        bool on = enabled.IsChecked == true;
        autoCaption.IsEnabled = on;
        foreach (var c in new UIElement[] { providerTiles, model, loadModels, url, key, name, test, remove, keyLink }) c.IsEnabled = on;
        ShowHeader();
    }

    async Task LoadModelsAsync(bool quiet = false)
    {
        if (current == null) return;
        var conn = current;
        var provider = AiProviders.Get(conn.Provider);
        if (provider.NeedsKey && !typedKeys.ContainsKey(conn.Id) && !AiKeys.Has(conn.Id, conn.Provider))
        {
            // No key yet: say so under the field and take the person straight to the key box.
            if (!quiet) { Note(L.F("Primero pegá tu API key de {0} abajo y después tocá «Ver modelos».", L.T(provider.Name)), "ErrorInk"); key.BringIntoView(); key.Focus(); }
            return;
        }
        loadModels.IsEnabled = false;
        if (!quiet) Note(L.T("Buscando los modelos de tu cuenta…"), "Muted");
        try
        {
            var all = await AiClient.ListModelsAsync(conn, typedKeys.GetValueOrDefault(conn.Id));
            if (conn != current) return;
            all = all.Where(ModelCuration.IsChat).ToList();
            listed[conn.Id] = all;
            var curated = ModelCuration.Curate(conn.Provider, all);
            if (curated.Count == 0) { Note(L.T("La cuenta no devolvió modelos de chat. Escribí el nombre a mano."), "Muted"); return; }
            Offer(curated, L.F("{0} modelos recientes de tu cuenta. Elegí uno de la lista.", curated.Count), "Muted", open: !quiet);
            showAll.Content = L.F("Mostrar todos ({0})", all.Count);
            showAll.Visibility = all.Count > curated.Count ? Visibility.Visible : Visibility.Collapsed;
        }
        catch (AiException ex) { if (conn == current && !quiet) Note(ex.Message, "ErrorInk"); }
        finally { loadModels.IsEnabled = enabled.IsChecked == true; }
    }

    /// <summary>Puts these models in the combo (the provider's recommended ones that the account has come first).</summary>
    void Offer(List<string> ids, string note, string brush, bool open)
    {
        if (current == null) return;
        var p = AiProviders.Get(current.Provider);
        var choices = p.Models.Select(m => m.Id).Where(ids.Contains).ToList();
        foreach (var id in ids) if (!choices.Contains(id)) choices.Add(id);
        loading = true;
        try { string keep = current.Model; model.ItemsSource = choices; model.Text = keep; }
        finally { loading = false; }
        Note(note, brush);
        if (open && choices.Count > 0) { model.Focus(); model.IsDropDownOpen = true; }
    }

    /// <summary>Status of the model field, right under it (the bottom status line is out of sight while choosing).</summary>
    void Note(string text, string brush) { modelNote.Text = text; modelNote.SetResourceReference(TextBlock.ForegroundProperty, brush); }

    async Task TestAsync()
    {
        if (current == null) return;
        test.IsEnabled = false;
        SetStatus(L.F("Probando {0}…", current.DisplayName()), "Muted");
        try
        {
            var client = AiClient.FromConnection(current, typedKeys.GetValueOrDefault(current.Id));
            var answer = await client.CompleteAsync(new AiRequest("You are a connection test. Reply with exactly: OK", "Reply with exactly: OK", [], 50));
            SetStatus("✓ " + L.T("Conexión correcta. El modelo respondió: ") + (answer.Length > 40 ? answer[..40] + "…" : answer), "Ok");
        }
        catch (AiException ex) { SetStatus(ex.Message, "ErrorInk"); }
        finally { test.IsEnabled = enabled.IsChecked == true; }
    }

    void SetStatus(string text, string brush) { status.Text = text; status.SetResourceReference(TextBlock.ForegroundProperty, brush); }

    bool Commit()
    {
        var st = Store.Settings;
        bool on = enabled.IsChecked == true;
        if (on && working.Count == 0) { SetStatus(L.T("Agregá al menos una conexión, o apagá la IA."), "ErrorInk"); return false; }
        foreach (var c in working)
        {
            var p = AiProviders.Get(c.Provider);
            if (AiProviders.ModelFor(c).Length == 0) { Select(c); SetStatus(L.T("Falta elegir el modelo de esta conexión."), "ErrorInk"); return false; }
            if (p.NeedsKey && !typedKeys.ContainsKey(c.Id) && !AiKeys.Has(c.Id, c.Provider)) { Select(c); SetStatus(L.T("Falta la API key de esta conexión."), "ErrorInk"); return false; }
        }
        foreach (var id in deleted) AiKeys.Delete(id);
        foreach (var c in working) if (typedKeys.TryGetValue(c.Id, out var k)) AiKeys.Save(c.Id, c.Provider, k);
        st.AiConnections = working.Select(Clone).ToList();
        st.AiActiveId = working.Any(c => c.Id == activeId) ? activeId : working.FirstOrDefault()?.Id ?? "";
        st.AiEnabled = on;
        st.AiAutoCaption = autoCaption.IsChecked == true;
        Store.SaveSettings();
        return true;
    }

    // ---- building blocks

    static AiConnection Clone(AiConnection c) => new() { Id = c.Id, Name = c.Name, Provider = c.Provider, Model = c.Model, BaseUrl = c.BaseUrl };

    static UIElement SwitchRow(CheckBox box, string title, string description, bool last)
    {
        var row = new DockPanel { Margin = new Thickness(0, 12, 0, 12) };
        DockPanel.SetDock(box, Dock.Right);
        row.Children.Add(box);
        var texts = new StackPanel { Margin = new Thickness(0, 0, 20, 0) };
        texts.Children.Add(new TextBlock { Text = title, FontWeight = FontWeights.SemiBold });
        var d = Muted(description); d.Margin = new Thickness(0, 2, 0, 0);
        texts.Children.Add(d);
        texts.Background = Brushes.Transparent; texts.Cursor = Cursors.Hand;
        texts.MouseLeftButtonUp += (_, _) => { if (box.IsEnabled) { box.IsChecked = box.IsChecked != true; box.RaiseEvent(new RoutedEventArgs(System.Windows.Controls.Primitives.ButtonBase.ClickEvent)); } };
        row.Children.Add(texts);
        var frame = new Border { Child = row, BorderThickness = new Thickness(0, 0, 0, last ? 0 : 1) };
        frame.SetResourceReference(Border.BorderBrushProperty, "FaintBorder");
        return frame;
    }

    static StackPanel Row(string glyph, string text)
    {
        var sp = new StackPanel { Orientation = Orientation.Horizontal };
        sp.Children.Add(new TextBlock { Text = glyph, FontFamily = new FontFamily("Segoe Fluent Icons, Segoe MDL2 Assets"), FontSize = 12, Margin = new Thickness(0, 0, 8, 0), VerticalAlignment = VerticalAlignment.Center });
        sp.Children.Add(new TextBlock { Text = text, VerticalAlignment = VerticalAlignment.Center });
        return sp;
    }

    static Button Primary(string text)
    {
        var b = new Button { Content = text, FontWeight = FontWeights.SemiBold };
        b.SetResourceReference(BackgroundProperty, "Accent"); b.SetResourceReference(ForegroundProperty, "OnAccent"); b.SetResourceReference(BorderBrushProperty, "Accent");
        return b;
    }

    static Border Card(UIElement child, Thickness padding)
    {
        var b = new Border { Child = child, Padding = padding, BorderThickness = new Thickness(1) };
        b.SetResourceReference(Border.BackgroundProperty, "Surface");
        b.SetResourceReference(Border.BorderBrushProperty, "FaintBorder");
        b.SetResourceReference(Border.CornerRadiusProperty, "PanelRadius");
        return b;
    }

    static TextBlock Label(string text, double top = 0)
    {
        var t = new TextBlock { Text = text, FontSize = 10.5, FontWeight = FontWeights.Bold, Margin = new Thickness(0, top, 0, 0) };
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

    /// <summary>Screenshot run: sample connections in memory only (nothing is saved).</summary>
    internal static AiSettingsWindow ForShot(Window owner)
    {
        var w = new AiSettingsWindow { Owner = owner };
        if (w.working.Count == 0)
        {
            var g = new AiConnection { Provider = "gemini", Model = "gemini-4-hydrogen" };
            w.working.Add(g); w.working.Add(new AiConnection { Provider = "anthropic", Model = "claude-opus-5-5" });
            w.activeId = g.Id; w.enabled.IsChecked = true; w.Select(g); w.SyncEnabled();
        }
        return w;
    }

    public static void ShowFor(Window? owner)
    {
        var w = new AiSettingsWindow();
        if (owner != null && owner.IsVisible) w.Owner = owner; else w.WindowStartupLocation = WindowStartupLocation.CenterScreen;
        w.ShowDialog();
    }
}
#endif
