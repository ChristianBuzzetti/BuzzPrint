namespace BuzzPrint;

/// <summary>One block of a guide topic: numbered steps, bullet tips, or keyboard shortcuts.</summary>
public sealed record GuideBlock(string Heading, string Kind, IReadOnlyList<(string a, string b)> Items);

/// <summary>A guide topic. <see cref="Try"/> are (button label, action id) handled by the main window.</summary>
public sealed record GuideTopic(string Id, string Glyph, string Title, string Lead, IReadOnlyList<GuideBlock> Blocks, IReadOnlyList<(string Label, string Action)> Try, bool AiOnly = false);

/// <summary>
/// Everything BuzzPrint can do, written for people (not developers), in both languages. Each topic: what it is,
/// how to use it step by step, tips and shortcuts, and buttons that open the feature right away.
/// </summary>
public static class GuideContent
{
    static string S(string es, string en) => L.Es ? es : en;
    static GuideBlock Steps(string heading, params string[] items) => new(heading, "steps", items.Select(i => (i, "")).ToList());
    static GuideBlock Tips(string heading, params string[] items) => new(heading, "tips", items.Select(i => (i, "")).ToList());
    static GuideBlock Keys(string heading, params (string key, string what)[] items) => new(heading, "keys", items);

    public static IReadOnlyList<GuideTopic> All()
    {
        var st = Store.Settings;
        string capture = st.HotkeyCapture == "PrintScreen" ? S("ImprPant", "Print Screen") : st.HotkeyCapture;
        string full = st.HotkeyFull.Replace("PrintScreen", S("ImprPant", "Print Screen"));
        var list = new List<GuideTopic>
        {
            new("start", "", S("Primeros pasos", "Getting started"),
                S("BuzzPrint captura tu pantalla, la anota, la guarda en una biblioteca ordenada y la convierte en documentos listos para compartir. Vive en la bandeja del sistema: siempre está a mano, aunque cierres la ventana.",
                  "BuzzPrint captures your screen, annotates it, keeps it in a tidy library and turns it into documents ready to share. It lives in the system tray: always at hand, even when you close the window."),
                [
                    Steps(S("TU PRIMERA CAPTURA", "YOUR FIRST CAPTURE"),
                        S($"Tocá {capture}. La pantalla se oscurece.", $"Press {capture}. The screen dims."),
                        S("Arrastrá para elegir una región, o hacé clic sobre una ventana para capturarla entera.", "Drag to pick a region, or click a window to capture all of it."),
                        S("Listo: la captura queda en el portapapeles (pegala con Ctrl+V donde quieras) y guardada en la biblioteca.", "Done: the capture is on the clipboard (paste it anywhere with Ctrl+V) and saved in the library.")),
                    Tips(S("BUENO SABER", "GOOD TO KNOW"),
                        S("Cerrar la ventana no cierra BuzzPrint: sigue en la bandeja, junto al reloj. Para salir del todo, clic derecho en el ícono → Salir.", "Closing the window doesn't quit BuzzPrint: it stays in the tray, next to the clock. To quit, right-click the icon → Exit."),
                        S("Si ImprPant abre la Herramienta Recortes de Windows, BuzzPrint te ofrece quedarse con la tecla (Ayuda → esta guía → Atajos).", "If Print Screen opens Windows' Snipping Tool, BuzzPrint offers to take the key (see Shortcuts)."),
                        S("Todo se guarda en tu PC. Nada se envía a internet salvo que vos uses una función de IA.", "Everything stays on your PC. Nothing goes online unless you use an AI feature.")),
                ],
                [(S("Capturar ahora", "Capture now"), "capture")]),

            new("capture", "", S("Capturar", "Capturing"),
                S("Tres formas de capturar, según lo que necesites. Todas copian al portapapeles y se guardan en la biblioteca.", "Three ways to capture, depending on what you need. All of them copy to the clipboard and save to the library."),
                [
                    Keys(S("FORMAS DE CAPTURAR", "WAYS TO CAPTURE"),
                        (capture, S("Región o ventana: arrastrá un área o hacé clic en una ventana.", "Region or window: drag an area or click a window.")),
                        (full, S("Pantalla completa, al instante.", "Full screen, right away.")),
                        (S("Menú Captura → Con demora", "Capture menu → Delayed"), S($"Espera {st.DelaySeconds} segundos antes de capturar: ideal para menús desplegables que se cierran.", $"Waits {st.DelaySeconds} seconds before capturing: ideal for dropdown menus that close."))),
                    Tips(S("DESPUÉS DE CAPTURAR", "AFTER CAPTURING"),
                        S("En Opciones elegís qué pasa después: copiar al portapapeles, abrir el editor enseguida o conservar la original al editar.", "In Options you choose what happens next: copy to the clipboard, open the editor right away, or keep the original when editing."),
                        S("Las capturas se guardan en Imágenes\\BuzzPrint\\capturas. Lo podés cambiar en Opciones → Carpetas.", "Captures are saved in Pictures\\BuzzPrint\\capturas. You can change it in Options → Folders."),
                        S("BuzzPrint lee el texto de cada captura (en tu PC, sin internet) para que después la encuentres buscando una palabra que aparece en ella.", "BuzzPrint reads the text in each capture (on your PC, offline) so you can later find it by searching a word that appears in it.")),
                ],
                [(S("Capturar región", "Capture a region"), "capture"), (S("Pantalla completa", "Full screen"), "full")]),

            new("quick", "", S("Menú rápido", "Quick menu"),
                S("Tus últimas capturas en una grilla flotante, para pegarlas en cualquier programa sin abrir la biblioteca.", "Your latest captures in a floating grid, to paste them into any app without opening the library."),
                [
                    Steps(S("CÓMO SE USA", "HOW TO USE IT"),
                        S($"En el programa donde querés pegar (Teams, un mail, Word…), tocá {st.HotkeyPick}.", $"In the app where you want to paste (Teams, a mail, Word…), press {st.HotkeyPick}."),
                        S("Elegí la captura con un clic o con su número (1 a 9).", "Pick the capture with a click or its number (1 to 9)."),
                        S("Se pega sola en la ventana donde estabas.", "It pastes itself into the window you were in.")),
                    Keys(S("ATAJOS DEL MENÚ", "MENU SHORTCUTS"),
                        ("1 – 9", S("Copiar y pegar esa captura", "Copy and paste that capture")),
                        (S("Shift + número", "Shift + number"), S("Solo copiar, sin pegar", "Copy only, don't paste")),
                        ("E", S("Abrir la captura en el editor", "Open the capture in the editor")),
                        ("Esc", S("Cerrar el menú", "Close the menu"))),
                ],
                [(S("Abrir el menú rápido", "Open the quick menu"), "quick")]),

            new("editor", "", S("Editor y anotaciones", "Editor and annotations"),
                S("Marcá lo importante, tapá datos sensibles, numerá pasos y recortá. Las anotaciones se pueden mover y editar hasta que guardes.", "Mark what matters, hide sensitive data, number steps and crop. Annotations can be moved and edited until you save."),
                [
                    Keys(S("HERRAMIENTAS (Y SU TECLA)", "TOOLS (AND THEIR KEY)"),
                        ("V", S("Seleccionar y mover anotaciones", "Select and move annotations")),
                        ("A", S("Flecha", "Arrow")), ("L", S("Línea", "Line")), ("R", S("Rectángulo", "Rectangle")), ("E", S("Elipse", "Ellipse")),
                        ("H", S("Resaltador", "Highlighter")), ("P", S("Lápiz libre", "Freehand pen")), ("T", S("Texto", "Text")),
                        ("N", S("Paso numerado: 1, 2, 3… para guías", "Numbered step: 1, 2, 3… for guides")),
                        ("B", S("Pixelar / ocultar datos (contraseñas, nombres, mails)", "Pixelate / hide data (passwords, names, mails)")),
                        ("C", S("Recortar", "Crop"))),
                    Keys(S("ATAJOS DEL EDITOR", "EDITOR SHORTCUTS"),
                        ("Ctrl+Z / Ctrl+Y", S("Deshacer / rehacer", "Undo / redo")), ("Ctrl+S", S("Guardar", "Save")), ("Ctrl+C", S("Copiar la imagen anotada", "Copy the annotated image")),
                        ("Supr", S("Borrar la anotación elegida", "Delete the selected annotation")), ("Ctrl+0 / Ctrl+1", S("Ajustar a la ventana / tamaño real", "Fit to window / actual size")),
                        ("Ctrl + / Ctrl -", S("Acercar / alejar", "Zoom in / out"))),
                    Tips(S("TRUCOS", "TIPS"),
                        S("Arriba elegís color, grosor y tamaño del texto antes de dibujar.", "Pick color, width and text size at the top before drawing."),
                        S("Con \"Conservar la original al editar\" activo, al guardar se crea una copia anotada y la original queda intacta.", "With \"Keep the original when editing\" on, saving creates an annotated copy and the original stays untouched."),
                        S("Abajo está la tira con las demás capturas: clic para abrir otra, clic derecho para copiarla.", "At the bottom is the strip with your other captures: click to open one, right-click to copy it."),
                        S("El panel Resumen guarda una nota de la captura que después aparece al exportar.", "The Summary panel keeps a note for the capture that later appears when exporting.")),
                ],
                [(S("Abrir el editor", "Open the editor"), "editor")]),

            new("library", "", S("Biblioteca", "Library"),
                S("Todas tus capturas y grabaciones, con búsqueda por texto, secciones para ordenarlas y filtros por día.", "All your captures and recordings, with text search, sections to organize them and filters by day."),
                [
                    Tips(S("ENCONTRAR", "FIND"),
                        S("Ctrl+F y escribí: busca en nombres, secciones y también en el texto que aparece dentro de cada captura.", "Ctrl+F and type: it searches names, sections and also the text that appears inside each capture."),
                        S("Arriba filtrás por día y por sección, y podés agrupar las tarjetas por día, sección u origen.", "At the top you filter by day and section, and you can group the cards by day, section or source."),
                        S("A la izquierda: Hoy, Fijadas, Editadas y Grabaciones.", "On the left: Today, Pinned, Edited and Recordings.")),
                    Tips(S("ORDENAR", "ORGANIZE"),
                        S("Creá secciones (por proyecto, sistema o cliente) con \"Nueva sección\" y mové capturas con clic derecho → Mover a sección.", "Create sections (per project, system or client) with \"New section\" and move captures with right-click → Move to section."),
                        S("F2 renombra, Ctrl+P fija arriba, Supr las manda a la Papelera de reciclaje (se pueden recuperar).", "F2 renames, Ctrl+P pins to the top, Delete sends them to the Recycle Bin (they can be restored)."),
                        S("Ctrl+clic o Shift+clic para elegir varias; Ctrl+A elige todas.", "Ctrl+click or Shift+click to pick several; Ctrl+A picks all.")),
                    Keys(S("ATAJOS", "SHORTCUTS"),
                        ("Enter", S("Abrir en el editor", "Open in the editor")), ("Ctrl+C", S("Copiar la imagen", "Copy the image")),
                        ("Ctrl+Shift+C", S("Copiar varias como una sola imagen", "Copy several as one image")), ("Ctrl+Shift+T", S("Copiar el texto de la captura", "Copy the text in the capture")),
                        ("Ctrl+E", S("Exportar las elegidas", "Export the selected ones"))),
                ],
                [(S("Ver mis grabaciones", "See my recordings"), "recordings"), (S("Abrir la carpeta", "Open the folder"), "folder")]),

            new("record", "", S("Grabar video y audio", "Recording video and audio"),
                S("Grabá una región, una ventana o la pantalla, con el sonido del sistema y tu micrófono. Ideal para mostrar un error que no se ve en una sola imagen.", "Record a region, a window or the screen, with system sound and your microphone. Great for showing an issue a single image can't."),
                [
                    Steps(S("CÓMO SE GRABA", "HOW TO RECORD"),
                        S($"Tocá {st.HotkeyRecord} (o el botón Grabar).", $"Press {st.HotkeyRecord} (or the Record button)."),
                        S("Elegí la región o la ventana, igual que al capturar.", "Pick the region or window, just like capturing."),
                        S("Aparece una barra con el tiempo. Para terminar, tocá de nuevo el atajo o Detener.", "A bar with the time appears. To finish, press the shortcut again or Stop.")),
                    Tips(S("OPCIONES", "OPTIONS"),
                        S("En Opciones → Grabación activás el audio del sistema, el micrófono y si se ve el puntero.", "In Options → Recording you turn on system audio, the microphone and whether the pointer shows."),
                        S("Menú Grabación → Grabar solo audio, para notas de voz o una llamada.", "Recording menu → Record audio only, for voice notes or a call."),
                        S("Las grabaciones se guardan en Videos\\BuzzPrint y aparecen en Grabaciones.", "Recordings are saved in Videos\\BuzzPrint and show up in Recordings.")),
                ],
                [(S("Ver grabaciones", "See recordings"), "recordings")]),

            new("export", "", S("Exportar documentos", "Exporting documents"),
                S("Elegí capturas y armá un PowerPoint, un Word, un PDF o una imagen larga, en orden, con títulos y notas: una guía paso a paso o un informe en segundos.", "Pick captures and build a PowerPoint, a Word file, a PDF or one long image, in order, with titles and notes: a step-by-step guide or a report in seconds."),
                [
                    Steps(S("PASO A PASO", "STEP BY STEP"),
                        S("En la biblioteca elegí las capturas (Ctrl+clic en el orden que quieras) y tocá Exportar (Ctrl+E).", "In the library pick the captures (Ctrl+click in the order you want) and press Export (Ctrl+E)."),
                        S("Reordená con las flechas o \"Más antiguas / más nuevas primero\". Escribí un título y una nota para cada paso.", "Reorder with the arrows or \"Oldest / newest first\". Type a title and a note for each step."),
                        S("Elegí el formato y el diseño (plantilla), y qué incluir: portada, numeración, datos de origen, una captura por página.", "Choose the format and the design (template), and what to include: cover, numbering, source details, one capture per page."),
                        S("Tocá Exportar: se abre el archivo listo para compartir.", "Press Export: the file opens, ready to share.")),
                    Tips(S("TRUCOS", "TIPS"),
                        S("Los títulos y notas que escribas vuelven a la biblioteca, para la próxima vez.", "The titles and notes you type go back to the library, for next time."),
                        S("Con IA activa, \"Redactar con IA\" escribe el título, la introducción y la nota de cada paso por vos.", "With AI on, \"Draft with AI\" writes the title, the introduction and each step's note for you.")),
                ],
                [(S("Exportar ahora", "Export now"), "export")]),

            new("templates", "", S("Plantillas de documento", "Document templates"),
                S("El diseño de lo que exportás: colores, tipografías, portada, encabezados, pie y logo. Hay doce incluidas y podés crear hasta doce propias.", "The design of what you export: colors, fonts, cover, headings, footer and logo. Twelve are built in and you can create up to twelve of your own."),
                [
                    Steps(S("CREAR LA TUYA", "CREATE YOUR OWN"),
                        S("En Exportar → Diseño → Cambiar…, tocá \"Crear plantilla\" (o duplicá una que te guste).", "In Export → Design → Change…, press \"Create template\" (or duplicate one you like)."),
                        S("Elegí colores, tipografías, el estilo de la portada y de los títulos, el pie y tu logo. La vista previa muestra cómo queda.", "Choose colors, fonts, the cover and heading style, the footer and your logo. The preview shows how it looks."),
                        S("Guardala: queda elegida para tus próximos exports.", "Save it: it's chosen for your next exports.")),
                    Tips(S("ESTILOS DISPONIBLES", "AVAILABLE STYLES"),
                        S("Portada con imagen (tu primera captura al costado del título), con banda, a todo color, minimalista o con barra.", "Cover with an image (your first capture beside the title), with a band, full color, minimal or with a bar."),
                        S("Títulos de cada paso sobre una línea o en una banda de color; pie con texto o en barra con número de página.", "Step titles over a line or in a colored band; a text footer or a bar with the page number."),
                        S("En PowerPoint, el texto puede ir al costado de la captura (diseño lado a lado).", "In PowerPoint, the text can go beside the capture (side-by-side layout).")),
                ],
                [(S("Ver plantillas", "See templates"), "templates")]),

            new("ai", "", S("Inteligencia artificial", "Artificial intelligence"),
                S("Conectá el servicio de IA que prefieras (Claude, Gemini, ChatGPT, OpenRouter, Ollama local u otro) y dejá que BuzzPrint escriba por vos. Nada se envía sin que lo pidas: cada función te muestra antes qué capturas van y a qué servicio.",
                  "Connect the AI service you prefer (Claude, Gemini, ChatGPT, OpenRouter, local Ollama or another) and let BuzzPrint write for you. Nothing is sent unless you ask: every feature first shows which captures go to which service."),
                [
                    Steps(S("CONECTAR", "CONNECT"),
                        S("Barra lateral → IA (o menú IA → Configurar IA). Agregá una conexión y elegí el proveedor.", "Sidebar → AI (or AI menu → Set up AI). Add a connection and pick the provider."),
                        S("Pegá tu API key (con \"Conseguir una API key\" vas directo a la página del proveedor) y tocá \"Ver modelos\".", "Paste your API key (\"Get an API key\" takes you to the provider's page) and press \"See models\"."),
                        S("Probá la conexión y guardá. Podés tener varias y elegir cuál usar.", "Test the connection and save. You can have several and choose which one to use.")),
                    Keys(S("QUÉ PUEDE HACER", "WHAT IT CAN DO"),
                        (S("Título y resumen", "Title and summary"), S("Un nombre corto y un resumen para cada captura (también automático al capturar).", "A short name and a summary for each capture (also automatic when capturing).")),
                        (S("Guía paso a paso", "Step-by-step guide"), S("Redacta el documento entero a partir de tus capturas.", "Drafts the whole document from your captures.")),
                        (S("Proteger datos", "Protect data"), S("Encuentra mails, IPs, claves, nombres y números y guarda una copia pixelada.", "Finds mails, IPs, keys, names and numbers and saves a pixelated copy.")),
                        (S("Consultar la captura", "Ask about the capture"), S("Preguntale qué significa un error, qué revisar o cómo seguir.", "Ask what an error means, what to check or how to continue.")),
                        (S("Reporte de incidente", "Incident report"), S("Un reporte listo para Jira o un mail; se copia con formato o se guarda como imagen o texto.", "A report ready for Jira or a mail; copy it formatted or save it as an image or text.")),
                        (S("Ordenar la biblioteca", "Organize the library"), S("Nombres y secciones para muchas capturas en una sola consulta; revisás antes de aplicar.", "Names and sections for many captures in a single request; you review before applying."))),
                    Tips(S("BUENO SABER", "GOOD TO KNOW"),
                        S("Todo esto también está en el editor, en el botón ✦ IA, para la captura abierta.", "All of this is also in the editor, under the ✦ AI button, for the open capture."),
                        S("Las keys gratuitas tienen pocos pedidos por minuto: si el servicio pide esperar, verás una cuenta regresiva. Podés cancelar cuando quieras.", "Free keys allow few requests per minute: if the service asks to wait you'll see a countdown. You can cancel any time."),
                        S("Con Ollama la IA corre en tu PC y nada sale de ella.", "With Ollama the AI runs on your PC and nothing leaves it.")),
                ],
                [(S("Configurar IA", "Set up AI"), "ai")], AiOnly: true),

            new("personalize", "", S("Personalizar", "Personalize"),
                S("Hacé tuya la app: paletas, color de acento, tipografías, bordes, animaciones, el logo y a Boberto, la mascota que te acompaña.", "Make the app yours: palettes, accent color, fonts, corners, animations, the logo and Boberto, the companion that keeps you company."),
                [
                    Tips(S("QUÉ PODÉS CAMBIAR", "WHAT YOU CAN CHANGE"),
                        S("Estilo: 23 paletas claras y oscuras, y un color de acento propio.", "Style: 23 light and dark palettes, and your own accent color."),
                        S("Texto: tipografías de la interfaz y del editor, y el tamaño.", "Text: interface and editor fonts, and the size."),
                        S("Interfaz: paneles y botones redondeados o rectos, por separado. Y el idioma (español o inglés).", "Interface: rounded or square panels and buttons, separately. And the language (Spanish or English)."),
                        S("Efectos y logo: la forma, los ojos y los colores del logo; animaciones y movimiento.", "Effects & Logo: the logo's shape, eyes and colors; animations and motion."),
                        S("Boberto Studio: gestos, ropa, accesorios, fondos y marcos para Boberto.", "Boberto Studio: gestures, outfits, accessories, backgrounds and frames for Boberto.")),
                ],
                [(S("Abrir Personalizar", "Open Personalize"), "personalize")]),

            new("keys", "", S("Atajos de teclado", "Keyboard shortcuts"),
                S("Todos los atajos en un lugar. Los globales funcionan desde cualquier programa.", "All shortcuts in one place. The global ones work from any app."),
                [
                    Keys(S("GLOBALES", "GLOBAL"),
                        (capture, S("Capturar región o ventana", "Capture a region or window")), (full, S("Capturar pantalla completa", "Capture full screen")),
                        (st.HotkeyPick, S("Menú rápido de capturas", "Quick capture menu")), (st.HotkeyRecord, S("Grabar / detener video", "Record / stop video")),
                        (st.HotkeyCaptureAlt, S("Captura alternativa si Windows usa ImprPant", "Alternative capture if Windows uses Print Screen"))),
                    Keys(S("BIBLIOTECA", "LIBRARY"),
                        ("Ctrl+F", S("Buscar", "Search")), ("Enter", S("Abrir en el editor", "Open in the editor")), ("F2", S("Renombrar", "Rename")),
                        ("Ctrl+P", S("Fijar arriba", "Pin to the top")), ("Supr", S("Enviar a la Papelera", "Send to the Recycle Bin")), ("Ctrl+E", S("Exportar", "Export")),
                        ("Ctrl+A", S("Elegir todas", "Select all"))),
                    Tips(S("SI IMPRPANT ABRE RECORTES", "IF PRINT SCREEN OPENS SNIPPING TOOL"),
                        S("Windows 11 usa ImprPant para su Herramienta Recortes. BuzzPrint te pregunta al iniciar si querés que la tecla sea suya; también podés apagarlo en Configuración → Accesibilidad → Teclado → \"Usar ImprPant para abrir la captura de pantalla\".", "Windows 11 uses Print Screen for its Snipping Tool. BuzzPrint asks at startup whether it should take the key; you can also turn it off in Settings → Accessibility → Keyboard → \"Use the Print screen key to open screen capture\"."),
                        S("El cambio puede pedir cerrar sesión en Windows para aplicarse.", "The change may need you to sign out of Windows to apply.")),
                ],
                []),

            new("privacy", "", S("Privacidad y datos", "Privacy and data"),
                S("Tus capturas son tuyas: quedan en tu PC.", "Your captures are yours: they stay on your PC."),
                [
                    Tips(S("DÓNDE ESTÁ CADA COSA", "WHERE THINGS ARE"),
                        S("Capturas: Imágenes\\BuzzPrint\\capturas. Grabaciones: Videos\\BuzzPrint. Las podés cambiar en Opciones.", "Captures: Pictures\\BuzzPrint\\capturas. Recordings: Videos\\BuzzPrint. You can change them in Options."),
                        S("El texto de las capturas se lee con el reconocimiento de Windows, en tu PC y sin internet.", "The text in captures is read with Windows' recognition, on your PC and offline."),
                        S("Las API keys de IA se guardan cifradas con tu usuario de Windows.", "AI API keys are stored encrypted with your Windows user.")),
                    Tips(S("CON IA", "WITH AI"),
                        S("Solo se envía lo que vos pedís, al servicio que elegiste, y la app te lo muestra antes de enviar.", "Only what you ask for is sent, to the service you chose, and the app shows it to you before sending."),
                        S("\"Proteger datos\" tapa información sensible en una copia, sin tocar la original.", "\"Protect data\" hides sensitive information in a copy, without touching the original.")),
                ],
                [(S("Abrir la carpeta de capturas", "Open the captures folder"), "folder")]),
        };
#if !PREVIEW
        list.RemoveAll(t => t.AiOnly);
#endif
        return list;
    }
}
