namespace BuzzPrint;

/// <summary>The app's names: window and tray titles, the settings folder in LocalAppData, the single-instance lock
/// and the startup entry. There is one BuzzPrint, with AI built in (off until the person turns it on).</summary>
public static class AppInfo
{
    public const string Name = "BuzzPrint";
    public const string Id = "BuzzPrint";
    public const string DataFolderName = "BuzzPrint";
    /// <summary>Settings folder of the former "BuzzPrint AI Preview" build (0.5.0 on GitHub); its AI settings are
    /// brought over once.</summary>
    public const string FormerPreviewFolderName = "BuzzPrint Preview";
}
