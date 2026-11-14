using System.Globalization;
using System.Text.RegularExpressions;

namespace BuzzPrint.Ai;

/// <summary>
/// Turns a provider's raw model catalogue into the short list people actually want: chat models only, no previews,
/// dated snapshots or aliases, the whole newest generation of each family, plus the newest light model (lite, mini,
/// nano, haiku) of an older generation when the newest one has none. It reads the version from the id, so a new
/// generation shows up by itself the day the provider lists it — nothing here names a specific model.
/// </summary>
public static partial class ModelCuration
{
    // Not text chat: media, speech, embeddings, search, tools-only and legacy completion models.
    static readonly string[] NotChat = ["embed", "tts", "whisper", "dall-e", "moderation", "transcribe", "audio", "realtime", "image",
        "imagen", "aqa", "veo", "search", "lyria", "banana", "sora", "computer-use", "robotics", "learnlm", "native", "live",
        "babbage", "davinci", "codex", "deep-research", "guard", "vision-only"];
    static readonly string[] NotStable = ["preview", "exp", "latest", "beta", "tuning", "test"];
    static readonly string[] Light = ["lite", "mini", "nano", "haiku", "small", "8b"];

    /// <summary>Families each hosted provider is curated to; other providers list every chat model they have.</summary>
    static readonly Dictionary<string, string[]> Families = new()
    {
        ["gemini"] = ["gemini"],
        ["openai"] = ["gpt"],
        ["anthropic"] = ["claude"],
    };

    public static bool IsChat(string id) => !NotChat.Any(n => id.Contains(n, StringComparison.OrdinalIgnoreCase));

    [GeneratedRegex(@"^(?<pre>[^0-9]*?)(?<ver>\d+(?:[.\-]\d{1,2})?)(?=$|[-:_a-z])", RegexOptions.IgnoreCase)]
    private static partial Regex VersionPattern();
    [GeneratedRegex(@"-(\d{3,4}|\d{4}-\d{2}-\d{2}|\d{8})$")]
    private static partial Regex Snapshot();

    record Parsed(string Id, string Family, double Version, bool IsLight);

    static Parsed? Parse(string id)
    {
        string name = id.Contains('/') ? id[(id.LastIndexOf('/') + 1)..] : id;
        string vendor = id.Contains('/') ? id[..id.LastIndexOf('/')] + "/" : "";
        if (Snapshot().IsMatch(name)) return null;
        var m = VersionPattern().Match(name);
        if (!m.Success) return null;
        string family = new string(m.Groups["pre"].Value.TakeWhile(char.IsLetter).ToArray()).ToLowerInvariant();
        if (family.Length == 0) return null;
        string ver = m.Groups["ver"].Value.Replace('-', '.');
        if (!double.TryParse(ver, NumberStyles.Float, CultureInfo.InvariantCulture, out double v)) return null;
        bool light = name.Split(['-', '_', '.', ':']).Any(t => Light.Contains(t, StringComparer.OrdinalIgnoreCase));   // whole words: "gemini" is not "mini"
        return new Parsed(id, vendor + family, v, light);
    }

    /// <summary>The curated list, newest first. Falls back to every chat model when nothing in the catalogue parses.</summary>
    public static List<string> Curate(string providerId, IEnumerable<string> ids)
    {
        var chat = ids.Where(IsChat).Distinct().ToList();
        var stable = chat.Where(id => !NotStable.Any(n => id.Contains(n, StringComparison.OrdinalIgnoreCase))).ToList();
        var parsed = stable.Select(Parse).OfType<Parsed>().ToList();
        if (Families.TryGetValue(providerId, out var allowed))
            parsed = parsed.Where(p => allowed.Any(f => p.Family.EndsWith(f, StringComparison.Ordinal))).ToList();
        else if (providerId is "ollama" or "custom") return chat.OrderBy(x => x, StringComparer.OrdinalIgnoreCase).ToList();
        if (parsed.Count == 0) return chat.OrderBy(x => x, StringComparer.OrdinalIgnoreCase).ToList();

        var keep = new List<Parsed>();
        foreach (var family in parsed.GroupBy(p => p.Family))
        {
            double newest = family.Max(p => p.Version);
            var top = family.Where(p => p.Version == newest).ToList();
            keep.AddRange(top);
            if (top.Any(p => p.IsLight)) continue;
            var olderLight = family.Where(p => p.IsLight && p.Version < newest).ToList();
            if (olderLight.Count == 0) continue;
            double best = olderLight.Max(p => p.Version);
            keep.AddRange(olderLight.Where(p => p.Version == best));
        }
        return keep.OrderByDescending(p => p.Version).ThenBy(p => p.IsLight).ThenBy(p => p.Id, StringComparer.OrdinalIgnoreCase)
                   .Select(p => p.Id).ToList();
    }
}
