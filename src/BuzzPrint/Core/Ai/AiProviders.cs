#if PREVIEW
namespace BuzzPrint.Ai;

/// <summary>A provider the person can pick. Claude goes through Anthropic's SDK; every other entry speaks the
/// OpenAI-compatible chat API at <see cref="BaseUrl"/> (OpenAI itself, Gemini, OpenRouter, Ollama, any custom URL).</summary>
public sealed record AiProvider(
    string Id, string Name, string BaseUrl, string SuggestedModel, bool NeedsKey, string KeyHelpUrl, string Hint);

public static class AiProviders
{
    public static readonly IReadOnlyList<AiProvider> All =
    [
        new("anthropic", "Claude (Anthropic)", "", "claude-opus-5-5", true, "https://console.anthropic.com/settings/keys",
            "Recomendado. Para algo más barato y rápido: claude-haiku-5-5."),
        new("openai", "ChatGPT (OpenAI)", "https://api.openai.com/v1", "", true, "https://platform.openai.com/api-keys",
            "Escribí el nombre de un modelo con visión de tu cuenta de OpenAI."),
        new("gemini", "Gemini (Google)", "https://generativelanguage.googleapis.com/v1beta/openai", "", true, "https://aistudio.google.com/apikey",
            "Escribí el nombre de un modelo Gemini de tu cuenta de Google AI Studio."),
        new("openrouter", "OpenRouter (muchos modelos)", "https://openrouter.ai/api/v1", "", true, "https://openrouter.ai/keys",
            "Escribí el id del modelo tal como figura en openrouter.ai/models."),
        new("ollama", "Ollama (local, sin internet)", "http://localhost:11434/v1", "", false, "https://ollama.com/download",
            "Nada sale de tu PC. Necesita Ollama instalado y un modelo con visión descargado (ollama pull …)."),
        new("custom", "Otro compatible con OpenAI", "", "", true, "",
            "Cualquier servicio con API compatible con OpenAI: LM Studio, Azure OpenAI, Mistral, etc."),
    ];

    public static AiProvider Get(string id) => All.FirstOrDefault(p => p.Id == id) ?? All[0];

    /// <summary>The model to use: what the person typed, else the provider's suggestion.</summary>
    public static string ModelFor(Settings s) => s.AiModel.Trim().Length > 0 ? s.AiModel.Trim() : Get(s.AiProvider).SuggestedModel;

    /// <summary>Endpoint for OpenAI-compatible providers: the preset URL, or the one typed for "custom".</summary>
    public static string BaseUrlFor(Settings s) => s.AiProvider == "custom" ? s.AiBaseUrl.Trim().TrimEnd('/') : Get(s.AiProvider).BaseUrl;
}
#endif
