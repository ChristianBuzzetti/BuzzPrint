using System.Windows.Markup;

namespace BuzzPrint;

/// <summary>UI language. Source strings are Spanish; English (the default) comes from <see cref="LocEn"/>.
/// The language is read once at startup; switching restarts the app so every window is rebuilt in the new language.</summary>
public static class L
{
    public static string Lang { get; set; } = "en";
    public static bool Es => Lang == "es";
    public static string T(string es) => Es ? es : LocEn.Map.TryGetValue(es, out var en) ? en : es;
    public static string F(string es, params object[] args) => string.Format(T(es), args);
    /// <summary>"1 captura" / "3 capturas" in the current language.</summary>
    public static string Count(int n, string esOne, string esMany) => n + " " + T(n == 1 ? esOne : esMany);
}

/// <summary>XAML: Text="{local:T 'Texto en español'}".</summary>
[MarkupExtensionReturnType(typeof(string))]
public sealed class TExtension : MarkupExtension
{
    public TExtension() { }
    public TExtension(string text) { Text = text; }
    [ConstructorArgument("text")] public string Text { get; set; } = "";
    public override object ProvideValue(IServiceProvider serviceProvider) => L.T(Text);
}
