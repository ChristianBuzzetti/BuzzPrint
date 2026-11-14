using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;

namespace BuzzPrint;

/// <summary>
/// While AI works: the window is dimmed under a card with Boberto spinning, what is happening and an indeterminate
/// bar, and the window takes no input until it ends. The card has Cancel, and the work is cancelled by itself after
/// <see cref="Idle"/> without progress (each <see cref="Text"/> update counts as progress, so long batches keep going).
/// Pass <see cref="Token"/> to the AI call and catch <see cref="OperationCanceledException"/> with <see cref="Stopped"/>.
/// </summary>
public sealed class AiBusy : IDisposable
{
    static readonly TimeSpan Idle = TimeSpan.FromMinutes(2);
    static bool lastTimedOut;

    readonly Window owner;
    readonly Window veil;
    readonly TextBlock detail;
    readonly Button cancel;
    readonly bool wasEnabled;
    readonly CancellationTokenSource cts = new();
    readonly System.Windows.Threading.DispatcherTimer watchdog;
    bool closed;

    AiBusy(Window owner, string text)
    {
        this.owner = owner;
        wasEnabled = owner.IsEnabled;
        lastTimedOut = false;

        var mark = new BobertoMark { Width = 64, Height = 64, HorizontalAlignment = HorizontalAlignment.Center, Follow = false, Spinning = true };
        mark.SetResourceReference(BobertoMark.TileProperty, "Accent");
        var title = new TextBlock { Text = L.T("La IA está trabajando…"), FontWeight = FontWeights.Bold, HorizontalAlignment = HorizontalAlignment.Center, Margin = new Thickness(0, 14, 0, 0) };
        title.SetResourceReference(TextBlock.FontSizeProperty, "SectionFontSize");
        title.SetResourceReference(TextBlock.ForegroundProperty, "Ink");
        detail = new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap, TextAlignment = TextAlignment.Center, Margin = new Thickness(0, 6, 0, 0), MaxWidth = 300 };
        detail.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        var bar = new ProgressBar { IsIndeterminate = true, Height = 4, Margin = new Thickness(0, 16, 0, 0), BorderThickness = new Thickness(0) };
        bar.SetResourceReference(Control.ForegroundProperty, "Accent");
        bar.SetResourceReference(Control.BackgroundProperty, "AlternateRow");
        var hint = new TextBlock { Text = L.T("Puede tardar unos segundos. Si pasan 2 minutos sin avances, se cancela sola."), FontSize = 11, TextAlignment = TextAlignment.Center, TextWrapping = TextWrapping.Wrap, Margin = new Thickness(0, 10, 0, 0), MaxWidth = 300 };
        hint.SetResourceReference(TextBlock.ForegroundProperty, "Muted");
        cancel = new Button { Content = L.T("Cancelar"), MinWidth = 120, HorizontalAlignment = HorizontalAlignment.Center, Margin = new Thickness(0, 16, 0, 0), IsCancel = true };
        cancel.Click += (_, _) => Cancel(timedOut: false);
        var stack = new StackPanel { Width = 320 };
        foreach (var e in new UIElement[] { mark, title, detail, bar, hint, cancel }) stack.Children.Add(e);
        var card = new Border { Child = stack, Padding = new Thickness(26, 24, 26, 20), BorderThickness = new Thickness(1), HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
        card.SetResourceReference(Border.BackgroundProperty, "SurfaceRaised");
        card.SetResourceReference(Border.BorderBrushProperty, "StrongBorder");
        card.SetResourceReference(Border.CornerRadiusProperty, "StripRadius");
        card.Effect = new System.Windows.Media.Effects.DropShadowEffect { BlurRadius = 30, ShadowDepth = 8, Direction = 270, Opacity = .45 };
        var dim = new Grid { Background = new SolidColorBrush(Color.FromArgb(0x8C, 0x05, 0x06, 0x0A)) };
        dim.Children.Add(card);

        veil = new Window
        {
            Owner = owner, WindowStyle = WindowStyle.None, AllowsTransparency = true, Background = Brushes.Transparent,
            ShowInTaskbar = false, ResizeMode = ResizeMode.NoResize, ShowActivated = true, Content = dim,
            WindowStartupLocation = WindowStartupLocation.Manual, FontFamily = owner.FontFamily,
        };
        Place();
        owner.LocationChanged += Follow;
        owner.SizeChanged += Follow;
        owner.StateChanged += Follow;
        owner.Closed += OwnerClosed;
        owner.IsEnabled = false;
        watchdog = new System.Windows.Threading.DispatcherTimer { Interval = Idle };
        watchdog.Tick += (_, _) => { watchdog.Stop(); Cancel(timedOut: true); };
        watchdog.Start();
        veil.Show();
    }

    public static AiBusy Show(Window owner, string text) => new(owner, text);
    internal Window Veil => veil;   // screenshot run

    /// <summary>Cancelled by the Cancel button or by the idle timeout.</summary>
    public CancellationToken Token => cts.Token;

    /// <summary>What the AI is doing now (progress like "3/10"); also restarts the idle timeout.</summary>
    public string Text { set { detail.Text = value; if (!closed) { watchdog.Stop(); watchdog.Start(); } } }

    void Cancel(bool timedOut)
    {
        if (closed || cts.IsCancellationRequested) return;
        lastTimedOut = timedOut;
        cancel.IsEnabled = false;
        detail.Text = timedOut ? L.T("Tardó demasiado: cancelando…") : L.T("Cancelando…");
        cts.Cancel();
    }

    /// <summary>After an <see cref="OperationCanceledException"/>: a short status for a cancel, a notice for a timeout.</summary>
    public static string Stopped(Window owner, string title)
    {
        if (!lastTimedOut) return L.T("Cancelado.");
        PromptWindow.Info(owner, title, L.T("La IA pasó 2 minutos sin responder y se canceló.") + "\n\n" + L.T("Probá de nuevo, con menos capturas, o revisá la conexión en IA → Configurar IA."));
        return L.T("La IA tardó demasiado.");
    }

    void Follow(object? sender, EventArgs e) => Place();
    void OwnerClosed(object? sender, EventArgs e) => Dispose();

    /// <summary>Covers the owner's client area, in device-independent units.</summary>
    void Place()
    {
        if (owner.Content is not FrameworkElement content || PresentationSource.FromVisual(owner) is not { CompositionTarget: { } target }) return;
        var from = target.TransformFromDevice;
        Point tl = from.Transform(content.PointToScreen(new Point(0, 0)));
        Point br = from.Transform(content.PointToScreen(new Point(content.ActualWidth, content.ActualHeight)));
        veil.Left = tl.X - content.Margin.Left; veil.Top = tl.Y - content.Margin.Top;
        veil.Width = Math.Max(1, br.X - tl.X + content.Margin.Left + content.Margin.Right);
        veil.Height = Math.Max(1, br.Y - tl.Y + content.Margin.Top + content.Margin.Bottom);
    }

    public void Dispose()
    {
        if (closed) return;
        closed = true;
        watchdog.Stop();
        owner.LocationChanged -= Follow; owner.SizeChanged -= Follow; owner.StateChanged -= Follow; owner.Closed -= OwnerClosed;
        owner.IsEnabled = wasEnabled;
        veil.Close();
        cts.Dispose();
        if (owner.IsVisible) owner.Activate();
    }
}
