using DColor = System.Drawing.Color;

namespace BuzzPrint;

/// <summary>The active DBMON palette as System.Drawing colors, for the GDI+ capture overlay.</summary>
public static class DrawTheme
{
    static DColor Get(string key)
    {
        var c = (System.Windows.Media.Color)System.Windows.Media.ColorConverter.ConvertFromString(Themes.Current.Colors[key]);
        return DColor.FromArgb(c.A, c.R, c.G, c.B);
    }
    public static DColor Accent => Get("Accent");
    public static DColor Header => Get("Header");
    public static DColor Ink => Get("Ink");
    public static DColor Muted => Get("Muted");
    public static DColor OnAccent => Get("OnAccent");
    public static DColor A(DColor c, double alpha) => DColor.FromArgb((int)Math.Round(255 * alpha), c);
    public static System.Drawing.Font Mono(float size)
    {
        return new System.Drawing.Font("Consolas", size);
    }
}
