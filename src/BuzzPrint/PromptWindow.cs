using System.Windows;
using System.Windows.Controls;
using System.Windows.Documents;
using System.Windows.Input;
using System.Windows.Media;

namespace BuzzPrint;

/// <summary>App dialog: living mark, centred title and message, details as label/value rows in a card, optional
/// input, quiet secondary and primary accent action.</summary>
public sealed class PromptWindow : Window
{
    readonly TextBox? input;
    bool accepted;

    PromptWindow(string title, string message, string? value, string primary, bool cancel, string? cancelText = null)
    {
        SetResourceReference(StyleProperty, typeof(Window));
        TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        Title = title + " · BuzzPrint";
        Width = 480; SizeToContent = SizeToContent.Height; ResizeMode = ResizeMode.NoResize;
        WindowStartupLocation = WindowStartupLocation.CenterOwner; ShowInTaskbar = false;
        var root = new StackPanel { Margin = new Thickness(32, 28, 32, 22) };

        // Centred hero: the living mark, the title and the first paragraph of the message.
        var mark = new BobertoMark { Width = 64, Height = 64, HorizontalAlignment = HorizontalAlignment.Center, Margin = new Thickness(0, 0, 0, 12) };
        mark.SetResourceReference(BobertoMark.TileProperty, "Accent");
        root.Children.Add(mark);
        var head = new TextBlock { Text = title, FontWeight = FontWeights.Bold, TextWrapping = TextWrapping.Wrap, TextAlignment = TextAlignment.Center };
        head.SetResourceReference(TextBlock.FontSizeProperty, "TitleFontSize");
        root.Children.Add(head);
        var paragraphs = message.Replace("\r\n", "\n").Split("\n\n", StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        if (paragraphs.Length > 0)
        {
            var msg = new TextBlock { Text = paragraphs[0], TextWrapping = TextWrapping.Wrap, TextAlignment = TextAlignment.Center, Margin = new Thickness(0, 8, 0, 0), LineHeight = 20 };
            msg.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
            root.Children.Add(msg);
        }
        // Further paragraphs are details, in a quiet card; "Label: value" lines become label/value rows.
        if (paragraphs.Length > 1)
        {
            var card = new Border { Margin = new Thickness(0, 18, 0, 0), Padding = new Thickness(16, 4, 16, 4), BorderThickness = new Thickness(1) };
            card.SetResourceReference(Border.BackgroundProperty, "SurfaceRaised");
            card.SetResourceReference(Border.BorderBrushProperty, "FaintBorder");
            card.SetResourceReference(Border.CornerRadiusProperty, "StripRadius");
            var rows = new StackPanel();
            foreach (var para in paragraphs.Skip(1))
                foreach (var line in para.Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
                {
                    if (rows.Children.Count > 0) { var rule = new Border { Height = 1 }; rule.SetResourceReference(Border.BackgroundProperty, "FaintBorder"); rows.Children.Add(rule); }
                    int colon = line.IndexOf(": ", StringComparison.Ordinal);
                    if (colon is > 0 and <= 24)
                    {
                        var row = new StackPanel { Margin = new Thickness(0, 8, 0, 8) };
                        var l = new TextBlock { Text = line[..colon].ToUpperInvariant(), FontSize = 10.5 };
                        l.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont");
                        l.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
                        var v = new TextBlock { Text = line[(colon + 2)..], TextWrapping = TextWrapping.Wrap, Margin = new Thickness(0, 3, 0, 0) };
                        if (line.Contains(":\\") || line.Contains('/')) { v.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont"); v.FontSize = 12; }
                        row.Children.Add(l); row.Children.Add(v);
                        rows.Children.Add(row);
                    }
                    else rows.Children.Add(new TextBlock { Text = line, TextWrapping = TextWrapping.Wrap, Margin = new Thickness(0, 8, 0, 8) });
                }
            card.Child = rows;
            root.Children.Add(card);
        }
        if (value != null)
        {
            input = new TextBox { Text = value, Margin = new Thickness(0, 18, 0, 0) };
            root.Children.Add(input);
            Loaded += (_, _) => { input.Focus(); input.SelectAll(); };
        }
        var foot = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right, Margin = new Thickness(0, 22, 0, 0) };
        if (cancel)
        {
            var no = new Button { Content = cancelText ?? L.T("Cancelar"), IsCancel = true, MinWidth = 110 };
            foot.Children.Add(no);
        }
        var ok = new Button { Content = primary, IsDefault = true, MinWidth = 110, FontWeight = FontWeights.SemiBold, Margin = new Thickness(8, 0, 0, 0) };
        ok.SetResourceReference(BackgroundProperty, "Accent"); ok.SetResourceReference(ForegroundProperty, "OnAccent"); ok.SetResourceReference(BorderBrushProperty, "Accent");
        ok.Click += (_, _) => { accepted = true; Close(); };
        if (!cancel) ok.IsCancel = true;
        foot.Children.Add(ok);
        root.Children.Add(foot);
        Content = root;
        PreviewKeyDown += (_, e) => { if (e.Key == Key.Escape) Close(); };
    }

    /// <summary>Owner only when it is on screen; otherwise the dialog centers on the screen.</summary>
    PromptWindow Owned(Window? owner)
    {
        if (owner != null && owner.IsVisible) Owner = owner; else WindowStartupLocation = WindowStartupLocation.CenterScreen;
        return this;
    }

    /// <summary>For the screenshot run: shown, not modal.</summary>
    public static Window ForShot(Window owner, string title, string message, string? value)
        => new PromptWindow(title, message, value, value != null ? L.T("Guardar") : "OK", value != null) { Owner = owner };

    public static string? Ask(Window? owner, string title, string message, string value)
    {
        var w = new PromptWindow(title, message, value, L.T("Guardar"), true) .Owned(owner);
        w.ShowDialog();
        return w.accepted ? w.input!.Text : null;
    }

    public static bool Confirm(Window? owner, string title, string message, string primary, string? cancelText = null)
    {
        var w = new PromptWindow(title, message, null, primary, true, cancelText) .Owned(owner);
        w.ShowDialog();
        return w.accepted;
    }

    public static void Info(Window? owner, string title, string message)
    {
        new PromptWindow(title, message, null, L.Es ? "Listo" : "OK", false) .Owned(owner).ShowDialog();
    }
}
