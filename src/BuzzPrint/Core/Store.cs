#nullable disable
using System;
using System.Collections;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Windows.Forms;

namespace BuzzPrint
{
    /// <summary>One stored capture. The PNG at Path is always the final (annotated) image.</summary>
    public class CaptureItem
    {
        public string Id;
        public string Path;          // final image shown / copied
        public string OrigPath;      // untouched capture (only exists once edited)
        public string MetaPath;      // json: title, source, annotations
        public DateTime Created;
        public string Title = "";
        public string Source = "";
        public bool Pinned;
        public int Width, Height;
        public string Kind = "image";   // image | video | audio
        public string ThumbPath;        // recordings: first frame or waveform
        public double Duration;         // recordings, seconds
        public string Note = "";        // free text kept for exports (Word/PowerPoint/PDF)
        public bool IsRecording { get { return Kind != "image"; } }

        public string Display
        {
            get { return string.IsNullOrEmpty(Title) ? Created.ToString("dd/MM HH:mm:ss") : Title; }
        }
    }

    public class Settings
    {
        public string HotkeyCapture = "PrintScreen";
        public string HotkeyFull = "Ctrl+PrintScreen";
        public string HotkeyPick = "Ctrl+Alt+V";
        public string HotkeyCaptureAlt = "Ctrl+Shift+X";   // used if PrintScreen is taken by Windows
        public bool CopyAfterCapture = true;
        public bool OpenEditorAfterCapture = false;
        public bool PasteOnPick = true;
        public int DelaySeconds = 3;
        public int QuickPickCount = 12;
        public string HotkeyRecord = "Ctrl+Alt+R";
        public bool RecordSystemAudio = true;
        public bool RecordMicrophone = true;
        public bool RecordCursor = true;
        public int RecordFps = 30;
        public string CaptureFolder = "";   // empty = Pictures\BuzzPrint\capturas
        public string RecordFolder = "";    // empty = Videos\BuzzPrint
        public string Language = "en";      // en | es
        public string ExportFormat = "pptx"; // pptx | docx | pdf | png
        public bool ExportCover = true;
        public bool ExportNumbers = true;
        public bool ExportCaptions = true;
        public bool ExportMeta = true;
        public bool ExportOnePerPage = true;
        public bool ExportLandscape = false;
        public bool ExportOpen = true;
        public bool ExportSaveText = true;   // titles and notes typed in the export window go back to the library
        public string ExportFolder = "";     // empty = Documents
        public string MarkShape = "hex";     // BuzzPrint logo (Personalize → Effects → Logo)
        public string MarkHead = "#0E1015";
        public string MarkEyes = "oval";
        public string MarkTile = "accent";
    }

    public static class Store
    {
        public static string Root;
        public static string CaptureDir;
        public static string OrigDir;
        public static string RecordDir;
        public static string RecordMetaDir;
        public static string DefaultCaptureDir { get { return System.IO.Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyPictures), "BuzzPrint", "capturas"); } }
        public static string DefaultRecordDir { get { return System.IO.Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyVideos), "BuzzPrint"); } }
        public static Settings Settings = new Settings();
        public static event EventHandler Changed;

        static readonly MiniJson Json = new MiniJson();
        static readonly Dictionary<string, KeyValuePair<DateTime, Bitmap>> thumbs = new Dictionary<string, KeyValuePair<DateTime, Bitmap>>();
        static List<CaptureItem> cache, recCache;

        public static void Init()
        {
            LoadSettings();
            ApplyFolders();
            ImportFromSnagAI();
        }

        /// <summary>Captures and recordings folders from the settings (empty = Pictures / Videos defaults).</summary>
        public static void ApplyFolders()
        {
            CaptureDir = Usable(Settings.CaptureFolder) ?? DefaultCaptureDir;
            RecordDir = Usable(Settings.RecordFolder) ?? DefaultRecordDir;
            Root = System.IO.Path.GetDirectoryName(CaptureDir);
            OrigDir = System.IO.Path.Combine(CaptureDir, ".orig");
            RecordMetaDir = System.IO.Path.Combine(RecordDir, ".meta");
            foreach (var d in new[] { CaptureDir, OrigDir, RecordDir, RecordMetaDir })
                try { Directory.CreateDirectory(d); } catch { }
            foreach (var d in new[] { OrigDir, RecordMetaDir })
                try { new DirectoryInfo(d).Attributes |= FileAttributes.Hidden; } catch { }
            cache = null; recCache = null;
        }

        static string Usable(string path)
        {
            if (string.IsNullOrWhiteSpace(path)) return null;
            try { var full = System.IO.Path.GetFullPath(path); Directory.CreateDirectory(full); return full; } catch { return null; }
        }

        /// <summary>Points captures or recordings at a new folder, optionally moving the existing files (with their
        /// hidden sidecar folder). Returns how many files were moved.</summary>
        public static int ChangeFolder(bool recordings, string folder, bool move)
        {
            string oldDir = recordings ? RecordDir : CaptureDir;
            string oldSide = recordings ? RecordMetaDir : OrigDir;
            string def = recordings ? DefaultRecordDir : DefaultCaptureDir;
            string value = string.Equals(System.IO.Path.GetFullPath(folder).TrimEnd('\\'), def.TrimEnd('\\'), StringComparison.OrdinalIgnoreCase) ? "" : folder;
            if (recordings) Settings.RecordFolder = value; else Settings.CaptureFolder = value;
            SaveSettings();
            ApplyFolders();
            string newDir = recordings ? RecordDir : CaptureDir, newSide = recordings ? RecordMetaDir : OrigDir;
            int moved = 0;
            if (move && !string.Equals(oldDir, newDir, StringComparison.OrdinalIgnoreCase))
            {
                foreach (var pattern in recordings ? new[] { "*.mp4", "*.mp3" } : new[] { "*.png" })
                    foreach (var f in Directory.GetFiles(oldDir, pattern))
                        try { string to = System.IO.Path.Combine(newDir, System.IO.Path.GetFileName(f)); if (!File.Exists(to)) { File.Move(f, to); moved++; } } catch { }
                if (Directory.Exists(oldSide))
                    foreach (var f in Directory.GetFiles(oldSide))
                        try { string to = System.IO.Path.Combine(newSide, System.IO.Path.GetFileName(f)); if (!File.Exists(to)) File.Move(f, to); } catch { }
            }
            lock (thumbs) thumbs.Clear();
            RaiseChanged();
            return moved;
        }

        /// <summary>Captures taken with the earlier SnagAI build are copied once (the old folder is left untouched).</summary>
        static void ImportFromSnagAI()
        {
            try
            {
                string marker = System.IO.Path.Combine(OrigDir, ".snagai-imported");
                string old = System.IO.Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyPictures), "SnagAI", "capturas");
                if (File.Exists(marker) || !Directory.Exists(old)) return;
                foreach (var f in Directory.GetFiles(old, "*.png"))
                {
                    string to = System.IO.Path.Combine(CaptureDir, System.IO.Path.GetFileName(f));
                    if (!File.Exists(to)) File.Copy(f, to);
                }
                string oldOrig = System.IO.Path.Combine(old, ".orig");
                if (Directory.Exists(oldOrig))
                    foreach (var f in Directory.GetFiles(oldOrig))
                    {
                        string to = System.IO.Path.Combine(OrigDir, System.IO.Path.GetFileName(f));
                        if (!File.Exists(to)) File.Copy(f, to);
                    }
                File.WriteAllText(marker, DateTime.Now.ToString("o"));
            }
            catch { }
        }

        static string SettingsPath { get { Directory.CreateDirectory(DataFolder.Root); return DataFolder.File("capture-settings.json"); } }

        public static void LoadSettings()
        {
            try
            {
                if (File.Exists(SettingsPath))
                    Settings = Json.Deserialize<Settings>(File.ReadAllText(SettingsPath)) ?? new Settings();
            }
            catch { Settings = new Settings(); }
            SaveSettings();
        }

        public static void SaveSettings()
        {
            try { File.WriteAllText(SettingsPath, Json.Serialize(Settings)); } catch { }
        }

        public static void RaiseChanged()
        {
            cache = null; recCache = null;
            if (Changed != null) Changed(null, EventArgs.Empty);
        }

        public static List<CaptureItem> All()
        {
            if (cache != null) return cache;
            var list = new List<CaptureItem>();
            foreach (var f in Directory.GetFiles(CaptureDir, "*.png"))
            {
                string id = System.IO.Path.GetFileNameWithoutExtension(f);
                var it = new CaptureItem
                {
                    Id = id,
                    Path = f,
                    OrigPath = System.IO.Path.Combine(OrigDir, id + ".png"),
                    MetaPath = System.IO.Path.Combine(OrigDir, id + ".json"),
                    Created = File.GetCreationTime(f)
                };
                DateTime parsed;
                if (DateTime.TryParseExact(id.Length >= 19 ? id.Substring(0, 19) : id, "yyyyMMdd_HHmmss_fff",
                        null, System.Globalization.DateTimeStyles.None, out parsed))
                    it.Created = parsed;
                ReadPngSize(it);
                var meta = ReadMeta(it);
                if (meta != null)
                {
                    object o;
                    if (meta.TryGetValue("title", out o) && o != null) it.Title = o.ToString();
                    if (meta.TryGetValue("source", out o) && o != null) it.Source = o.ToString();
                    if (meta.TryGetValue("pinned", out o) && o is bool) it.Pinned = (bool)o;
                    if (meta.TryGetValue("note", out o) && o != null) it.Note = o.ToString();
                }
                list.Add(it);
            }
            cache = list.OrderByDescending(x => x.Pinned).ThenByDescending(x => x.Created).ToList();
            return cache;
        }

        /// <summary>Video (mp4) and audio (mp3) recordings, newest first (pinned on top).</summary>
        public static List<CaptureItem> Recordings()
        {
            if (recCache != null) return recCache;
            var list = new List<CaptureItem>();
            try
            {
                foreach (var f in Directory.GetFiles(RecordDir))
                {
                    string ext = System.IO.Path.GetExtension(f).ToLowerInvariant();
                    if (ext != ".mp4" && ext != ".mp3") continue;
                    string id = System.IO.Path.GetFileNameWithoutExtension(f);
                    var it = new CaptureItem
                    {
                        Id = id, Path = f, Kind = ext == ".mp4" ? "video" : "audio",
                        OrigPath = System.IO.Path.Combine(RecordMetaDir, id + ".none"),
                        MetaPath = System.IO.Path.Combine(RecordMetaDir, id + ".json"),
                        ThumbPath = System.IO.Path.Combine(RecordMetaDir, id + ".png"),
                        Created = File.GetCreationTime(f)
                    };
                    DateTime parsed;
                    if (DateTime.TryParseExact(id.Length >= 19 ? id.Substring(0, 19) : id, "yyyyMMdd_HHmmss_fff", null, System.Globalization.DateTimeStyles.None, out parsed)) it.Created = parsed;
                    var meta = ReadMeta(it);
                    if (meta != null)
                    {
                        object o;
                        if (meta.TryGetValue("title", out o) && o != null) it.Title = o.ToString();
                        if (meta.TryGetValue("source", out o) && o != null) it.Source = o.ToString();
                        if (meta.TryGetValue("pinned", out o) && o is bool) it.Pinned = (bool)o;
                        if (meta.TryGetValue("duration", out o) && o != null) { double d; if (double.TryParse(Convert.ToString(o, System.Globalization.CultureInfo.InvariantCulture), System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out d)) it.Duration = d; }
                    }
                    list.Add(it);
                }
            }
            catch { }
            recCache = list.OrderByDescending(x => x.Pinned).ThenByDescending(x => x.Created).ToList();
            return recCache;
        }

        static void ReadPngSize(CaptureItem it)
        {
            try
            {
                using (var fs = new FileStream(it.Path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete))
                {
                    var h = new byte[24];
                    if (fs.Read(h, 0, 24) == 24 && h[1] == 'P' && h[2] == 'N' && h[3] == 'G')
                    {
                        it.Width = (h[16] << 24) | (h[17] << 16) | (h[18] << 8) | h[19];
                        it.Height = (h[20] << 24) | (h[21] << 16) | (h[22] << 8) | h[23];
                    }
                }
            }
            catch { }
        }

        public static Dictionary<string, object> ReadMeta(CaptureItem it)
        {
            try
            {
                if (File.Exists(it.MetaPath))
                    return Json.Deserialize<Dictionary<string, object>>(File.ReadAllText(it.MetaPath));
            }
            catch { }
            return null;
        }

        public static void WriteMeta(CaptureItem it, Dictionary<string, object> meta)
        {
            File.WriteAllText(it.MetaPath, Json.Serialize(meta));
        }

        public static void UpdateMeta(CaptureItem it, string key, object value)
        {
            var meta = ReadMeta(it) ?? new Dictionary<string, object>();
            meta[key] = value;
            WriteMeta(it, meta);
        }

        public static CaptureItem Add(Bitmap bmp, string source)
        {
            string id = DateTime.Now.ToString("yyyyMMdd_HHmmss_fff");
            string path = System.IO.Path.Combine(CaptureDir, id + ".png");
            bmp.Save(path, ImageFormat.Png);
            var it = new CaptureItem
            {
                Id = id, Path = path, Created = DateTime.Now, Source = source ?? "",
                OrigPath = System.IO.Path.Combine(OrigDir, id + ".png"),
                MetaPath = System.IO.Path.Combine(OrigDir, id + ".json")
            };
            if (!string.IsNullOrEmpty(source))
                WriteMeta(it, new Dictionary<string, object> { { "source", source } });
            RaiseChanged();
            return All().FirstOrDefault(x => x.Id == id) ?? it;
        }

        public static void Delete(CaptureItem it)
        {
            foreach (var p in new[] { it.Path, it.OrigPath, it.MetaPath, it.ThumbPath })
                try { if (p != null && File.Exists(p)) File.Delete(p); } catch { }
            lock (thumbs) thumbs.Remove(it.Id);
            RaiseChanged();
        }

        /// <summary>Loads a bitmap without keeping the file locked.</summary>
        public static Bitmap LoadBitmap(string path)
        {
            using (var fs = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete))
            using (var img = Image.FromStream(fs))
            {
                var bmp = new Bitmap(img.Width, img.Height, PixelFormat.Format32bppArgb);
                using (var g = Graphics.FromImage(bmp)) g.DrawImage(img, 0, 0, img.Width, img.Height);
                return bmp;
            }
        }

        public static Bitmap Thumb(CaptureItem it, int size)
        {
            DateTime mt;
            try { mt = File.GetLastWriteTimeUtc(it.Path); } catch { return null; }
            string key = it.Id + "@" + size;
            lock (thumbs)
            {
                KeyValuePair<DateTime, Bitmap> kv;
                if (thumbs.TryGetValue(key, out kv) && kv.Key == mt) return kv.Value;
            }
            Bitmap t;
            try
            {
                using (var src = LoadBitmap(it.Path))
                {
                    double s = Math.Min((double)size / src.Width, (double)size / src.Height);
                    if (s > 1) s = 1;
                    int w = Math.Max(1, (int)(src.Width * s)), h = Math.Max(1, (int)(src.Height * s));
                    t = new Bitmap(w, h);
                    using (var g = Graphics.FromImage(t))
                    {
                        g.InterpolationMode = InterpolationMode.HighQualityBicubic;
                        g.DrawImage(src, 0, 0, w, h);
                    }
                }
            }
            catch { return null; }
            lock (thumbs) thumbs[key] = new KeyValuePair<DateTime, Bitmap>(mt, t);
            return t;
        }

        public static void CopyToClipboard(Bitmap bmp)
        {
            var data = new DataObject();
            data.SetData(DataFormats.Bitmap, true, bmp);
            var ms = new MemoryStream();
            bmp.Save(ms, ImageFormat.Png);
            data.SetData("PNG", false, ms);
            Clipboard.SetDataObject(data, true, 10, 100);
        }

        public static bool CopyToClipboard(CaptureItem it)
        {
            if (it.IsRecording) { try { CopyFileToClipboard(it); return true; } catch { return false; } }
            try
            {
                using (var bmp = LoadBitmap(it.Path)) CopyToClipboard(bmp);
                return true;
            }
            catch (Exception ex)
            {
                MessageBox.Show(L.T("No se pudo copiar: ") + ex.Message, "BuzzPrint");
                return false;
            }
        }

        public static void CopyFileToClipboard(CaptureItem it)
        {
            var sc = new System.Collections.Specialized.StringCollection();
            sc.Add(it.Path);
            Clipboard.SetFileDropList(sc);
        }
    }

    /// <summary>System.Text.Json with the old JavaScriptSerializer shape: public fields, and untyped JSON read as
    /// Dictionary&lt;string, object&gt; / List&lt;object&gt; / string / long / double / bool.</summary>
    public sealed class MiniJson
    {
        static readonly JsonSerializerOptions Options = new JsonSerializerOptions { IncludeFields = true, WriteIndented = true };
        public string Serialize(object value) { return JsonSerializer.Serialize(value, value.GetType(), Options); }
        public T Deserialize<T>(string text)
        {
            if (typeof(T) == typeof(Dictionary<string, object>))
            {
                using (var doc = JsonDocument.Parse(text)) return (T)Plain(doc.RootElement);
            }
            return JsonSerializer.Deserialize<T>(text, Options);
        }
        static object Plain(JsonElement e)
        {
            switch (e.ValueKind)
            {
                case JsonValueKind.Object: return e.EnumerateObject().ToDictionary(p => p.Name, p => Plain(p.Value));
                case JsonValueKind.Array: return e.EnumerateArray().Select(Plain).ToList();
                case JsonValueKind.String: return e.GetString();
                case JsonValueKind.Number: long l; return e.TryGetInt64(out l) ? (object)l : e.GetDouble();
                case JsonValueKind.True: return true;
                case JsonValueKind.False: return false;
                default: return null;
            }
        }
    }
}