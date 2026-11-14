#if PREVIEW
using System.IO;
using System.Security.Cryptography;
using System.Text;

namespace BuzzPrint.Ai;

/// <summary>
/// API keys, one per provider, encrypted with Windows DPAPI for the current Windows user and kept apart from the
/// settings JSON (LocalAppData\&lt;app&gt;\ai-keys\&lt;provider&gt;.bin). Another user, or a copy of the file on another PC,
/// cannot decrypt them. Keys are never logged or shown back in full.
/// </summary>
public static class AiKeys
{
    static readonly byte[] Entropy = Encoding.UTF8.GetBytes("BuzzPrint.AiKey.v1");
    static string PathFor(string provider) => Path.Combine(DataFolder.Root, "ai-keys", provider + ".bin");

    public static void Save(string provider, string key)
    {
        string p = PathFor(provider);
        if (string.IsNullOrWhiteSpace(key)) { Delete(provider); return; }
        Directory.CreateDirectory(Path.GetDirectoryName(p)!);
        byte[] data = ProtectedData.Protect(Encoding.UTF8.GetBytes(key.Trim()), Entropy, DataProtectionScope.CurrentUser);
        File.WriteAllBytes(p, data);
    }

    public static string? Load(string provider)
    {
        try
        {
            string p = PathFor(provider);
            if (!File.Exists(p)) return null;
            return Encoding.UTF8.GetString(ProtectedData.Unprotect(File.ReadAllBytes(p), Entropy, DataProtectionScope.CurrentUser));
        }
        catch (Exception ex) when (ex is IOException or CryptographicException or UnauthorizedAccessException) { return null; }
    }

    public static bool Has(string provider) => File.Exists(PathFor(provider));

    public static void Delete(string provider)
    {
        try { File.Delete(PathFor(provider)); } catch (IOException) { } catch (UnauthorizedAccessException) { }
    }

    /// <summary>"sk-ant-…4f2a": enough to recognize which key is stored, never the whole key.</summary>
    public static string Masked(string key) => key.Length <= 10 ? new string('•', key.Length) : key[..6] + "…" + key[^4..];
}
#endif
