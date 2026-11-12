using System.IO;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using D = System.Drawing;

namespace BuzzPrint;

/// <summary>
/// BuzzPrint.exe --shot DIR : visual check on a throwaway library (never the user's captures). Opens the library,
/// editor, quick menu and Personalize, and saves each window with PrintWindow (includes WebView2/Boberto).
/// </summary>
static class Shots
{
    [DllImport("user32.dll")] static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hwnd, out Native.RECT rect);

    public static async Task RunAsync(MainWindow main)
    {
        string dir = App.ShotDir!;
        Directory.CreateDirectory(dir);
        var log = new List<string>();
        try
        {
            Store.CaptureDir = Path.Combine(dir, "library");
            Store.OrigDir = Path.Combine(Store.CaptureDir, ".orig");
            if (Directory.Exists(Store.CaptureDir)) Directory.Delete(Store.CaptureDir, true);
            Directory.CreateDirectory(Store.OrigDir);
            Store.RecordDir = Path.Combine(dir, "recordings");
            Store.RecordMetaDir = Path.Combine(Store.RecordDir, ".meta");
            if (Directory.Exists(Store.RecordDir)) Directory.Delete(Store.RecordDir, true);
            Directory.CreateDirectory(Store.RecordMetaDir);
            Seed();
            Store.RaiseChanged();

            main.Show();
            main.WindowState = WindowState.Normal;
            await Task.Delay(9000);
            Save(main, Path.Combine(dir, "01-library.png"), log);

            EditorWindow.Open(Store.All().Skip(1).First());
            await Task.Delay(800);
            EditorWindow.Instance!.DemoAnnotate();
            await Task.Delay(1200);
            Save(EditorWindow.Instance!, Path.Combine(dir, "02-editor.png"), log);

            var quick = QuickPickWindow.ForShot();
            quick.Show();
            await Task.Delay(900);
            Save(quick, Path.Combine(dir, "03-quick.png"), log);
            quick.Close();

            var personalize = new PersonalizationWindow(App.Current.Appearance, App.Current.ApplyAppearance) { Owner = main };
            personalize.Show();
            await Task.Delay(6000);
            Save(personalize, Path.Combine(dir, "04-personalize.png"), log);
            personalize.Sections.SelectedIndex = 2;
            await Task.Delay(800);
            Save(personalize, Path.Combine(dir, "04b-personalize-interface.png"), log);
            personalize.Sections.SelectedIndex = 3;
            await Task.Delay(800);
            Save(personalize, Path.Combine(dir, "04c-personalize-logo.png"), log);
            // A custom logo, shown in the header, then back to the person's own (never saved: Shots keep settings intact).
            var ownMark = BobertoMark.Current;
            BobertoMark.Current = new MarkStyle("ghost", "#F4F4F5", "happy", "#8B5CF6");
            await Task.Delay(500);
            Save(main, Path.Combine(dir, "04d-custom-logo.png"), log);
            BobertoMark.Current = ownMark;
            personalize.Close();

            // Export: the window, then every format written for real (checked afterwards with Office).
            var exportItems = Store.All().ToList();
            Store.UpdateMeta(exportItems.OrderBy(i => i.Created).First(), "note", "Abrí el workspace y elegí la conexión.\nDespués ejecutá la consulta con F5.");
            Store.RaiseChanged();
            exportItems = Store.All().ToList();
            var export = ExportWindow.ForShot(main, exportItems);
            export.Show();
            await Task.Delay(1200);
            Save(export, Path.Combine(dir, "09-export.png"), log);
            export.Close();
            string exportDir = Path.Combine(dir, "export");
            try { if (Directory.Exists(exportDir)) Directory.Delete(exportDir, true); }
            catch (IOException) { exportDir += "-" + DateTime.Now.ToString("HHmmss"); }   // a viewer still has the last run open
            Directory.CreateDirectory(exportDir);
            var pages = exportItems.OrderBy(i => i.Created).Select(it => new ExportPage { Item = it, Title = string.IsNullOrEmpty(it.Title) ? it.Source : it.Title, Note = it.Note }).ToList();
            var opt = new ExportOptions { Title = "Demo BuzzPrint — " + DateTime.Now.ToString("d"), Accent = ExportWindow.AccentHex() };
            foreach (var f in new[] { ExportFormat.Pptx, ExportFormat.Docx, ExportFormat.Pdf, ExportFormat.Png })
            {
                string file = Path.Combine(exportDir, "demo" + Exporter.Extension(f));
                var sw = System.Diagnostics.Stopwatch.StartNew();
                try { Exporter.Write(file, f, pages, opt); log.Add("export " + f + " " + new FileInfo(file).Length + " bytes " + sw.ElapsedMilliseconds + " ms"); }
                catch (Exception ex) { log.Add("EXPORT FAIL " + f + " " + ex); }
            }
            opt.Landscape = true;
            try { Exporter.Write(Path.Combine(exportDir, "demo-landscape.docx"), ExportFormat.Docx, pages, opt); Exporter.Write(Path.Combine(exportDir, "demo-landscape.pdf"), ExportFormat.Pdf, pages, opt); }
            catch (Exception ex) { log.Add("EXPORT FAIL landscape " + ex); }

            // Recordings: the empty tab shows the live Boberto stage; then a real 3 s video of the library window
            // and a 2 s system-audio clip (never the microphone) into the throwaway folder.
            main.Activate();
            log.Add("stage live before empty=" + (main.EmptyStage?.IsLive ?? false));
            main.ShowRecordings();
            await Task.Delay(250);
            Save(main, Path.Combine(dir, "06a-empty-instant.png"), log);
            await Task.Delay(4750);
            Save(main, Path.Combine(dir, "06-recordings-empty.png"), log);
            log.Add("stage live=" + (main.EmptyStage?.IsLive ?? false));
            Store.Settings.RecordMicrophone = false;
            GetWindowRect(new WindowInteropHelper(main).Handle, out var wr);
            var area = new D.Rectangle(wr.Left + 8, wr.Top + 8, wr.Right - wr.Left - 16, wr.Bottom - wr.Top - 16);
            var bar = new RecordingBar(RecordKind.Video, area, "BuzzPrint");
            bar.Show();
            var (rec, err) = await Recorder.StartAsync(RecordKind.Video, area, "BuzzPrint — demo");
            if (rec == null) log.Add("REC FAIL " + err);
            else
            {
                bar.Attach(rec);
                await Task.Delay(3000);
                Save(bar, Path.Combine(dir, "07-recording-bar.png"), log);
                bar.ShowSaving();
                var (item, err2) = await rec.StopAsync(true);
                log.Add(item != null ? "recorded " + item.Path + " " + new FileInfo(item.Path).Length + " bytes, " + item.Duration + " s" : "REC SAVE FAIL " + err2);
            }
            bar.Close();
            var (arec, aerr) = await Recorder.StartAsync(RecordKind.Audio, D.Rectangle.Empty, "Demo audio");
            if (arec == null) log.Add("AUDIO FAIL " + aerr);
            else
            {
                await Task.Delay(2000);
                var (aitem, aerr2) = await arec.StopAsync(true);
                log.Add(aitem != null ? "audio " + aitem.Path + " " + new FileInfo(aitem.Path).Length + " bytes" : "AUDIO SAVE FAIL " + aerr2);
            }
            Store.RaiseChanged();
            await Task.Delay(1500);
            Save(main, Path.Combine(dir, "08-recordings.png"), log);

            App.Current.ApplyAppearance(App.Current.Appearance with { ThemeId = "light" });
            EditorWindow.Instance?.Discard();
            EditorWindow.Instance?.Close();
            main.Activate();
            await Task.Delay(3000);
            Save(main, Path.Combine(dir, "05-library-light.png"), log);
            log.Add("OK");
        }
        catch (Exception ex) { log.Add("FAIL " + ex); }
        File.WriteAllLines(Path.Combine(dir, "shots.log"), log);
        App.Current.ExitApp();
    }

    static void Seed()
    {
        // Sample screenshots for the throwaway library: BUZZPRINT_SHOT_MEDIA overrides; missing files are skipped.
        string media = Environment.GetEnvironmentVariable("BUZZPRINT_SHOT_MEDIA") ?? @"C:\chris\Coordinacion\DBMON_SQL\docs\media";
        var sources = new (string File, string Source, string Title)[]
        {
            ("workspace.png", "DBMON SQL — Workspace", ""),
            ("parameters.png", "Parámetros de consulta", "Formulario de parámetros"),
            ("recipes.png", "Biblioteca de recetas", ""),
            ("repeat.png", "Repetir cada 5 s", ""),
            ("studio-crown.png", "Boberto Studio", "Boberto con corona"),
            ("palette-forest.png", "Paleta Forest", ""),
        };
        foreach (var s in sources)
        {
            string p = Path.Combine(media, s.File);
            if (!File.Exists(p)) continue;
            using var bmp = Store.LoadBitmap(p);
            var it = Store.Add(bmp, s.Source);
            if (s.Title.Length > 0) Store.UpdateMeta(it, "title", s.Title);
            if (s.File == "recipes.png") Store.UpdateMeta(it, "pinned", true);
            Thread.Sleep(15);
        }
    }

    static void Save(Window w, string path, List<string> log)
    {
        if (w.WindowState == WindowState.Minimized) { w.WindowState = WindowState.Normal; log.Add("restored minimized " + w.GetType().Name); }
        var h = new WindowInteropHelper(w).Handle;
        GetWindowRect(h, out var r);
        int width = r.Right - r.Left, height = r.Bottom - r.Top;
        using var bmp = new D.Bitmap(width, height);
        using (var g = D.Graphics.FromImage(bmp))
        {
            var hdc = g.GetHdc();
            PrintWindow(h, hdc, 2);
            g.ReleaseHdc(hdc);
        }
        bmp.Save(path, D.Imaging.ImageFormat.Png);
        log.Add("saved " + path + " " + width + "x" + height);
    }
}
