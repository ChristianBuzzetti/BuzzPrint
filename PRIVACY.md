# BuzzPrint privacy policy

*Last updated: November 16, 2026*

BuzzPrint is a screen capture and recording app for Windows. It works entirely on your PC.

**What BuzzPrint does not do**

- It does not collect, send or sell personal data.
- It has no accounts, analytics, telemetry, ads or tracking.
- Your captures, recordings, notes and exported documents never leave your PC unless you share them yourself, or send them to the AI service you set up (see below).

**What stays on your PC**

- Captures and recordings, in the folders you choose (by default `Pictures\BuzzPrint` and `Videos\BuzzPrint`).
- Settings and appearance, in the app's local data folder.
- When you record, BuzzPrint captures the screen area you select and, if you turn them on, the system audio and your microphone. Windows asks for microphone permission and you can withdraw it in Settings → Privacy & security → Microphone.

**AI features (optional, off by default)**

- BuzzPrint includes optional AI features (titles and captions, protect sensitive data, ask about a capture, incident reports, document drafts). They are off until you turn them on and add your own API key for a provider you choose (for example Anthropic, OpenAI, Google or any OpenAI-compatible service).
- Only what you ask for is sent: the selected captures (and any text you type) go directly from your PC to that provider, using your key. BuzzPrint shows you what will be sent before sending it. Automatic captions, if you turn them on, send each new capture.
- BuzzPrint has no server of its own and never sees that data; the provider's own privacy policy and terms apply to it.
- API keys are stored on your PC, encrypted with your Windows user (DPAPI).
- Reading text in captures (OCR) uses Windows and stays on your PC.

**Network use**

- Apart from the AI provider you set up, the Microsoft Store version makes no network connections of its own.
- The version downloaded from GitHub connects to the internet only when you agree to download a missing component: FFmpeg from gyan.dev, or Microsoft Edge WebView2 from Microsoft. Those sites have their own privacy policies.
- Boberto, the animated companion, is rendered by Microsoft Edge WebView2, a Windows component governed by Microsoft's privacy statement.

**Children**

BuzzPrint does not knowingly collect any information from anyone, including children.

**Contact**

Questions about this policy: open an issue at https://github.com/ChristianBuzzetti/BuzzPrint/issues

---

# Política de privacidad de BuzzPrint

*Última actualización: 16 de noviembre de 2026*

BuzzPrint es una app de capturas y grabación de pantalla para Windows. Funciona completamente en tu PC.

- No recopila, envía ni vende datos personales. No tiene cuentas, analíticas, telemetría, publicidad ni seguimiento.
- Tus capturas, grabaciones, notas y documentos exportados quedan en tu PC salvo que vos los compartas o los mandes al servicio de IA que configuraste.
- Al grabar, captura el área que elegís y, si los activás, el audio del sistema y el micrófono. Windows pide permiso para el micrófono y lo podés quitar en Configuración → Privacidad y seguridad → Micrófono.
- IA (opcional, apagada de fábrica): solo funciona si la activás y cargás tu propia API key del proveedor que elijas. Se envía solo lo que pedís (las capturas elegidas y el texto que escribas), directo desde tu PC a ese proveedor, y BuzzPrint te lo muestra antes de enviar; el pie automático, si lo activás, envía cada captura nueva. BuzzPrint no tiene servidores propios; a esos datos se les aplica la política del proveedor. Las API keys quedan en tu PC, cifradas con tu usuario de Windows. El OCR usa Windows y no sale de tu PC.
- Fuera del proveedor de IA que configures, la versión de Microsoft Store no hace conexiones de red propias. La versión de GitHub solo se conecta si aceptás descargar un componente faltante (FFmpeg desde gyan.dev o WebView2 desde Microsoft).
- Consultas: https://github.com/ChristianBuzzetti/BuzzPrint/issues
