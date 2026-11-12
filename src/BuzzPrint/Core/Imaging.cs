using System.IO;
using System.Windows.Media;
using System.Windows.Media.Imaging;

namespace BuzzPrint;

/// <summary>WPF thumbnails (cached by file and write time) and GDI+ → WPF bitmap conversion.</summary>
public static class Imaging
{
    static readonly Dictionary<string, (DateTime Stamp, BitmapSource Image)> cache = new(StringComparer.OrdinalIgnoreCase);

    public static BitmapSource? Thumb(string path, int width)
    {
        DateTime stamp;
        try { stamp = File.GetLastWriteTimeUtc(path); } catch { return null; }
        string key = path + "@" + width;
        if (cache.TryGetValue(key, out var hit) && hit.Stamp == stamp) return hit.Image;
        try
        {
            var bi = new BitmapImage();
            using (var fs = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete))
            {
                bi.BeginInit();
                bi.CacheOption = BitmapCacheOption.OnLoad;
                bi.DecodePixelWidth = width;
                bi.StreamSource = fs;
                bi.EndInit();
            }
            bi.Freeze();
            cache[key] = (stamp, bi);
            return bi;
        }
        catch { return null; }
    }

    public static void Forget(string path)
    {
        foreach (var k in cache.Keys.Where(k => k.StartsWith(path + "@", StringComparison.OrdinalIgnoreCase)).ToList()) cache.Remove(k);
    }

    /// <summary>Copies a 32bpp GDI+ bitmap into a frozen WPF bitmap (96 DPI so 1 px = 1 DIP).</summary>
    public static BitmapSource ToSource(System.Drawing.Bitmap bmp)
    {
        var rect = new System.Drawing.Rectangle(0, 0, bmp.Width, bmp.Height);
        var data = bmp.LockBits(rect, System.Drawing.Imaging.ImageLockMode.ReadOnly, System.Drawing.Imaging.PixelFormat.Format32bppPArgb);
        try
        {
            var src = BitmapSource.Create(bmp.Width, bmp.Height, 96, 96, PixelFormats.Pbgra32, null, data.Scan0, data.Stride * bmp.Height, data.Stride);
            src.Freeze();
            return src;
        }
        finally { bmp.UnlockBits(data); }
    }
}
