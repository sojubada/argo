//! 발표 문서(pptx·ppt)를 PDF 로.
//!
//! 쪽마다 SVG 를 그려 usvg/svg2pdf 로 PDF 에 옮긴다. 마스터·레이아웃에서 오는
//! 배경과 도형은 한 번만 그려 여러 쪽이 나눠 쓴다(같은 그림이 쪽마다 들어가 PDF 가
//! 부풀지 않게).

mod chart;
mod color;
mod fill;
mod fontbook;
mod geom;
mod metafile;
mod package;
pub(crate) mod ppt;
mod render;
mod svg;
mod table;
mod text;

use std::collections::HashMap;

use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};
use roxmltree::{Document, ParsingOptions};
use svg2pdf::usvg;

use color::Palette;
use fontbook::FontBook;
use package::Package;
use render::{child, path_child, Renderer, Scope, Theme, EMU};
use svg::Svg;

pub(crate) fn is_presentation_zip(bytes: &[u8]) -> bool {
    let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else { return false };
    let found = archive.by_name("ppt/presentation.xml").is_ok();
    found
}

struct PdfOut {
    pdf: Pdf,
    alloc: Ref,
    catalog: Ref,
    tree: Ref,
    pages: Vec<Ref>,
    cache: HashMap<String, Ref>,
}

impl PdfOut {
    fn new() -> PdfOut {
        let mut alloc = Ref::new(1);
        let catalog = alloc.bump();
        let tree = alloc.bump();
        PdfOut { pdf: Pdf::new(), alloc, catalog, tree, pages: Vec::new(), cache: HashMap::new() }
    }

    fn layer(&mut self, svg: &str, options: &usvg::Options<'_>) -> Option<Ref> {
        let tree = usvg::Tree::from_str(svg, options).ok()?;
        let (chunk, xobject) = svg2pdf::to_chunk(&tree, svg2pdf::ConversionOptions::default()).ok()?;
        let mut mapping = HashMap::new();
        let alloc = &mut self.alloc;
        chunk.renumber_into(&mut self.pdf, |old| *mapping.entry(old).or_insert_with(|| alloc.bump()));
        mapping.get(&xobject).copied()
    }

    fn page(&mut self, w: f32, h: f32, layers: &[Ref]) {
        let page_id = self.alloc.bump();
        let content_id = self.alloc.bump();
        let mut content = Content::new();
        let names: Vec<String> = (0..layers.len()).map(|i| format!("L{i}")).collect();
        for name in &names {
            content.save_state();
            content.transform([w, 0.0, 0.0, h, 0.0, 0.0]);
            content.x_object(Name(name.as_bytes()));
            content.restore_state();
        }
        self.pdf.stream(content_id, &content.finish());
        let mut page = self.pdf.page(page_id);
        page.media_box(Rect::new(0.0, 0.0, w, h));
        page.parent(self.tree);
        page.contents(content_id);
        {
            let mut res = page.resources();
            let mut xo = res.x_objects();
            for (i, r) in layers.iter().enumerate() {
                xo.pair(Name(names[i].as_bytes()), *r);
            }
        }
        page.finish();
        self.pages.push(page_id);
    }

    fn finish(mut self) -> Option<Vec<u8>> {
        if self.pages.is_empty() {
            return None;
        }
        self.pdf.pages(self.tree).kids(self.pages.iter().copied()).count(self.pages.len() as i32);
        self.pdf.catalog(self.catalog).pages(self.tree);
        Some(self.pdf.finish())
    }
}

/// pptx 바이트 → PDF.
pub(crate) fn convert(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let pkg = Package::open(bytes)?;
    convert_package(&pkg)
}

fn convert_package(pkg: &Package) -> Result<Vec<u8>, String> {
    // XML 부분을 모두 읽어 둔다 (서로 가리키며 오간다).
    let mut texts: Vec<(String, String)> = Vec::new();
    for name in pkg_xml_names(pkg) {
        if let Some(t) = pkg.text(&name) {
            texts.push((name, t));
        }
    }
    let opts = ParsingOptions { allow_dtd: true, nodes_limit: 50_000_000 };
    let docs: HashMap<String, Document> =
        texts.iter().filter_map(|(k, v)| Document::parse_with_options(v, opts).ok().map(|d| (k.clone(), d))).collect();

    let pres_path = "ppt/presentation.xml";
    let pres = docs.get(pres_path).ok_or("presentation.xml 없음")?;
    let root = pres.root_element();
    let size = child(root, "sldSz");
    let sw = size.and_then(|s| s.attribute("cx")).and_then(|v| v.parse::<f64>().ok()).unwrap_or(9144000.0) / EMU;
    let sh = size.and_then(|s| s.attribute("cy")).and_then(|v| v.parse::<f64>().ok()).unwrap_or(6858000.0) / EMU;
    let default_text = child(root, "defaultTextStyle");
    let pres_rels = pkg.rels(pres_path);
    let slides: Vec<String> = child(root, "sldIdLst")
        .map(|l| {
            l.children()
                .filter(|c| c.has_tag_name("sldId"))
                .filter_map(|c| c.attributes().find(|a| a.name() == "id" && a.namespace().is_some()).map(|a| a.value().to_owned()))
                .filter_map(|rid| pres_rels.get(&rid).map(|r| r.target.clone()))
                .collect()
        })
        .unwrap_or_default();
    if slides.is_empty() {
        return Err("슬라이드가 없다".into());
    }

    let (fonts_raw, aliases) = {
        let fonts = crate::hwp::SVG_FONTS.lock().map(|f| f.clone()).unwrap_or_default();
        let aliases = crate::fonts::SVG_ALIASES.lock().map(|a| a.clone()).unwrap_or_default();
        (fonts, aliases)
    };
    let fontbook = FontBook::new(&fonts_raw, &aliases);
    let mut options = usvg::Options::default();
    options.fontdb = fontbook.db.clone();
    options.font_resolver = fontbook.resolver();

    let mut r = Renderer { pkg, docs: &docs, fonts: fontbook, images: 0, missing_images: 0, charts: 0 };
    let mut out = PdfOut::new();
    let mut drawn = 0usize;
    let mut failed = 0usize;

    for (index, slide_path) in slides.iter().enumerate() {
        let Some(slide) = docs.get(slide_path.as_str()) else { continue };
        let sroot = slide.root_element();
        if sroot.attribute("show") == Some("0") {
            continue;
        }
        let srels = pkg.rels(slide_path);
        let layout_path = srels.values().find(|r| r.kind == "slideLayout").map(|r| r.target.clone()).unwrap_or_default();
        let lrels = pkg.rels(&layout_path);
        let master_path = lrels.values().find(|r| r.kind == "slideMaster").map(|r| r.target.clone()).unwrap_or_default();
        let mrels = pkg.rels(&master_path);
        let theme_path = mrels.values().find(|r| r.kind == "theme").map(|r| r.target.clone()).unwrap_or_default();
        let layout = docs.get(layout_path.as_str()).map(|d| d.root_element());
        let master = docs.get(master_path.as_str()).map(|d| d.root_element());
        let theme = Theme::read(docs.get(theme_path.as_str()));

        // 색 바꿈: 마스터 → 레이아웃 → 슬라이드
        let mut map = master.and_then(|m| child(m, "clrMap")).map(Palette::read_map).unwrap_or_default();
        let mut sig = theme_path.clone();
        for over in [layout, Some(sroot)].into_iter().flatten() {
            if let Some(o) = path_child(over, &["clrMapOvr", "overrideClrMapping"]) {
                map = Palette::read_map(o);
                sig.push_str(&format!("|{:?}", o.range()));
            }
        }
        let palette = Palette { scheme: theme.scheme.clone(), map };

        let layout_tree = tree_of(layout);
        let master_tree = tree_of(master);
        let slide_bg = bg_of(Some(sroot));
        let show_master_layout = layout.and_then(|l| l.attribute("showMasterSp")) != Some("0");
        let show_master_slide = sroot.attribute("showMasterSp") != Some("0");

        let slide_no = index + 1;
        let base_key = format!(
            "{}|{}|{}|{}|{}",
            if slide_bg.is_some() { slide_path.as_str() } else { layout_path.as_str() },
            layout_path,
            sig,
            show_master_layout,
            show_master_slide
        );
        let base = if let Some(hit) = out.cache.get(&base_key) {
            Some(*hit)
        } else {
            let mut svg = Svg::new("a");
            // 배경
            let (bg, bg_part) = match slide_bg {
                Some(b) => (Some(b), slide_path.as_str()),
                None => match bg_of(layout) {
                    Some(b) => (Some(b), layout_path.as_str()),
                    None => (bg_of(master), master_path.as_str()),
                },
            };
            let bg_rels = pkg.rels(bg_part);
            if let Some(bg) = bg {
                let scope = Scope {
                    part: bg_part,
                    rels: &bg_rels,
                    palette: &palette,
                    theme: &theme,
                    layout_tree: None,
                    master_tree: None,
                    master_root: master,
                    default_text,
                    skip_placeholders: true,
                    slide_no,
                };
                r.background(&mut svg, bg, &scope, sw, sh);
            } else {
                svg.body.push_str(&format!("<rect width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>", geom::f(sw), geom::f(sh)));
            }
            if show_master_slide {
                if show_master_layout {
                    if let Some(t) = master_tree {
                        let scope = Scope {
                            part: &master_path,
                            rels: &mrels,
                            palette: &palette,
                            theme: &theme,
                            layout_tree: None,
                            master_tree: None,
                            master_root: master,
                            default_text,
                            skip_placeholders: true,
                            slide_no,
                        };
                        r.tree(&mut svg, t, &scope, None);
                    }
                }
                if let Some(t) = layout_tree {
                    let scope = Scope {
                        part: &layout_path,
                        rels: &lrels,
                        palette: &palette,
                        theme: &theme,
                        layout_tree: None,
                        master_tree: None,
                        master_root: master,
                        default_text,
                        skip_placeholders: true,
                        slide_no,
                    };
                    r.tree(&mut svg, t, &scope, None);
                }
            }
            let svg_text = svg.finish(sw, sh);
            let made = out.layer(&svg_text, &options);
            if let Some(m) = made {
                out.cache.insert(base_key.clone(), m);
            }
            made
        };

        // 슬라이드 자신
        let mut svg = Svg::new("s");
        if let Some(t) = tree_of(Some(sroot)) {
            let scope = Scope {
                part: slide_path,
                rels: &srels,
                palette: &palette,
                theme: &theme,
                layout_tree,
                master_tree,
                master_root: master,
                default_text,
                skip_placeholders: false,
                slide_no,
            };
            let work = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                r.tree(&mut svg, t, &scope, None);
            }));
            if work.is_err() {
                failed += 1;
            }
        }
        let svg_text = svg.finish(sw, sh);
        let own = out.layer(&svg_text, &options);
        if own.is_none() {
            failed += 1;
        }
        let layers: Vec<Ref> = [base, own].into_iter().flatten().collect();
        if layers.is_empty() {
            continue;
        }
        out.page(sw as f32, sh as f32, &layers);
        drawn += 1;
    }
    crate::note(format!(
        "슬라이드 {drawn}장 (그림 {}개, 못 그린 그림 {}개, 차트 {}개, 실패 {failed})",
        r.images, r.missing_images, r.charts
    ));
    out.finish().ok_or_else(|| "그린 쪽이 없다".to_owned())
}

fn pkg_xml_names(pkg: &Package) -> Vec<String> {
    pkg.names().into_iter().filter(|n| n.ends_with(".xml") && !n.starts_with("customXml/") && !n.starts_with("docProps/")).collect()
}

fn tree_of<'a, 'i>(n: Option<roxmltree::Node<'a, 'i>>) -> Option<roxmltree::Node<'a, 'i>> {
    n.and_then(|n| path_child(n, &["cSld", "spTree"]))
}

fn bg_of<'a, 'i>(n: Option<roxmltree::Node<'a, 'i>>) -> Option<roxmltree::Node<'a, 'i>> {
    n.and_then(|n| path_child(n, &["cSld", "bg"]))
}
