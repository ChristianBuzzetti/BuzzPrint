using System.IO;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;

namespace BuzzPrint;

/// <summary>
/// BuzzPrint.exe --icon DIR : draws the app mark (BobertoMark) as vectors and writes buzzprint.ico (16–256, PNG entries),
/// buzzprint.png (256) and a preview sheet to DIR. The mark is Boberto's black rounded-hexagon head with two white
/// eyes on a rose tile; each size is drawn on its own pixel grid so it stays crisp at 16 px.
/// </summary>
static class IconMaker
{
    public static Task RunAsync(string dir)
    {
        Directory.CreateDirectory(dir);
        var log = new List<string>();
        try
        {
            var sizes = new[] { 16, 20, 24, 32, 40, 48, 64, 128, 256 };
            var entries = new List<(int size, byte[] data)>();
            foreach (int s in sizes)
            {
                byte[] png = Png(Render(s));
                entries.Add((s, png));
                if (s == 256) File.WriteAllBytes(Path.Combine(dir, "buzzprint.png"), png);
            }
            WriteIco(Path.Combine(dir, "buzzprint.ico"), entries);
            File.WriteAllBytes(Path.Combine(dir, "preview.png"), Png(Preview(sizes)));
            log.Add("OK");
        }
        catch (Exception ex) { log.Add("FAIL " + ex); }
        File.WriteAllLines(Path.Combine(dir, "icon.log"), log);
        App.Current.ExitApp();
        return Task.CompletedTask;
    }

    static readonly Brush Rose = new SolidColorBrush(Color.FromRgb(0xF4, 0x3F, 0x5E));

    /// <summary>The mark at one size, drawn in pixel units of that size.</summary>
    public static BitmapSource Render(int size)
    {
        var visual = new DrawingVisual();
        using (var g = visual.RenderOpen()) BobertoMark.Draw(g, size, new MarkStyle(), Rose);   // the shipped icon is always the default mark
        var bmp = new RenderTargetBitmap(size, size, 96, 96, PixelFormats.Pbgra32);
        bmp.Render(visual);
        bmp.Freeze();
        return bmp;
    }

    // Preview sheet: every size on dark and light, at 1× and magnified 4×, to judge the small sizes.
    static BitmapSource Preview(int[] sizes)
    {
        int w = 40 + sizes.Sum(x => x + 24), h = 2 * (256 + 40) + 160;
        var visual = new DrawingVisual();
        using (var g = visual.RenderOpen())
        {
            g.DrawRectangle(new SolidColorBrush(Color.FromRgb(0x1B, 0x1F, 0x27)), null, new Rect(0, 0, w, h / 2));
            g.DrawRectangle(new SolidColorBrush(Color.FromRgb(0xF3, 0xF4, 0xF6)), null, new Rect(0, h / 2, w, h / 2));
            for (int row = 0; row < 2; row++)
            {
                double x = 20, y0 = row * h / 2 + 20;
                foreach (int s in sizes) { g.DrawImage(Render(s), new Rect(x, y0, s, s)); x += s + 24; }
                x = 20;
                foreach (int s in sizes.Where(z => z <= 32))
                {
                    var img = Render(s);
                    var group = new DrawingGroup(); RenderOptions.SetBitmapScalingMode(group, BitmapScalingMode.NearestNeighbor);
                    using (var gg = group.Open()) gg.DrawImage(img, new Rect(x, y0 + 260, s * 4, s * 4));
                    g.DrawDrawing(group);
                    x += s * 4 + 24;
                }
            }
        }
        var bmp = new RenderTargetBitmap(w, h, 96, 96, PixelFormats.Pbgra32);
        bmp.Render(visual);
        return bmp;
    }

    static byte[] Png(BitmapSource src)
    {
        var enc = new PngBitmapEncoder();
        enc.Frames.Add(BitmapFrame.Create(src));
        using var ms = new MemoryStream();
        enc.Save(ms);
        return ms.ToArray();
    }

    static void WriteIco(string path, List<(int size, byte[] data)> entries)
    {
        using var w = new BinaryWriter(File.Create(path));
        w.Write((short)0); w.Write((short)1); w.Write((short)entries.Count);
        int offset = 6 + 16 * entries.Count;
        foreach (var (size, data) in entries)
        {
            w.Write((byte)(size >= 256 ? 0 : size)); w.Write((byte)(size >= 256 ? 0 : size));
            w.Write((byte)0); w.Write((byte)0); w.Write((short)1); w.Write((short)32);
            w.Write(data.Length); w.Write(offset);
            offset += data.Length;
        }
        foreach (var (_, data) in entries) w.Write(data);
    }
}
