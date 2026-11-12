using System.Globalization;
using System.IO;
using System.Text;

namespace BuzzPrint;

// LocalAppData\BuzzPrint holds appearance, capture settings and the Boberto runtime profile.
// Captures themselves live in Pictures\BuzzPrint so they are easy to find and back up.
public static class DataFolder
{
    public const string Name = "BuzzPrint";
    public static string Root => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), Name);
    public static string File(string name) => Path.Combine(Root, name);
}

public static class AtomicFiles
{
    // Stage alongside the destination so rename/replace stays on the same volume; a failed write keeps the original.
    public static void SaveUtf8(string path, string text)
    {
        string fullPath = Path.GetFullPath(path);
        Directory.CreateDirectory(Path.GetDirectoryName(fullPath)!);
        string staging = fullPath + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            System.IO.File.WriteAllText(staging, text, new UTF8Encoding(false));
            if (System.IO.File.Exists(fullPath)) System.IO.File.Replace(staging, fullPath, null);
            else System.IO.File.Move(staging, fullPath);
        }
        finally { if (System.IO.File.Exists(staging)) System.IO.File.Delete(staging); }
    }
}
