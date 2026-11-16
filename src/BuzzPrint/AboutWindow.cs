using System.Diagnostics;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;

namespace BuzzPrint;

/// <summary>Help → About: a centred card with the living mark, name and version chips, what the app does, and the
/// credits as label/value rows. Replaces the generic dialog, whose side-by-side layout left the mark cramped.</summary>
public sealed class AboutWindow : Window
{
    const string Repo = "github.com/ChristianBuzzetti/BuzzPrint";

    AboutWindow()
    {
        SetResourceReference(StyleProperty, typeof(Window));
        TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        Title = L.T("Acerca de BuzzPrint");
        Width = 480; SizeToContent = SizeToContent.Height; ResizeMode = ResizeMode.NoResize;
        WindowStartupLocation = WindowStartupLocation.CenterOwner; ShowInTaskbar = false;

        var root = new StackPanel { Margin = new Thickness(32, 30, 32, 22) };

        // Hero: the mark, the name, version and edition.
        var mark = new BobertoMark { Width = 84, Height = 84, HorizontalAlignment = HorizontalAlignment.Center, Margin = new Thickness(0, 0, 0, 14) };
        mark.SetResourceReference(BobertoMark.TileProperty, "Accent");
        root.Children.Add(mark);
        var name = new TextBlock { Text = "BuzzPrint", FontWeight = FontWeights.Bold, HorizontalAlignment = HorizontalAlignment.Center };
        name.SetResourceReference(TextBlock.FontSizeProperty, "TitleFontSize");
        root.Children.Add(name);

        var v = typeof(App).Assembly.GetName().Version;
        var chips = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Center, Margin = new Thickness(0, 8, 0, 0) };
        if (v != null) chips.Children.Add(Chip("v" + v.Major + "." + v.Minor + "." + v.Build, accent: false));
        root.Children.Add(chips);

        var about = new TextBlock
        {
            Text = L.T("Capturas de región, ventana y pantalla completa, grabación de video y audio, biblioteca, menú rápido para copiar y pegar, editor de anotaciones y exportación a PowerPoint, Word y PDF."),
            TextWrapping = TextWrapping.Wrap, TextAlignment = TextAlignment.Center, Margin = new Thickness(0, 16, 0, 0), LineHeight = 20
        };
        about.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        root.Children.Add(about);

        // Credits as label / value rows inside a quiet card.
        var card = new Border { Margin = new Thickness(0, 20, 0, 0), Padding = new Thickness(16, 6, 16, 6), BorderThickness = new Thickness(1) };
        card.SetResourceReference(Border.BackgroundProperty, "SurfaceRaised");
        card.SetResourceReference(Border.BorderBrushProperty, "FaintBorder");
        card.SetResourceReference(Border.CornerRadiusProperty, "StripRadius");
        var rows = new Grid();
        rows.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        rows.ColumnDefinitions.Add(new ColumnDefinition());
        var link = new TextBlock { Text = Repo, Cursor = Cursors.Hand, TextDecorations = TextDecorations.Underline };
        link.SetResourceReference(TextBlock.ForegroundProperty, "Accent");
        link.MouseLeftButtonUp += (_, _) => Open("https://" + Repo);
        AddRow(rows, L.T("LICENCIA"), Value(L.T("MIT · © 2026 Christian Buzzetti")));
        AddRow(rows, L.T("CÓDIGO"), link);
        AddRow(rows, L.T("FUENTES"), Value(L.T("Inter y JetBrains Mono (SIL Open Font License)")));
        AddRow(rows, L.T("VIDEO Y AUDIO"), Value("FFmpeg · ffmpeg.org"));
        card.Child = rows;
        root.Children.Add(card);

        // Footer: repository on the left, close on the right.
        var foot = new DockPanel { Margin = new Thickness(0, 20, 0, 0) };
        var ok = new Button { Content = L.Es ? "Listo" : "OK", IsDefault = true, IsCancel = true, MinWidth = 110, FontWeight = FontWeights.SemiBold };
        ok.SetResourceReference(BackgroundProperty, "Accent"); ok.SetResourceReference(ForegroundProperty, "OnAccent"); ok.SetResourceReference(BorderBrushProperty, "Accent");
        ok.Click += (_, _) => Close();
        DockPanel.SetDock(ok, Dock.Right);
        foot.Children.Add(ok);
        var github = new Button { Content = L.T("Ver en GitHub"), HorizontalAlignment = HorizontalAlignment.Left, MinWidth = 110 };
        github.Click += (_, _) => Open("https://" + Repo);
        foot.Children.Add(github);
        root.Children.Add(foot);

        Content = root;
        PreviewKeyDown += (_, e) => { if (e.Key == Key.Escape) Close(); };
    }

    static Border Chip(string text, bool accent)
    {
        var t = new TextBlock { Text = text, FontSize = 11, FontWeight = FontWeights.SemiBold };
        t.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont");
        t.SetResourceReference(TextBlock.ForegroundProperty, accent ? "Accent" : "Muted");
        var b = new Border { Child = t, Padding = new Thickness(9, 3, 9, 3), Margin = new Thickness(3, 0, 3, 0), BorderThickness = new Thickness(1), CornerRadius = new CornerRadius(10) };
        b.SetResourceReference(Border.BackgroundProperty, accent ? "AccentWeak" : "SurfaceRaised");
        b.SetResourceReference(Border.BorderBrushProperty, accent ? "AccentCardRing" : "FaintBorder");
        return b;
    }

    static TextBlock Value(string text) => new() { Text = text, TextWrapping = TextWrapping.Wrap };

    static void AddRow(Grid grid, string label, FrameworkElement value)
    {
        int r = grid.RowDefinitions.Count;
        grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        var l = new TextBlock { Text = label, FontSize = 10.5, Margin = new Thickness(0, 9, 18, 9), VerticalAlignment = VerticalAlignment.Center };
        l.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont");
        l.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        value.Margin = new Thickness(0, 9, 0, 9);
        value.VerticalAlignment = VerticalAlignment.Center;
        Grid.SetRow(l, r); Grid.SetRow(value, r); Grid.SetColumn(value, 1);
        grid.Children.Add(l); grid.Children.Add(value);
        if (r > 0)
        {
            var rule = new Border { Height = 1, VerticalAlignment = VerticalAlignment.Top };
            rule.SetResourceReference(Border.BackgroundProperty, "FaintBorder");
            Grid.SetRow(rule, r); Grid.SetColumnSpan(rule, 2);
            grid.Children.Add(rule);
        }
    }

    static void Open(string url)
    {
        try { Process.Start(new ProcessStartInfo(url) { UseShellExecute = true }); } catch (System.ComponentModel.Win32Exception) { }
    }

    /// <summary>For the screenshot run: shown, not modal.</summary>
    public static Window ForShot(Window owner) => new AboutWindow { Owner = owner };

    public static void Show(Window? owner)
    {
        var w = new AboutWindow();
        if (owner != null && owner.IsVisible) w.Owner = owner; else w.WindowStartupLocation = WindowStartupLocation.CenterScreen;
        w.ShowDialog();
    }
}
