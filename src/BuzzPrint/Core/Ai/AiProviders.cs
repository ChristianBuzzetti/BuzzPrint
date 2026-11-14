#if PREVIEW
namespace BuzzPrint.Ai;

/// <summary>A suggested model: id as the API expects it and a short note ("Recommended", "Fast and cheap").</summary>
public sealed record AiModelChoice(string Id, string Note);

/// <summary>A provider the person can pick. Claude goes through Anthropic's SDK; every other entry speaks the
/// OpenAI-compatible chat API at <see cref="BaseUrl"/> (OpenAI itself, Gemini, OpenRouter, Ollama, any custom URL).</summary>
public sealed record AiProvider(
    string Id, string Name, string BaseUrl, IReadOnlyList<AiModelChoice> Models, bool NeedsKey, string KeyHelpUrl, string Hint)
{
    public string SuggestedModel => Models.Count > 0 ? Models[0].Id : "";
    public override string ToString() => Name;
}

public static class AiProviders
{
    public static readonly IReadOnlyList<AiProvider> All =
    [
        new("anthropic", "Claude (Anthropic)", "",
            [new("claude-opus-5-5", "Recomendado: el más capaz"), new("claude-sonnet-5-5", "Equilibrado: rápido y muy bueno"), new("claude-haiku-5-5", "Rápido y económico")],
            true, "https://console.anthropic.com/settings/keys", "Claude lee muy bien pantallas y documentos. La key se crea en console.anthropic.com."),
        new("gemini", "Gemini (Google)", "https://generativelanguage.googleapis.com/v1beta/openai", [], true, "https://aistudio.google.com/apikey",
            "Tocá «Ver modelos» para elegir entre los modelos de tu cuenta. Las keys gratuitas tienen pocos pedidos por minuto."),
        new("openai", "ChatGPT (OpenAI)", "https://api.openai.com/v1", [], true, "https://platform.openai.com/api-keys",
            "Tocá «Ver modelos» para elegir un modelo con visión de tu cuenta de OpenAI."),
        new("openrouter", "OpenRouter (muchos modelos)", "https://openrouter.ai/api/v1", [], true, "https://openrouter.ai/keys",
            "Tocá «Ver modelos» o escribí el id tal como figura en openrouter.ai/models."),
        new("ollama", "Ollama (local, sin internet)", "http://localhost:11434/v1", [], false, "https://ollama.com/download",
            "Nada sale de tu PC. Necesita Ollama abierto y un modelo con visión descargado (ollama pull …)."),
        new("custom", "Otro compatible con OpenAI", "", [], true, "",
            "Cualquier servicio con API compatible con OpenAI: LM Studio, Azure OpenAI, Mistral, etc."),
    ];

    public static AiProvider Get(string id) => All.FirstOrDefault(p => p.Id == id) ?? All[0];

    public static string ModelFor(AiConnection c) => c.Model.Trim().Length > 0 ? c.Model.Trim() : Get(c.Provider).SuggestedModel;

    public static string BaseUrlFor(AiConnection c) => c.Provider == "custom" ? c.BaseUrl.Trim().TrimEnd('/') : Get(c.Provider).BaseUrl;
}

/// <summary>Saved connections and the active one, with a one-time migration from the single-provider settings.</summary>
public static class AiConnections
{
    /// <summary>The connection AI features use, or null when none is set up.</summary>
    public static AiConnection? Active(Settings s)
    {
        Migrate(s);
        return s.AiConnections.FirstOrDefault(c => c.Id == s.AiActiveId) ?? s.AiConnections.FirstOrDefault();
    }

    public static string DisplayName(this AiConnection c)
    {
        if (c.Name.Trim().Length > 0) return c.Name.Trim();
        string provider = AiProviders.Get(c.Provider).Name;
        int paren = provider.IndexOf(" (", StringComparison.Ordinal);
        if (paren > 0) provider = provider[..paren];
        string model = AiProviders.ModelFor(c);
        return model.Length > 0 ? provider + " · " + model : provider;
    }

    /// <summary>The first build stored one provider in AiProvider/AiModel/AiBaseUrl with its key named after the
    /// provider; that becomes a connection whose Id is the provider, so the stored key still matches.</summary>
    static void Migrate(Settings s)
    {
        if (s.AiConnections.Count > 0) return;
        var p = AiProviders.Get(s.AiProvider);
        if (!AiKeys.Has(p.Id) && !(p.Id == "ollama" && s.AiModel.Trim().Length > 0)) return;
        s.AiConnections.Add(new AiConnection { Id = p.Id, Provider = p.Id, Model = s.AiModel, BaseUrl = s.AiBaseUrl });
        s.AiActiveId = p.Id;
        Store.SaveSettings();
    }
}
#endif
