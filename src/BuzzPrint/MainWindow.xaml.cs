using System.Collections.ObjectModel;
using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Media.Imaging;

namespace BuzzPrint;

public sealed class CaptureCard
{
    public CaptureItem Item { get; init; } = null!;
    public BitmapSource? Thumb { get; init; }
    public string Title { get; init; } = "";
    public string Meta { get; init; } = "";
    public double CardWidth { get; init; }
    public double ThumbHeight { get; init; }
    public string Group { get; init; } = "";
    public Visibility PinnedVisibility => Item.Pinned ? Visibility.Visible : Visibility.Collapsed;
    public Visibility EditedVisibility => !Item.IsRecording && File.Exists(Item.OrigPath) ? Visibility.Visible : Visibility.Collapsed;
    public Visibility KindVisibility => Item.IsRecording ? Visibility.Visible : Visibility.Collapsed;
    public Visibility PlayVisibility => Item.Kind == "video" && Thumb != null ? Visibility.Visible : Visibility.Collapsed;
    public Visibility NoThumbVisibility => Thumb == null || Item.Kind == "audio" ? Visibility.Visible : Visibility.Collapsed;
    public string KindGlyph => Item.Kind == "audio" ? "\uE8D6" : Item.Kind == "video" ? "\uE714" : "\uEB9F";
    public string KindText => (Item.Kind == "video" ? "VIDEO" : "AUDIO") + "  " + Recorder.Clock(TimeSpan.FromSeconds(Item.Duration));
}

public partial class MainWindow : Window
{
    readonly ObservableCollection<CaptureCard> cards = [];
    BobertoDockControl? bobertoDock;
    BobertoStageControl? emptyBoberto;
    bool companionAllowed, shellMotion;
    AppearanceSettings appearance => App.Current.Appearance;

    public MainWindow()
    {
        InitializeComponent();
        Library.ItemsSource = cards;
        CopyToggle.IsChecked = Store.Settings.CopyAfterCapture;
        EditorToggle.IsChecked = Store.Settings.OpenEditorAfterCapture;
        KeepOriginalToggle.IsChecked = Store.Settings.EditKeepOriginal;
        PasteToggle.IsChecked = Store.Settings.PasteOnPick;
        StartupToggle.IsChecked = App.StartupEnabled;
        SystemAudioToggle.IsChecked = Store.Settings.RecordSystemAudio;
        MicToggle.IsChecked = Store.Settings.RecordMicrophone;
        CursorToggle.IsChecked = Store.Settings.RecordCursor;
        LangEnMenu.IsChecked = !L.Es;
        LangEsMenu.IsChecked = L.Es;
        ShowFolders();
        App.Current.RecordingChanged += OnRecordingChanged;

        App.Current.AppearanceChanged += OnAppearance;
        ContentRendered += (_, _) => { if (companionAllowed) return; companionAllowed = true; SyncBobertoDock(appearance); };
        StateChanged += (_, _) => _ = bobertoDock?.PauseAsync(WindowState == WindowState.Minimized || !appearance.BobertoVisible);
        SizeChanged += (_, _) => AdaptHeader();
        HeaderLeft.SizeChanged += (_, _) => SizeBobertoBand();
        Closing += (_, e) => { if (!App.Current.Exiting && !App.IsShot) { e.Cancel = true; Hide(); } };
        Closed += (_, _) => { emptyBoberto?.Dispose(); emptyBoberto = null; if (bobertoDock != null) { BobertoHost.Content = null; bobertoDock.Dispose(); bobertoDock = null; } };
        Store.Changed += (_, _) => Dispatcher.BeginInvoke(Reload);
        PreviewKeyDown += OnKey;

        OnAppearance(appearance);
        RefreshShortcuts();
        Reload();
    }

    // ================================================================== appearance (as DBMON's MainWindow.Appearance)
    void OnAppearance(AppearanceSettings settings)
    {
        ShowBobertoMenu.IsChecked = settings.BobertoVisible;
        BobertoPanel.Visibility = settings.BobertoVisible ? Visibility.Visible : Visibility.Collapsed;
        ShowOptionsMenu.IsChecked = settings.ShowMessages;
        OptionsPanel.Visibility = settings.ShowMessages ? Visibility.Visible : Visibility.Collapsed;
        OptionsColumn.Width = new GridLength(settings.ShowMessages ? 320 : 0);
        AdaptHeader();
        SyncBobertoDock(settings);
        ApplyShellMotion(settings);
        Reload();
    }

    void SyncBobertoDock(AppearanceSettings settings)
    {
        bool pause = !settings.BobertoVisible || WindowState == WindowState.Minimized;
        if (bobertoDock != null) { _ = bobertoDock.ApplyThemeAsync(settings); if (bobertoDock.IsPaused != pause) _ = bobertoDock.PauseAsync(pause); return; }
        if (!companionAllowed || !settings.BobertoVisible) return;
        bobertoDock = new BobertoDockControl(settings, null);
        bobertoDock.StudioRequested += () => Dispatcher.BeginInvoke(() => App.Current.OpenPersonalization(this, true));
        BobertoHost.Content = bobertoDock;
        if (pause) _ = bobertoDock.PauseAsync(true);
    }

    public void EnsureBobertoLive() => _ = bobertoDock?.EnsureLiveAsync();

    void AdaptHeader()
    {
        bool shortWindow = ActualHeight is > 0 and < 720;
        Resources["TopMenuGlyph"] = ActualWidth is > 0 and < 1180 ? Visibility.Collapsed : Visibility.Visible;
        bool narrow = ActualWidth is > 0 and < 1180;
        var labels = narrow ? Visibility.Collapsed : Visibility.Visible;
        QuickText.Visibility = EditorText.Visibility = FolderText.Visibility = PersonalizeText.Visibility = CaptureKeyHint.Visibility = labels;
        Subtitle.Visibility = ActualWidth is > 0 and < 1320 ? Visibility.Collapsed : Visibility.Visible;
        SearchBox.Width = narrow ? 200 : 300;
        HeaderBar.Padding = shortWindow ? new Thickness(20, 6, 16, 6) : new Thickness(20, 10, 16, 10);
        MainMenu.Padding = shortWindow ? new Thickness(16, 4, 16, 0) : new Thickness(16, 8, 16, 0);
        SizeBobertoBand();
    }

    const double BobertoStrip = 35, BobertoMinWindow = 900, BobertoMaxShare = 0.30;
    void SizeBobertoBand()
    {
        double width = BobertoBandWidth();
        BobertoColumn.Width = new GridLength(width);
        BobertoPanel.Width = width;
        BobertoPanel.Height = Math.Max(0, HeaderLeft.ActualHeight);
    }
    double BobertoBandWidth()
    {
        if (!appearance.BobertoVisible || ActualWidth is > 0 and < BobertoMinWindow) return 0;
        double height = HeaderLeft.ActualHeight > 0 ? HeaderLeft.ActualHeight : 150;
        double width = Math.Min(Math.Round(height * 16 / 9) + BobertoStrip, ActualWidth > 0 ? Math.Round(ActualWidth * BobertoMaxShare) : double.MaxValue);
        return Math.Abs(BobertoColumn.Width.Value - width) <= 2 ? BobertoColumn.Width.Value : width;
    }

    // DBMON's living details: the topbar edge sweeps every 7 s and the brand dot pulses; both still when motion is off.
    void ApplyShellMotion(AppearanceSettings settings)
    {
        var sweep = (Color)Application.Current.Resources["EdgeSweepColor"];
        var brush = new LinearGradientBrush { StartPoint = new Point(0, 0), EndPoint = new Point(1, 0) };
        foreach (var (offset, alpha) in new[] { (0d, 0), (.37, 0), (.5, (int)sweep.A), (.63, 0), (1d, 0) }) brush.GradientStops.Add(new GradientStop(Color.FromArgb((byte)alpha, sweep.R, sweep.G, sweep.B), offset));
        var travel = new TranslateTransform(-.65, 0); brush.RelativeTransform = travel;
        bool motion = settings.BobertoMotion && SystemParameters.ClientAreaAnimation;
        EdgeSweep.Fill = motion ? brush : null;
        if (motion) travel.BeginAnimation(TranslateTransform.XProperty, new DoubleAnimation(-.65, .65, TimeSpan.FromSeconds(7)) { RepeatBehavior = RepeatBehavior.Forever });
        BrandDot.BeginAnimation(OpacityProperty, motion ? new DoubleAnimation(1, .45, TimeSpan.FromSeconds(1.2)) { AutoReverse = true, RepeatBehavior = RepeatBehavior.Forever } : null);
        BuildWordmark(motion);
        shellMotion = motion;
    }

    // DBMON's .logo-wave: mono bold letters in dim ink; each one lights up with the accent at 30% of a 3 s cycle, 0.18 s apart.
    public const string WordmarkText = "BUZZPRINT";
    void BuildWordmark(bool motion)
    {
        Wordmark.Children.Clear();
        BrandMark.Animated = motion;   // eyes follow the cursor and blink; still with animations off
        var dim = ((SolidColorBrush)Application.Current.Resources["Muted"]).Color;
        var accent = ((SolidColorBrush)Application.Current.Resources["Accent"]).Color;
        for (int i = 0; i < WordmarkText.Length; i++)
        {
            var ink = new SolidColorBrush(dim);
            var letter = new TextBlock { Text = WordmarkText[i].ToString(), FontWeight = FontWeights.Bold, FontSize = 18, Foreground = ink, Margin = new Thickness(0, 0, 1, 0) };
            letter.SetResourceReference(TextBlock.FontFamilyProperty, "MonoFont");
            Wordmark.Children.Add(letter);
            if (!motion) continue;
            var wave = new ColorAnimationUsingKeyFrames { Duration = TimeSpan.FromSeconds(3), RepeatBehavior = RepeatBehavior.Forever, BeginTime = TimeSpan.FromSeconds(i * .18) };
            var ease = new KeySpline(.42, 0, .58, 1);
            wave.KeyFrames.Add(new LinearColorKeyFrame(dim, KeyTime.FromPercent(0)));
            wave.KeyFrames.Add(new SplineColorKeyFrame(accent, KeyTime.FromPercent(.3), ease));
            wave.KeyFrames.Add(new SplineColorKeyFrame(dim, KeyTime.FromPercent(1), ease));
            ink.BeginAnimation(SolidColorBrush.ColorProperty, wave);
        }
    }

    void WordmarkClick(object sender, MouseButtonEventArgs e)
    {
        if (!shellMotion) return;
        var shift = Wordmark.RenderTransform as TranslateTransform ?? new TranslateTransform(); Wordmark.RenderTransform = shift;
        var glitch = new DoubleAnimationUsingKeyFrames { Duration = TimeSpan.FromSeconds(.5) };
        foreach (var (x, at) in new[] { (0d, 0d), (-2d, .1), (2d, .2), (-1d, .3), (1d, .4), (0d, .5) }) glitch.KeyFrames.Add(new DiscreteDoubleKeyFrame(x, KeyTime.FromTimeSpan(TimeSpan.FromSeconds(at))));
        shift.BeginAnimation(TranslateTransform.XProperty, glitch);
    }

    void PersonalizeClick(object sender, RoutedEventArgs e) => App.Current.OpenPersonalization(this);
    void BobertoClick(object sender, RoutedEventArgs e) => App.Current.OpenPersonalization(this, true);
    void BobertoHideClick(object sender, RoutedEventArgs e) { App.Current.ApplyAppearance(appearance with { BobertoVisible = false }); App.Current.SaveAppearance(); }
    void BobertoToggleClick(object sender, RoutedEventArgs e) { App.Current.ApplyAppearance(appearance with { BobertoVisible = ShowBobertoMenu.IsChecked }); App.Current.SaveAppearance(); }
    void OptionsToggleClick(object sender, RoutedEventArgs e) { App.Current.ApplyAppearance(appearance with { ShowMessages = ShowOptionsMenu.IsChecked }); App.Current.SaveAppearance(); }

    // ================================================================== library
    public void RefreshShortcuts()
    {
        var st = Store.Settings;
        string cap = App.Current.CaptureHotkeyText.Length > 0 ? App.Current.CaptureHotkeyText : st.HotkeyCapture;
        string Key(string k) => k.Replace("PrintScreen", "PrtSc").Replace("+", " ");
        ShortcutList.ItemsSource = new[]
        {
            new { Key = Key(cap), Text = L.T("Región o ventana") },
            new { Key = Key(st.HotkeyFull), Text = L.T("Pantalla completa") },
            new { Key = Key(st.HotkeyRecord), Text = L.T("Grabar / detener video") },
            new { Key = Key(st.HotkeyPick), Text = L.T("Menú rápido") },
            new { Key = L.T("Doble clic"), Text = L.T("Editar y anotar") },
            new { Key = L.T("Arrastrar"), Text = L.T("Soltar en otra app") },
        };
        RecordKeyHint.Text = Key(st.HotkeyRecord);
        StatusRecordKey.Text = Key(st.HotkeyRecord);
        MenuRecord.InputGestureText = Key(st.HotkeyRecord);
        CaptureKeyHint.Text = Key(cap);
        StatusCaptureKey.Text = Key(cap);
        StatusPickKey.Text = Key(st.HotkeyPick);
        MenuCapture.InputGestureText = Key(cap);
        MenuFull.InputGestureText = Key(st.HotkeyFull);
        MenuDelayed.InputGestureText = st.DelaySeconds + " s";
        MenuQuick.InputGestureText = Key(st.HotkeyPick);
        emptyCaptureHint = L.F("Apretá {0} y arrastrá una región o hacé clic en una ventana.", Key(cap));
        emptyRecordHint = L.F("Apretá {0} para grabar video, o elegí Grabación → Grabar solo audio.", Key(st.HotkeyRecord));
        EmptyHint.Text = Filters.SelectedIndex == 4 ? emptyRecordHint : emptyCaptureHint;
    }

    void Reload()
    {
        if (Library == null) return;
        var keep = new HashSet<string>(Selected.Select(x => x.Id));
        var view = System.Windows.Data.CollectionViewSource.GetDefaultView(cards);
        view.GroupDescriptions.Clear();
        string q = Search.Text.Trim().ToLowerInvariant();
        bool recordings = Filters.SelectedIndex == 4;
        var all = recordings ? Store.Recordings() : Store.All();
        IEnumerable<CaptureItem> shown = all;
        switch (Filters.SelectedIndex)
        {
            case 1: shown = shown.Where(x => x.Created.Date == DateTime.Today); break;
            case 2: shown = shown.Where(x => x.Pinned); break;
            case 3: shown = shown.Where(x => File.Exists(x.OrigPath)); break;
        }
        var tabList = shown.ToList();
        FillSubFilters(tabList);
        shown = tabList.Where(x => DayMatches(x.Created, dayKey));
        if (sectionKey == "none") shown = shown.Where(x => x.Section.Length == 0);
        else if (sectionKey != "all") shown = shown.Where(x => x.Section == sectionKey);
        if (q.Length > 0) shown = shown.Where(it => it.Display.ToLowerInvariant().Contains(q) || it.Source.ToLowerInvariant().Contains(q) || it.Section.ToLowerInvariant().Contains(q) || it.Id.Contains(q));
        var list = Arrange(shown).ToList();
        bool compact = appearance.Compact;
        double w = compact ? 210 : 260, th = compact ? 120 : 150;
        CtxCopy.Header = L.T(recordings ? "Copiar archivo" : "Copiar imagen");
        CtxExport.Visibility = CtxStitch.Visibility = recordings ? Visibility.Collapsed : Visibility.Visible;
        CtxEdit.Header = L.T(recordings ? "Reproducir" : "Editar y anotar");
        cards.Clear();
        foreach (var it in list)
        {
            cards.Add(new CaptureCard
            {
                Item = it,
                Thumb = it.IsRecording ? (File.Exists(it.ThumbPath) ? Imaging.Thumb(it.ThumbPath, 480) : null) : Imaging.Thumb(it.Path, 480),
                Title = string.IsNullOrEmpty(it.Title) ? (string.IsNullOrEmpty(it.Source) ? L.T(it.IsRecording ? "Grabación" : "Captura") : it.Source) : it.Title,
                Meta = it.Created.ToString("dd/MM HH:mm") + (it.Width > 0 ? "  ·  " + it.Width + "×" + it.Height : ""),
                CardWidth = w, ThumbHeight = th,
                Group = GroupOf(it)
            });
        }
        // Grouping on: group headers. Off: no GroupStyle at all, or WPF keeps the per-group layout (one card per row).
        if (groupStyle == null && Library.GroupStyle.Count > 0) groupStyle = Library.GroupStyle[0];
        if (groupKey != "none")
        {
            if (Library.GroupStyle.Count == 0 && groupStyle != null) Library.GroupStyle.Add(groupStyle);
            view.GroupDescriptions.Add(new System.Windows.Data.PropertyGroupDescription(nameof(CaptureCard.Group)));
        }
        else Library.GroupStyle.Clear();
        foreach (var c in cards) if (keep.Contains(c.Item.Id)) Library.SelectedItems.Add(c);
        EmptyState.Visibility = all.Count == 0 ? Visibility.Visible : Visibility.Hidden;   // Hidden keeps its size, so the stage Boberto loads in the background
        EmptyTitle.Text = L.T(recordings ? "Todavía no hay grabaciones" : "Todavía no hay capturas");
        EmptyHint.Text = recordings ? emptyRecordHint : emptyCaptureHint;
        SyncEmptyBoberto(all.Count == 0);
        CountText.Text = list.Count == all.Count ? (recordings ? L.Count(all.Count, "GRABACIÓN", "GRABACIONES") : L.Count(all.Count, "CAPTURA", "CAPTURAS")) : L.F("{0} DE {1}", list.Count, all.Count);
        UpdateStatus();
    }

    // ================================================================== day, section and grouping
    string dayKey = "any", sectionKey = "all", groupKey = Store.Settings.LibraryGroupBy;
    internal void ShotView(string group, string section = "all") { groupKey = group; sectionKey = section; Reload(); }
    bool fillingFilters;
    GroupStyle? groupStyle;

    static bool DayMatches(DateTime when, string key)
    {
        var d = when.Date; var today = DateTime.Today;
        return key switch
        {
            "any" => true,
            "today" => d == today,
            "yesterday" => d == today.AddDays(-1),
            "week" => d > today.AddDays(-7),
            "month" => d.Year == today.Year && d.Month == today.Month,
            _ => key.StartsWith("d:") && d.ToString("yyyyMMdd") == key[2..]
        };
    }

    static string DayLabel(DateTime day)
    {
        if (day == DateTime.Today) return L.T("Hoy") + " · " + day.ToString("dd/MM/yyyy");
        if (day == DateTime.Today.AddDays(-1)) return L.T("Ayer") + " · " + day.ToString("dd/MM/yyyy");
        string t = day.ToString("dddd dd/MM/yyyy");
        return char.ToUpper(t[0]) + t[1..];
    }

    string GroupOf(CaptureItem it) => groupKey switch
    {
        "day" => DayLabel(it.Created.Date),
        "section" => it.Section.Length > 0 ? it.Section : L.T("Sin sección"),
        "source" => string.IsNullOrWhiteSpace(it.Source) ? L.T("Sin origen") : it.Source,
        _ => ""
    };

    /// <summary>Order cards so groups come out in a sensible order: newest day first, sections in their own order
    /// (no section last), sources alphabetically. Without grouping, the library order (pinned first) stays.</summary>
    IEnumerable<CaptureItem> Arrange(IEnumerable<CaptureItem> items)
    {
        switch (groupKey)
        {
            case "day": return items.OrderByDescending(x => x.Created);
            case "section":
                var order = Store.SectionNames();
                return items.OrderBy(x => x.Section.Length == 0 ? int.MaxValue : order.IndexOf(x.Section)).ThenByDescending(x => x.Pinned).ThenByDescending(x => x.Created);
            case "source": return items.OrderBy(x => string.IsNullOrWhiteSpace(x.Source) ? "\uffff" : x.Source, StringComparer.CurrentCultureIgnoreCase).ThenByDescending(x => x.Created);
            default: return items;
        }
    }

    /// <summary>Day and section choices come from the captures in the current tab, with counts; the choice is kept
    /// while it still exists.</summary>
    void FillSubFilters(List<CaptureItem> items)
    {
        fillingFilters = true;
        try
        {
            var days = new List<(string, string)> { ("any", L.T("Cualquier día")) };
            void Add(string key, string text) { int n = items.Count(x => DayMatches(x.Created, key)); if (n > 0) days.Add((key, text + "  (" + n + ")")); }
            Add("today", L.T("Hoy")); Add("yesterday", L.T("Ayer")); Add("week", L.T("Últimos 7 días")); Add("month", L.T("Este mes"));
            foreach (var g in items.GroupBy(x => x.Created.Date).OrderByDescending(g => g.Key))
                days.Add(("d:" + g.Key.ToString("yyyyMMdd"), DayLabel(g.Key) + "  (" + g.Count() + ")"));
            dayKey = Fill(DayFilter, days, dayKey);

            var names = Store.SectionNames();
            var sections = new List<(string, string)> { ("all", L.T("Todas las secciones")) };
            foreach (var name in names) sections.Add((name, name + "  (" + items.Count(x => x.Section == name) + ")"));
            if (names.Count > 0) sections.Add(("none", L.T("Sin sección") + "  (" + items.Count(x => x.Section.Length == 0) + ")"));
            sectionKey = Fill(SectionFilter, sections, sectionKey);

            groupKey = Fill(GroupByBox, new List<(string, string)> { ("none", L.T("Sin agrupar")), ("day", L.T("Día")), ("section", L.T("Sección")), ("source", L.T("Origen")) }, groupKey);
        }
        finally { fillingFilters = false; }
    }

    static string Fill(ComboBox box, List<(string key, string text)> options, string selected)
    {
        if (!options.Any(o => o.key == selected)) selected = options[0].key;
        box.Items.Clear();
        foreach (var (key, text) in options)
        {
            var item = new ComboBoxItem { Content = text, Tag = key };
            box.Items.Add(item);
            if (key == selected) box.SelectedItem = item;
        }
        return selected;
    }

    void SubFilterChanged(object sender, SelectionChangedEventArgs e)
    {
        if (fillingFilters || sender is not ComboBox box || box.SelectedItem is not ComboBoxItem { Tag: string key }) return;
        if (box == DayFilter) dayKey = key;
        else if (box == SectionFilter) sectionKey = key;
        else { groupKey = key; Store.Settings.LibraryGroupBy = key; Store.SaveSettings(); }
        Reload();
    }

    // Right-click > Move to section: existing sections, a new one, or none.
    void LibraryMenuOpening(object sender, ContextMenuEventArgs e)
    {
        CtxSection.Items.Clear();
        var sel = Selected;
        CtxSection.IsEnabled = sel.Count > 0;
        foreach (var name in Store.SectionNames())
        {
            var m = new MenuItem { Header = name, IsChecked = sel.Count > 0 && sel.All(x => x.Section == name) };
            m.Click += (_, _) => { Store.SetSection(Selected, name); StatusText.Text = L.F("Movidas a «{0}»", name); };
            CtxSection.Items.Add(m);
        }
        if (CtxSection.Items.Count > 0) CtxSection.Items.Add(new Separator());
        var add = new MenuItem { Header = L.T("Nueva sección…") };
        add.Click += (_, _) => NewSectionClick(this, new RoutedEventArgs());
        CtxSection.Items.Add(add);
        var clear = new MenuItem { Header = L.T("Quitar de la sección"), IsEnabled = sel.Any(x => x.Section.Length > 0) };
        clear.Click += (_, _) => Store.SetSection(Selected, "");
        CtxSection.Items.Add(clear);
    }

    /// <summary>Creates a section; the selected captures (if any) move into it.</summary>
    void NewSectionClick(object sender, RoutedEventArgs e)
    {
        var sel = Selected;
        var name = PromptWindow.Ask(this, L.T("Nueva sección"), sel.Count > 0 ? L.F("Las {0} capturas elegidas pasan a esta sección.", sel.Count) : L.T("Un nombre para agrupar capturas a tu gusto: un proyecto, un cliente, un tema."), "");
        if (string.IsNullOrWhiteSpace(name)) return;
        name = name.Trim();
        if (!Store.AddSection(name) && sel.Count == 0) { StatusText.Text = L.F("La sección «{0}» ya existe", name); return; }
        if (sel.Count > 0) Store.SetSection(sel, name); else Reload();
        StatusText.Text = L.F("Sección «{0}» lista", name);
    }

    internal void RenameSection(string from)
    {
        var to = PromptWindow.Ask(this, L.T("Renombrar sección"), L.T("Las capturas de la sección se mueven al nombre nuevo."), from);
        if (string.IsNullOrWhiteSpace(to) || to.Trim() == from) return;
        Store.RenameSection(from, to.Trim());
        if (sectionKey == from) sectionKey = to.Trim();
        Reload();
        StatusText.Text = L.F("Sección «{0}» renombrada a «{1}»", from, to.Trim());
    }

    internal void DeleteSection(string name, bool ask = true)
    {
        int n = Store.All().Concat(Store.Recordings()).Count(x => x.Section == name);
        if (ask && !PromptWindow.Confirm(this, L.T("Eliminar sección"), L.F("Se elimina la sección «{0}». Sus capturas quedan en la biblioteca, sin sección.", name) + (n > 0 ? " (" + L.Count(n, "captura", "capturas") + ")" : ""), L.T("Eliminar"))) return;
        Store.DeleteSection(name);
        if (sectionKey == name) sectionKey = "all";
        Reload();
        StatusText.Text = L.F("Sección «{0}» eliminada", name);
    }

    void SelectSection(string name)
    {
        Library.SelectedItems.Clear();
        foreach (var c in cards.Where(c => c.Item.Section == name)) Library.SelectedItems.Add(c);
        Library.Focus();
    }

    /// <summary>Each section with Rename / Delete / Select, then "New section…". Used by the Sections button,
    /// the Library menu and the section group headers.</summary>
    void FillSectionsMenu(ItemsControl menu)
    {
        menu.Items.Clear();
        var names = Store.SectionNames();
        MenuItem Item(string header, Action click) { var m = new MenuItem { Header = header }; m.Click += (_, _) => click(); return m; }
        foreach (var name in names)
        {
            int n = Store.All().Concat(Store.Recordings()).Count(x => x.Section == name);
            var sub = new MenuItem { Header = name, InputGestureText = n.ToString() };
            sub.Items.Add(Item(L.T("Ver solo esta sección"), () => { sectionKey = name; Reload(); }));
            sub.Items.Add(Item(L.T("Seleccionar sus capturas"), () => SelectSection(name)));
            sub.Items.Add(new Separator());
            sub.Items.Add(Item(L.T("Renombrar…"), () => RenameSection(name)));
            var del = Item(L.T("Eliminar…"), () => DeleteSection(name));
            del.SetResourceReference(ForegroundProperty, "ErrorInk");
            sub.Items.Add(del);
            menu.Items.Add(sub);
        }
        if (names.Count == 0) menu.Items.Add(new MenuItem { Header = L.T("Todavía no hay secciones"), IsEnabled = false });
        menu.Items.Add(new Separator());
        menu.Items.Add(Item(L.T("Nueva sección…"), () => NewSectionClick(this, new RoutedEventArgs())));
    }

    void SectionsButtonClick(object sender, RoutedEventArgs e)
    {
        var menu = new ContextMenu { PlacementTarget = (UIElement)sender, Placement = System.Windows.Controls.Primitives.PlacementMode.Bottom };
        FillSectionsMenu(menu);
        menu.IsOpen = true;
    }

    void SectionsMenuOpened(object sender, RoutedEventArgs e) { if (e.OriginalSource == sender) FillSectionsMenu((MenuItem)sender); }

    // Right-click on a group header (grouped by section): that section's actions.
    void GroupHeaderMenu(object sender, ContextMenuEventArgs e)
    {
        if (groupKey != "section" || sender is not FrameworkElement { DataContext: System.Windows.Data.CollectionViewGroup g } el || g.Name is not string name || name == L.T("Sin sección")) { e.Handled = true; return; }
        var menu = new ContextMenu();
        MenuItem Item(string header, Action click) { var m = new MenuItem { Header = header }; m.Click += (_, _) => click(); return m; }
        menu.Items.Add(Item(L.T("Seleccionar sus capturas"), () => SelectSection(name)));
        menu.Items.Add(Item(L.T("Renombrar…"), () => RenameSection(name)));
        var del = Item(L.T("Eliminar…"), () => DeleteSection(name)); del.SetResourceReference(ForegroundProperty, "ErrorInk"); menu.Items.Add(del);
        el.ContextMenu = menu;
    }

    // The empty library hosts a second, live Boberto (the engine's peana) with the same Studio look and gestures.
    // It is created once and kept alive while hidden, so emptying the library shows the right look at once (no reload,
    // no default-look flash); it waves when it reappears.
    void SyncEmptyBoberto(bool empty)
    {
        if (appearance.BobertoVisible && emptyBoberto == null) { emptyBoberto = new BobertoStageControl(appearance); EmptyBoberto.Content = emptyBoberto; }
        else if (!appearance.BobertoVisible && emptyBoberto != null) { EmptyBoberto.Content = null; emptyBoberto.Dispose(); emptyBoberto = null; }
        if (emptyBoberto == null) return;
        _ = emptyBoberto.ApplyThemeAsync(appearance);
        if (empty && !wasEmpty) emptyBoberto.Wave();
        wasEmpty = empty;
    }
    bool wasEmpty;
    internal BobertoStageControl? EmptyStage => emptyBoberto;

    string emptyCaptureHint = "", emptyRecordHint = "";

    void UpdateStatus()
    {
        var all = Store.All();
        long bytes = 0;
        foreach (var it in all) try { bytes += new FileInfo(it.Path).Length; } catch { }
        var recs = Store.Recordings();
        foreach (var it in recs) try { bytes += new FileInfo(it.Path).Length; } catch { }
        StatusText.Text = (Recorder.Current != null ? "● REC " + L.T("grabando") : L.T("Listo")) + " · " + L.Count(all.Count, "captura", "capturas") + " · " + L.Count(recs.Count, "grabación", "grabaciones") + " · " + (bytes / 1024d / 1024d).ToString("0.0") + " MB";
        var sel = Selected;
        SelectionText.Text = sel.Count == 1 ? sel[0].Display + (sel[0].Width > 0 ? " · " + sel[0].Width + "×" + sel[0].Height : "") : sel.Count > 1 ? L.F("{0} seleccionadas", sel.Count) + " · " + L.T("Ctrl+E exporta, Ctrl+Shift+C las une") : "";
    }

    List<CaptureItem> Selected => Library.SelectedItems.Cast<CaptureCard>().Select(c => c.Item).ToList();

    void SearchChanged(object sender, TextChangedEventArgs e) => Reload();
    void FilterChanged(object sender, SelectionChangedEventArgs e) { if (e.Source == Filters) Reload(); }
    void LibrarySelectionChanged(object sender, SelectionChangedEventArgs e) => UpdateStatus();
    void LibraryDoubleClick(object sender, MouseButtonEventArgs e) { if (ItemUnder(e) != null) EditClick(sender, e); }

    CaptureCard? ItemUnder(MouseEventArgs e)
    {
        var d = e.OriginalSource as DependencyObject;
        while (d != null && d is not ListBoxItem) d = VisualTreeHelper.GetParent(d);
        return (d as ListBoxItem)?.DataContext as CaptureCard;
    }

    Point dragStart; bool maybeDrag;
    void LibraryMouseDown(object sender, MouseButtonEventArgs e) { dragStart = e.GetPosition(Library); maybeDrag = ItemUnder(e) != null; }
    void LibraryMouseMove(object sender, MouseEventArgs e)
    {
        if (!maybeDrag || e.LeftButton != MouseButtonState.Pressed) return;
        var p = e.GetPosition(Library);
        if (Math.Abs(p.X - dragStart.X) < 6 && Math.Abs(p.Y - dragStart.Y) < 6) return;
        maybeDrag = false;
        var files = Selected.Select(x => x.Path).ToArray();
        if (files.Length > 0) DragDrop.DoDragDrop(Library, new DataObject(DataFormats.FileDrop, files), DragDropEffects.Copy);
    }

    void OnKey(object sender, KeyEventArgs e)
    {
        if (Search.IsKeyboardFocusWithin)
        {
            if (e.Key == Key.Escape) { Search.Text = ""; Library.Focus(); e.Handled = true; }
            else if (e.Key == Key.Down) { Library.Focus(); if (Library.Items.Count > 0 && Library.SelectedIndex < 0) Library.SelectedIndex = 0; e.Handled = true; }
            return;
        }
        bool ctrl = (Keyboard.Modifiers & ModifierKeys.Control) != 0;
        bool shift = (Keyboard.Modifiers & ModifierKeys.Shift) != 0;
        if (ctrl && shift && e.Key == Key.C) { CopyStitchedClick(this, e); e.Handled = true; }
        else if (ctrl && e.Key == Key.C) { CopyClick(this, e); e.Handled = true; }
        else if (ctrl && e.Key == Key.E) { ExportClick(this, e); e.Handled = true; }
        else if (ctrl && e.Key == Key.F) { FocusSearchClick(this, e); e.Handled = true; }
        else if (ctrl && e.Key == Key.P) { PinClick(this, e); e.Handled = true; }
        else if (e.Key == Key.Delete) { DeleteClick(this, e); e.Handled = true; }
        else if (e.Key == Key.F2) { RenameClick(this, e); e.Handled = true; }
        else if (e.Key == Key.Enter) { EditClick(this, e); e.Handled = true; }
    }

    // ================================================================== commands
    void CaptureClick(object sender, RoutedEventArgs e) => App.Current.Capture(CaptureMode.Region, this);
    void FullClick(object sender, RoutedEventArgs e) => App.Current.Capture(CaptureMode.Full, this);
    void DelayedClick(object sender, RoutedEventArgs e) => App.Current.Capture(CaptureMode.Delayed, this);
    void QuickClick(object sender, RoutedEventArgs e) => QuickPickWindow.Toggle(IntPtr.Zero);
    void ExitClick(object sender, RoutedEventArgs e) => App.Current.ExitApp();
    void CaptureMenuClick(object sender, RoutedEventArgs e)
    {
        var menu = new ContextMenu { PlacementTarget = CaptureMenuButton, Placement = System.Windows.Controls.Primitives.PlacementMode.Bottom };
        void Add(string h, string g, RoutedEventHandler click) { var m = new MenuItem { Header = h, InputGestureText = g }; m.Click += click; menu.Items.Add(m); }
        Add(L.T("Región o ventana"), MenuCapture.InputGestureText, CaptureClick);
        Add(L.T("Pantalla completa"), MenuFull.InputGestureText, FullClick);
        Add(L.T("Con demora"), MenuDelayed.InputGestureText, DelayedClick);
        menu.IsOpen = true;
    }

    // ================================================================== recording
    void RecordVideoClick(object sender, RoutedEventArgs e) { if (App.Current.IsRecording) App.Current.StopRecording(true); else App.Current.StartRecording(RecordKind.Video, this); }
    void RecordAudioClick(object sender, RoutedEventArgs e) { if (!App.Current.IsRecording) App.Current.StartRecording(RecordKind.Audio, null); }
    void StopRecordClick(object sender, RoutedEventArgs e) => App.Current.StopRecording(true);
    void ShowRecordingsClick(object sender, RoutedEventArgs e) => ShowRecordings();
    void RecordFolderClick(object sender, RoutedEventArgs e) => Process.Start("explorer.exe", "\"" + Store.RecordDir + "\"");
    void RecordMenuClick(object sender, RoutedEventArgs e)
    {
        var menu = new ContextMenu { PlacementTarget = RecordMenuButton, Placement = System.Windows.Controls.Primitives.PlacementMode.Bottom };
        void Add(string h, string g, RoutedEventHandler click, bool enabled = true) { var m = new MenuItem { Header = h, InputGestureText = g, IsEnabled = enabled }; m.Click += click; menu.Items.Add(m); }
        bool rec = App.Current.IsRecording;
        Add(L.T("Grabar video"), MenuRecord.InputGestureText, RecordVideoClick, !rec);
        Add(L.T("Grabar solo audio"), "", RecordAudioClick, !rec);
        Add(L.T("Detener grabación"), "", StopRecordClick, rec);
        menu.Items.Add(new Separator());
        Add(L.T("Ver grabaciones"), "", ShowRecordingsClick);
        menu.IsOpen = true;
    }
    public void ShowRecordings() { if (Filters.SelectedIndex != 4) Filters.SelectedIndex = 4; else Reload(); }
    void OnRecordingChanged()
    {
        bool rec = App.Current.IsRecording;
        RecordText.Text = L.T(rec ? "Detener" : "Grabar");
        RecordDot.Visibility = rec ? Visibility.Collapsed : Visibility.Visible;
        RecordStopGlyph.Visibility = rec ? Visibility.Visible : Visibility.Collapsed;
        StatusRecordText.Text = L.T(rec ? "detener" : "grabar");
        MenuRecord.IsEnabled = MenuRecordAudio.IsEnabled = !rec;
        MenuStopRecord.IsEnabled = rec;
        UpdateStatus();
    }

    // ================================================================== folders and language
    void ShowFolders()
    {
        StoragePath.Text = Store.CaptureDir;
        RecordPath.Text = Store.RecordDir;
        FolderStatus.Text = Shorten(Store.CaptureDir);
    }
    static string Shorten(string path)
    {
        var parts = path.TrimEnd('\\').Split('\\');
        return parts.Length <= 2 ? path : parts[^2] + "\\" + parts[^1];
    }
    void ChangeCaptureFolderClick(object sender, RoutedEventArgs e) => ChangeFolder(false);
    void ChangeRecordFolderClick(object sender, RoutedEventArgs e) => ChangeFolder(true);
    void ChangeFolder(bool recordings)
    {
        var dlg = new Microsoft.Win32.OpenFolderDialog
        {
            Title = L.T(recordings ? "Carpeta para videos y audio" : "Carpeta para las capturas"),
            InitialDirectory = recordings ? Store.RecordDir : Store.CaptureDir
        };
        if (dlg.ShowDialog(this) != true) return;
        string current = recordings ? Store.RecordDir : Store.CaptureDir;
        if (string.Equals(Path.GetFullPath(dlg.FolderName).TrimEnd('\\'), current.TrimEnd('\\'), StringComparison.OrdinalIgnoreCase)) return;
        int count = recordings ? Store.Recordings().Count : Store.All().Count;
        bool move = count > 0 && PromptWindow.Confirm(this, L.T("Cambiar carpeta"),
            L.F("¿Mover las {0} existentes a la carpeta nueva? Si elegís No, quedan donde están y la biblioteca muestra la carpeta nueva.", L.Count(count, recordings ? "grabación" : "captura", recordings ? "grabaciones" : "capturas")), L.T("Mover"), L.T("No, solo cambiar"));
        int moved = Store.ChangeFolder(recordings, dlg.FolderName, move);
        ShowFolders();
        StatusText.Text = L.T("Carpeta actualizada") + (move ? " · " + L.F("{0} archivos movidos", moved) : "");
    }
    void LangEnClick(object sender, RoutedEventArgs e) => SetLanguage("en");
    void LangEsClick(object sender, RoutedEventArgs e) => SetLanguage("es");
    void SetLanguage(string lang)
    {
        LangEnMenu.IsChecked = !L.Es;
        LangEsMenu.IsChecked = L.Es;
        if (lang == L.Lang) return;
        App.Current.SetLanguage(lang);
    }

    void CopyClick(object sender, RoutedEventArgs e)
    {
        var it = Selected.FirstOrDefault();
        if (it != null && Store.CopyToClipboard(it)) StatusText.Text = L.T(it.IsRecording ? "Archivo copiado" : "Copiada al portapapeles") + " · " + it.Display;
    }
    void CopyFileClick(object sender, RoutedEventArgs e) { var it = Selected.FirstOrDefault(); if (it != null) { Store.CopyFileToClipboard(it); StatusText.Text = L.T("Archivo copiado") + " · " + it.Path; } }
    void EditClick(object sender, RoutedEventArgs e)
    {
        var it = Selected.FirstOrDefault();
        if (it != null && it.IsRecording) { App.OpenRecording(it); return; }
        EditorWindow.Open(it ?? Store.All().FirstOrDefault());
    }
    void PinClick(object sender, RoutedEventArgs e) { foreach (var it in Selected) Store.UpdateMeta(it, "pinned", !it.Pinned); Store.RaiseChanged(); }
    void RenameClick(object sender, RoutedEventArgs e)
    {
        var it = Selected.FirstOrDefault();
        if (it == null) return;
        string current = string.IsNullOrEmpty(it.Title) ? it.Source : it.Title;
        var name = PromptWindow.Ask(this, L.T(it.IsRecording ? "Renombrar grabación" : "Renombrar captura"), L.T("Un nombre para encontrarla rápido en la biblioteca y en el menú."), current);
        if (name == null) return;
        Store.UpdateMeta(it, "title", name.Trim());
        Store.RaiseChanged();
    }
    void DeleteClick(object sender, RoutedEventArgs e)
    {
        var sel = Selected;
        if (sel.Count == 0) return;
        bool recs = sel[0].IsRecording;
        if (!PromptWindow.Confirm(this, L.T("Eliminar") + " " + (recs ? L.Count(sel.Count, "grabación", "grabaciones") : L.Count(sel.Count, "captura", "capturas")), L.T(recs ? "Se borran del disco. No se puede deshacer." : "Se borran del disco junto con sus anotaciones. No se puede deshacer."), L.T("Eliminar"))) return;
        foreach (var it in sel) { Imaging.Forget(it.Path); Store.Delete(it); }
    }
    // ================================================================== export
    /// <summary>Selected captures in the order they were picked; whether that order was chosen on purpose (Ctrl+clic in
    /// an order different from the library's) decides if the export keeps it or goes chronological.</summary>
    (List<CaptureItem> items, bool picked) ExportSelection()
    {
        var sel = Selected;
        if (sel.Count == 0) return (cards.Select(c => c.Item).ToList(), false);
        var shown = cards.Select(c => c.Item.Id).ToList();
        var byLibrary = sel.OrderBy(x => shown.IndexOf(x.Id)).Select(x => x.Id);
        return (sel, sel.Count > 1 && !sel.Select(x => x.Id).SequenceEqual(byLibrary));
    }
    void ExportClick(object sender, RoutedEventArgs e)
    {
        if (Filters.SelectedIndex == 4) { PromptWindow.Info(this, L.T("Exportar capturas"), L.T("Las grabaciones no se exportan a documentos. Elegí capturas en la pestaña Todas.")); return; }
        var (items, picked) = ExportSelection();
        RunExport(items, picked);
    }
    void ExportTodayClick(object sender, RoutedEventArgs e)
    {
        var today = Store.All().Where(x => x.Created.Date == DateTime.Today).ToList();
        if (today.Count == 0) { PromptWindow.Info(this, L.T("Exportar capturas"), L.T("Hoy todavía no hay capturas.")); return; }
        RunExport(today, false);
    }
    void RunExport(List<CaptureItem> items, bool picked)
    {
        ExportWindow.Open(this, items, picked);
        if (ExportWindow.LastExport is string path && File.Exists(path) && File.GetLastWriteTime(path) > DateTime.Now.AddSeconds(-30))
            StatusText.Text = L.T("Exportado") + " · " + Path.GetFileName(path);
    }
    void SelectTodayClick(object sender, RoutedEventArgs e)
    {
        if (Filters.SelectedIndex == 4) Filters.SelectedIndex = 0;
        Library.SelectedItems.Clear();
        foreach (var c in cards.Where(c => c.Item.Created.Date == DateTime.Today)) Library.SelectedItems.Add(c);
        Library.Focus();
    }
    /// <summary>Ctrl+Shift+C: the selected captures stacked in one image (numbered, with titles) on the clipboard,
    /// ready to paste into a chat or an email.</summary>
    void CopyStitchedClick(object sender, RoutedEventArgs e)
    {
        var (items, picked) = ExportSelection();
        var images = items.Where(i => !i.IsRecording && File.Exists(i.Path)).ToList();
        if (Selected.Count == 0 || images.Count == 0) { StatusText.Text = L.T("Elegí dos o más capturas para unirlas en una imagen"); return; }
        if (!picked) images = images.OrderBy(i => i.Created).ToList();
        var pages = images.Select(it => new ExportPage { Item = it, Title = string.IsNullOrEmpty(it.Title) ? it.Source : it.Title, Note = it.Note ?? "" }).ToList();
        try
        {
            using var bmp = Exporter.Stitch(pages, new ExportOptions { Cover = false, Numbers = images.Count > 1, Captions = true, Meta = false, Accent = ExportWindow.AccentHex() });
            Store.CopyToClipboard(bmp);
            StatusText.Text = L.F("{0} unidas en una imagen y copiadas", images.Count) + " · " + bmp.Width + "×" + bmp.Height;
        }
        catch (Exception ex) when (ex is ArgumentException or OutOfMemoryException or System.Runtime.InteropServices.ExternalException)
        {
            PromptWindow.Info(this, L.T("No se pudo copiar"), L.T("La imagen resultante es demasiado grande. Probá con menos capturas o exportá a PDF."));
        }
    }

    void RevealClick(object sender, RoutedEventArgs e) { var it = Selected.FirstOrDefault(); if (it != null) Process.Start("explorer.exe", "/select,\"" + it.Path + "\""); }
    void FolderClick(object sender, RoutedEventArgs e) => Process.Start("explorer.exe", "\"" + (Filters.SelectedIndex == 4 ? Store.RecordDir : Store.CaptureDir) + "\"");
    void FocusSearchClick(object sender, RoutedEventArgs e) { Search.Focus(); Search.SelectAll(); }
    void SelectAllClick(object sender, RoutedEventArgs e) => Library.SelectAll();
    async void ComponentsClick(object sender, RoutedEventArgs e) => await App.Current.CheckComponentsAsync();
    void AboutClick(object sender, RoutedEventArgs e)
    {
        var v = typeof(App).Assembly.GetName().Version;
        string version = v == null ? "" : " " + v.Major + "." + v.Minor + "." + v.Build;
        PromptWindow.Info(this, "BuzzPrint" + version,
            L.T("Capturas de región, ventana y pantalla completa, grabación de video y audio, biblioteca, menú rápido para copiar y pegar, editor de anotaciones y exportación a PowerPoint, Word y PDF.") + "\n\n" +
            "© 2026 Christian Buzzetti · " + L.T("Licencia MIT") + "\n" + "github.com/ChristianBuzzetti/BuzzPrint" + "\n\n" +
            L.T("Fuentes Inter y JetBrains Mono (SIL Open Font License).") + "\n" + L.T("Video y audio: FFmpeg (ffmpeg.org)."));
    }

    void OptionChanged(object sender, RoutedEventArgs e)
    {
        Store.Settings.CopyAfterCapture = CopyToggle.IsChecked == true;
        Store.Settings.OpenEditorAfterCapture = EditorToggle.IsChecked == true;
        Store.Settings.EditKeepOriginal = KeepOriginalToggle.IsChecked == true;
        Store.Settings.PasteOnPick = PasteToggle.IsChecked == true;
        Store.Settings.RecordSystemAudio = SystemAudioToggle.IsChecked == true;
        Store.Settings.RecordMicrophone = MicToggle.IsChecked == true;
        Store.Settings.RecordCursor = CursorToggle.IsChecked == true;
        Store.SaveSettings();
        if (sender == StartupToggle) { App.StartupEnabled = StartupToggle.IsChecked == true; StartupToggle.IsChecked = App.StartupEnabled; }
    }
}
