using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;

namespace BuzzPrint;

/// <summary>Edits one custom document template with a live preview: name, colors, fonts, cover, steps, frame, footer.</summary>
public sealed class TemplateEditWindow : Window
{
    readonly DocTemplate t;
    readonly string appAccent;
    readonly Border preview = new() { Margin = new Thickness(0, 6, 0, 0), HorizontalAlignment = HorizontalAlignment.Left };
    bool saved;

    TemplateEditWindow(Window? owner, DocTemplate template, string appAccent, string title, string purpose)
    {
        t = template.Copy();
        this.appAccent = appAccent;
        SetResourceReference(StyleProperty, typeof(Window));
        TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        Title = title + " · BuzzPrint";
        Width = 980; Height = 720; MinWidth = 820; MinHeight = 560; ShowInTaskbar = false;
        if (owner is { IsVisible: true }) { Owner = owner; WindowStartupLocation = WindowStartupLocation.CenterOwner; } else WindowStartupLocation = WindowStartupLocation.CenterScreen;

        var root = new DockPanel { Margin = new Thickness(24, 20, 24, 18) };
        var head = new StackPanel();
        var h = new TextBlock { Text = title, FontWeight = FontWeights.SemiBold }; h.SetResourceReference(TextBlock.FontSizeProperty, "TitleFontSize"); head.Children.Add(h);
        var p = Muted(purpose); p.Margin = new Thickness(0, 4, 0, 14); head.Children.Add(p);
        var top = BobertoMark.WithBadge(head, 48); DockPanel.SetDock(top, Dock.Top); root.Children.Add(top);

        var foot = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right, Margin = new Thickness(0, 14, 0, 0) };
        var cancel = new Button { Content = L.T("Cancelar"), IsCancel = true, MinWidth = 110 }; cancel.Click += (_, _) => Close();
        var save = new Button { Content = L.T("Guardar plantilla"), IsDefault = true, MinWidth = 150, Margin = new Thickness(8, 0, 0, 0), FontWeight = FontWeights.SemiBold };
        save.SetResourceReference(BackgroundProperty, "Accent"); save.SetResourceReference(ForegroundProperty, "OnAccent"); save.SetResourceReference(BorderBrushProperty, "Accent");
        save.Click += (_, _) => { saved = true; Close(); };
        foot.Children.Add(cancel); foot.Children.Add(save);
        DockPanel.SetDock(foot, Dock.Bottom); root.Children.Add(foot);

        // Right: live preview.
        var right = new StackPanel { Width = 400, Margin = new Thickness(24, 0, 0, 0) };
        right.Children.Add(Label(L.T("VISTA PREVIA")));
        right.Children.Add(preview);
        right.Children.Add(Muted(L.T("Portada y una página con un paso. Así se ve en PowerPoint, Word, PDF e imagen; en PDF las tipografías se aproximan a Helvetica, Times o Courier."), new Thickness(0, 12, 0, 0)));
        DockPanel.SetDock(right, Dock.Right); root.Children.Add(right);

        // Left: the fields.
        var form = new StackPanel();
        form.Children.Add(Label(L.T("NOMBRE")));
        form.Children.Add(Field(t.Name, v => t.Name = v));
        form.Children.Add(Label(L.T("DESCRIPCIÓN"), 12));
        form.Children.Add(Field(t.Description, v => t.Description = v));

        form.Children.Add(Label(L.T("COLORES"), 16));
        var appAccentBox = new CheckBox { Content = L.T("Usar el color de acento de la app"), IsChecked = t.Accent == "app", Margin = new Thickness(0, 4, 0, 6) };
        var colors = new WrapPanel();
        var accentRow = ColorField(L.T("Acento"), t.Accent == "app" ? appAccent : t.Accent, v => t.Accent = v);
        accentRow.IsEnabled = t.Accent != "app";
        appAccentBox.Click += (_, _) => { bool app = appAccentBox.IsChecked == true; accentRow.IsEnabled = !app; t.Accent = app ? "app" : ((TextBox)((DockPanel)accentRow).Children[^1]).Text.Trim().TrimStart('#'); Refresh(); };
        form.Children.Add(appAccentBox);
        colors.Children.Add(accentRow);
        colors.Children.Add(ColorField(L.T("Texto"), t.Ink, v => t.Ink = v));
        colors.Children.Add(ColorField(L.T("Texto suave"), t.Muted, v => t.Muted = v));
        colors.Children.Add(ColorField(L.T("Detalles"), t.Faint, v => t.Faint = v));
        colors.Children.Add(ColorField(L.T("Líneas"), t.Rule, v => t.Rule = v));
        colors.Children.Add(ColorField(L.T("Fondo de página"), t.Page, v => t.Page = v));
        colors.Children.Add(ColorField(L.T("Fondo de portada"), t.CoverFill.Length == 6 ? t.CoverFill : t.AccentHex(appAccent), v => t.CoverFill = v));
        colors.Children.Add(ColorField(L.T("Texto de portada"), t.CoverInk, v => t.CoverInk = v));
        form.Children.Add(colors);

        form.Children.Add(Label(L.T("TIPOGRAFÍAS"), 16));
        var fonts = new WrapPanel();
        fonts.Children.Add(FontField(L.T("Títulos"), t.HeadingFont, v => t.HeadingFont = v));
        fonts.Children.Add(FontField(L.T("Texto"), t.BodyFont, v => t.BodyFont = v));
        form.Children.Add(fonts);

        form.Children.Add(Label(L.T("DISEÑO"), 16));
        var layout = new WrapPanel();
        layout.Children.Add(Choice(L.T("Portada"), t.Cover, [("bar", L.T("Barra de acento")), ("band", L.T("Banda superior")), ("full", L.T("A todo color")), ("minimal", L.T("Minimalista"))], v => t.Cover = v));
        layout.Children.Add(Choice(L.T("Pasos"), t.Steps, [("inline", L.T("Texto de acento")), ("pill", L.T("Etiqueta")), ("number", L.T("Número grande"))], v => t.Steps = v));
        string frameKind = t.Frame is "none" or "accent" ? t.Frame : "color";
        layout.Children.Add(Choice(L.T("Marco de las capturas"), frameKind, [("color", L.T("Gris fino")), ("accent", L.T("Color de acento")), ("none", L.T("Sin marco"))], v => t.Frame = v == "color" ? "D1D5DB" : v));
        form.Children.Add(layout);
        var rule = new CheckBox { Content = L.T("Línea de acento arriba de cada página"), IsChecked = t.TopRule, Margin = new Thickness(0, 8, 0, 0) };
        rule.Click += (_, _) => { t.TopRule = rule.IsChecked == true; Refresh(); };
        form.Children.Add(rule);
        form.Children.Add(Label(L.T("PIE DE PÁGINA"), 16));
        form.Children.Add(Field(t.Footer, v => t.Footer = v));
        form.Children.Add(Muted(L.T("Por ejemplo el nombre de tu empresa o equipo. Vacío = sin pie."), new Thickness(0, 4, 0, 0)));

        root.Children.Add(new ScrollViewer { Content = form, VerticalScrollBarVisibility = ScrollBarVisibility.Auto, Padding = new Thickness(0, 0, 10, 0) });
        Content = root;
        PreviewKeyDown += (_, e) => { if (e.Key == Key.Escape) Close(); };
        Refresh();
    }

    void Refresh() => preview.Child = TemplatePreview.Build(t, appAccent, 190);

    static TextBlock Muted(string text, Thickness? margin = null)
    {
        var b = new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap, Margin = margin ?? new Thickness(0) };
        b.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        b.SetResourceReference(TextBlock.FontSizeProperty, "SmallFontSize");
        return b;
    }
    static TextBlock Label(string text, double top = 0)
    {
        var b = new TextBlock { Text = text, FontWeight = FontWeights.SemiBold, Margin = new Thickness(0, top, 0, 6) };
        b.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        b.SetResourceReference(TextBlock.FontSizeProperty, "SmallFontSize");
        return b;
    }
    TextBox Field(string value, Action<string> set)
    {
        var box = new TextBox { Text = value };
        box.TextChanged += (_, _) => { set(box.Text); Refresh(); };
        return box;
    }

    /// <summary>A labeled hex box with a swatch; a valid color updates the template and the preview.</summary>
    FrameworkElement ColorField(string label, string hex, Action<string> set)
    {
        var row = new DockPanel { Width = 196, Margin = new Thickness(0, 0, 12, 8) };
        var name = new TextBlock { Text = label, Width = 96, VerticalAlignment = VerticalAlignment.Center, TextTrimming = TextTrimming.CharacterEllipsis };
        DockPanel.SetDock(name, Dock.Left); row.Children.Add(name);
        var swatch = new Border { Width = 22, Height = 22, CornerRadius = new CornerRadius(5), BorderThickness = new Thickness(1), Margin = new Thickness(0, 0, 6, 0) };
        swatch.SetResourceReference(Border.BorderBrushProperty, "StrongBorder");
        DockPanel.SetDock(swatch, Dock.Left); row.Children.Add(swatch);
        var box = new TextBox { Text = "#" + hex.TrimStart('#'), FontFamily = new FontFamily("Consolas") };
        void Paint()
        {
            string v = box.Text.Trim().TrimStart('#');
            if (System.Text.RegularExpressions.Regex.IsMatch(v, "^[0-9A-Fa-f]{6}$")) { swatch.Background = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#" + v)); set(v.ToUpperInvariant()); Refresh(); }
        }
        box.TextChanged += (_, _) => Paint();
        swatch.Background = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#" + hex.TrimStart('#')));
        row.Children.Add(box);
        return row;
    }

    FrameworkElement FontField(string label, string value, Action<string> set)
    {
        var col = new StackPanel { Width = 196, Margin = new Thickness(0, 0, 12, 8) };
        col.Children.Add(new TextBlock { Text = label, Margin = new Thickness(0, 0, 0, 4) });
        var combo = new ComboBox { IsEditable = true, Text = value, ItemsSource = Fonts.SystemFontFamilies.Select(f => f.Source).OrderBy(n => n).ToList() };
        combo.AddHandler(System.Windows.Controls.Primitives.TextBoxBase.TextChangedEvent, new TextChangedEventHandler((_, _) => { if ((combo.Text ?? "").Trim().Length > 0) { set(combo.Text.Trim()); Refresh(); } }));
        col.Children.Add(combo);
        return col;
    }

    FrameworkElement Choice(string label, string value, (string id, string name)[] options, Action<string> set)
    {
        var col = new StackPanel { Width = 196, Margin = new Thickness(0, 0, 12, 8) };
        col.Children.Add(new TextBlock { Text = label, Margin = new Thickness(0, 0, 0, 4) });
        var combo = new ComboBox { ItemsSource = options.Select(o => o.name).ToList(), SelectedIndex = Math.Max(0, Array.FindIndex(options, o => o.id == value)) };
        combo.SelectionChanged += (_, _) => { if (combo.SelectedIndex >= 0) { set(options[combo.SelectedIndex].id); Refresh(); } };
        col.Children.Add(combo);
        return col;
    }

    /// <summary>Shows the editor; the edited copy when saved, null when cancelled.</summary>
    public static DocTemplate? Edit(Window? owner, DocTemplate template, string appAccent, string title, string purpose)
    {
        var w = new TemplateEditWindow(owner, template, appAccent, title, purpose);
        w.ShowDialog();
        return w.saved ? w.t.Normalized() : null;
    }
}
