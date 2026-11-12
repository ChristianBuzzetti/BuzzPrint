using System.Runtime.InteropServices;
using System.Text.Json;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media.Imaging;
using Microsoft.Web.WebView2.Core;
using Microsoft.Web.WebView2.Wpf;

namespace BuzzPrint;

// Empty-library companion: the engine's "peana" (a mirror of the dock character with the same Studio look) on a
// transparent WebView2. It plays a gesture every few seconds, waves on arrival, follows the cursor with its eyes,
// and plays a random gesture on click. Nothing is drawn until the engine reports ready (with the person's look already
// applied), so the default orange Boberto never flashes. Without WebView2 it shows the packaged image.
public sealed class BobertoStageControl:Grid,IDisposable
{
    private readonly WebView2 view=new() {Visibility=Visibility.Hidden};
    private readonly Image fallback=new() {Source=new BitmapImage(new Uri("pack://application:,,,/Assets/boberto.png")),Width=120,Height=120,Visibility=Visibility.Collapsed};
    private AppearanceSettings appearance;
    private bool started,disposed,live;
    public BobertoStageControl(AppearanceSettings settings)
    {
        appearance=settings;Children.Add(fallback);Children.Add(view);
        Loaded+=async(_,_)=>await StartAsync();
    }
    private async Task StartAsync()
    {
        if(started||disposed)return;started=true;
        try {
            var environment=await BobertoRuntime.EnvironmentAsync(BobertoRuntime.DefaultProfile);
            if(disposed)return;
            view.DefaultBackgroundColor=System.Drawing.Color.Transparent;
            await view.EnsureCoreWebView2Async(environment);
            if(disposed)return;
            var core=view.CoreWebView2;BobertoRuntime.Harden(core,environment,()=>{});
            core.WebMessageReceived+=(_,args)=> {
                if(disposed||!BobertoRuntime.LocalUri(args.Source)||args.WebMessageAsJson.Length>8192)return;
                try {
                    using var message=JsonDocument.Parse(args.WebMessageAsJson);
                    if(message.RootElement.GetProperty("type").GetString()=="ready"){live=true;_=ApplyThemeAsync(appearance);view.Visibility=Visibility.Visible;}
                }catch(Exception ex)when(ex is JsonException or KeyNotFoundException or InvalidOperationException){}
            };
            core.Navigate(BobertoRuntime.Page("stage.html"));
        }catch(Exception ex)when(ex is WebView2RuntimeNotFoundException or System.IO.IOException or UnauthorizedAccessException or COMException or InvalidOperationException or ArgumentException){fallback.Visibility=Visibility.Visible;}
    }
    public async Task ApplyThemeAsync(AppearanceSettings settings)
    {
        appearance=settings;if(!live||disposed||view.CoreWebView2==null)return;
        string tokens=JsonSerializer.Serialize(BobertoRuntime.ThemeTokens(settings)),theme=JsonSerializer.Serialize(BobertoRuntime.ThemeName(settings));
        try{await view.CoreWebView2.ExecuteScriptAsync($"window.desktopStage&&window.desktopStage.applyTheme({tokens},{theme},{(settings.BobertoMotion?"true":"false")})");}
        catch(Exception ex)when(ex is InvalidOperationException or COMException){}
    }
    public void Wave(){if(live&&!disposed)try{_=view.CoreWebView2?.ExecuteScriptAsync("window.desktopStage&&window.desktopStage.wave()");}catch(Exception ex)when(ex is InvalidOperationException or COMException){}}
    internal async Task<string> EvaluateAsync(string script)=>live&&view.CoreWebView2!=null?await view.CoreWebView2.ExecuteScriptAsync(script):"null";
    public bool IsLive=>live;
    public void Dispose(){if(disposed)return;disposed=true;try{view.Dispose();}catch(Exception ex)when(ex is InvalidOperationException or COMException){}}
}
