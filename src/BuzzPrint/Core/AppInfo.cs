namespace BuzzPrint;

/// <summary>Names that tell the stable app and the AI preview apart, so both can be installed and open at once:
/// window and tray titles, the settings folder in LocalAppData, the single-instance lock and the startup entry.
/// Captures and recordings stay in the same Pictures / Videos folders, so the preview sees the real library.</summary>
public static class AppInfo
{
#if PREVIEW
    public const string Name = "BuzzPrint AI Preview";
    public const string Id = "BuzzPrintPreview";
    public const string DataFolderName = "BuzzPrint Preview";
    public const bool IsPreview = true;
#else
    public const string Name = "BuzzPrint";
    public const string Id = "BuzzPrint";
    public const string DataFolderName = "BuzzPrint";
    public const bool IsPreview = false;
#endif
}
