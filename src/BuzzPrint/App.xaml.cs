using System.Diagnostics;
using System.IO;
using System.Text.Json;
using System.Threading;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using Microsoft.Win32;
using WinForms = System.Windows.Forms;

namespace BuzzPrint;

public enum CaptureMode { Region, Full, Delayed }

public partial class App : Application
{
    public static new App Current => (App)Application.Current;
    public AppearanceSettings Appearance { get; private set; } = new();
    public event Action<AppearanceSettings>? AppearanceChanged;
    public event Action? RecordingChanged;
    public string CaptureHotkeyText { get; private set; } = "";
    public bool Exiting { get; private set; }
    public static bool IsShot { get; private set; }
    public static string? ShotDir { get; private set; }

    MainWindow? main;
    WinForms.NotifyIcon? tray;
    HotkeyWindow? hotkeys;
    Mutex? mutex;
    bool capturing, recordStarting;
    CaptureItem? lastToastItem;
    RecordingBar? recordingBar;

    static string? Arg(string[] args, string name) { int i = Array.IndexOf(args, name); return i < 0 ? null : i + 1 < args.Length ? args[i + 1] : ""; }

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
        string? iconDir = Arg(e.Args, "--icon");
        string? shotDir = Arg(e.Args, "--shot");
        IsShot = shotDir != null;
        if (IsShot) ShotDir = shotDir!.Length > 0 ? shotDir : Path.GetTempPath();

        if (!IsShot && iconDir == null && Array.IndexOf(e.Args, "--fetch-ffmpeg") < 0)
        {
            mutex = new Mutex(false, "BuzzPrint_single_instance");
            bool owned;
            // A language switch relaunches the app: the new process waits for the old one to let go.
            try { owned = mutex.WaitOne(Array.IndexOf(e.Args, "--relaunch") >= 0 ? 8000 : 0); } catch (AbandonedMutexException) { owned = true; }
            var showSignal = new EventWaitHandle(false, EventResetMode.AutoReset, "BuzzPrint_show");
            if (!owned) { showSignal.Set(); Shutdown(); return; }
            ThreadPool.RegisterWaitForSingleObject(showSignal, (_, _) => Dispatcher.BeginInvoke(ShowMain), null, -1, false);
        }

        Store.Init();
        L.Lang = Arg(e.Args, "--lang") is { Length: > 0 } forced ? forced : Store.Settings.Language == "es" ? "es" : "en";
        var culture = System.Globalization.CultureInfo.GetCultureInfo(L.Es ? "es-AR" : "en-US");
        System.Globalization.CultureInfo.DefaultThreadCurrentCulture = culture;
        Thread.CurrentThread.CurrentCulture = culture;
        BobertoMark.Current = MarkStyle.FromSettings(Store.Settings);
        LoadAppearance();
        EventManager.RegisterClassHandler(typeof(Window), FrameworkElement.LoadedEvent, new RoutedEventHandler((sender, _) => Themes.ApplyChrome((Window)sender, Themes.Current)));
        if (iconDir != null) { _ = IconMaker.RunAsync(iconDir.Length > 0 ? iconDir : Path.GetTempPath()); return; }
        // --fetch-ffmpeg LOG: download FFmpeg without asking (support and tests); writes the result to LOG and exits.
        if (Arg(e.Args, "--fetch-ffmpeg") is string fetchLog)
        {
            Dispatcher.BeginInvoke(async () =>
            {
                string result;
                try { result = "OK " + await Dependencies.DownloadFFmpegAsync(null); }
                catch (Exception ex) { result = "FAIL " + ex.GetType().Name + ": " + ex.Message; }
                if (fetchLog.Length > 0) File.WriteAllText(fetchLog, result);
                ExitApp();
            });
            return;
        }
        if (Appearance.BobertoVisible) BobertoRuntime.Prewarm(BobertoRuntime.DefaultProfile);

        main = new MainWindow();
        if (IsShot) { _ = Shots.RunAsync(main); return; }

        SetupTray();
        hotkeys = new HotkeyWindow();
        RegisterHotkeys();
        main.RefreshShortcuts();
        if (Array.IndexOf(e.Args, "--tray") < 0) main.Show();
        // WebView2 (Boberto, Studio) ships with Windows 11; on a PC without it, offer Microsoft's installer once.
        if (!Dependencies.WebView2Installed() && !Store.Settings.WebView2Declined)
            Dispatcher.BeginInvoke(async () => await OfferWebView2Async(), System.Windows.Threading.DispatcherPriority.ApplicationIdle);
        else Toast(L.T("BuzzPrint está activo"), CaptureHotkeyText + " " + L.T("para capturar") + " · " + Store.Settings.HotkeyPick + " " + L.T("para elegir una captura"));
    }

    protected override void OnExit(ExitEventArgs e)
    {
        if (tray != null) tray.Visible = false;
        hotkeys?.Dispose();
        try { mutex?.ReleaseMutex(); } catch (ApplicationException) { }
        base.OnExit(e);
    }

    // ------------------------------------------------------------------ language
    /// <summary>Saves the language and restarts BuzzPrint so every window is rebuilt in it.</summary>
    /// <summary>Installs WebView2 if the person agrees, then restarts so Boberto loads; declining is remembered.</summary>
    public async Task OfferWebView2Async()
    {
        if (await Dependencies.OfferWebView2Async(main))
        {
            if (PromptWindow.Confirm(main, L.T("WebView2 instalado"), L.T("Reiniciá BuzzPrint para que aparezca Boberto."), L.T("Reiniciar"), L.T("Más tarde"))) Relaunch();
        }
        else { Store.Settings.WebView2Declined = true; Store.SaveSettings(); }
    }

    /// <summary>Help > Check components: offers whatever is missing, or says all is in place.</summary>
    public async Task CheckComponentsAsync()
    {
        bool web = Dependencies.WebView2Installed(), ff = Recorder.FFmpeg() != null;
        if (web && ff) { PromptWindow.Info(main, L.T("Componentes"), L.T("Todo instalado: WebView2 para Boberto y FFmpeg para grabar.") + "\n\nFFmpeg: " + Recorder.FFmpeg()); return; }
        if (!ff && await Dependencies.OfferFFmpegAsync(main) != null) PromptWindow.Info(main, L.T("FFmpeg listo"), L.T("Ya podés grabar video y audio."));
        if (!web) { Store.Settings.WebView2Declined = false; await OfferWebView2Async(); }
    }

    void Relaunch()
    {
        try { Process.Start(new ProcessStartInfo(Environment.ProcessPath!, "--relaunch") { UseShellExecute = false }); }
        catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException) { return; }
        ExitApp();
    }

    public void SetLanguage(string lang)
    {
        if (lang == L.Lang) return;
        if (Recorder.Current != null) { PromptWindow.Info(main!, L.T("Grabación en curso"), L.T("Detené la grabación antes de cambiar el idioma.")); return; }
        Store.Settings.Language = lang;
        Store.SaveSettings();
        try { Process.Start(new ProcessStartInfo(Environment.ProcessPath!, "--relaunch") { UseShellExecute = false }); }
        catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException) { return; }
        ExitApp();
    }

    // ------------------------------------------------------------------ appearance (DBMON's AppearanceSettings)
    void LoadAppearance()
    {
        try { Appearance = AppearanceStore.Load(AppearanceStore.PathName); }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or JsonException or ArgumentException) { Appearance = new(); }
        ApplyAppearance(Appearance);
    }

    public void ApplyAppearance(AppearanceSettings settings)
    {
        settings.Validate();
        Appearance = settings;
        var palette = Themes.WithAccent(Themes.All.Single(p => p.Id == settings.ThemeId), settings.AccentHex);
        Themes.Apply(palette);
        AppTypography.Apply(settings);
        foreach (Window w in Windows) Themes.ApplyChrome(w, palette);
        ApplyMarkIcon();
        AppearanceChanged?.Invoke(settings);
    }

    public void SaveAppearance()
    {
        if (IsShot) return;
        try { AppearanceStore.Save(AppearanceStore.PathName, Appearance); }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or ArgumentException) { MessageBox.Show(L.T("No se pudo guardar la apariencia: ") + ex.Message, "BuzzPrint"); }
    }

    bool personalizing;
    public void OpenPersonalization(Window owner, bool studio = false)
    {
        if (personalizing) return;
        personalizing = true;
        try { new PersonalizationWindow(Appearance, ApplyAppearance, studio) { Owner = owner }.ShowDialog(); }
        finally { personalizing = false; }
        main?.EnsureBobertoLive();
        SaveAppearance();
    }

    // ------------------------------------------------------------------ logo
    /// <summary>Restyles the BuzzPrint logo everywhere (header marks, window and tray icons) and saves it.</summary>
    public void SetMark(MarkStyle style)
    {
        BobertoMark.Current = style;
        style.Save(Store.Settings);
        Store.SaveSettings();
        ApplyMarkIcon();
    }

    /// <summary>Window and tray icons follow the custom logo (the .exe file keeps the shipped icon).</summary>
    void ApplyMarkIcon()
    {
        var accent = Resources["Accent"] as System.Windows.Media.Brush ?? System.Windows.Media.Brushes.Crimson;
        Resources["AppIcon"] = BobertoMark.Render(64, accent);
        if (tray == null) return;
        var frame = BobertoMark.Render(32, accent);
        var encoder = new System.Windows.Media.Imaging.PngBitmapEncoder();
        encoder.Frames.Add(System.Windows.Media.Imaging.BitmapFrame.Create(frame));
        using var ms = new MemoryStream();
        encoder.Save(ms);
        ms.Position = 0;
        using var bmp = new System.Drawing.Bitmap(ms);
        IntPtr handle = bmp.GetHicon();
        var old = trayIconHandle;
        tray.Icon = System.Drawing.Icon.FromHandle(handle);
        trayIconHandle = handle;
        if (old != IntPtr.Zero) DestroyIcon(old);
    }
    IntPtr trayIconHandle;
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern bool DestroyIcon(IntPtr handle);

    // ------------------------------------------------------------------ tray
    void SetupTray()
    {
        tray = new WinForms.NotifyIcon { Text = "BuzzPrint", Visible = true };
        ApplyMarkIcon();
        tray.MouseUp += (_, e) =>
        {
            if (e.Button == WinForms.MouseButtons.Left) { if (Recorder.Current != null) StopRecording(true); else QuickPickWindow.Toggle(IntPtr.Zero); }
            else if (e.Button == WinForms.MouseButtons.Right) ShowTrayMenu();
        };
        tray.MouseDoubleClick += (_, e) => { if (e.Button == WinForms.MouseButtons.Left) ShowMain(); };
        tray.BalloonTipClicked += (_, _) =>
        {
            if (lastToastItem == null) return;
            if (lastToastItem.IsRecording) OpenRecording(lastToastItem); else EditorWindow.Open(lastToastItem);
        };
    }

    void ShowTrayMenu()
    {
        var menu = new ContextMenu { Placement = System.Windows.Controls.Primitives.PlacementMode.MousePoint };
        MenuItem Item(string header, string gesture, Action click)
        {
            var m = new MenuItem { Header = header, InputGestureText = gesture };
            m.Click += (_, _) => click();
            menu.Items.Add(m);
            return m;
        }
        Item(L.T("Capturar región o ventana"), CaptureHotkeyText, () => Capture(CaptureMode.Region, null));
        Item(L.T("Capturar pantalla completa"), Store.Settings.HotkeyFull, () => Capture(CaptureMode.Full, null));
        Item(L.T("Capturar con demora"), Store.Settings.DelaySeconds + " s", () => Capture(CaptureMode.Delayed, null));
        menu.Items.Add(new Separator());
        if (Recorder.Current != null) Item(L.T("Detener grabación"), Store.Settings.HotkeyRecord, () => StopRecording(true));
        else
        {
            Item(L.T("Grabar video"), Store.Settings.HotkeyRecord, () => StartRecording(RecordKind.Video, null));
            Item(L.T("Grabar solo audio"), "", () => StartRecording(RecordKind.Audio, null));
        }
        menu.Items.Add(new Separator());
        Item(L.T("Menú rápido"), Store.Settings.HotkeyPick, () => QuickPickWindow.Toggle(IntPtr.Zero));
        Item(L.T("Biblioteca"), "", ShowMain);
        Item(L.T("Editor"), "", () => EditorWindow.Open(Store.All().FirstOrDefault()));
        Item(L.T("Personalizar…"), "", () => { ShowMain(); OpenPersonalization(main!); });
        menu.Items.Add(new Separator());
        Item(L.T("Salir"), "", ExitApp);
        // A context menu needs a foreground window of this process to close when clicking elsewhere.
        var anchor = new Window { WindowStyle = WindowStyle.None, ShowInTaskbar = false, Width = 0, Height = 0, AllowsTransparency = true, Background = System.Windows.Media.Brushes.Transparent, Left = -100, Top = -100 };
        anchor.Show(); anchor.Activate();
        menu.Closed += (_, _) => anchor.Close();
        menu.IsOpen = true;
    }

    public void Toast(string title, string text, CaptureItem? item = null)
    {
        lastToastItem = item;
        tray?.ShowBalloonTip(2500, title, string.IsNullOrEmpty(text) ? " " : text, WinForms.ToolTipIcon.None);
    }

    public void ShowMain()
    {
        if (main == null) return;
        if (!main.IsVisible) main.Show();
        if (main.WindowState == WindowState.Minimized) main.WindowState = WindowState.Normal;
        main.Activate();
    }

    public async void ExitApp()
    {
        if (Exiting) return;
        Exiting = true;
        if (Recorder.Current is { } rec) { recordingBar?.ShowSaving(); await rec.StopAsync(true); }
        foreach (Window w in Windows.Cast<Window>().ToArray()) w.Close();
        Shutdown();
    }

    // ------------------------------------------------------------------ hotkeys
    void RegisterHotkeys()
    {
        var st = Store.Settings;
        string fails = "";
        if (hotkeys!.Register(1, st.HotkeyCapture)) CaptureHotkeyText = st.HotkeyCapture;
        else if (hotkeys.Register(1, st.HotkeyCaptureAlt))
        {
            CaptureHotkeyText = st.HotkeyCaptureAlt;
            fails += L.F("{0} está tomada (¿Windows la usa para la Herramienta Recortes?). Se usa {1}.", st.HotkeyCapture, st.HotkeyCaptureAlt) + "\n";
        }
        else fails += L.F("No se pudo registrar {0} ni {1}.", st.HotkeyCapture, st.HotkeyCaptureAlt) + "\n";
        if (!hotkeys.Register(2, st.HotkeyFull)) fails += L.F("No se pudo registrar {0}.", st.HotkeyFull) + "\n";
        if (!hotkeys.Register(3, st.HotkeyPick)) fails += L.F("No se pudo registrar {0}.", st.HotkeyPick) + "\n";
        if (!hotkeys.Register(4, st.HotkeyRecord)) fails += L.F("No se pudo registrar {0}.", st.HotkeyRecord) + "\n";
        if (fails.Length > 0) Toast(L.T("Atajos de teclado"), fails.Trim());
    }

    internal void OnHotkey(int id)
    {
        switch (id)
        {
            case 1: Capture(CaptureMode.Region, null); break;
            case 2: Capture(CaptureMode.Full, null); break;
            case 3: QuickPickWindow.Toggle(Native.GetForegroundWindow()); break;
            case 4: if (Recorder.Current != null) StopRecording(true); else StartRecording(RecordKind.Video, null); break;
        }
    }

    // ------------------------------------------------------------------ capture flow
    public async void Capture(CaptureMode mode, Window? hideWhileCapturing)
    {
        if (capturing) return;
        capturing = true;
        bool restore = false;
        try
        {
            if (hideWhileCapturing != null && hideWhileCapturing.IsVisible)
            {
                hideWhileCapturing.Hide();
                restore = true;
                await Task.Delay(250);
            }
            if (mode == CaptureMode.Delayed)
            {
                int secs = Math.Max(1, Store.Settings.DelaySeconds);
                Toast(L.T("Captura con demora"), L.F("Capturando en {0} s… abrí el menú que quieras capturar.", secs));
                await Task.Delay(secs * 1000);
            }
            CaptureResult? r = mode == CaptureMode.Full ? CaptureOverlay.FullScreen() : CaptureOverlay.Run();
            if (r == null) return;
            CaptureItem it;
            using (r.Image) it = Store.Add(r.Image, r.Source);
            if (Store.Settings.CopyAfterCapture) Store.CopyToClipboard(it);
            if (Store.Settings.OpenEditorAfterCapture) EditorWindow.Open(it);
            else Toast(L.T(Store.Settings.CopyAfterCapture ? "Captura copiada y guardada" : "Captura guardada"),
                       L.T("Clic acá para editarla") + " · " + Store.Settings.HotkeyPick + " " + L.T("para elegir otra"), it);
        }
        catch (Exception ex) { MessageBox.Show(L.T("Error al capturar: ") + ex.Message, "BuzzPrint"); }
        finally
        {
            capturing = false;
            if (restore && hideWhileCapturing != null) { hideWhileCapturing.Show(); hideWhileCapturing.Activate(); }
        }
    }

    // ------------------------------------------------------------------ recording flow
    public bool IsRecording => Recorder.Current != null || recordStarting;

    /// <summary>Video: choose a region, window or screen with the capture overlay, a 3-2-1 countdown, then record.
    /// Audio: starts at once. A floating bar (excluded from the recording) shows time and level and stops it.</summary>
    public async void StartRecording(RecordKind kind, Window? hide)
    {
        if (IsRecording || capturing) return;
        // Recording needs FFmpeg: offer to download it the first time instead of failing.
        if (Recorder.FFmpeg() == null && (IsShot || await Dependencies.OfferFFmpegAsync(main) == null)) return;
        recordStarting = true;
        bool restore = false;
        try
        {
            if (hide != null && hide.IsVisible) { hide.Hide(); restore = kind == RecordKind.Audio; await Task.Delay(250); }
            var area = System.Drawing.Rectangle.Empty;
            string source = L.T("Solo audio");
            if (kind == RecordKind.Video)
            {
                var pick = CaptureOverlay.SelectArea();
                if (pick == null) { restore = true; return; }
                area = pick.Area; source = pick.Source;
            }
            recordingBar = new RecordingBar(kind, area, source);
            recordingBar.Show();
            if (kind == RecordKind.Video && !await recordingBar.CountdownAsync(3)) { recordingBar.Close(); recordingBar = null; restore = true; return; }
            var (rec, error) = await Recorder.StartAsync(kind, area, source);
            if (rec == null)
            {
                recordingBar.Close(); recordingBar = null; restore = true;
                PromptWindow.Info(main!, L.T("No se pudo grabar"), error ?? "");
                return;
            }
            recordingBar.Attach(rec);
            if (rec.Warning != null) Toast(L.T("Grabando"), rec.Warning);
            restore = false;
        }
        finally
        {
            recordStarting = false;
            RecordingChanged?.Invoke();
            if (restore && hide != null) { hide.Show(); hide.Activate(); }
        }
    }

    public async void StopRecording(bool keep)
    {
        var rec = Recorder.Current;
        if (rec == null) return;
        recordingBar?.ShowSaving();
        var (item, error) = await rec.StopAsync(keep);
        recordingBar?.Close(); recordingBar = null;
        RecordingChanged?.Invoke();
        if (!keep) return;
        if (item == null) { PromptWindow.Info(main!, L.T("No se pudo guardar la grabación"), error ?? ""); return; }
        if (Store.Settings.CopyAfterCapture) Store.CopyFileToClipboard(item);
        main?.ShowRecordings();
        Toast(L.T(item.Kind == "video" ? "Video guardado" : "Audio guardado"),
              Recorder.Clock(TimeSpan.FromSeconds(item.Duration)) + " · " + (Store.Settings.CopyAfterCapture ? L.T("copiado como archivo") + " · " : "") + L.T("clic para abrirlo"), item);
    }

    public static void OpenRecording(CaptureItem it)
    {
        try { Process.Start(new ProcessStartInfo(it.Path) { UseShellExecute = true }); }
        catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException) { MessageBox.Show(ex.Message, "BuzzPrint"); }
    }

    // ------------------------------------------------------------------ startup with Windows
    const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
    public static bool StartupEnabled
    {
        get { using var k = Registry.CurrentUser.OpenSubKey(RunKey); return k?.GetValue("BuzzPrint") != null; }
        set
        {
            using var k = Registry.CurrentUser.CreateSubKey(RunKey);
            if (value) k.SetValue("BuzzPrint", "\"" + Environment.ProcessPath + "\" --tray");
            else k.DeleteValue("BuzzPrint", false);
        }
    }
}

/// <summary>Hidden window that receives WM_HOTKEY.</summary>
sealed class HotkeyWindow : WinForms.NativeWindow, IDisposable
{
    readonly List<int> ids = [];
    public HotkeyWindow() { CreateHandle(new WinForms.CreateParams()); }

    public bool Register(int id, string spec)
    {
        if (!Parse(spec, out uint mods, out WinForms.Keys key)) return false;
        Native.UnregisterHotKey(Handle, id);
        if (!Native.RegisterHotKey(Handle, id, mods | Native.MOD_NOREPEAT, (uint)key)) return false;
        ids.Add(id);
        return true;
    }

    public static bool Parse(string spec, out uint mods, out WinForms.Keys key)
    {
        mods = 0; key = WinForms.Keys.None;
        if (string.IsNullOrEmpty(spec)) return false;
        foreach (var raw in spec.Split('+'))
        {
            var p = raw.Trim();
            switch (p.ToLowerInvariant())
            {
                case "ctrl": case "control": mods |= Native.MOD_CONTROL; break;
                case "alt": mods |= Native.MOD_ALT; break;
                case "shift": mods |= Native.MOD_SHIFT; break;
                case "win": mods |= Native.MOD_WIN; break;
                default: if (!Enum.TryParse(p, true, out key)) return false; break;
            }
        }
        return key != WinForms.Keys.None;
    }

    protected override void WndProc(ref WinForms.Message m)
    {
        if (m.Msg == Native.WM_HOTKEY) App.Current.OnHotkey(m.WParam.ToInt32());
        base.WndProc(ref m);
    }

    public void Dispose()
    {
        foreach (var id in ids) Native.UnregisterHotKey(Handle, id);
        DestroyHandle();
    }
}
