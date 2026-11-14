using System.Globalization;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;

namespace BuzzPrint;

/// <summary>
/// The BuzzPrint guide: every feature, explained step by step, with its shortcuts and a button that opens it. Topics
/// on the left (with search), the selected topic on the right as cards, previous / next at the bottom.
/// </summary>
public sealed class GuideWindow : Window
{
    static GuideWindow? open;
    readonly IReadOnlyList<GuideTopic> topics = GuideContent.All();
    readonly StackPanel nav = new();
    readonly StackPanel page = new() { Margin = new Thickness(28, 6, 28, 28), MaxWidth = 860, HorizontalAlignment = HorizontalAlignment.Left };
    readonly ScrollViewer pageScroll;
    readonly TextBox search = new() { Margin = new Thickness(0, 0, 0, 12) };
    GuideTopic current;

    GuideWindow(Window? owner, string? topicId)
    {
        SetResourceReference(StyleProperty, typeof(Window));
        TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        Title = L.T("Guía de BuzzPrint") + " · " + AppInfo.Name;
        Width = 1140; Height = 800; MinWidth = 820; MinHeight = 560;
        if (owner is { IsVisible: true }) { Owner = owner; WindowStartupLocation = WindowStartupLocation.CenterOwner; } else WindowStartupLocation = WindowStartupLocation.CenterScreen;
        current = topics.FirstOrDefault(t => t.Id == topicId) ?? topics[0];

        var root = new DockPanel { Margin = new Thickness(24, 20, 24, 18) };
        var head = new StackPanel();
        var h = new TextBlock { Text = L.T("Guía de BuzzPrint"), FontWeight = FontWeights.SemiBold }; h.SetResourceReference(TextBlock.FontSizeProperty, "TitleFontSize"); head.Children.Add(h);
        head.Children.Add(Muted(L.T("Todo lo que podés hacer, paso a paso. Tocá un botón para probar cada función."), new Thickness(0, 4, 0, 14)));
        var top = BobertoMark.WithBadge(head, 48); DockPanel.SetDock(top, Dock.Top); root.Children.Add(top);

        // Left: search and topics.
        var left = new DockPanel { Width = 270 };
        var searchBox = new Grid();
        var hint = Muted("  " + L.T("Buscar en la guía…"), new Thickness(8, 0, 0, 0)); hint.VerticalAlignment = VerticalAlignment.Center; hint.IsHitTestVisible = false;
        search.TextChanged += (_, _) => { hint.Visibility = search.Text.Length == 0 ? Visibility.Visible : Visibility.Collapsed; RenderNav(); };
        searchBox.Children.Add(search); searchBox.Children.Add(hint);
        hint.Margin = new Thickness(10, 0, 0, 12);
        DockPanel.SetDock(searchBox, Dock.Top); left.Children.Add(searchBox);
        left.Children.Add(new ScrollViewer { Content = nav, VerticalScrollBarVisibility = ScrollBarVisibility.Auto });
        var leftCard = Card(left, new Thickness(12));
        leftCard.Margin = new Thickness(0, 0, 16, 0);
        DockPanel.SetDock(leftCard, Dock.Left); root.Children.Add(leftCard);

        // Right: the topic.
        pageScroll = new ScrollViewer { Content = page, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        root.Children.Add(Card(pageScroll, new Thickness(0)));
        Content = root;
        PreviewKeyDown += (_, e) =>
        {
            if (e.Key == Key.Escape) Close();
            else if (e.Key == Key.F && Keyboard.Modifiers == ModifierKeys.Control) { search.Focus(); search.SelectAll(); e.Handled = true; }
        };
        Closed += (_, _) => open = null;
        RenderNav();
        ShowTopic(current);
    }

    // ------------------------------------------------------------------ navigation
    static bool Has(string text, string q) => CultureInfo.InvariantCulture.CompareInfo.IndexOf(text, q, CompareOptions.IgnoreCase | CompareOptions.IgnoreNonSpace) >= 0;
    static bool Matches(GuideTopic t, string q) => q.Length == 0 || Has(t.Title, q) || Has(t.Lead, q) || t.Blocks.Any(b => Has(b.Heading, q) || b.Items.Any(i => Has(i.a, q) || Has(i.b, q)));

    void RenderNav()
    {
        nav.Children.Clear();
        string q = search.Text.Trim();
        var shown = topics.Where(t => Matches(t, q)).ToList();
        if (shown.Count == 0) { nav.Children.Add(Muted(L.T("Sin resultados. Probá con otra palabra."), new Thickness(6, 6, 0, 0))); return; }
        foreach (var t in shown)
        {
            var topic = t;
            var item = new RadioButton { GroupName = "guide", IsChecked = t == current, Cursor = Cursors.Hand };
            item.SetResourceReference(StyleProperty, "NavItem");
            var row = new DockPanel();
            var g = new TextBlock { Text = t.Glyph, FontFamily = new FontFamily("Segoe Fluent Icons, Segoe MDL2 Assets"), FontSize = 15, Width = 30, VerticalAlignment = VerticalAlignment.Center };
            row.Children.Add(g);
            row.Children.Add(new TextBlock { Text = t.Title, VerticalAlignment = VerticalAlignment.Center, TextTrimming = TextTrimming.CharacterEllipsis });
            item.Content = row;
            item.Checked += (_, _) => { if (current != topic) ShowTopic(topic); };
            nav.Children.Add(item);
        }
        if (q.Length > 0 && !shown.Contains(current)) ShowTopic(shown[0]);
    }

    void ShowTopic(GuideTopic t)
    {
        current = t;
        foreach (RadioButton r in nav.Children.OfType<RadioButton>()) r.IsChecked = ((DockPanel)r.Content).Children.OfType<TextBlock>().Last().Text == t.Title;
        page.Children.Clear();

        // Hero: icon tile, title, lead and the "try it" buttons.
        var hero = new DockPanel { Margin = new Thickness(0, 22, 0, 0) };
        var tile = new Border { Width = 60, Height = 60, CornerRadius = new CornerRadius(16), Margin = new Thickness(0, 0, 18, 0), VerticalAlignment = VerticalAlignment.Top };
        tile.SetResourceReference(Border.BackgroundProperty, "Accent");
        var glyph = new TextBlock { Text = t.Glyph, FontFamily = new FontFamily("Segoe Fluent Icons, Segoe MDL2 Assets"), FontSize = 28, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
        glyph.SetResourceReference(TextBlock.ForegroundProperty, "OnAccent");
        tile.Child = glyph;
        DockPanel.SetDock(tile, Dock.Left); hero.Children.Add(tile);
        var texts = new StackPanel();
        var title = new TextBlock { Text = t.Title, FontWeight = FontWeights.Bold, FontSize = 26, TextWrapping = TextWrapping.Wrap };
        texts.Children.Add(title);
        var lead = new TextBlock { Text = t.Lead, TextWrapping = TextWrapping.Wrap, Margin = new Thickness(0, 6, 0, 0), LineHeight = 22, FontSize = 14.5 };
        lead.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        texts.Children.Add(lead);
        if (t.Try.Count > 0)
        {
            var actions = new WrapPanel { Margin = new Thickness(0, 14, 0, 0) };
            bool first = true;
            foreach (var (label, action) in t.Try)
            {
                var b = new Button { Content = label, MinWidth = 150, Margin = new Thickness(0, 0, 8, 8), Padding = new Thickness(16, 6, 16, 6) };
                if (first) { b.FontWeight = FontWeights.SemiBold; b.SetResourceReference(BackgroundProperty, "Accent"); b.SetResourceReference(ForegroundProperty, "OnAccent"); b.SetResourceReference(BorderBrushProperty, "Accent"); }
                string id = action;
                b.Click += (_, _) => Try(id);
                actions.Children.Add(b);
                first = false;
            }
            texts.Children.Add(actions);
        }
        hero.Children.Add(texts);
        page.Children.Add(hero);

        // Blocks.
        foreach (var block in t.Blocks) page.Children.Add(Block(block));

        // Previous / next.
        int i = topics.ToList().IndexOf(t);
        var pager = new DockPanel { Margin = new Thickness(0, 22, 0, 0) };
        if (i + 1 < topics.Count) { var next = PagerButton(L.T("Siguiente") + ": " + topics[i + 1].Title + "  →", topics[i + 1]); DockPanel.SetDock(next, Dock.Right); pager.Children.Add(next); }
        if (i > 0) { var prev = PagerButton("←  " + L.T("Anterior") + ": " + topics[i - 1].Title, topics[i - 1]); prev.HorizontalAlignment = HorizontalAlignment.Left; pager.Children.Add(prev); }
        page.Children.Add(pager);
        pageScroll.ScrollToTop();
    }

    Button PagerButton(string text, GuideTopic to)
    {
        var b = new Button { Content = text, Padding = new Thickness(14, 6, 14, 6) };
        b.Click += (_, _) => ShowTopic(to);
        return b;
    }

    // ------------------------------------------------------------------ blocks
    FrameworkElement Block(GuideBlock b)
    {
        var body = new StackPanel();
        var label = new TextBlock { FontWeight = FontWeights.Bold, Margin = new Thickness(0, 0, 0, 12) };
        var mark = new System.Windows.Documents.Run("◤ "); mark.SetResourceReference(System.Windows.Documents.TextElement.ForegroundProperty, "Accent");
        label.Inlines.Add(mark); label.Inlines.Add(new System.Windows.Documents.Run(b.Heading));
        label.SetResourceReference(TextBlock.FontSizeProperty, "SmallFontSize");
        label.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        body.Children.Add(label);
        int n = 0;
        foreach (var (a, desc) in b.Items)
        {
            n++;
            var row = new DockPanel { Margin = new Thickness(0, 0, 0, 10) };
            switch (b.Kind)
            {
                case "steps":
                {
                    var num = new Border { Width = 26, Height = 26, CornerRadius = new CornerRadius(13), Margin = new Thickness(0, 0, 12, 0), VerticalAlignment = VerticalAlignment.Top };
                    num.SetResourceReference(Border.BackgroundProperty, "Accent");
                    var nt = new TextBlock { Text = n.ToString(), FontWeight = FontWeights.Bold, FontSize = 12.5, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
                    nt.SetResourceReference(TextBlock.ForegroundProperty, "OnAccent");
                    num.Child = nt;
                    DockPanel.SetDock(num, Dock.Left); row.Children.Add(num);
                    row.Children.Add(Body(a, new Thickness(0, 3, 0, 0)));
                    break;
                }
                case "keys":
                {
                    var chip = new Border { BorderThickness = new Thickness(1), Padding = new Thickness(9, 3, 9, 4), Margin = new Thickness(0, 0, 14, 0), MinWidth = 150, VerticalAlignment = VerticalAlignment.Top, HorizontalAlignment = HorizontalAlignment.Left };
                    chip.SetResourceReference(Border.BorderBrushProperty, "StrongBorder");
                    chip.SetResourceReference(Border.BackgroundProperty, "Surface");
                    chip.SetResourceReference(Border.CornerRadiusProperty, "ControlRadius");
                    var kt = new TextBlock { Text = a, FontWeight = FontWeights.SemiBold, TextWrapping = TextWrapping.Wrap };
                    kt.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont");
                    chip.Child = kt;
                    var col = new Border { Width = 210, Child = chip };
                    DockPanel.SetDock(col, Dock.Left); row.Children.Add(col);
                    row.Children.Add(Body(desc, new Thickness(0, 3, 0, 0)));
                    break;
                }
                default:
                {
                    var dot = new Border { Width = 6, Height = 6, CornerRadius = new CornerRadius(3), Margin = new Thickness(4, 8, 14, 0), VerticalAlignment = VerticalAlignment.Top };
                    dot.SetResourceReference(Border.BackgroundProperty, "Accent");
                    DockPanel.SetDock(dot, Dock.Left); row.Children.Add(dot);
                    row.Children.Add(Body(a, new Thickness(0)));
                    break;
                }
            }
            body.Children.Add(row);
        }
        var card = Card(body, new Thickness(20, 16, 20, 8));
        card.Margin = new Thickness(0, 18, 0, 0);
        card.SetResourceReference(Border.BackgroundProperty, "SurfaceRaised");
        return card;
    }

    static TextBlock Body(string text, Thickness margin) => new() { Text = text, TextWrapping = TextWrapping.Wrap, Margin = margin, LineHeight = 21, FontSize = 14 };

    static TextBlock Muted(string text, Thickness margin)
    {
        var b = new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap, Margin = margin };
        b.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
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

    // ------------------------------------------------------------------ "try it"
    void Try(string action)
    {
        var main = App.Current.Library;
        if (main == null) return;
        // Capturing and recording need the screen free: the guide steps aside first.
        if (action is "capture" or "full") Close();
        main.GuideAction(action, this);
    }

    /// <summary>Opens the guide (or brings it forward), optionally on a topic.</summary>
    public static void Open(Window? owner, string? topicId = null)
    {
        if (open != null)
        {
            if (topicId != null && open.topics.FirstOrDefault(t => t.Id == topicId) is { } t) open.ShowTopic(t);
            if (open.WindowState == WindowState.Minimized) open.WindowState = WindowState.Normal;
            open.Activate();
            return;
        }
        open = new GuideWindow(owner, topicId);
        open.Show();
    }

    /// <summary>For the screenshot run.</summary>
    internal static Window ForShot(Window owner, string topicId) => new GuideWindow(owner, topicId);
}
