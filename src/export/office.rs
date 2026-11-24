//! PowerPoint (16:9, una captura por diapositiva) y Word (A4, títulos en el panel de navegación).
//! Port línea por línea de Exporter.cs.

use std::io;

use super::zip::Zip;
use super::layout::{self, Al, El, Src, Va};
use super::{fit, meta_line, size_of, step_label, subtitle, xml as x, Opts, Page, Template};
use crate::i18n::es;

const SLIDE_W: i64 = 12192000;
const SLIDE_H: i64 = 6858000;
const IN: i64 = 914400;
const PT: i64 = 12700;
const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const ROOT_RELS: &str = "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">";
const REL_CORE: &str = "<Relationship Id=\"rIdCore\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/><Relationship Id=\"rIdApp\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties\" Target=\"docProps/app.xml\"/>";
const CT_PROPS: &str = "<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/><Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>";

fn f(v: f64) -> i64 {
    v as i64
}

fn props(zip: &mut Zip, title: &str) {
    let s = crate::clock::Stamp::now();
    let now = format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", s.year, s.month, s.day, s.hour, s.minute, s.second);
    zip.xml("docProps/core.xml", &format!("<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><dc:title>{}</dc:title><dc:creator>BuzzPrint</dc:creator><dcterms:created xsi:type=\"dcterms:W3CDTF\">{now}</dcterms:created><dcterms:modified xsi:type=\"dcterms:W3CDTF\">{now}</dcterms:modified></cp:coreProperties>", x(title)));
    zip.xml("docProps/app.xml", "<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>BuzzPrint</Application></Properties>");
}

fn png_bytes(p: &Page) -> io::Result<Vec<u8>> {
    std::fs::read(&p.item.path)
}

fn image_size(path: &std::path::Path) -> (i64, i64) {
    crate::image::load_png(path).map(|i| (i.w as i64, i.h as i64)).unwrap_or((1, 1))
}

// ------------------------------------------------------------------ piezas DrawingML

struct Dml<'a> {
    t: &'a Template,
}

impl Dml<'_> {
    fn xfrm(&self, x0: i64, y: i64, w: i64, h: i64) -> String {
        format!("<a:xfrm><a:off x=\"{x0}\" y=\"{y}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>", w.max(1), h.max(1))
    }
    fn run(&self, text: &str, size: i32, bold: bool, color: &str) -> String {
        let font = x(if bold { &self.t.heading_font } else { &self.t.body_font });
        format!(
            "<a:r><a:rPr lang=\"{}\" sz=\"{size}\"{} dirty=\"0\"><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill><a:latin typeface=\"{font}\"/><a:cs typeface=\"{font}\"/></a:rPr><a:t>{}</a:t></a:r>",
            if es() { "es-AR" } else { "en-US" },
            if bold { " b=\"1\"" } else { "" },
            x(text)
        )
    }
    fn shape(&self, id: i32, x0: i64, y: i64, w: i64, h: i64, paragraphs: &str, anchor: &str) -> String {
        format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Text {id}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap=\"square\" lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\" anchor=\"{anchor}\"><a:normAutofit/></a:bodyPr><a:lstStyle/>{paragraphs}</p:txBody></p:sp>", self.xfrm(x0, y, w, h))
    }
    #[allow(clippy::too_many_arguments)]
    fn text(&self, id: i32, x0: i64, y: i64, w: i64, h: i64, size: i32, bold: bool, color: &str, text: &str, anchor: &str, align: &str) -> String {
        let mut ps = String::new();
        for line in text.replace('\r', "").split('\n') {
            ps.push_str(&format!("<a:p><a:pPr algn=\"{align}\"/>{}<a:endParaRPr lang=\"es-AR\" sz=\"{size}\" dirty=\"0\"/></a:p>", if line.is_empty() { String::new() } else { self.run(line, size, bold, color) }));
        }
        self.shape(id, x0, y, w, h, &ps, anchor)
    }
    fn rect(&self, id: i32, x0: i64, y: i64, w: i64, h: i64, color: &str) -> String {
        format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Rule {id}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr></p:sp>", self.xfrm(x0, y, w, h))
    }
    fn boxr(&self, id: i32, x0: i64, y: i64, w: i64, h: i64, fill: &str) -> String {
        format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Box {id}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"roundRect\"><a:avLst><a:gd name=\"adj\" fmla=\"val 12000\"/></a:avLst></a:prstGeom><a:solidFill><a:srgbClr val=\"{fill}\"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr></p:sp>", self.xfrm(x0, y, w, h))
    }
    #[allow(clippy::too_many_arguments)]
    fn pill(&self, id: i32, x0: i64, y: i64, w: i64, h: i64, text: &str, fill: &str, color: &str) -> String {
        format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Step {id}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"roundRect\"><a:avLst><a:gd name=\"adj\" fmla=\"val 50000\"/></a:avLst></a:prstGeom><a:solidFill><a:srgbClr val=\"{fill}\"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr><p:txBody><a:bodyPr wrap=\"none\" lIns=\"45720\" tIns=\"0\" rIns=\"45720\" bIns=\"0\" anchor=\"ctr\"/><a:lstStyle/><a:p><a:pPr algn=\"ctr\"/>{}</a:p></p:txBody></p:sp>", self.xfrm(x0, y, w, h), self.run(text, 1400, true, color))
    }
    #[allow(clippy::too_many_arguments)]
    fn pic_ref(&self, id: i32, x0: i64, y: i64, w: i64, h: i64, descr: &str, rid: &str, ln: &str) -> String {
        format!("<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"Picture {id}\" descr=\"{}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>{ln}</p:spPr></p:pic>", x(descr), self.xfrm(x0, y, w, h))
    }
    fn ln(&self, accent: &str, width: i64) -> String {
        if self.t.frame == "none" {
            "<a:ln><a:noFill/></a:ln>".into()
        } else {
            format!("<a:ln w=\"{width}\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></a:ln>", self.t.frame_hex(accent))
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn pic(&self, id: i32, x0: i64, y: i64, w: i64, h: i64, descr: &str, accent: &str) -> String {
        format!("<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"Capture {id}\" descr=\"{}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"rId2\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>{}</p:spPr></p:pic>", x(descr), self.xfrm(x0, y, w, h), self.ln(accent, 9525))
    }
    fn theme(&self, accent: &str) -> String {
        let sys = |n: &str, v: &str| format!("<a:{n}><a:srgbClr val=\"{v}\"/></a:{n}>");
        let fill = "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>";
        let line = "<a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln>";
        format!(
            "<a:theme xmlns:a=\"{NS_A}\" name=\"BuzzPrint\"><a:themeElements><a:clrScheme name=\"BuzzPrint\">{}{}{}{}{}{}{}{}{}{}{}{}</a:clrScheme><a:fontScheme name=\"BuzzPrint\"><a:majorFont><a:latin typeface=\"{}\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont><a:minorFont><a:latin typeface=\"{}\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont></a:fontScheme><a:fmtScheme name=\"BuzzPrint\"><a:fillStyleLst>{fill}{fill}{fill}</a:fillStyleLst><a:lnStyleLst>{line}{line}{line}</a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst>{fill}{fill}{fill}</a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>",
            sys("dk1", &self.t.ink), sys("lt1", "FFFFFF"), sys("dk2", "374151"), sys("lt2", "F3F4F6"), sys("accent1", accent), sys("accent2", "2684FF"), sys("accent3", "10B981"),
            sys("accent4", "F59E0B"), sys("accent5", "8B5CF6"), sys("accent6", "6B7280"), sys("hlink", "2684FF"), sys("folHlink", "8B5CF6"),
            x(&self.t.heading_font), x(&self.t.body_font)
        )
    }
}

// ------------------------------------------------------------------ PowerPoint

pub fn pptx(pages: &[Page], o: &Opts) -> io::Result<Vec<u8>> {
    let tp = &o.t;
    let d = Dml { t: tp };
    let mut zip = Zip::new();
    let slides = pages.len() + o.cover as usize;
    let mut ct = String::from("<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/presProps.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presProps+xml\"/><Override PartName=\"/ppt/viewProps.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml\"/><Override PartName=\"/ppt/tableStyles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml\"/><Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/><Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/><Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>");
    ct.push_str(CT_PROPS);
    for i in 1..=slides {
        ct.push_str(&format!("<Override PartName=\"/ppt/slides/slide{i}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"));
    }
    ct.push_str("</Types>");
    zip.xml("[Content_Types].xml", &ct);
    zip.xml("_rels/.rels", &format!("{ROOT_RELS}<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"ppt/presentation.xml\"/>{REL_CORE}</Relationships>"));
    props(&mut zip, &o.title);

    let p_ns = format!("xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"");
    let mut pres = format!("<p:presentation {p_ns} saveSubsetFonts=\"1\"><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst><p:sldIdLst>");
    let mut pres_rels = format!("{ROOT_RELS}<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster\" Target=\"slideMasters/slideMaster1.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"theme/theme1.xml\"/><Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/presProps\" Target=\"presProps.xml\"/><Relationship Id=\"rId4\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/viewProps\" Target=\"viewProps.xml\"/><Relationship Id=\"rId5\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/tableStyles\" Target=\"tableStyles.xml\"/>");
    for i in 1..=slides {
        pres.push_str(&format!("<p:sldId id=\"{}\" r:id=\"rIdS{i}\"/>", 255 + i));
        pres_rels.push_str(&format!("<Relationship Id=\"rIdS{i}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{i}.xml\"/>"));
    }
    pres.push_str(&format!("</p:sldIdLst><p:sldSz cx=\"{SLIDE_W}\" cy=\"{SLIDE_H}\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>"));
    zip.xml("ppt/presentation.xml", &pres);
    pres_rels.push_str("</Relationships>");
    zip.xml("ppt/_rels/presentation.xml.rels", &pres_rels);
    zip.xml("ppt/presProps.xml", &format!("<p:presentationPr {p_ns}/>"));
    zip.xml("ppt/viewProps.xml", &format!("<p:viewPr {p_ns}><p:normalViewPr><p:restoredLeft sz=\"15620\"/><p:restoredTop sz=\"94660\"/></p:normalViewPr><p:gridSpacing cx=\"76200\" cy=\"76200\"/></p:viewPr>"));
    zip.xml("ppt/tableStyles.xml", &format!("<a:tblStyleLst xmlns:a=\"{NS_A}\" def=\"{{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}}\"/>"));
    let grp = "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>";
    zip.xml("ppt/slideMasters/slideMaster1.xml", &format!("<p:sldMaster {p_ns}><p:cSld><p:bg><p:bgRef idx=\"1001\"><a:schemeClr val=\"bg1\"/></p:bgRef></p:bg><p:spTree>{grp}</p:spTree></p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz=\"3200\"><a:latin typeface=\"+mj-lt\"/></a:defRPr></a:lvl1pPr></p:titleStyle><p:bodyStyle><a:lvl1pPr><a:defRPr sz=\"1800\"><a:latin typeface=\"+mn-lt\"/></a:defRPr></a:lvl1pPr></p:bodyStyle><p:otherStyle><a:lvl1pPr><a:defRPr sz=\"1800\"><a:latin typeface=\"+mn-lt\"/></a:defRPr></a:lvl1pPr></p:otherStyle></p:txStyles></p:sldMaster>"));
    zip.xml("ppt/slideMasters/_rels/slideMaster1.xml.rels", &format!("{ROOT_RELS}<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"../theme/theme1.xml\"/></Relationships>"));
    zip.xml("ppt/slideLayouts/slideLayout1.xml", &format!("<p:sldLayout {p_ns} type=\"blank\" preserve=\"1\"><p:cSld name=\"Blank\"><p:spTree>{grp}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"));
    zip.xml("ppt/slideLayouts/_rels/slideLayout1.xml.rels", &format!("{ROOT_RELS}<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster\" Target=\"../slideMasters/slideMaster1.xml\"/></Relationships>"));
    zip.xml("ppt/theme/theme1.xml", &d.theme(&o.accent));

    let bg = if tp.page == "FFFFFF" { String::new() } else { format!("<p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>", tp.page) };
    let slide = |body: &str| format!("<p:sld {p_ns}><p:cSld>{bg}<p:spTree>{grp}{body}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>");

    // Medios: cada captura una vez (image1.png…) y el logo.
    for (i, p) in pages.iter().enumerate() {
        zip.add(&format!("ppt/media/image{}.png", i + 1), &png_bytes(p)?, false);
    }
    let logo = tp.logo_path();
    let mut logo_aspect = None;
    if let Some(l) = &logo {
        let (lw, lh) = image_size(l);
        logo_aspect = Some(lw as f32 / lh.max(1) as f32);
        zip.add("ppt/media/logo.png", &std::fs::read(l)?, false);
    }
    let doc = layout::fixed(&super::shots(pages, o), o, super::Format::Pptx, logo_aspect);
    let e = |v: f32| (v as f64 * PT as f64).round() as i64;
    for (n, els) in doc.pages.iter().enumerate() {
        let n = n + 1;
        let mut body = String::new();
        let mut id = 2;
        let mut caps: Vec<usize> = Vec::new();
        for el in els {
            match el {
                El::Fill { r, color, radius } => {
                    if *radius > 0.2 {
                        let adj = ((*radius / r.w.min(r.h).max(0.01)) * 100000.0).clamp(0.0, 50000.0) as i64;
                        body += &format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Box {id}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"roundRect\"><a:avLst><a:gd name=\"adj\" fmla=\"val {adj}\"/></a:avLst></a:prstGeom><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr></p:sp>", d.xfrm(e(r.x), e(r.y), e(r.w), e(r.h)));
                    } else {
                        body += &d.rect(id, e(r.x), e(r.y), e(r.w), e(r.h), color);
                    }
                }
                El::Line { r, color, width, radius } => {
                    let geom = if *radius > 0.2 { "roundRect" } else { "rect" };
                    let adj = ((*radius / r.w.min(r.h).max(0.01)) * 100000.0).clamp(0.0, 50000.0) as i64;
                    let av = if *radius > 0.2 { format!("<a:gd name=\"adj\" fmla=\"val {adj}\"/>") } else { String::new() };
                    body += &format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Frame {id}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"{geom}\"><a:avLst>{av}</a:avLst></a:prstGeom><a:noFill/><a:ln w=\"{}\"><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill></a:ln></p:spPr></p:sp>", d.xfrm(e(r.x), e(r.y), e(r.w), e(r.h)), e(*width));
                }
                El::Dot { r, color } => {
                    body += &format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Dot {id}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"ellipse\"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr></p:sp>", d.xfrm(e(r.x), e(r.y), e(r.w), e(r.h)));
                }
                El::Text { r, text, size, head, color, al, va, .. } => {
                    let anchor = match va {
                        Va::T => "t",
                        Va::M => "ctr",
                        Va::B => "b",
                    };
                    let align = match al {
                        Al::L => "l",
                        Al::C => "ctr",
                        Al::R => "r",
                    };
                    body += &d.text(id, e(r.x), e(r.y), e(r.w), e(r.h), (*size * 100.0).round() as i32, *head, color, text, anchor, align);
                }
                El::Img { r, src } => match src {
                    Src::Logo => {
                        if logo.is_some() {
                            body += &d.pic_ref(id, e(r.x), e(r.y), e(r.w), e(r.h), "Logo", "rIdLogo", "<a:ln><a:noFill/></a:ln>");
                        }
                    }
                    Src::Cap(i) => {
                        if let Some(p) = pages.get(*i) {
                            body += &d.pic_ref(id, e(r.x), e(r.y), e(r.w), e(r.h), &p.title, &format!("rIdC{i}"), "<a:ln><a:noFill/></a:ln>");
                            if !caps.contains(i) {
                                caps.push(*i);
                            }
                        }
                    }
                },
            }
            id += 1;
        }
        zip.xml(&format!("ppt/slides/slide{n}.xml"), &slide(&body));
        let mut rels = format!("{ROOT_RELS}<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/>");
        for i in caps {
            rels += &format!("<Relationship Id=\"rIdC{i}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/image{}.png\"/>", i + 1);
        }
        if logo.is_some() {
            rels += "<Relationship Id=\"rIdLogo\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/logo.png\"/>";
        }
        rels += "</Relationships>";
        zip.xml(&format!("ppt/slides/_rels/slide{n}.xml.rels"), &rels);
    }
    Ok(zip.finish())
}

// ------------------------------------------------------------------ Word

pub fn docx(pages: &[Page], o: &Opts) -> io::Result<Vec<u8>> {
    let tp = &o.t;
    let mut zip = Zip::new();
    zip.xml("[Content_Types].xml", &format!("<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/><Override PartName=\"/word/footer1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml\"/><Override PartName=\"/word/settings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml\"/>{CT_PROPS}</Types>"));
    zip.xml("word/settings.xml", &format!("<w:settings xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">{}</w:settings>", if tp.page != "FFFFFF" { "<w:displayBackgroundShape/>" } else { "" }));
    zip.xml("_rels/.rels", &format!("{ROOT_RELS}<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>{REL_CORE}</Relationships>"));
    props(&mut zip, &o.title);
    let lang = if es() { "es-AR" } else { "en-US" };
    let (hf, bf) = (x(&tp.heading_font), x(&tp.body_font));
    let fill = tp.cover_fill_hex(&o.accent);
    let filled = tp.cover == "band" || tp.cover == "full";
    let title_ppr = match tp.cover.as_str() {
        "band" | "full" => format!("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{fill}\"/><w:spacing w:before=\"{}\" w:after=\"0\"/><w:ind w:left=\"0\"/>", if tp.cover == "full" { 2400 } else { 0 }),
        "minimal" => format!("<w:pBdr><w:bottom w:val=\"single\" w:sz=\"8\" w:space=\"10\" w:color=\"{}\"/></w:pBdr><w:jc w:val=\"center\"/><w:spacing w:before=\"1800\" w:after=\"160\"/>", o.accent),
        _ => format!("<w:pBdr><w:left w:val=\"single\" w:sz=\"36\" w:space=\"12\" w:color=\"{}\"/></w:pBdr><w:spacing w:after=\"60\"/><w:ind w:left=\"240\"/>", o.accent),
    };
    let sub_ppr = match tp.cover.as_str() {
        "band" | "full" => format!("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{fill}\"/><w:spacing w:after=\"480\"/>"),
        "minimal" => "<w:jc w:val=\"center\"/><w:spacing w:after=\"480\"/>".into(),
        _ => "<w:spacing w:after=\"480\"/><w:ind w:left=\"240\"/>".into(),
    };
    let (title_ink, sub_ink) = if filled { (tp.cover_ink.clone(), tp.cover_ink.clone()) } else { (tp.ink.clone(), tp.muted.clone()) };
    let band = tp.header == "band";
    let bar = tp.footer_style == "bar";
    let h2_rule = if band {
        format!("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{fill}\"/><w:ind w:left=\"0\"/>")
    } else if tp.top_rule {
        format!("<w:pBdr><w:top w:val=\"single\" w:sz=\"18\" w:space=\"8\" w:color=\"{}\"/></w:pBdr>", o.accent)
    } else {
        String::new()
    };
    let h2_ink = if band { "FFFFFF".to_string() } else { tp.ink.clone() };
    let h2_size = if tp.header == "plain" { 34 } else { 30 };
    let foot_ppr = if bar { format!("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{fill}\"/>") } else { String::new() };
    let foot_ink = if bar { "FFFFFF".to_string() } else { tp.faint.clone() };
    zip.xml("word/styles.xml", &format!(
        "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"{bf}\" w:hAnsi=\"{bf}\" w:cs=\"{bf}\"/><w:sz w:val=\"21\"/><w:szCs w:val=\"21\"/><w:lang w:val=\"{lang}\"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after=\"120\" w:line=\"276\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/><w:rPr><w:color w:val=\"{ink}\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr>{title_ppr}</w:pPr><w:rPr><w:rFonts w:ascii=\"{hf}\" w:hAnsi=\"{hf}\"/><w:color w:val=\"{title_ink}\"/><w:sz w:val=\"52\"/><w:szCs w:val=\"52\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Subtitle\"><w:name w:val=\"Subtitle\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr>{sub_ppr}</w:pPr><w:rPr><w:color w:val=\"{sub_ink}\"/><w:sz w:val=\"24\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr><w:keepNext/>{h2_rule}<w:spacing w:before=\"240\" w:after=\"120\"/><w:outlineLvl w:val=\"1\"/></w:pPr><w:rPr><w:rFonts w:ascii=\"{hf}\" w:hAnsi=\"{hf}\"/><w:color w:val=\"{h2_ink}\"/><w:sz w:val=\"{h2_size}\"/><w:szCs w:val=\"{h2_size}\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Caption\"><w:name w:val=\"caption\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/><w:pPr><w:spacing w:before=\"60\" w:after=\"160\"/></w:pPr><w:rPr><w:color w:val=\"{faint}\"/><w:sz w:val=\"17\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Footer\"><w:name w:val=\"footer\"/><w:basedOn w:val=\"Normal\"/><w:pPr>{foot_ppr}<w:spacing w:after=\"0\"/></w:pPr><w:rPr><w:color w:val=\"{foot_ink}\"/><w:sz w:val=\"16\"/></w:rPr></w:style></w:styles>",
        ink = tp.ink, faint = tp.faint
    ));
    let foot_text = [tp.footer.as_str(), tp.org.as_str(), o.title.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join("  ·  ");
    zip.xml("word/footer1.xml", &format!(
        "<w:ftr xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:p><w:pPr><w:pStyle w:val=\"Footer\"/><w:tabs><w:tab w:val=\"right\" w:pos=\"{}\"/></w:tabs></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r><w:r><w:tab/></w:r><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r><w:r><w:t xml:space=\"preserve\"> / </w:t></w:r><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> NUMPAGES </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p></w:ftr>",
        if o.landscape { 14570 } else { 9638 }, x(&foot_text)
    ));

    let (page_w, page_h, margin) = if o.landscape { (16838i64, 11906i64, 1134i64) } else { (11906, 16838, 1134) };
    let text_w = (page_w - 2 * margin) * 635;
    let max_h = (page_h - 2 * margin - 2600) * 635;
    let mut rels = format!("{ROOT_RELS}<Relationship Id=\"rIdStyles\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/><Relationship Id=\"rIdFooter\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer\" Target=\"footer1.xml\"/><Relationship Id=\"rIdSettings\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings\" Target=\"settings.xml\"/>");
    let mut body = String::new();
    let para = |style: &str, runs: &str, extra: &str| format!("<w:p><w:pPr><w:pStyle w:val=\"{style}\"/>{extra}</w:pPr>{runs}</w:p>");
    let run = |text: &str, color: Option<&str>, bold: bool| {
        let rpr = if color.is_some() || bold { format!("<w:rPr>{}{}</w:rPr>", if bold { "<w:b/>" } else { "" }, color.map(|c| format!("<w:color w:val=\"{c}\"/>")).unwrap_or_default()) } else { String::new() };
        format!("<w:r>{rpr}<w:t xml:space=\"preserve\">{}</w:t></w:r>", x(text))
    };
    let lines = |text: &str| text.replace('\r', "").split('\n').map(|l| run(l, None, false)).collect::<Vec<_>>().join("<w:r><w:br/></w:r>");
    let drawing = |rid: &str, w: i64, h: i64, doc_id: i32, name: &str, descr: &str, ln: &str| {
        format!("<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{w}\" cy=\"{h}\"/><wp:docPr id=\"{doc_id}\" name=\"{}\" descr=\"{}\"/><wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect=\"1\"/></wp:cNvGraphicFramePr><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"{doc_id}\" name=\"{}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>{ln}</pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>", x(name), x(descr), x(name))
    };
    let frame_ln = |w: i64| if tp.frame == "none" { "<a:ln><a:noFill/></a:ln>".to_string() } else { format!("<a:ln w=\"{w}\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></a:ln>", tp.frame_hex(&o.accent)) };
    let logo = tp.logo_path();
    if let Some(l) = &logo {
        zip.add("word/media/logo.png", &std::fs::read(l)?, false);
        rels.push_str("<Relationship Id=\"rIdLogo\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/logo.png\"/>");
    }
    if o.cover {
        if let Some(l) = &logo {
            let (lw, lh) = image_size(l);
            let (mut h, mut w) = (571500i64, 571500 * lw / lh.max(1));
            if w > 2011680 {
                h = h * 2011680 / w;
                w = 2011680;
            }
            body += &format!("<w:p><w:pPr><w:spacing w:after=\"480\"/>{}</w:pPr>{}</w:p>", if tp.cover == "minimal" { "<w:jc w:val=\"center\"/>" } else { "" }, drawing("rIdLogo", w, h, 9001, "Logo", "Logo", "<a:ln><a:noFill/></a:ln>"));
        }
        body += &para("Title", &run(&o.title, None, false), "");
        body += &para("Subtitle", &run(&subtitle(pages.len()), None, false), "");
        if tp.cover == "hero" {
            let (cw, ch) = size_of(&pages[0].item);
            let (hw, hh) = fit(cw as i64, ch as i64, text_w, max_h / 2);
            body += &format!("<w:p><w:pPr><w:spacing w:after=\"240\"/></w:pPr>{}</w:p>", drawing("rIdImg1", hw, hh, 9002, "Cover", &pages[0].title, &format!("<a:ln w=\"28575\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></a:ln>", o.accent)));
        }
        if !o.intro.trim().is_empty() {
            body += &format!("<w:p>{}</w:p>", lines(o.intro.trim()));
        }
        if !tp.org.is_empty() {
            body += &format!("<w:p><w:pPr><w:spacing w:before=\"480\"/><w:jc w:val=\"right\"/></w:pPr>{}</w:p>", run(&tp.org, Some(&tp.muted), true));
        }
    }
    // Sombra suave en las capturas (marco "sombra").
    let shadow = if tp.frame_style == "shadow" { "<a:effectLst><a:outerShdw blurRad=\"50800\" dist=\"25400\" dir=\"5400000\" algn=\"t\" rotWithShape=\"0\"><a:srgbClr val=\"000000\"><a:alpha val=\"30000\"/></a:srgbClr></a:outerShdw></a:effectLst>" } else { "" };
    let layout_kind = tp.layout.as_str();
    let split = layout_kind == "split" || layout_kind == "split-right";
    let card = layout_kind == "card";
    let no_borders = "<w:tblBorders><w:top w:val=\"nil\"/><w:left w:val=\"nil\"/><w:bottom w:val=\"nil\"/><w:right w:val=\"nil\"/><w:insideH w:val=\"nil\"/><w:insideV w:val=\"nil\"/></w:tblBorders>";
    let tw_twips = page_w - 2 * margin;
    for (i, p) in pages.iter().enumerate() {
        let rid = format!("rIdImg{}", i + 1);
        let file = format!("image{}.png", i + 1);
        zip.add(&format!("word/media/{file}"), &png_bytes(p)?, false);
        rels.push_str(&format!("<Relationship Id=\"{rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/{file}\"/>"));
        let break_before = o.one_per_page && (i > 0 || o.cover);
        let title = if o.captions { p.title.trim().to_string() } else { String::new() };
        let mut page_break = if break_before { "<w:pageBreakBefore/>".to_string() } else { String::new() };
        let mut heading = String::new();
        if o.numbers || !title.is_empty() {
            let step = if !o.numbers {
                String::new()
            } else {
                match tp.steps.as_str() {
                    "pill" | "circle" => {
                        let label = if tp.steps == "circle" { (i + 1).to_string() } else { step_label(i) };
                        format!("<w:r><w:rPr><w:b/><w:color w:val=\"{}\"/><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{}\"/></w:rPr><w:t xml:space=\"preserve\"> {} </w:t></w:r>{}", if band { &fill } else { "FFFFFF" }, if band { "FFFFFF" } else { &o.accent }, x(&label), run("  ", None, false))
                    }
                    "number" => format!("<w:r><w:rPr><w:color w:val=\"{}\"/><w:sz w:val=\"48\"/><w:szCs w:val=\"48\"/></w:rPr><w:t xml:space=\"preserve\">{:02}  </w:t></w:r>", if band { "FFFFFF" } else { &o.accent }, i + 1),
                    _ => run(&format!("{}{}", step_label(i), if title.is_empty() { "" } else { "  " }), Some(if band { "FFFFFF" } else { &o.accent }), false),
                }
            };
            let runs = format!("{step}{}", if title.is_empty() { String::new() } else { run(&title, None, false) });
            heading = para("Heading2", &runs, &page_break);
            page_break.clear();
        }
        let (iw, ih) = size_of(&p.item);
        let (iw, ih) = (iw as i64, ih as i64);
        // Ancho disponible para la captura según el diseño (en EMU).
        let img_w = if split { text_w * 62 / 100 } else if card { text_w - 2 * 180 * 635 } else { text_w };
        let (mut w, mut h) = fit(iw, ih, img_w, if split { max_h * 7 / 10 } else { max_h });
        if w > iw * 9525 {
            (w, h) = (iw * 9525, ih * 9525);
        }
        let pic = drawing(&rid, w, h, (i + 1) as i32, &file, &p.title, &format!("{}{shadow}", frame_ln(6350))).replace(&format!("name=\"{}\" descr", x(&file)), &format!("name=\"Capture {}\" descr", i + 1));
        let caption = if o.meta { para("Caption", &run(&meta_line(p), None, false), "<w:jc w:val=\"center\"/>") } else { para("Caption", "", "") };
        let note = if p.note.trim().is_empty() { String::new() } else { para("Normal", &lines(p.note.trim()), "") };
        if split {
            // Tabla de dos columnas sin bordes: texto a un lado, captura al otro.
            if break_before {
                body += "<w:p><w:pPr><w:pageBreakBefore/><w:spacing w:after=\"0\"/></w:pPr></w:p>";
            }
            let heading = heading.replace("<w:pageBreakBefore/>", "");
            let text_col = tw_twips * 34 / 100;
            let img_col = tw_twips - text_col;
            let text_cell = format!("<w:tc><w:tcPr><w:tcW w:w=\"{text_col}\" w:type=\"dxa\"/></w:tcPr>{}{}{}</w:tc>", heading, if note.is_empty() { para("Normal", "", "") } else { note.clone() }, if o.meta { para("Caption", &run(&meta_line(p), None, false), "") } else { String::new() });
            let img_cell = format!("<w:tc><w:tcPr><w:tcW w:w=\"{img_col}\" w:type=\"dxa\"/><w:vAlign w:val=\"center\"/></w:tcPr><w:p><w:pPr><w:spacing w:before=\"120\" w:after=\"120\"/><w:jc w:val=\"center\"/></w:pPr>{pic}</w:p></w:tc>");
            let cells = if layout_kind == "split-right" { format!("{img_cell}{text_cell}") } else { format!("{text_cell}{img_cell}") };
            let grid = if layout_kind == "split-right" { format!("<w:gridCol w:w=\"{img_col}\"/><w:gridCol w:w=\"{text_col}\"/>") } else { format!("<w:gridCol w:w=\"{text_col}\"/><w:gridCol w:w=\"{img_col}\"/>") };
            body += &format!("<w:tbl><w:tblPr><w:tblW w:w=\"{tw_twips}\" w:type=\"dxa\"/>{no_borders}<w:tblLayout w:type=\"fixed\"/><w:tblCellMar><w:left w:w=\"120\" w:type=\"dxa\"/><w:right w:w=\"120\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr><w:tblGrid>{grid}</w:tblGrid><w:tr><w:trPr><w:cantSplit/></w:trPr>{cells}</w:tr></w:tbl>");
            body += &para("Normal", "", "<w:spacing w:after=\"200\"/>");
            continue;
        }
        if card {
            // Recuadro: una celda con fondo suave y borde, con todo el paso adentro.
            let shade = super::layout::mix(&tp.page, &o.accent, 0.05);
            let border = format!("<w:tblBorders><w:top w:val=\"single\" w:sz=\"6\" w:color=\"{r}\"/><w:left w:val=\"single\" w:sz=\"6\" w:color=\"{r}\"/><w:bottom w:val=\"single\" w:sz=\"6\" w:color=\"{r}\"/><w:right w:val=\"single\" w:sz=\"6\" w:color=\"{r}\"/></w:tblBorders>", r = tp.rule);
            if break_before {
                body += "<w:p><w:pPr><w:pageBreakBefore/><w:spacing w:after=\"0\"/></w:pPr></w:p>";
            }
            let heading = heading.replace("<w:pageBreakBefore/>", "");
            body += &format!(
                "<w:tbl><w:tblPr><w:tblW w:w=\"{tw_twips}\" w:type=\"dxa\"/>{border}<w:tblLayout w:type=\"fixed\"/><w:tblCellMar><w:top w:w=\"120\" w:type=\"dxa\"/><w:left w:w=\"180\" w:type=\"dxa\"/><w:bottom w:w=\"120\" w:type=\"dxa\"/><w:right w:w=\"180\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w=\"{tw_twips}\"/></w:tblGrid><w:tr><w:trPr><w:cantSplit/></w:trPr><w:tc><w:tcPr><w:tcW w:w=\"{tw_twips}\" w:type=\"dxa\"/><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{shade}\"/></w:tcPr>{heading}<w:p><w:pPr><w:spacing w:after=\"0\"/><w:jc w:val=\"center\"/></w:pPr>{pic}</w:p>{caption}{}</w:tc></w:tr></w:tbl>",
                if note.is_empty() { para("Normal", "", "<w:spacing w:after=\"0\"/>") } else { note }
            );
            body += &para("Normal", "", "<w:spacing w:after=\"240\"/>");
            continue;
        }
        body += &heading;
        body += &format!("<w:p><w:pPr>{page_break}<w:spacing w:after=\"0\"/><w:jc w:val=\"center\"/></w:pPr>{pic}</w:p>");
        body += &caption;
        body += &note;
    }
    body += &format!("<w:sectPr><w:footerReference w:type=\"default\" r:id=\"rIdFooter\"/><w:pgSz w:w=\"{page_w}\" w:h=\"{page_h}\"{}/><w:pgMar w:top=\"{margin}\" w:right=\"{margin}\" w:bottom=\"{margin}\" w:left=\"{margin}\" w:header=\"567\" w:footer=\"567\" w:gutter=\"0\"/></w:sectPr>", if o.landscape { " w:orient=\"landscape\"" } else { "" });
    zip.xml("word/document.xml", &format!("<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"{NS_R}\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"{NS_A}\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">{}<w:body>{body}</w:body></w:document>", if tp.page != "FFFFFF" { format!("<w:background w:color=\"{}\"/>", tp.page) } else { String::new() }));
    rels.push_str("</Relationships>");
    zip.xml("word/_rels/document.xml.rels", &rels);
    Ok(zip.finish())
}
