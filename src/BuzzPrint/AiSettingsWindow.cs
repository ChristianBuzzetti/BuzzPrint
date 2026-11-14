#if PREVIEW
using System.Diagnostics;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Documents;
using System.Windows.Input;
using System.Windows.Media;
using BuzzPrint.Ai;

namespace BuzzPrint;

/// <summary>IA → Configurar IA…: on/off, provider, model, endpoint (custom) and API key (stored encrypted), with a
/// connection test that sends one tiny text request (no captures).</summary>
public sealed class AiSettingsWindow : Window
{
    readonly CheckBox enabled = new() { Content = L.T("Usar IA en BuzzPrint") };
    readonly CheckBox autoCaption = new() { Content = L.T("Pie automático en cada captura nueva"), Margin = new Thickness(0, 14, 0, 0) };
    readonly ComboBox provider = new() { DisplayMemberPath = nameof(AiProvider.Name), Margin = new Thickness(0, 4, 0, 0) };
    readonly TextBlock hint = Muted("");
    readonly TextBox model = new();
    readonly TextBox url = new();
    readonly PasswordBox key = new();
    readonly TextBlock keyState = Muted("");
    readonly TextBlock status = new() { TextWrapping = TextWrapping.Wrap, Margin = new Thickness(0, 12, 0, 0) };
    readonly StackPanel urlRow, keyRow;
    readonly Hyperlink keyLink = new(new Run(L.T("Conseguir una API key")));
    readonly Button test = new() { Content = L.T("Probar conexión"), MinWidth = 120 };

    AiSettingsWindow()
    {
        SetResourceReference(StyleProperty, typeof(Window));
        TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        Title = L.T("Configurar IA") + " · " + AppInfo.Name;
        Width = 560; SizeToContent = SizeToContent.Height; ResizeMode = ResizeMode.NoResize;
        WindowStartupLocation = WindowStartupLocation.CenterOwner; ShowInTaskbar = false;

        var root = new StackPanel { Margin = new Thickness(24, 20, 24, 18) };
        var head = new TextBlock { FontWeight = FontWeights.SemiBold };
        var mark = new Run("◤ "); mark.SetResourceReference(TextElement.ForegroundProperty, "Accent");
        head.Inlines.Add(mark); head.Inlines.Add(new Run(L.T("Configurar IA")));
        head.SetResourceReference(TextBlock.FontSizeProperty, "SectionFontSize");
        root.Children.Add(head);
        var intro = Muted(L.T("Nada se envía solo: cada función de IA es un botón y te muestra antes qué capturas se mandan y a qué servicio. Con Ollama, nada sale de tu PC."));
        intro.Margin = new Thickness(0, 6, 0, 14);
        root.Children.Add(intro);

        root.Children.Add(enabled);
        root.Children.Add(Label(L.T("Proveedor")));
        provider.ItemsSource = AiProviders.All;
        root.Children.Add(provider);
        hint.Margin = new Thickness(0, 4, 0, 0);
        root.Children.Add(hint);

        root.Children.Add(Label(L.T("Modelo")));
        root.Children.Add(model);

        urlRow = new StackPanel();
        urlRow.Children.Add(Label(L.T("URL del servicio (termina en /v1)")));
        urlRow.Children.Add(url);
        root.Children.Add(urlRow);

        keyRow = new StackPanel();
        keyRow.Children.Add(Label(L.T("API key")));
        keyRow.Children.Add(key);
        var keyFoot = new DockPanel { Margin = new Thickness(0, 4, 0, 0) };
        var linkText = new TextBlock(keyLink) { HorizontalAlignment = HorizontalAlignment.Right };
        DockPanel.SetDock(linkText, Dock.Right);
        keyFoot.Children.Add(linkText);
        keyFoot.Children.Add(keyState);
        keyRow.Children.Add(keyFoot);
        var plans = Muted(L.T("Ojo: ChatGPT Plus o Claude Pro no incluyen la API. La key se saca aparte en la consola del proveedor y se paga por uso."));
        plans.Margin = new Thickness(0, 6, 0, 0);
        keyRow.Children.Add(plans);
        root.Children.Add(keyRow);

        root.Children.Add(autoCaption);
        var autoNote = Muted(L.T("Cada captura nueva se envía al proveedor elegido para escribir su título y pie. Si lo dejás apagado, lo pedís a mano: clic derecho → Título y pie con IA."));
        autoNote.Margin = new Thickness(0, 4, 0, 0);
        root.Children.Add(autoNote);
        root.Children.Add(status);

        var foot = new DockPanel { Margin = new Thickness(0, 16, 0, 0), LastChildFill = false };
        foot.Children.Add(test);
        var save = new Button { Content = L.T("Guardar"), IsDefault = true, MinWidth = 96, FontWeight = FontWeights.SemiBold, Margin = new Thickness(8, 0, 0, 0) };
        save.SetResourceReference(BackgroundProperty, "Accent"); save.SetResourceReference(ForegroundProperty, "OnAccent"); save.SetResourceReference(BorderBrushProperty, "Accent");
        var cancel = new Button { Content = L.T("Cancelar"), IsCancel = true, MinWidth = 96 };
        DockPanel.SetDock(save, Dock.Right); DockPanel.SetDock(cancel, Dock.Right);
        foot.Children.Add(save); foot.Children.Add(cancel);
        root.Children.Add(foot);
        Content = root;

        var st = Store.Settings;
        enabled.IsChecked = st.AiEnabled;
        autoCaption.IsChecked = st.AiAutoCaption;
        provider.SelectedItem = AiProviders.Get(st.AiProvider);
        model.Text = st.AiModel;
        url.Text = st.AiBaseUrl;
        provider.SelectionChanged += (_, _) => { model.Text = ""; ShowProvider(); };
        enabled.Click += (_, _) => Sync();
        keyLink.Click += (_, _) => { var p = Current; if (p.KeyHelpUrl.Length > 0) try { Process.Start(new ProcessStartInfo(p.KeyHelpUrl) { UseShellExecute = true }); } catch (System.ComponentModel.Win32Exception) { } };
        test.Click += async (_, _) => await TestAsync();
        save.Click += (_, _) => { Apply(Store.Settings, saveKey: true); Store.SaveSettings(); Close(); };
        PreviewKeyDown += (_, e) => { if (e.Key == Key.Escape) Close(); };
        ShowProvider();
    }

    AiProvider Current => (AiProvider?)provider.SelectedItem ?? AiProviders.All[0];

    void ShowProvider()
    {
        var p = Current;
        hint.Text = L.T(p.Hint);
        SetPlaceholder(model, p.SuggestedModel.Length > 0 ? p.SuggestedModel : L.T("nombre del modelo"));
        urlRow.Visibility = p.Id == "custom" ? Visibility.Visible : Visibility.Collapsed;
        keyRow.Visibility = p.NeedsKey ? Visibility.Visible : Visibility.Collapsed;
        keyLink.IsEnabled = p.KeyHelpUrl.Length > 0;
        key.Password = "";
        string? stored = AiKeys.Load(p.Id);
        keyState.Text = stored != null ? L.F("Guardada: {0}. Escribí otra para reemplazarla.", AiKeys.Masked(stored)) : L.T("Todavía no hay una key guardada.");
        status.Text = "";
        Sync();
    }

    void Sync()
    {
        bool on = enabled.IsChecked == true;
        foreach (var c in new UIElement[] { provider, model, url, key, test, autoCaption }) c.IsEnabled = on;
    }

    /// <summary>Copies the form into settings; the key only when one was typed (an empty box keeps the stored one).</summary>
    void Apply(Settings s, bool saveKey)
    {
        s.AiEnabled = enabled.IsChecked == true;
        s.AiAutoCaption = autoCaption.IsChecked == true;
        s.AiProvider = Current.Id;
        s.AiModel = model.Text.Trim();
        s.AiBaseUrl = url.Text.Trim();
        if (saveKey && key.Password.Length > 0) AiKeys.Save(Current.Id, key.Password);
    }

    async Task TestAsync()
    {
        var trial = new Settings();
        Apply(trial, saveKey: false);
        trial.AiEnabled = true;
        // Test with the key in the box if one was typed, else the stored one.
        string? typed = key.Password.Length > 0 ? key.Password : null;
        test.IsEnabled = false;
        status.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        status.Text = L.F("Probando {0} · {1}…", Current.Name, AiProviders.ModelFor(trial));
        try
        {
            var client = AiClient.FromSettings(trial, typed);
            var answer = await client.CompleteAsync(new AiRequest(
                "You are a connection test. Reply with exactly: OK", "Reply with exactly: OK", [], 50));
            status.SetResourceReference(TextBlock.ForegroundProperty, "Accent");
            status.Text = L.T("Conexión correcta. El modelo respondió: ") + (answer.Length > 60 ? answer[..60] + "…" : answer);
        }
        catch (AiException ex)
        {
            status.SetResourceReference(TextBlock.ForegroundProperty, "ErrorInk");
            status.Text = ex.Message;
        }
        finally { test.IsEnabled = enabled.IsChecked == true; }
    }

    static TextBlock Muted(string text)
    {
        var t = new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap };
        t.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        t.SetResourceReference(TextBlock.FontSizeProperty, "SmallFontSize");
        return t;
    }

    static TextBlock Label(string text)
    {
        var t = new TextBlock { Text = text, FontWeight = FontWeights.SemiBold, Margin = new Thickness(0, 12, 0, 0) };
        return t;
    }

    static void SetPlaceholder(TextBox box, string text) => box.ToolTip = text;

    public static void ShowFor(Window? owner)
    {
        var w = new AiSettingsWindow();
        if (owner != null && owner.IsVisible) w.Owner = owner; else w.WindowStartupLocation = WindowStartupLocation.CenterScreen;
        w.ShowDialog();
    }
}
#endif
