using System.IO;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;
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
        var keepSectionsAll = new List<string>(Store.Settings.Sections ?? new List<string>());
        string keepGroupAll = Store.Settings.LibraryGroupBy;
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
            // Default names, sections and grouping (the person's section list and grouping are restored at the end).
            log.Add("default name: " + Store.DefaultName("Región", new DateTime(2026, 11, 12, 16, 5, 33)) + " | " + Store.DefaultName("DBMON SQL — Workspace: master", DateTime.Now));
            var seeded = Store.All().OrderBy(x => x.Created).ToList();
            Store.SetSection(seeded.Take(3), "Proyecto DBMON");
            Store.SetSection(seeded.Skip(3).Take(2), "Boberto");
            main.ShotView("section");
            await Task.Delay(1500);
            Save(main, Path.Combine(dir, "01b-library-sections.png"), log);
            main.ShotView("day", "Proyecto DBMON");
            await Task.Delay(1200);
            Save(main, Path.Combine(dir, "01c-library-day-filtered.png"), log);
            // Rename and delete a section, as the Sections menu does (without the confirmation dialogs).
            Store.RenameSection("Boberto", "Mascota");
            log.Add("rename: sections=" + string.Join("|", Store.SectionNames()) + " in Mascota=" + Store.All().Count(x => x.Section == "Mascota") + " in Boberto=" + Store.All().Count(x => x.Section == "Boberto"));
            main.DeleteSection("Mascota", ask: false);
            log.Add("delete: sections=" + string.Join("|", Store.SectionNames()) + " in Mascota=" + Store.All().Count(x => x.Section == "Mascota") + " total=" + Store.All().Count);
            main.ShotView("section");
            await Task.Delay(1200);
            Save(main, Path.Combine(dir, "01d-library-after-delete.png"), log);
            main.ShotView("none");

            EditorWindow.Open(Store.All().Skip(1).First());
            await Task.Delay(800);
            EditorWindow.Instance!.DemoAnnotate();
            await Task.Delay(1200);
            Save(EditorWindow.Instance!, Path.Combine(dir, "02-editor.png"), log);
            // Saving an edit keeps the original as it was and adds "<name>_editada".
            Store.Settings.EditKeepOriginal = true;
            var before = Store.All().Count;
            var original = EditorWindow.Instance!.Current!;
            long originalBytes = new FileInfo(original.Path).Length;
            EditorWindow.Instance!.SaveForShot();
            var after = Store.All();
            var copy = EditorWindow.Instance!.Current!;
            log.Add("keep original: " + before + " -> " + after.Count + " captures, original unchanged=" + (new FileInfo(original.Path).Length == originalBytes && !File.Exists(original.OrigPath)) + ", copy=" + copy.Display + " edited=" + File.Exists(copy.OrigPath));

            var quick = QuickPickWindow.ForShot();
            quick.Show();
            await Task.Delay(900);
            Save(quick, Path.Combine(dir, "03-quick.png"), log);
            quick.Close();

            var personalize = new PersonalizationWindow(App.Current.Appearance, App.Current.ApplyAppearance) { Owner = main };
            personalize.Show();
            await Task.Delay(6000);
            log.Add("personalize visible=" + personalize.IsVisible + " loaded=" + personalize.IsLoaded + " size=" + personalize.ActualWidth + "x" + personalize.ActualHeight + " state=" + personalize.WindowState + " main=" + main.WindowState + "/" + main.IsVisible);
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
#if PREVIEW
            var aiWin = AiSettingsWindow.ForShot(main);
            aiWin.Show();
            await Task.Delay(900);
            Save(aiWin, Path.Combine(dir, "10-ai-settings.png"), log);
            aiWin.Close();
#endif
            var about = AboutWindow.ForShot(main);
            about.Show();
            await Task.Delay(700);
            Save(about, Path.Combine(dir, "12-about.png"), log);
            about.Close();
            foreach (var (file, w) in new[]
            {
                ("13-dialog-components.png", PromptWindow.ForShot(main, L.T("Componentes"), L.T("Todo instalado: WebView2 para Boberto y FFmpeg para grabar.") + "\n\nWebView2: " + L.T("Instalado · muestra a Boberto") + "\nFFmpeg: C:\\BuzzPrint-Preview\\tools\\ffmpeg.exe", null)),
                ("14-dialog-rename.png", PromptWindow.ForShot(main, L.T("Renombrar"), L.T("Nuevo nombre para la captura."), "Formulario de parámetros")),
            })
            {
                w.Show();
                await Task.Delay(600);
                Save(w, Path.Combine(dir, file), log);
                w.Close();
            }
#if PREVIEW
            {
                string sampleReport = "Summary\nDisk almost full on the demo app server\n\nDescription\nThe monitoring screen shows the data volume of DEMO-APP-01 at 96% after the nightly batch.\n\nSteps to reproduce\n1. Open the storage dashboard\n2. Filter by DEMO-APP-01\n\nActual result\n\"ALERT: /data 96% used (threshold 90%)\"\n\nEnvironment\n- Server: DEMO-APP-01\n- Volume: /data\n\nSuggested severity\nMedium — the batch still finishes, but the next run may fail.";
                var report = AiTextWindow.ForShot(main, "Incident report", "Sample data, invented for the screenshot run.", sampleReport);
                report.Show();
                await Task.Delay(600);
                Save(report, Path.Combine(dir, "15-report.png"), log);
                report.Close();
                var png = ReportFormat.Image("Incident report", sampleReport, (Color)ColorConverter.ConvertFromString("#" + ExportWindow.AccentHex()));
                var pngEnc = new System.Windows.Media.Imaging.PngBitmapEncoder();
                pngEnc.Frames.Add(System.Windows.Media.Imaging.BitmapFrame.Create(png));
                using (var f = File.Create(Path.Combine(dir, "16-report-image.png"))) pngEnc.Save(f);
                File.WriteAllText(Path.Combine(dir, "17-report.html"), ReportFormat.FullHtmlPage("Incident report", ReportFormat.Html("Incident report", sampleReport, ExportWindow.AccentHex())));
                var busy = AiBusy.Show(main, "Writing the report from 3 capture(s)…");
                await Task.Delay(900);
                Save(busy.Veil, Path.Combine(dir, "18-ai-busy.png"), log);
                busy.Dispose();
            }
#endif
            // The 3D head turn: every shape looking left, front, right, up and down, at 96 px and at 32 px.
            {
                var sheet = new DrawingVisual();
                var looks = new[] { new Vector(-3.4, 0), new Vector(-1.7, -1.3), default, new Vector(1.7, 1.3), new Vector(3.4, 0), new Vector(0, -2.6), new Vector(0, 2.6) };
                var accent = (Brush)Application.Current.Resources["Accent"];
                using (var g = sheet.RenderOpen())
                {
                    g.DrawRectangle(new SolidColorBrush(Color.FromRgb(0x10, 0x12, 0x18)), null, new Rect(0, 0, looks.Length * 104 + 60, MarkStyle.Shapes.Length * 104));
                    for (int r = 0; r < MarkStyle.Shapes.Length; r++)
                    {
                        var st = BobertoMark.Current with { Shape = MarkStyle.Shapes[r] };
                        for (int c = 0; c < looks.Length; c++)
                        {
                            g.PushTransform(new TranslateTransform(c * 104 + 4, r * 104 + 4));
                            BobertoMark.Draw(g, 96, st, accent, looks[c]);
                            g.Pop();
                        }
                        g.PushTransform(new TranslateTransform(looks.Length * 104 + 10, r * 104 + 34));
                        BobertoMark.Draw(g, 32, st, accent, looks[0]);
                        g.Pop();
                    }
                }
                var bmp = new System.Windows.Media.Imaging.RenderTargetBitmap(looks.Length * 104 + 60, MarkStyle.Shapes.Length * 104, 96, 96, PixelFormats.Pbgra32);
                bmp.Render(sheet);
                var enc = new System.Windows.Media.Imaging.PngBitmapEncoder();
                enc.Frames.Add(System.Windows.Media.Imaging.BitmapFrame.Create(bmp));
                using (var f = File.Create(Path.Combine(dir, "11-mark-3d.png"))) enc.Save(f);
            }
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

            // Every built-in template, in every format; PDFs are rendered to PNG pages to review them.
            foreach (var tpl in DocTemplates.BuiltIns)
            {
                var topt = new ExportOptions { Title = "Demo " + tpl.Name, Intro = "Sample document to review the template.", T = tpl, Accent = tpl.AccentHex(ExportWindow.AccentHex()) };
                foreach (var f in new[] { ExportFormat.Pptx, ExportFormat.Docx, ExportFormat.Pdf, ExportFormat.Png })
                {
                    string file = Path.Combine(exportDir, "tpl-" + tpl.Id + Exporter.Extension(f));
                    try { Exporter.Write(file, f, pages.Take(2).ToList(), topt); log.Add("template " + tpl.Id + " " + f + " " + new FileInfo(file).Length + " bytes"); }
                    catch (Exception ex) { log.Add("TEMPLATE FAIL " + tpl.Id + " " + f + " " + ex); }
                }
#if PREVIEW
                try { await RenderPdfPages(Path.Combine(exportDir, "tpl-" + tpl.Id + ".pdf"), Path.Combine(dir, "tpl-" + tpl.Id)); }
                catch (Exception ex) { log.Add("PDF RENDER FAIL " + tpl.Id + " " + ex.Message); }
#endif
            }
            {
                var tw = TemplatesWindow.ForShot(main, "buzzprint", ExportWindow.AccentHex());
                tw.Show();
                await Task.Delay(800);
                Save(tw, Path.Combine(dir, "19-templates.png"), log);
                tw.Close();
            }

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

            App.Current.ApplyAppearance(App.Current.Appearance with { ThemeId = "glaciar" });
            EditorWindow.Instance?.Discard();
            EditorWindow.Instance?.Close();
            main.Activate();
            await Task.Delay(3000);
            Save(main, Path.Combine(dir, "05-library-light.png"), log);
            log.Add("OK");
        }
        catch (Exception ex) { log.Add("FAIL " + ex); }
        try { Store.Settings.Sections = keepSectionsAll; Store.Settings.LibraryGroupBy = keepGroupAll; Store.SaveSettings(); } catch { }
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

#if PREVIEW
    /// <summary>Renders the first pages of a PDF next to <paramref name="prefix"/> (-p1.png, -p2.png) with Windows.Data.Pdf.</summary>
    static async Task RenderPdfPages(string pdfPath, string prefix)
    {
        var file = await global::Windows.Storage.StorageFile.GetFileFromPathAsync(pdfPath);
        var pdf = await global::Windows.Data.Pdf.PdfDocument.LoadFromFileAsync(file);
        for (uint i = 0; i < Math.Min(2u, pdf.PageCount); i++)
        {
            using var page = pdf.GetPage(i);
            using var ras = new global::Windows.Storage.Streams.InMemoryRandomAccessStream();
            await page.RenderToStreamAsync(ras, new global::Windows.Data.Pdf.PdfPageRenderOptions { DestinationWidth = 700 });
            ras.Seek(0);
            using var src = ras.AsStreamForRead();
            using var dst = File.Create(prefix + "-p" + (i + 1) + ".png");
            await src.CopyToAsync(dst);
        }
    }
#endif

    static void Save(Window w, string path, List<string> log)
    {
        if (w.WindowState == WindowState.Minimized) { w.WindowState = WindowState.Normal; log.Add("restored minimized " + w.GetType().Name); }
        // BUZZPRINT_SHOT_RENDER=1: draw the window's content off screen (works with the session locked; no title bar,
        // and WebView2 parts such as Boberto stay blank).
        if (Environment.GetEnvironmentVariable("BUZZPRINT_SHOT_RENDER") == "1" && w.Content is FrameworkElement root)
        {
            root.UpdateLayout();
            double dpi = VisualTreeHelper.GetDpi(root).PixelsPerDip;
            var rtb = new System.Windows.Media.Imaging.RenderTargetBitmap((int)(root.ActualWidth * dpi), (int)(root.ActualHeight * dpi), 96 * dpi, 96 * dpi, PixelFormats.Pbgra32);
            var dv = new DrawingVisual();
            using (var dc = dv.RenderOpen())
            {
                dc.DrawRectangle(w.Background ?? Brushes.Black, null, new Rect(0, 0, root.ActualWidth, root.ActualHeight));
                dc.DrawRectangle(new VisualBrush(root), null, new Rect(0, 0, root.ActualWidth, root.ActualHeight));
            }
            rtb.Render(dv);
            var enc = new System.Windows.Media.Imaging.PngBitmapEncoder();
            enc.Frames.Add(System.Windows.Media.Imaging.BitmapFrame.Create(rtb));
            using (var fs = File.Create(path)) enc.Save(fs);
            log.Add("rendered " + path + " " + rtb.PixelWidth + "x" + rtb.PixelHeight);
            return;
        }
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
