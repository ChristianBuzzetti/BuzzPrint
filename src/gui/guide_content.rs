//! Contenido de la Guía (port de GuideContent.cs, convertido del original): todo lo que hace BuzzPrint, escrito
//! para personas, en los dos idiomas. Cada tema: qué es, cómo se usa, trucos y atajos, y botones que abren la función.

pub struct Block {
    pub heading: String,
    pub kind: &'static str, // steps | tips | keys
    pub items: Vec<(String, String)>,
}

pub struct Topic {
    pub id: &'static str,
    pub title: String,
    pub lead: String,
    pub blocks: Vec<Block>,
    pub tries: Vec<(String, &'static str)>,
    pub ai_only: bool,
}

fn s(es: &str, en: &str) -> String {
    if crate::i18n::es() { es.to_string() } else { en.to_string() }
}

macro_rules! steps {
    ($h:expr, $($i:expr),* $(,)?) => { Block { heading: $h.to_string(), kind: "steps", items: vec![$(($i.to_string(), String::new())),*] } };
}
macro_rules! tips {
    ($h:expr, $($i:expr),* $(,)?) => { Block { heading: $h.to_string(), kind: "tips", items: vec![$(($i.to_string(), String::new())),*] } };
}
macro_rules! keys {
    ($h:expr, $(($k:expr, $w:expr)),* $(,)?) => { Block { heading: $h.to_string(), kind: "keys", items: vec![$(($k.to_string(), $w.to_string())),*] } };
}
macro_rules! topic {
    ($id:expr, $title:expr, $lead:expr, $blocks:expr, $tries:expr) => { topic!($id, $title, $lead, $blocks, $tries, false) };
    ($id:expr, $title:expr, $lead:expr, $blocks:expr, $tries:expr, $ai:expr) => {{
        let tries: Vec<(String, &'static str)> = $tries;
        Topic { id: $id, title: $title.to_string(), lead: $lead.to_string(), blocks: $blocks, tries, ai_only: $ai }
    }};
}

pub fn all() -> Vec<Topic> {
    let st = crate::store::settings();
    let capture = if st.hotkey_capture == "PrintScreen" { s("ImprPant", "Print Screen") } else { st.hotkey_capture.clone() };
    let full = st.hotkey_full.replace("PrintScreen", &s("ImprPant", "Print Screen"));
    let (pick, record, alt, delay) = (st.hotkey_pick.clone(), st.hotkey_record.clone(), st.hotkey_capture_alt.clone(), st.delay_seconds);
    vec![

            topic!("start", s("Primeros pasos", "Getting started"),
                s("BuzzPrint captura tu pantalla, la anota, la guarda en una biblioteca ordenada y la convierte en documentos listos para compartir. Vive en la bandeja del sistema: siempre está a mano, aunque cierres la ventana.",
                  "BuzzPrint captures your screen, annotates it, keeps it in a tidy library and turns it into documents ready to share. It lives in the system tray: always at hand, even when you close the window."),
                vec![
                    steps!(s("TU PRIMERA CAPTURA", "YOUR FIRST CAPTURE"),
                        s(&format!("Tocá {capture}. La pantalla se oscurece."), &format!("Press {capture}. The screen dims.")),
                        s("Arrastrá para elegir una región, o hacé clic sobre una ventana para capturarla entera.", "Drag to pick a region, or click a window to capture all of it."),
                        s("Listo: la captura queda en el portapapeles (pegala con Ctrl+V donde quieras) y guardada en la biblioteca.", "Done: the capture is on the clipboard (paste it anywhere with Ctrl+V) and saved in the library.")),
                    tips!(s("BUENO SABER", "GOOD TO KNOW"),
                        s("Cerrar la ventana no cierra BuzzPrint: sigue en la bandeja, junto al reloj. Para salir del todo, clic derecho en el ícono → Salir.", "Closing the window doesn't quit BuzzPrint: it stays in the tray, next to the clock. To quit, right-click the icon → Exit."),
                        s("Si ImprPant abre la Herramienta Recortes de Windows, BuzzPrint te ofrece quedarse con la tecla (Ayuda → esta guía → Atajos).", "If Print Screen opens Windows' Snipping Tool, BuzzPrint offers to take the key (see Shortcuts)."),
                        s("Todo se guarda en tu PC. Nada se envía a internet salvo que vos uses una función de IA.", "Everything stays on your PC. Nothing goes online unless you use an AI feature.")),
                ],
                vec![(s("Capturar ahora", "Capture now"), "capture")]),

            topic!("capture", s("Capturar", "Capturing"),
                s("Tres formas de capturar, según lo que necesites. Todas copian al portapapeles y se guardan en la biblioteca.", "Three ways to capture, depending on what you need. All of them copy to the clipboard and save to the library."),
                vec![
                    keys!(s("FORMAS DE CAPTURAR", "WAYS TO CAPTURE"),
                        (capture.as_str(), s("Región o ventana: arrastrá un área o hacé clic en una ventana.", "Region or window: drag an area or click a window.")),
                        (full.as_str(), s("Pantalla completa, al instante.", "Full screen, right away.")),
                        (s("Menú Captura → Con demora", "Capture menu → Delayed"), s(&format!("Espera {delay} segundos antes de capturar: ideal para menús desplegables que se cierran."), &format!("Waits {delay} seconds before capturing: ideal for dropdown menus that close.")))),
                    tips!(s("DESPUÉS DE CAPTURAR", "AFTER CAPTURING"),
                        s("En Opciones elegís qué pasa después: copiar al portapapeles, abrir el editor enseguida o conservar la original al editar.", "In Options you choose what happens next: copy to the clipboard, open the editor right away, or keep the original when editing."),
                        s("Las capturas se guardan en Imágenes\\BuzzPrint\\capturas. Lo podés cambiar en Opciones → Carpetas.", "Captures are saved in Pictures\\BuzzPrint\\capturas. You can change it in Options → Folders."),
                        s("BuzzPrint lee el texto de cada captura (en tu PC, sin internet) para que después la encuentres buscando una palabra que aparece en ella.", "BuzzPrint reads the text in each capture (on your PC, offline) so you can later find it by searching a word that appears in it.")),
                ],
                vec![(s("Capturar región", "Capture a region"), "capture"), (s("Pantalla completa", "Full screen"), "full")]),

            topic!("quick", s("Menú rápido", "Quick menu"),
                s("Tus últimas capturas en una grilla flotante, para pegarlas en cualquier programa sin abrir la biblioteca.", "Your latest captures in a floating grid, to paste them into any app without opening the library."),
                vec![
                    steps!(s("CÓMO SE USA", "HOW TO USE IT"),
                        s(&format!("En el programa donde querés pegar (Teams, un mail, Word…), tocá {pick}."), &format!("In the app where you want to paste (Teams, a mail, Word…), press {pick}.")),
                        s("Elegí la captura con un clic o con su número (1 a 9).", "Pick the capture with a click or its number (1 to 9)."),
                        s("Se pega sola en la ventana donde estabas.", "It pastes itself into the window you were in.")),
                    keys!(s("ATAJOS DEL MENÚ", "MENU SHORTCUTS"),
                        ("1 – 9", s("Copiar y pegar esa captura", "Copy and paste that capture")),
                        (s("Shift + número", "Shift + number"), s("Solo copiar, sin pegar", "Copy only, don't paste")),
                        ("E", s("Abrir la captura en el editor", "Open the capture in the editor")),
                        ("Esc", s("Cerrar el menú", "Close the menu"))),
                ],
                vec![(s("Abrir el menú rápido", "Open the quick menu"), "quick")]),

            topic!("editor", s("Editor y anotaciones", "Editor and annotations"),
                s("Marcá lo importante, tapá datos sensibles, numerá pasos y recortá. Las anotaciones se pueden mover y editar hasta que guardes.", "Mark what matters, hide sensitive data, number steps and crop. Annotations can be moved and edited until you save."),
                vec![
                    keys!(s("HERRAMIENTAS (Y SU TECLA)", "TOOLS (AND THEIR KEY)"),
                        ("V", s("Seleccionar y mover anotaciones", "Select and move annotations")),
                        ("A", s("Flecha", "Arrow")), ("L", s("Línea", "Line")), ("R", s("Rectángulo", "Rectangle")), ("E", s("Elipse", "Ellipse")),
                        ("H", s("Resaltador", "Highlighter")), ("P", s("Lápiz libre", "Freehand pen")), ("T", s("Texto", "Text")),
                        ("N", s("Paso numerado: 1, 2, 3… para guías", "Numbered step: 1, 2, 3… for guides")),
                        ("B", s("Pixelar / ocultar datos (contraseñas, nombres, mails)", "Pixelate / hide data (passwords, names, mails)")),
                        ("C", s("Recortar", "Crop"))),
                    keys!(s("ATAJOS DEL EDITOR", "EDITOR SHORTCUTS"),
                        ("Ctrl+Z / Ctrl+Y", s("Deshacer / rehacer", "Undo / redo")), ("Ctrl+S", s("Guardar", "Save")), ("Ctrl+C", s("Copiar la imagen anotada", "Copy the annotated image")),
                        ("Supr", s("Borrar la anotación elegida", "Delete the selected annotation")), ("Ctrl+0 / Ctrl+1", s("Ajustar a la ventana / tamaño real", "Fit to window / actual size")),
                        ("Ctrl + / Ctrl -", s("Acercar / alejar", "Zoom in / out"))),
                    tips!(s("TRUCOS", "TIPS"),
                        s("Arriba elegís color, grosor y tamaño del texto antes de dibujar.", "Pick color, width and text size at the top before drawing."),
                        s("Con \"Conservar la original al editar\" activo, al guardar se crea una copia anotada y la original queda intacta.", "With \"Keep the original when editing\" on, saving creates an annotated copy and the original stays untouched."),
                        s("Abajo está la tira con las demás capturas: clic para abrir otra, clic derecho para copiarla.", "At the bottom is the strip with your other captures: click to open one, right-click to copy it."),
                        s("El panel Resumen guarda una nota de la captura que después aparece al exportar.", "The Summary panel keeps a note for the capture that later appears when exporting.")),
                ],
                vec![(s("Abrir el editor", "Open the editor"), "editor")]),

            topic!("library", s("Biblioteca", "Library"),
                s("Todas tus capturas y grabaciones, con búsqueda por texto, secciones para ordenarlas y filtros por día.", "All your captures and recordings, with text search, sections to organize them and filters by day."),
                vec![
                    tips!(s("ENCONTRAR", "FIND"),
                        s("Ctrl+F y escribí: busca en nombres, secciones y también en el texto que aparece dentro de cada captura.", "Ctrl+F and type: it searches names, sections and also the text that appears inside each capture."),
                        s("Arriba filtrás por día y por sección, y podés agrupar las tarjetas por día, sección u origen.", "At the top you filter by day and section, and you can group the cards by day, section or source."),
                        s("A la izquierda: Hoy, Fijadas, Editadas y Grabaciones.", "On the left: Today, Pinned, Edited and Recordings.")),
                    tips!(s("ORDENAR", "ORGANIZE"),
                        s("Creá secciones (por proyecto, sistema o cliente) con \"Nueva sección\" y mové capturas con clic derecho → Mover a sección.", "Create sections (per project, system or client) with \"New section\" and move captures with right-click → Move to section."),
                        s("F2 renombra, Ctrl+P fija arriba, Supr las manda a la Papelera de reciclaje (se pueden recuperar).", "F2 renames, Ctrl+P pins to the top, Delete sends them to the Recycle Bin (they can be restored)."),
                        s("Ctrl+clic o Shift+clic para elegir varias; Ctrl+A elige todas.", "Ctrl+click or Shift+click to pick several; Ctrl+A picks all.")),
                    keys!(s("ATAJOS", "SHORTCUTS"),
                        ("Enter", s("Abrir en el editor", "Open in the editor")), ("Ctrl+C", s("Copiar la imagen", "Copy the image")),
                        ("Ctrl+Shift+C", s("Copiar varias como una sola imagen", "Copy several as one image")), ("Ctrl+Shift+T", s("Copiar el texto de la captura", "Copy the text in the capture")),
                        ("Ctrl+E", s("Exportar las elegidas", "Export the selected ones"))),
                ],
                vec![(s("Ver mis grabaciones", "See my recordings"), "recordings"), (s("Abrir la carpeta", "Open the folder"), "folder")]),

            topic!("record", s("Grabar video y audio", "Recording video and audio"),
                s("Grabá una región, una ventana o la pantalla, con el sonido del sistema y tu micrófono. Ideal para mostrar un error que no se ve en una sola imagen.", "Record a region, a window or the screen, with system sound and your microphone. Great for showing an issue a single image can't."),
                vec![
                    steps!(s("CÓMO SE GRABA", "HOW TO RECORD"),
                        s(&format!("Tocá {record} (o el botón Grabar)."), &format!("Press {record} (or the Record button).")),
                        s("Elegí la región o la ventana, igual que al capturar.", "Pick the region or window, just like capturing."),
                        s("Aparece una barra con el tiempo. Para terminar, tocá de nuevo el atajo o Detener.", "A bar with the time appears. To finish, press the shortcut again or Stop.")),
                    tips!(s("OPCIONES", "OPTIONS"),
                        s("En Opciones → Grabación activás el audio del sistema, el micrófono y si se ve el puntero.", "In Options → Recording you turn on system audio, the microphone and whether the pointer shows."),
                        s("Menú Grabación → Grabar solo audio, para notas de voz o una llamada.", "Recording menu → Record audio only, for voice notes or a call."),
                        s("Las grabaciones se guardan en Videos\\BuzzPrint y aparecen en Grabaciones.", "Recordings are saved in Videos\\BuzzPrint and show up in Recordings.")),
                ],
                vec![(s("Ver grabaciones", "See recordings"), "recordings")]),

            topic!("export", s("Exportar documentos", "Exporting documents"),
                s("Elegí capturas y armá un PowerPoint, un Word, un PDF o una imagen larga, en orden, con títulos y notas: una guía paso a paso o un informe en segundos.", "Pick captures and build a PowerPoint, a Word file, a PDF or one long image, in order, with titles and notes: a step-by-step guide or a report in seconds."),
                vec![
                    steps!(s("PASO A PASO", "STEP BY STEP"),
                        s("En la biblioteca elegí las capturas (Ctrl+clic en el orden que quieras) y tocá Exportar (Ctrl+E).", "In the library pick the captures (Ctrl+click in the order you want) and press Export (Ctrl+E)."),
                        s("Reordená con las flechas o \"Más antiguas / más nuevas primero\". Escribí un título y una nota para cada paso.", "Reorder with the arrows or \"Oldest / newest first\". Type a title and a note for each step."),
                        s("Elegí el formato y el diseño (plantilla), y qué incluir: portada, numeración, datos de origen, una captura por página.", "Choose the format and the design (template), and what to include: cover, numbering, source details, one capture per page."),
                        s("Tocá Exportar: se abre el archivo listo para compartir.", "Press Export: the file opens, ready to share.")),
                    tips!(s("TRUCOS", "TIPS"),
                        s("Los títulos y notas que escribas vuelven a la biblioteca, para la próxima vez.", "The titles and notes you type go back to the library, for next time."),
                        s("Con IA activa, \"Redactar con IA\" escribe el título, la introducción y la nota de cada paso por vos.", "With AI on, \"Draft with AI\" writes the title, the introduction and each step's note for you.")),
                ],
                vec![(s("Exportar ahora", "Export now"), "export")]),

            topic!("templates", s("Plantillas de documento", "Document templates"),
                s("El diseño de lo que exportás: colores, tipografías, portada, encabezados, pie y logo. Hay doce incluidas y podés crear hasta doce propias.", "The design of what you export: colors, fonts, cover, headings, footer and logo. Twelve are built in and you can create up to twelve of your own."),
                vec![
                    steps!(s("CREAR LA TUYA", "CREATE YOUR OWN"),
                        s("En Exportar → Diseño → Cambiar…, tocá \"Crear plantilla\" (o duplicá una que te guste).", "In Export → Design → Change…, press \"Create template\" (or duplicate one you like)."),
                        s("Elegí colores, tipografías, el estilo de la portada y de los títulos, el pie y tu logo. La vista previa muestra cómo queda.", "Choose colors, fonts, the cover and heading style, the footer and your logo. The preview shows how it looks."),
                        s("Guardala: queda elegida para tus próximos exports.", "Save it: it's chosen for your next exports.")),
                    tips!(s("ESTILOS DISPONIBLES", "AVAILABLE STYLES"),
                        s("Portada con imagen (tu primera captura al costado del título), con banda, a todo color, minimalista o con barra.", "Cover with an image (your first capture beside the title), with a band, full color, minimal or with a bar."),
                        s("Títulos de cada paso sobre una línea o en una banda de color; pie con texto o en barra con número de página.", "Step titles over a line or in a colored band; a text footer or a bar with the page number."),
                        s("En PowerPoint, el texto puede ir al costado de la captura (diseño lado a lado).", "In PowerPoint, the text can go beside the capture (side-by-side layout).")),
                ],
                vec![(s("Ver plantillas", "See templates"), "templates")]),

            topic!("ai", s("Inteligencia artificial", "Artificial intelligence"),
                s("Conectá el servicio de IA que prefieras (Claude, Gemini, ChatGPT, OpenRouter, Ollama local u otro) y dejá que BuzzPrint escriba por vos. Nada se envía sin que lo pidas: cada función te muestra antes qué capturas van y a qué servicio.",
                  "Connect the AI service you prefer (Claude, Gemini, ChatGPT, OpenRouter, local Ollama or another) and let BuzzPrint write for you. Nothing is sent unless you ask: every feature first shows which captures go to which service."),
                vec![
                    steps!(s("CONECTAR", "CONNECT"),
                        s("Barra lateral → IA (o menú IA → Configurar IA). Agregá una conexión y elegí el proveedor.", "Sidebar → AI (or AI menu → Set up AI). Add a connection and pick the provider."),
                        s("Pegá tu API key (con \"Conseguir una API key\" vas directo a la página del proveedor) y tocá \"Ver modelos\".", "Paste your API key (\"Get an API key\" takes you to the provider's page) and press \"See models\"."),
                        s("Probá la conexión y guardá. Podés tener varias y elegir cuál usar.", "Test the connection and save. You can have several and choose which one to use.")),
                    keys!(s("QUÉ PUEDE HACER", "WHAT IT CAN DO"),
                        (s("Título y resumen", "Title and summary"), s("Un nombre corto y un resumen para cada captura (también automático al capturar).", "A short name and a summary for each capture (also automatic when capturing).")),
                        (s("Guía paso a paso", "Step-by-step guide"), s("Redacta el documento entero a partir de tus capturas.", "Drafts the whole document from your captures.")),
                        (s("Proteger datos", "Protect data"), s("Encuentra mails, IPs, claves, nombres y números y guarda una copia pixelada.", "Finds mails, IPs, keys, names and numbers and saves a pixelated copy.")),
                        (s("Consultar la captura", "Ask about the capture"), s("Preguntale qué significa un error, qué revisar o cómo seguir.", "Ask what an error means, what to check or how to continue.")),
                        (s("Reporte de incidente", "Incident report"), s("Un reporte listo para Jira o un mail; se copia con formato o se guarda como imagen o texto.", "A report ready for Jira or a mail; copy it formatted or save it as an image or text.")),
                        (s("Ordenar la biblioteca", "Organize the library"), s("Nombres y secciones para muchas capturas en una sola consulta; revisás antes de aplicar.", "Names and sections for many captures in a single request; you review before applying."))),
                    tips!(s("BUENO SABER", "GOOD TO KNOW"),
                        s("Todo esto también está en el editor, en el botón IA, para la captura abierta.", "All of this is also in the editor, under the AI button, for the open capture."),
                        s("Las keys gratuitas tienen pocos pedidos por minuto: si el servicio pide esperar, verás una cuenta regresiva. Podés cancelar cuando quieras.", "Free keys allow few requests per minute: if the service asks to wait you'll see a countdown. You can cancel any time."),
                        s("Con Ollama la IA corre en tu PC y nada sale de ella.", "With Ollama the AI runs on your PC and nothing leaves it.")),
                ],
                vec![(s("Configurar IA", "Set up AI"), "ai")], true),

            topic!("personalize", s("Personalizar", "Personalize"),
                s("Hacé tuya la app: paletas, color de acento, tipografías, bordes, animaciones, el logo y a Boberto, la mascota que te acompaña.", "Make the app yours: palettes, accent color, fonts, corners, animations, the logo and Boberto, the companion that keeps you company."),
                vec![
                    tips!(s("QUÉ PODÉS CAMBIAR", "WHAT YOU CAN CHANGE"),
                        s("Estilo: 23 paletas claras y oscuras, y un color de acento propio.", "Style: 23 light and dark palettes, and your own accent color."),
                        s("Texto: tipografías de la interfaz y del editor, y el tamaño.", "Text: interface and editor fonts, and the size."),
                        s("Interfaz: paneles y botones redondeados o rectos, por separado. Y el idioma (español o inglés).", "Interface: rounded or square panels and buttons, separately. And the language (Spanish or English)."),
                        s("Efectos y logo: la forma, los ojos y los colores del logo; animaciones y movimiento.", "Effects & Logo: the logo's shape, eyes and colors; animations and motion."),
                        s("Boberto Studio: gestos, ropa, accesorios, fondos y marcos para Boberto.", "Boberto Studio: gestures, outfits, accessories, backgrounds and frames for Boberto.")),
                ],
                vec![(s("Abrir Personalizar", "Open Personalize"), "personalize")]),

            topic!("keys", s("Atajos de teclado", "Keyboard shortcuts"),
                s("Todos los atajos en un lugar. Los globales funcionan desde cualquier programa.", "All shortcuts in one place. The global ones work from any app."),
                vec![
                    keys!(s("GLOBALES", "GLOBAL"),
                        (capture.as_str(), s("Capturar región o ventana", "Capture a region or window")), (full.as_str(), s("Capturar pantalla completa", "Capture full screen")),
                        (pick.as_str(), s("Menú rápido de capturas", "Quick capture menu")), (record.as_str(), s("Grabar / detener video", "Record / stop video")),
                        (alt.as_str(), s("Captura alternativa si Windows usa ImprPant", "Alternative capture if Windows uses Print Screen"))),
                    keys!(s("BIBLIOTECA", "LIBRARY"),
                        ("Ctrl+F", s("Buscar", "Search")), ("Enter", s("Abrir en el editor", "Open in the editor")), ("F2", s("Renombrar", "Rename")),
                        ("Ctrl+P", s("Fijar arriba", "Pin to the top")), ("Supr", s("Enviar a la Papelera", "Send to the Recycle Bin")), ("Ctrl+E", s("Exportar", "Export")),
                        ("Ctrl+A", s("Elegir todas", "Select all"))),
                    tips!(s("SI IMPRPANT ABRE RECORTES", "IF PRINT SCREEN OPENS SNIPPING TOOL"),
                        s("Windows 11 usa ImprPant para su Herramienta Recortes. BuzzPrint te pregunta al iniciar si querés que la tecla sea suya; también podés apagarlo en Configuración → Accesibilidad → Teclado → \"Usar ImprPant para abrir la captura de pantalla\".", "Windows 11 uses Print Screen for its Snipping Tool. BuzzPrint asks at startup whether it should take the key; you can also turn it off in Settings → Accessibility → Keyboard → \"Use the Print screen key to open screen capture\"."),
                        s("El cambio puede pedir cerrar sesión en Windows para aplicarse.", "The change may need you to sign out of Windows to apply.")),
                ],
                vec![]),

            topic!("privacy", s("Privacidad y datos", "Privacy and data"),
                s("Tus capturas son tuyas: quedan en tu PC.", "Your captures are yours: they stay on your PC."),
                vec![
                    tips!(s("DÓNDE ESTÁ CADA COSA", "WHERE THINGS ARE"),
                        s("Capturas: Imágenes\\BuzzPrint\\capturas. Grabaciones: Videos\\BuzzPrint. Las podés cambiar en Opciones.", "Captures: Pictures\\BuzzPrint\\capturas. Recordings: Videos\\BuzzPrint. You can change them in Options."),
                        s("El texto de las capturas se lee con el reconocimiento de Windows, en tu PC y sin internet.", "The text in captures is read with Windows' recognition, on your PC and offline."),
                        s("Las API keys de IA se guardan cifradas con tu usuario de Windows.", "AI API keys are stored encrypted with your Windows user.")),
                    tips!(s("CON IA", "WITH AI"),
                        s("Solo se envía lo que vos pedís, al servicio que elegiste, y la app te lo muestra antes de enviar.", "Only what you ask for is sent, to the service you chose, and the app shows it to you before sending."),
                        s("\"Proteger datos\" tapa información sensible en una copia, sin tocar la original.", "\"Protect data\" hides sensitive information in a copy, without touching the original."),
                        s("Si la IA escribe algo inapropiado, avisanos con \"Reportar contenido inapropiado\", abajo en cada ventana de IA.", "If the AI writes something inappropriate, tell us with \"Report inappropriate content\", at the bottom of every AI window.")),
                ],
                vec![(s("Abrir la carpeta de capturas", "Open the captures folder"), "folder")]),
            ]
}
