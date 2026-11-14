#if PREVIEW
using System.IO;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using Anthropic;
using Anthropic.Exceptions;
using Anthropic.Models.Beta.Messages;

namespace BuzzPrint.Ai;

/// <summary>What went wrong, in words the person can act on (bad key, no credit, model name, offline…).</summary>
public sealed class AiException(string message, Exception? inner = null) : Exception(message, inner);

/// <summary>One request: instructions, the person's text and optional PNG images (captures).</summary>
public sealed record AiRequest(string System, string Prompt, IReadOnlyList<byte[]> Images, int MaxTokens = 4096);

/// <summary>
/// Sends one request to the configured provider and returns the text answer. Nothing is sent unless the person
/// asked for it with an explicit action; callers show what will be sent first.
/// </summary>
public abstract class AiClient
{
    public abstract Task<string> CompleteAsync(AiRequest request, CancellationToken ct = default);

    /// <summary>Client for the current settings, or an AiException saying what is missing.</summary>
    /// <summary>Client for the active connection, or an AiException saying what is missing.</summary>
    public static AiClient FromSettings(Settings s)
    {
        if (!s.AiEnabled) throw new AiException(L.T("La IA está apagada. Activala en IA → Configurar IA…"));
        var c = AiConnections.Active(s) ?? throw new AiException(L.T("Todavía no hay ninguna conexión de IA. Agregá una en IA → Configurar IA…"));
        return FromConnection(c);
    }

    /// <param name="keyOverride">A key typed but not saved yet (connection test).</param>
    public static AiClient FromConnection(AiConnection c, string? keyOverride = null)
    {
        var p = AiProviders.Get(c.Provider);
        string model = AiProviders.ModelFor(c);
        if (model.Length == 0) throw new AiException(L.T("Falta elegir el modelo de la conexión."));
        string? key = keyOverride ?? AiKeys.Load(c.Id);
        if (p.NeedsKey && string.IsNullOrWhiteSpace(key)) throw new AiException(L.T("Falta la API key de la conexión."));
        if (p.Id == "anthropic") return new ClaudeClient(key!, model);
        string url = AiProviders.BaseUrlFor(c);
        if (url.Length == 0) throw new AiException(L.T("Falta la URL del servicio."));
        return new OpenAiCompatibleClient(url, key, model);
    }

    /// <summary>Model ids the provider offers to this key (chat and vision candidates first). Empty when it can't list.</summary>
    public static async Task<List<string>> ListModelsAsync(AiConnection c, string? keyOverride, CancellationToken ct = default)
    {
        var p = AiProviders.Get(c.Provider);
        string? key = keyOverride ?? AiKeys.Load(c.Id);
        if (p.NeedsKey && string.IsNullOrWhiteSpace(key)) throw new AiException(L.T("Escribí la API key para ver los modelos de tu cuenta."));
        if (p.Id == "anthropic") return await ClaudeClient.ListAsync(key!, ct);
        string url = AiProviders.BaseUrlFor(c);
        if (url.Length == 0) throw new AiException(L.T("Falta la URL del servicio."));
        return await OpenAiCompatibleClient.ListAsync(url, key, ct);
    }
}

/// <summary>Claude through Anthropic's official C# SDK. Server-side fallback is on, so a request a safety
/// classifier declines is answered by the model Anthropic routes it to instead of failing.</summary>
sealed class ClaudeClient(string apiKey, string model) : AiClient
{
    readonly AnthropicClient client = new() { ApiKey = apiKey };

    public static async Task<List<string>> ListAsync(string apiKey, CancellationToken ct)
    {
        try
        {
            var page = await new AnthropicClient { ApiKey = apiKey }.Models.List(cancellationToken: ct);
            return page.Items.Select(m => m.ID).ToList();
        }
        catch (AnthropicUnauthorizedException ex) { throw new AiException(L.T("La API key de Claude no es válida."), ex); }
        catch (AnthropicIOException ex) { throw new AiException(L.T("No hay conexión con el servicio. Revisá internet o el proxy."), ex); }
        catch (AnthropicApiException ex) { throw new AiException(ex.Message, ex); }
    }

    public override async Task<string> CompleteAsync(AiRequest request, CancellationToken ct = default)
    {
        var content = new List<BetaContentBlockParam>();
        foreach (var png in request.Images)
            content.Add(new BetaImageBlockParam { Source = new BetaBase64ImageSource { Data = Convert.ToBase64String(png), MediaType = MediaType.ImagePng } });
        content.Add(new BetaTextBlockParam { Text = request.Prompt });
        try
        {
            var response = await client.Beta.Messages.Create(new MessageCreateParams
            {
                Model = model,
                MaxTokens = request.MaxTokens,
                System = request.System,
                Messages = [new BetaUserMessageParam { Content = content }],
                Betas = ["server-side-fallback-2026-07-01"],
                Fallbacks = new Default(),
            }, ct);
            if (response.StopReason == "refusal")
                throw new AiException(L.T("El modelo no quiso responder este pedido.") + (response.StopDetails?.Explanation is { Length: > 0 } why ? " " + why : ""));
            var text = string.Join("", response.Content.Select(b => b.Value).OfType<BetaTextBlock>().Select(t => t.Text));
            return text.Trim();
        }
        catch (AnthropicUnauthorizedException ex) { throw new AiException(L.T("La API key de Claude no es válida."), ex); }
        catch (AnthropicForbiddenException ex) { throw new AiException(L.T("La API key no tiene permiso para este modelo."), ex); }
        catch (AnthropicNotFoundException ex) { throw new AiException(L.F("El modelo «{0}» no existe o tu cuenta no tiene acceso.", model), ex); }
        catch (AnthropicRateLimitException ex) { throw new AiException(L.T("Demasiados pedidos seguidos o sin saldo. Probá en un rato o revisá tu cuenta."), ex); }
        catch (AnthropicBadRequestException ex) { throw new AiException(L.T("El pedido fue rechazado: ") + ex.Message, ex); }
        catch (Anthropic5xxException ex) { throw new AiException(L.T("El servicio de Claude tuvo un error. Probá de nuevo."), ex); }
        catch (AnthropicIOException ex) { throw new AiException(L.T("No hay conexión con el servicio. Revisá internet o el proxy."), ex); }
        catch (AnthropicApiException ex) { throw new AiException(ex.Message, ex); }
    }
}

/// <summary>Any provider with the OpenAI-compatible chat completions API (OpenAI, Gemini, OpenRouter, Ollama…).</summary>
sealed class OpenAiCompatibleClient(string baseUrl, string? apiKey, string model) : AiClient
{
    static readonly HttpClient Http = new(new HttpClientHandler
    {
        UseProxy = true, DefaultProxyCredentials = System.Net.CredentialCache.DefaultCredentials
    }) { Timeout = TimeSpan.FromMinutes(5) };

    static readonly string[] NotChat = ["embed", "tts", "whisper", "dall-e", "moderation", "transcribe", "audio", "realtime", "image-", "imagen", "aqa", "veo", "search"];

    public static async Task<List<string>> ListAsync(string baseUrl, string? apiKey, CancellationToken ct)
    {
        using var msg = new HttpRequestMessage(HttpMethod.Get, baseUrl + "/models");
        if (!string.IsNullOrWhiteSpace(apiKey)) msg.Headers.Authorization = new AuthenticationHeaderValue("Bearer", apiKey);
        HttpResponseMessage res;
        try { res = await Http.SendAsync(msg, ct); }
        catch (HttpRequestException ex) { throw new AiException(baseUrl.Contains("localhost") ? L.T("No responde Ollama en esta PC. ¿Está instalado y abierto?") : L.T("No hay conexión con el servicio. Revisá internet o el proxy."), ex); }
        string text = await res.Content.ReadAsStringAsync(ct);
        if (!res.IsSuccessStatusCode) throw new AiException((int)res.StatusCode is 401 or 403 ? L.T("La API key no es válida.") : L.T("El servicio devolvió un error: ") + (int)res.StatusCode);
        try
        {
            var ids = new List<string>();
            foreach (var m in JsonNode.Parse(text)?["data"]?.AsArray() ?? [])
            {
                string id = m?["id"]?.GetValue<string>() ?? "";
                if (id.StartsWith("models/", StringComparison.Ordinal)) id = id["models/".Length..];
                if (id.Length > 0 && !NotChat.Any(n => id.Contains(n, StringComparison.OrdinalIgnoreCase))) ids.Add(id);
            }
            return ids.Distinct().OrderBy(x => x, StringComparer.OrdinalIgnoreCase).ToList();
        }
        catch (Exception ex) when (ex is JsonException or InvalidOperationException) { throw new AiException(L.T("La respuesta del servicio no se pudo leer."), ex); }
    }

    public override async Task<string> CompleteAsync(AiRequest request, CancellationToken ct = default)
    {
        var userContent = new JsonArray { new JsonObject { ["type"] = "text", ["text"] = request.Prompt } };
        foreach (var png in request.Images)
            userContent.Add(new JsonObject
            {
                ["type"] = "image_url",
                ["image_url"] = new JsonObject { ["url"] = "data:image/png;base64," + Convert.ToBase64String(png) }
            });
        var body = new JsonObject
        {
            ["model"] = model,
            ["max_tokens"] = request.MaxTokens,
            ["messages"] = new JsonArray
            {
                new JsonObject { ["role"] = "system", ["content"] = request.System },
                new JsonObject { ["role"] = "user", ["content"] = userContent },
            },
        };
        using var msg = new HttpRequestMessage(HttpMethod.Post, baseUrl + "/chat/completions")
        {
            Content = new StringContent(body.ToJsonString(), Encoding.UTF8, "application/json")
        };
        if (!string.IsNullOrWhiteSpace(apiKey)) msg.Headers.Authorization = new AuthenticationHeaderValue("Bearer", apiKey);
        HttpResponseMessage res;
        // Free tiers allow few requests per minute: on 429 wait what the service asks (or 20 s) and retry, up to 3 times.
        for (int attempt = 0; ; attempt++)
        {
            using var send = new HttpRequestMessage(HttpMethod.Post, msg.RequestUri) { Content = new StringContent(body.ToJsonString(), Encoding.UTF8, "application/json") };
            if (msg.Headers.Authorization != null) send.Headers.Authorization = msg.Headers.Authorization;
            try { res = await Http.SendAsync(send, ct); }
            catch (HttpRequestException ex)
            {
                throw new AiException(baseUrl.Contains("localhost") ? L.T("No responde Ollama en esta PC. ¿Está instalado y abierto?") : L.T("No hay conexión con el servicio. Revisá internet o el proxy."), ex);
            }
            int code = (int)res.StatusCode;
            if (code is not (429 or 500 or 502 or 503 or 504) || attempt >= 3) break;
            var wait = res.Headers.RetryAfter?.Delta ?? TimeSpan.FromSeconds(code == 429 ? 20 : 6 * (attempt + 1));
            res.Dispose();
            await Task.Delay(wait > TimeSpan.FromSeconds(60) ? TimeSpan.FromSeconds(60) : wait, ct);
        }
        string text = await res.Content.ReadAsStringAsync(ct);
        if (!res.IsSuccessStatusCode)
        {
            string detail = ErrorMessage(text);
            throw new AiException((int)res.StatusCode switch
            {
                401 => L.T("La API key no es válida."),
                403 => L.T("La API key no tiene permiso para este modelo."),
                404 => L.F("El modelo «{0}» no existe o la URL es incorrecta.", model),
                429 => L.T("Demasiados pedidos seguidos o sin saldo. Probá en un rato o revisá tu cuenta."),
                500 or 502 or 503 or 504 => L.T("El servicio está saturado o con problemas en este momento. Probá de nuevo en unos minutos o usá otra conexión."),
                _ => L.T("El servicio devolvió un error: ") + (int)res.StatusCode,
            } + (detail.Length > 0 ? "\n" + detail : ""));
        }
        try
        {
            var root = JsonNode.Parse(text);
            return (root?["choices"]?[0]?["message"]?["content"]?.GetValue<string>() ?? "").Trim();
        }
        catch (Exception ex) when (ex is JsonException or InvalidOperationException) { throw new AiException(L.T("La respuesta del servicio no se pudo leer."), ex); }
    }

    static string ErrorMessage(string body)
    {
        try { return JsonNode.Parse(body)?["error"]?["message"]?.GetValue<string>() ?? ""; }
        catch (Exception ex) when (ex is JsonException or InvalidOperationException) { return body.Length > 300 ? body[..300] : body; }
    }
}
#endif
