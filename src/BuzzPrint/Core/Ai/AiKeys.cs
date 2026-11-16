using System.IO;
using System.Security.Cryptography;
using System.Text;

namespace BuzzPrint.Ai;

/// <summary>
/// API keys, one per connection and provider, encrypted with Windows DPAPI for the current Windows user and kept apart
/// from the settings JSON (LocalAppData\&lt;app&gt;\ai-keys\&lt;connection&gt;.&lt;provider&gt;.bin). A key is tied to the provider it
/// was entered for, so switching a connection from Gemini to ChatGPT does not send the Gemini key to OpenAI. Another
/// user, or a copy of the file on another PC, cannot decrypt them. Keys are never logged or shown back in full.
/// </summary>
public static class AiKeys
{
    static readonly byte[] Entropy = Encoding.UTF8.GetBytes("BuzzPrint.AiKey.v1");
    static string Dir => Path.Combine(DataFolder.Root, "ai-keys");
    static string PathFor(string id, string provider) => Path.Combine(Dir, id + "." + provider + ".bin");
    // The first preview stored "<provider>.bin" for its single connection, whose id is the provider name.
    static string LegacyPath(string id) => Path.Combine(Dir, id + ".bin");

    public static void Save(string id, string provider, string key)
    {
        if (string.IsNullOrWhiteSpace(key)) { Delete(id); return; }
        Directory.CreateDirectory(Dir);
        foreach (var old in Files(id)) try { File.Delete(old); } catch (IOException) { }
        File.WriteAllBytes(PathFor(id, provider), ProtectedData.Protect(Encoding.UTF8.GetBytes(key.Trim()), Entropy, DataProtectionScope.CurrentUser));
    }

    public static string? Load(string id, string provider)
    {
        string p = PathFor(id, provider);
        if (!File.Exists(p))
        {
            p = LegacyPath(id);
            if (!File.Exists(p) || (id != provider && AiProviders.All.Any(x => x.Id == id))) return null;
        }
        try { return Encoding.UTF8.GetString(ProtectedData.Unprotect(File.ReadAllBytes(p), Entropy, DataProtectionScope.CurrentUser)); }
        catch (Exception ex) when (ex is IOException or CryptographicException or UnauthorizedAccessException) { return null; }
    }

    public static bool Has(string id, string provider) => Load(id, provider) != null;

    /// <summary>Legacy probe used by the migration from the single-provider settings.</summary>
    public static bool HasLegacy(string provider) => File.Exists(LegacyPath(provider));

    public static void Delete(string id)
    {
        foreach (var f in Files(id)) try { File.Delete(f); } catch (IOException) { } catch (UnauthorizedAccessException) { }
    }

    static IEnumerable<string> Files(string id) =>
        Directory.Exists(Dir) ? Directory.GetFiles(Dir, id + ".*").Where(f => Path.GetFileName(f).StartsWith(id + ".", StringComparison.Ordinal)) : [];

    /// <summary>"sk-ant…4f2a": enough to recognize which key is stored, never the whole key.</summary>
    public static string Masked(string key) => key.Length <= 10 ? new string('•', key.Length) : key[..6] + "…" + key[^4..];
}
