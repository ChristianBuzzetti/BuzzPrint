using System.Runtime.InteropServices;

namespace BuzzPrint;

/// <summary>
/// Authenticode check through Windows' WinVerifyTrust: the file must carry a valid signature over its current
/// contents (not modified after signing) that chains to a trusted root. Reading the signer's certificate alone
/// (X509Certificate.CreateFromSignedFile) does not check the file, so it is not enough before running a download.
/// </summary>
public static class Authenticode
{
    static readonly Guid VerifyV2 = new("00AAC56B-CD44-11d0-8CC2-00C04FC295EE");   // WINTRUST_ACTION_GENERIC_VERIFY_V2
    const uint UiNone = 2, RevokeNone = 0, ChoiceFile = 1, StateActionIgnore = 0;

    /// <summary>True when the file's embedded signature is valid and trusted. Revocation is not checked, as before:
    /// proxies often block those lookups.</summary>
    public static bool IsTrusted(string path)
    {
        var info = new FileInfoW { cbStruct = (uint)Marshal.SizeOf<FileInfoW>(), pcwszFilePath = path };
        IntPtr pInfo = Marshal.AllocHGlobal(Marshal.SizeOf<FileInfoW>());
        Marshal.StructureToPtr(info, pInfo, false);
        try
        {
            var data = new TrustData
            {
                cbStruct = (uint)Marshal.SizeOf<TrustData>(),
                dwUIChoice = UiNone,
                fdwRevocationChecks = RevokeNone,
                dwUnionChoice = ChoiceFile,
                pFile = pInfo,
                dwStateAction = StateActionIgnore
            };
            return WinVerifyTrust(new IntPtr(-1), VerifyV2, ref data) == 0;   // INVALID_HANDLE_VALUE: never show UI
        }
        finally
        {
            Marshal.DestroyStructure<FileInfoW>(pInfo);
            Marshal.FreeHGlobal(pInfo);
        }
    }

    [DllImport("wintrust.dll", ExactSpelling = true, CharSet = CharSet.Unicode)]
    static extern int WinVerifyTrust(IntPtr hwnd, [MarshalAs(UnmanagedType.LPStruct)] Guid action, ref TrustData data);

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct FileInfoW   // WINTRUST_FILE_INFO
    {
        public uint cbStruct;
        [MarshalAs(UnmanagedType.LPWStr)] public string pcwszFilePath;
        public IntPtr hFile;
        public IntPtr pgKnownSubject;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct TrustData   // WINTRUST_DATA
    {
        public uint cbStruct;
        public IntPtr pPolicyCallbackData;
        public IntPtr pSIPClientData;
        public uint dwUIChoice;
        public uint fdwRevocationChecks;
        public uint dwUnionChoice;
        public IntPtr pFile;
        public uint dwStateAction;
        public IntPtr hWVTStateData;
        public IntPtr pwszURLReference;
        public uint dwProvFlags;
        public uint dwUIContext;
        public IntPtr pSignatureSettings;
    }
}
