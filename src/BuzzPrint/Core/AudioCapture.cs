using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;

namespace BuzzPrint;

/// <summary>WASAPI shared-mode capture to a WAV file: the default speakers in loopback (system audio) or the default
/// microphone. Silence is written for the stretches where loopback delivers no packets (nothing playing), so the
/// track keeps wall-clock time and stays in sync with the video.</summary>
public sealed class AudioCapture : IDisposable
{
    const int ShareModeShared = 0, StreamFlagsLoopback = 0x00020000, ClsCtxAll = 23, BufferFlagsSilent = 2;
    readonly bool loopback;
    readonly string path;
    Thread? worker;
    volatile bool running;
    FileStream? file;
    long dataBytes;
    public int SampleRate { get; private set; }
    public int BlockAlign { get; private set; }
    public float Level { get; private set; }
    public string? Error { get; private set; }

    AudioCapture(bool loopback, string path) { this.loopback = loopback; this.path = path; }
    public static AudioCapture ForSystem(string path) => new(true, path);
    public static AudioCapture ForMicrophone(string path) => new(false, path);

    public static bool HasMicrophone()
    {
        try
        {
            var en = (IMMDeviceEnumerator)new MMDeviceEnumeratorCo();
            return en.GetDefaultAudioEndpoint(1, 0, out var d) == 0 && d != null;
        }
        catch (COMException) { return false; }
        catch (InvalidCastException) { return false; }
    }

    /// <summary>Starts the capture thread; returns false (with Error) when the endpoint cannot be opened.</summary>
    public bool Start()
    {
        var ready = new ManualResetEventSlim();
        running = true;
        worker = new Thread(() => Run(ready)) { IsBackground = true, Name = loopback ? "BuzzPrint loopback" : "BuzzPrint mic", Priority = ThreadPriority.AboveNormal };
        worker.SetApartmentState(ApartmentState.MTA);
        worker.Start();
        ready.Wait(5000);
        return Error == null && running;
    }

    void Run(ManualResetEventSlim ready)
    {
        IAudioClient? client = null;
        IntPtr format = IntPtr.Zero;
        try
        {
            var en = (IMMDeviceEnumerator)new MMDeviceEnumeratorCo();
            Marshal.ThrowExceptionForHR(en.GetDefaultAudioEndpoint(loopback ? 0 : 1, 0, out var device));
            var iid = typeof(IAudioClient).GUID;
            Marshal.ThrowExceptionForHR(device.Activate(ref iid, ClsCtxAll, IntPtr.Zero, out var o));
            client = (IAudioClient)o;
            Marshal.ThrowExceptionForHR(client.GetMixFormat(out format));
            short channels = Marshal.ReadInt16(format, 2);
            SampleRate = Marshal.ReadInt32(format, 4);
            BlockAlign = Marshal.ReadInt16(format, 12);
            short bits = Marshal.ReadInt16(format, 14);
            short tag = Marshal.ReadInt16(format, 0);
            short extra = tag == unchecked((short)0xFFFE) ? Marshal.ReadInt16(format, 16) : (short)0;
            bool isFloat = tag == 3 || (tag == unchecked((short)0xFFFE) && Marshal.ReadInt32(format, 24) == 3);
            Marshal.ThrowExceptionForHR(client.Initialize(ShareModeShared, loopback ? StreamFlagsLoopback : 0, 10_000_000, 0, format, IntPtr.Zero));
            var cid = typeof(IAudioCaptureClient).GUID;
            Marshal.ThrowExceptionForHR(client.GetService(ref cid, out var co));
            var capture = (IAudioCaptureClient)co;

            var fmt = new byte[18 + extra];
            Marshal.Copy(format, fmt, 0, fmt.Length);
            file = new FileStream(path, FileMode.Create, FileAccess.Write, FileShare.Read, 1 << 16);
            WriteHeader(fmt);
            Marshal.ThrowExceptionForHR(client.Start());
            ready.Set();

            var clock = Stopwatch.StartNew();
            long frames = 0;
            byte[] buffer = new byte[BlockAlign * SampleRate];
            while (running)
            {
                Thread.Sleep(10);
                bool got = false;
                while (capture.GetNextPacketSize(out uint packet) == 0 && packet > 0)
                {
                    Marshal.ThrowExceptionForHR(capture.GetBuffer(out var data, out uint count, out uint flags, out _, out _));
                    int bytes = (int)count * BlockAlign;
                    if (buffer.Length < bytes) buffer = new byte[bytes];
                    if ((flags & BufferFlagsSilent) != 0) Array.Clear(buffer, 0, bytes);
                    else Marshal.Copy(data, buffer, 0, bytes);
                    capture.ReleaseBuffer(count);
                    Write(buffer, bytes);
                    frames += count;
                    got = true;
                    if (isFloat && bits == 32) Level = Peak(buffer, bytes);
                }
                // Loopback goes quiet when nothing plays: pad with silence to keep wall-clock time.
                long due = (long)(clock.Elapsed.TotalSeconds * SampleRate);
                if (!got && due - frames > SampleRate / 10)
                {
                    long pad = due - frames - SampleRate / 50;
                    while (pad > 0)
                    {
                        int n = (int)Math.Min(pad, buffer.Length / BlockAlign);
                        Array.Clear(buffer, 0, n * BlockAlign);
                        Write(buffer, n * BlockAlign);
                        frames += n; pad -= n;
                    }
                    Level = 0;
                }
            }
            client.Stop();
        }
        catch (Exception ex) when (ex is COMException or InvalidCastException or IOException or UnauthorizedAccessException or ArgumentException)
        {
            Error = ex.Message;
            running = false;
        }
        finally
        {
            ready.Set();
            if (format != IntPtr.Zero) Marshal.FreeCoTaskMem(format);
            if (client != null) Marshal.ReleaseComObject(client);
            Finish();
        }
    }

    static float Peak(byte[] b, int bytes)
    {
        float peak = 0;
        for (int i = 0; i + 4 <= bytes; i += 16) { float v = Math.Abs(BitConverter.ToSingle(b, i)); if (v > peak) peak = v; }
        return Math.Min(1, peak);
    }

    void Write(byte[] b, int n) { file!.Write(b, 0, n); dataBytes += n; }

    void WriteHeader(byte[] fmt)
    {
        var w = new BinaryWriter(file!, System.Text.Encoding.ASCII, true);
        w.Write("RIFF"u8); w.Write(0); w.Write("WAVE"u8);
        w.Write("fmt "u8); w.Write(fmt.Length); w.Write(fmt);
        w.Write("data"u8); w.Write(0);
        w.Flush();
    }

    void Finish()
    {
        if (file == null) return;
        try
        {
            long dataPos = file.Length - dataBytes - 4;
            file.Position = 4; file.Write(BitConverter.GetBytes((int)Math.Min(int.MaxValue, file.Length - 8)));
            file.Position = dataPos; file.Write(BitConverter.GetBytes((int)Math.Min(int.MaxValue, dataBytes)));
        }
        catch (IOException) { }
        file.Dispose(); file = null;
    }

    public void Stop() { running = false; worker?.Join(3000); }
    public void Dispose() => Stop();
    public bool HasData => dataBytes > 0;

    // ------------------------------------------------------------------ COM
    [ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")] class MMDeviceEnumeratorCo { }

    [ComImport, Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IMMDeviceEnumerator
    {
        [PreserveSig] int EnumAudioEndpoints(int dataFlow, int stateMask, out IntPtr devices);
        [PreserveSig] int GetDefaultAudioEndpoint(int dataFlow, int role, out IMMDevice endpoint);
    }

    [ComImport, Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IMMDevice
    {
        [PreserveSig] int Activate(ref Guid iid, int clsCtx, IntPtr activationParams, [MarshalAs(UnmanagedType.IUnknown)] out object iface);
    }

    [ComImport, Guid("1CB9AD4C-DBFA-4c32-B178-C2F568A703B2"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IAudioClient
    {
        [PreserveSig] int Initialize(int shareMode, int streamFlags, long bufferDuration, long periodicity, IntPtr format, IntPtr sessionGuid);
        [PreserveSig] int GetBufferSize(out uint frames);
        [PreserveSig] int GetStreamLatency(out long latency);
        [PreserveSig] int GetCurrentPadding(out uint frames);
        [PreserveSig] int IsFormatSupported(int shareMode, IntPtr format, out IntPtr closest);
        [PreserveSig] int GetMixFormat(out IntPtr format);
        [PreserveSig] int GetDevicePeriod(out long defaultPeriod, out long minimumPeriod);
        [PreserveSig] int Start();
        [PreserveSig] int Stop();
        [PreserveSig] int Reset();
        [PreserveSig] int SetEventHandle(IntPtr handle);
        [PreserveSig] int GetService(ref Guid iid, [MarshalAs(UnmanagedType.IUnknown)] out object service);
    }

    [ComImport, Guid("C8ADBD64-E71E-48a0-A4DE-185C395CD317"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IAudioCaptureClient
    {
        [PreserveSig] int GetBuffer(out IntPtr data, out uint frames, out uint flags, out ulong devicePosition, out ulong qpcPosition);
        [PreserveSig] int ReleaseBuffer(uint frames);
        [PreserveSig] int GetNextPacketSize(out uint frames);
    }
}
