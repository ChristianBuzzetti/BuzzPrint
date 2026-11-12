using System.Windows;
using System.Windows.Controls;
using System.Windows.Documents;
using System.Windows.Input;
using System.Windows.Media;

namespace BuzzPrint;

/// <summary>DBMON modal: ◤ title, muted purpose line, content, quiet secondary and primary accent action.</summary>
public sealed class PromptWindow : Window
{
    readonly TextBox? input;
    bool accepted;

    PromptWindow(string title, string message, string? value, string primary, bool cancel, string? cancelText = null)
    {
        SetResourceReference(StyleProperty, typeof(Window));
        TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        Title = title + " · BuzzPrint";
        Width = 520; SizeToContent = SizeToContent.Height; ResizeMode = ResizeMode.NoResize;
        WindowStartupLocation = WindowStartupLocation.CenterOwner; ShowInTaskbar = false;
        var root = new StackPanel { Margin = new Thickness(24, 20, 24, 18) };
        var head = new TextBlock { FontWeight = FontWeights.SemiBold };
        var mark = new Run("◤ "); mark.SetResourceReference(TextElement.ForegroundProperty, "Accent");
        head.Inlines.Add(mark); head.Inlines.Add(new Run(title));
        head.SetResourceReference(TextBlock.FontSizeProperty, "SectionFontSize");
        root.Children.Add(head);
        var msg = new TextBlock { Text = message, TextWrapping = TextWrapping.Wrap, Margin = new Thickness(0, 6, 0, 14) };
        msg.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        root.Children.Add(msg);
        if (value != null)
        {
            input = new TextBox { Text = value, Margin = new Thickness(0, 0, 0, 16) };
            root.Children.Add(input);
            Loaded += (_, _) => { input.Focus(); input.SelectAll(); };
        }
        var foot = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right };
        if (cancel)
        {
            var no = new Button { Content = cancelText ?? L.T("Cancelar"), IsCancel = true, MinWidth = 96 };
            foot.Children.Add(no);
        }
        var ok = new Button { Content = primary, IsDefault = true, MinWidth = 96, FontWeight = FontWeights.SemiBold, Margin = new Thickness(8, 0, 0, 0) };
        ok.SetResourceReference(BackgroundProperty, "Accent"); ok.SetResourceReference(ForegroundProperty, "OnAccent"); ok.SetResourceReference(BorderBrushProperty, "Accent");
        ok.Click += (_, _) => { accepted = true; Close(); };
        if (!cancel) ok.IsCancel = true;
        foot.Children.Add(ok);
        root.Children.Add(foot);
        Content = root;
        PreviewKeyDown += (_, e) => { if (e.Key == Key.Escape) Close(); };
    }

    public static string? Ask(Window owner, string title, string message, string value)
    {
        var w = new PromptWindow(title, message, value, L.T("Guardar"), true) { Owner = owner };
        w.ShowDialog();
        return w.accepted ? w.input!.Text : null;
    }

    public static bool Confirm(Window owner, string title, string message, string primary, string? cancelText = null)
    {
        var w = new PromptWindow(title, message, null, primary, true, cancelText) { Owner = owner };
        w.ShowDialog();
        return w.accepted;
    }

    public static void Info(Window owner, string title, string message)
    {
        new PromptWindow(title, message, null, L.Es ? "Listo" : "OK", false) { Owner = owner }.ShowDialog();
    }
}
