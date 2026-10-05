//! New pages drawn from a layout the browser made, for HTML and Markdown to
//! PDF.
//!
//! The browser lays the document out in the bundled Noto fonts and cuts it
//! into pages; this draws what it measured and nothing else. Text is always
//! set in the supplied fonts, never the standard 14, because the browser
//! measured it in them: drawn in Helvetica, every line would come out short
//! or long. Each piece of text is also fitted to the width the browser gave
//! it, which absorbs the small differences between two shapers.
//!
//! Positions are points from each page's top left, as the browser measures,
//! and are turned upright here.

use std::collections::BTreeMap;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

use crate::compression::write_compressed;
use crate::documents::text_string;
use crate::images::{image_matrix, prepare_image};
use crate::stamps::{FontFamily, number};
use crate::text::{SuppliedFont, Typesetter};

/// How far italic text leans: browsers slant a face without an italic by
/// about 14 degrees, and the bundled fonts have none.
const SLANT: f32 = 0.25;
const KAPPA: f32 = 0.552_284_8;
const LARGEST_PAGE: f32 = 14_400.0;

#[derive(Clone, Copy, Debug)]
pub struct PrintPage {
    pub width: f32,
    pub height: f32,
}

/// A box in points from the page's top left.
#[derive(Clone, Copy, Debug)]
pub struct Area {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

/// Corner radii, clockwise from the top left.
pub type Radii = [f32; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dash {
    Solid,
    Dashed,
    Dotted,
}

#[derive(Clone, Copy, Debug)]
pub enum LinkTarget<'a> {
    Uri(&'a str),
    /// A page from 0 and a height on it in points from its top.
    Page {
        page: usize,
        top: f32,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum PrintMark<'a> {
    Fill {
        area: Area,
        radii: Radii,
        color: u32,
    },
    /// A straight stroke between two points, centred on the line.
    Line {
        from: (f32, f32),
        to: (f32, f32),
        width: f32,
        dash: Dash,
        color: u32,
    },
    /// A stroke around `area`, centred on its edge.
    Outline {
        area: Area,
        radii: Radii,
        width: f32,
        dash: Dash,
        color: u32,
    },
    Text {
        text: &'a str,
        left: f32,
        baseline: f32,
        size: f32,
        /// The width the browser measured; 0 draws the text at its own width.
        width: f32,
        /// Letter spacing in points.
        spacing: f32,
        family: FontFamily,
        bold: bool,
        italic: bool,
        color: u32,
    },
    /// `image` indexes the images passed alongside.
    Image {
        area: Area,
        image: usize,
    },
    Link {
        area: Area,
        target: LinkTarget<'a>,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct Clip {
    pub area: Area,
    pub radii: Radii,
}

#[derive(Clone, Copy, Debug)]
pub struct PrintItem<'a> {
    /// From 0.
    pub page: usize,
    pub mark: PrintMark<'a>,
    pub opacity: f32,
    pub clip: Option<Clip>,
}

/// A heading, which becomes a bookmark.
#[derive(Clone, Copy, Debug)]
pub struct Heading<'a> {
    /// 1 to 6.
    pub level: u8,
    pub title: &'a str,
    pub page: usize,
    pub top: f32,
}

pub struct PrintDocument<'a> {
    pub pages: &'a [PrintPage],
    /// In the order they paint.
    pub items: &'a [PrintItem<'a>],
    pub images: &'a [&'a [u8]],
    pub headings: &'a [Heading<'a>],
    pub title: &'a str,
    /// A BCP 47 tag, or empty.
    pub language: &'a str,
}

pub fn print_pdf_bytes(
    print: &PrintDocument<'_>,
    fonts: &[SuppliedFont<'_>],
) -> Result<Vec<u8>, String> {
    check(print)?;
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_ids = print
        .pages
        .iter()
        .map(|_| document.new_object_id())
        .collect::<Vec<_>>();
    let mut typesetter = Typesetter::new(fonts)?;
    let mut canvases = print
        .pages
        .iter()
        .map(|page| Canvas::new(page.height))
        .collect::<Vec<_>>();
    let mut images = BTreeMap::<usize, Option<(ObjectId, u8)>>::new();
    let mut states = BTreeMap::<u32, ObjectId>::new();

    for item in print.items {
        let canvas = &mut canvases[item.page];
        let opacity = item.opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            continue;
        }
        if let PrintMark::Link { area, target } = item.mark {
            canvas.links.push((area, target));
            continue;
        }
        let wrapped = item.clip.is_some() || opacity < 1.0;
        if wrapped {
            canvas.content.push_str("q\n");
        }
        if let Some(clip) = item.clip {
            let path = canvas.rounded(clip.area, clip.radii);
            canvas.content.push_str(&path);
            canvas.content.push_str("W n\n");
        }
        if opacity < 1.0 {
            // Thousandths are as fine as anyone sees, and keep the states few.
            let key = (opacity * 1000.0).round() as u32;
            let id = *states.entry(key).or_insert_with(|| {
                document.add_object(dictionary! {
                    "Type" => "ExtGState",
                    "ca" => Object::Real(key as f32 / 1000.0),
                    "CA" => Object::Real(key as f32 / 1000.0),
                })
            });
            let name = format!("G{key}");
            canvas.states.set(name.as_bytes(), id);
            canvas.content.push_str(&format!("/{name} gs\n"));
        }
        match item.mark {
            PrintMark::Fill { area, radii, color } => {
                let path = canvas.rounded(area, radii);
                canvas
                    .content
                    .push_str(&format!("{} rg\n{path}f\n", rgb(color)));
            }
            PrintMark::Line {
                from,
                to,
                width,
                dash,
                color,
            } => {
                let height = canvas.height;
                canvas.content.push_str(&format!(
                    "{}{} m {} l S\n",
                    stroke(width, dash, color),
                    point(from.0, height - from.1),
                    point(to.0, height - to.1),
                ));
            }
            PrintMark::Outline {
                area,
                radii,
                width,
                dash,
                color,
            } => {
                let path = canvas.rounded(area, radii);
                canvas
                    .content
                    .push_str(&format!("{}{path}S\n", stroke(width, dash, color)));
            }
            PrintMark::Text {
                text,
                left,
                baseline,
                size,
                width,
                spacing,
                family,
                bold,
                italic,
                color,
            } => {
                let setting = typesetter.embedded(family, bold);
                let mut line = typesetter.line(text, &setting);
                if spacing != 0.0 {
                    line.space_letters(spacing / size * 1000.0);
                }
                let natural = line.width(size);
                // Two shapers agree to a fraction of a percent; anything far
                // off means the browser measured something else, and drawing
                // at the text's own width is the lesser mistake.
                let fit = if width > 0.0 && natural > 0.0 {
                    Some(width / natural).filter(|fit| (0.8..=1.25).contains(fit))
                } else {
                    None
                }
                .unwrap_or(1.0);
                typesetter.name_fonts(&mut document, &line, &mut canvas.fonts);
                let matrix = [
                    fit,
                    0.0,
                    if italic { SLANT } else { 0.0 },
                    1.0,
                    left,
                    canvas.height - baseline,
                ];
                let shown = typesetter.show(&line, size, matrix);
                canvas
                    .content
                    .push_str(&format!("BT\n{} rg\n{shown}ET\n", rgb(color)));
            }
            PrintMark::Image { area, image } => {
                // A picture the browser could decode and this cannot, an
                // arithmetic-coded JPEG say, is left out rather than losing
                // the whole document over it.
                let prepared = *images.entry(image).or_insert_with(|| {
                    let mut prepared = prepare_image(print.images[image]).ok()?;
                    if let Some(mask) = prepared.mask.take() {
                        let mask = document.add_object(mask);
                        prepared.stream.dict.set("SMask", mask);
                    }
                    let orientation = prepared.orientation;
                    Some((document.add_object(prepared.stream), orientation))
                });
                if let Some((id, orientation)) = prepared {
                    let name = format!("Im{image}");
                    canvas.images.set(name.as_bytes(), id);
                    let [a, b, c, d, e, f] = image_matrix(
                        orientation,
                        area.left,
                        canvas.height - area.top - area.height,
                        area.width,
                        area.height,
                    );
                    canvas.content.push_str(&format!(
                        "q\n{} {} {} {} {} {} cm\n/{name} Do\nQ\n",
                        number(a),
                        number(b),
                        number(c),
                        number(d),
                        number(e),
                        number(f)
                    ));
                }
            }
            PrintMark::Link { .. } => unreachable!(),
        }
        if wrapped {
            canvas.content.push_str("Q\n");
        }
    }

    for (index, canvas) in canvases.into_iter().enumerate() {
        let page = print.pages[index];
        let mut resources = Dictionary::new();
        if !canvas.fonts.is_empty() {
            resources.set("Font", canvas.fonts);
        }
        if !canvas.images.is_empty() {
            resources.set("XObject", canvas.images);
        }
        if !canvas.states.is_empty() {
            resources.set("ExtGState", canvas.states);
        }
        let content =
            document.add_object(Stream::new(Dictionary::new(), canvas.content.into_bytes()));
        let mut dictionary = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), Object::Real(page.width), Object::Real(page.height)],
            "Resources" => resources,
            "Contents" => content,
        };
        let annotations = canvas
            .links
            .iter()
            .map(|&(area, target)| {
                let rect = vec![
                    Object::Real(area.left),
                    Object::Real(page.height - area.top - area.height),
                    Object::Real(area.left + area.width),
                    Object::Real(page.height - area.top),
                ];
                let mut link = dictionary! {
                    "Type" => "Annot",
                    "Subtype" => "Link",
                    "Rect" => rect,
                    "Border" => vec![0.into(), 0.into(), 0.into()],
                };
                match target {
                    LinkTarget::Uri(uri) => link.set(
                        "A",
                        dictionary! { "S" => "URI", "URI" => Object::string_literal(uri) },
                    ),
                    LinkTarget::Page { page, top } => {
                        link.set("Dest", destination(page_ids[page], print.pages[page], top))
                    }
                }
                Object::Reference(document.add_object(link))
            })
            .collect::<Vec<_>>();
        if !annotations.is_empty() {
            dictionary.set("Annots", annotations);
        }
        document
            .objects
            .insert(page_ids[index], Object::Dictionary(dictionary));
    }
    typesetter.write(&mut document)?;

    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids.iter().map(|&id| Object::Reference(id)).collect::<Vec<_>>(),
            "Count" => page_ids.len() as i64,
        }),
    );
    let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
    if let Some(outlines) = outline(&mut document, print, &page_ids) {
        catalog.set("Outlines", outlines);
    }
    if !print.language.is_empty() {
        catalog.set("Lang", Object::string_literal(print.language));
    }
    let title = print.title.trim();
    if !title.is_empty() {
        catalog.set(
            "ViewerPreferences",
            dictionary! { "DisplayDocTitle" => true },
        );
        let info = document.add_object(dictionary! {
            "Title" => text_string(title),
            "Producer" => Object::string_literal("Plico"),
        });
        document.trailer.set("Info", info);
    }
    let catalog = document.add_object(catalog);
    document.trailer.set("Root", catalog);
    document.compress();
    write_compressed(document)
}

fn check(print: &PrintDocument<'_>) -> Result<(), String> {
    if print.pages.is_empty() {
        return Err("There is nothing to print.".into());
    }
    let finite = |values: &[f32]| values.iter().all(|value| value.is_finite());
    let page_ok = |page: &PrintPage| {
        finite(&[page.width, page.height])
            && page.width >= 1.0
            && page.height >= 1.0
            && page.width <= LARGEST_PAGE
            && page.height <= LARGEST_PAGE
    };
    if !print.pages.iter().all(page_ok) {
        return Err("Choose a valid page size.".into());
    }
    let area_ok = |area: &Area| {
        finite(&[area.left, area.top, area.width, area.height])
            && area.width >= 0.0
            && area.height >= 0.0
    };
    let corners_ok = |radii: &Radii| finite(radii) && radii.iter().all(|radius| *radius >= 0.0);
    let page_count = print.pages.len();
    for item in print.items {
        let valid = item.page < page_count
            && item.opacity.is_finite()
            && item
                .clip
                .is_none_or(|clip| area_ok(&clip.area) && corners_ok(&clip.radii))
            && match item.mark {
                PrintMark::Fill { area, radii, .. } => area_ok(&area) && corners_ok(&radii),
                PrintMark::Line {
                    from, to, width, ..
                } => finite(&[from.0, from.1, to.0, to.1, width]) && width > 0.0,
                PrintMark::Outline {
                    area, radii, width, ..
                } => area_ok(&area) && corners_ok(&radii) && width.is_finite() && width > 0.0,
                PrintMark::Text {
                    left,
                    baseline,
                    size,
                    width,
                    spacing,
                    ..
                } => finite(&[left, baseline, width, spacing]) && size > 0.0 && size <= 1_000.0,
                PrintMark::Image { area, image } => area_ok(&area) && image < print.images.len(),
                PrintMark::Link { area, target } => {
                    area_ok(&area)
                        && match target {
                            LinkTarget::Uri(uri) => !uri.is_empty(),
                            LinkTarget::Page { page, top } => page < page_count && top.is_finite(),
                        }
                }
            };
        if !valid {
            return Err("The document's layout could not be read.".into());
        }
    }
    if !print.headings.iter().all(|heading| {
        heading.page < page_count && heading.top.is_finite() && (1..=6).contains(&heading.level)
    }) {
        return Err("The document's headings could not be read.".into());
    }
    Ok(())
}

struct Canvas<'a> {
    height: f32,
    content: String,
    fonts: Dictionary,
    images: Dictionary,
    states: Dictionary,
    links: Vec<(Area, LinkTarget<'a>)>,
}

impl Canvas<'_> {
    fn new(height: f32) -> Self {
        Canvas {
            height,
            content: String::new(),
            fonts: Dictionary::new(),
            images: Dictionary::new(),
            states: Dictionary::new(),
            links: Vec::new(),
        }
    }

    /// A closed path around `area` with its corners rounded.
    fn rounded(&self, area: Area, radii: Radii) -> String {
        let (left, right) = (area.left, area.left + area.width);
        let (top, bottom) = (self.height - area.top, self.height - area.top - area.height);
        // Radii that together overrun a side all shrink by the same factor
        // (CSS Backgrounds 3, 5.5).
        let [top_left, top_right, bottom_right, bottom_left] = radii;
        let sides = [
            (top_left + top_right, area.width),
            (bottom_left + bottom_right, area.width),
            (top_left + bottom_left, area.height),
            (top_right + bottom_right, area.height),
        ];
        let scale = sides
            .iter()
            .filter(|(sum, _)| *sum > 0.0)
            .map(|(sum, side)| side / sum)
            .fold(1.0_f32, f32::min);
        let [top_left, top_right, bottom_right, bottom_left] = radii.map(|radius| radius * scale);
        if top_left + top_right + bottom_right + bottom_left == 0.0 {
            return format!(
                "{} {} {} {} re\n",
                number(left),
                number(bottom),
                number(area.width),
                number(area.height)
            );
        }
        let curve = |x0: f32, y0: f32, x1: f32, y1: f32, corner: (f32, f32)| {
            format!(
                "{} {} {} c\n",
                point(x0 + (corner.0 - x0) * KAPPA, y0 + (corner.1 - y0) * KAPPA),
                point(x1 + (corner.0 - x1) * KAPPA, y1 + (corner.1 - y1) * KAPPA),
                point(x1, y1)
            )
        };
        let mut path = format!("{} m\n", point(left + top_left, top));
        path.push_str(&format!("{} l\n", point(right - top_right, top)));
        if top_right > 0.0 {
            path.push_str(&curve(
                right - top_right,
                top,
                right,
                top - top_right,
                (right, top),
            ));
        }
        path.push_str(&format!("{} l\n", point(right, bottom + bottom_right)));
        if bottom_right > 0.0 {
            path.push_str(&curve(
                right,
                bottom + bottom_right,
                right - bottom_right,
                bottom,
                (right, bottom),
            ));
        }
        path.push_str(&format!("{} l\n", point(left + bottom_left, bottom)));
        if bottom_left > 0.0 {
            path.push_str(&curve(
                left + bottom_left,
                bottom,
                left,
                bottom + bottom_left,
                (left, bottom),
            ));
        }
        path.push_str(&format!("{} l\n", point(left, top - top_left)));
        if top_left > 0.0 {
            path.push_str(&curve(
                left,
                top - top_left,
                left + top_left,
                top,
                (left, top),
            ));
        }
        path.push_str("h\n");
        path
    }
}

fn point(x: f32, y: f32) -> String {
    format!("{} {}", number(x), number(y))
}

fn rgb(color: u32) -> String {
    let channel = |shift: u32| number(((color >> shift) & 0xFF) as f32 / 255.0);
    format!("{} {} {}", channel(16), channel(8), channel(0))
}

/// Stroke width, colour and dashes as browsers draw CSS borders: dashes three
/// widths long, dots round and a width apart.
fn stroke(width: f32, dash: Dash, color: u32) -> String {
    let pattern = match dash {
        Dash::Solid => "[] 0 d 0 J".to_owned(),
        Dash::Dashed => format!("[{0} {0}] 0 d 0 J", number(width * 3.0)),
        Dash::Dotted => format!("[0 {}] 0 d 1 J", number(width * 2.0)),
    };
    format!("{} w {pattern}\n{} RG\n", number(width), rgb(color))
}

fn destination(page: ObjectId, size: PrintPage, top: f32) -> Object {
    Object::Array(vec![
        Object::Reference(page),
        "XYZ".into(),
        Object::Integer(0),
        Object::Real((size.height - top).clamp(0.0, size.height)),
        Object::Null,
    ])
}

/// Bookmarks nested by heading level: a heading belongs under the nearest
/// heading before it with a lower level.
fn outline(
    document: &mut Document,
    print: &PrintDocument<'_>,
    page_ids: &[ObjectId],
) -> Option<ObjectId> {
    let headings = print
        .headings
        .iter()
        .filter(|heading| !heading.title.trim().is_empty())
        .collect::<Vec<_>>();
    if headings.is_empty() {
        return None;
    }
    let root = document.new_object_id();
    let ids = headings
        .iter()
        .map(|_| document.new_object_id())
        .collect::<Vec<_>>();
    // Each entry's parent, as an index into `headings`, or None for the root.
    let mut parents = Vec::<Option<usize>>::with_capacity(headings.len());
    let mut open = Vec::<usize>::new();
    for (index, heading) in headings.iter().enumerate() {
        while open
            .last()
            .is_some_and(|&last| headings[last].level >= heading.level)
        {
            open.pop();
        }
        parents.push(open.last().copied());
        open.push(index);
    }
    let children = |parent: Option<usize>| {
        (0..headings.len())
            .filter(|&index| parents[index] == parent)
            .collect::<Vec<_>>()
    };
    // Every entry is open, so each counts all that sit under it.
    let mut counts = vec![0_i64; headings.len()];
    for index in (0..headings.len()).rev() {
        if let Some(parent) = parents[index] {
            counts[parent] += counts[index] + 1;
        }
    }
    for (index, heading) in headings.iter().enumerate() {
        let parent = parents[index];
        let siblings = children(parent);
        let place = siblings.iter().position(|&sibling| sibling == index)?;
        let mut item = dictionary! {
            "Title" => text_string(heading.title.trim()),
            "Parent" => parent.map_or(root, |parent| ids[parent]),
            "Dest" => destination(page_ids[heading.page], print.pages[heading.page], heading.top),
        };
        if place > 0 {
            item.set("Prev", ids[siblings[place - 1]]);
        }
        if let Some(&next) = siblings.get(place + 1) {
            item.set("Next", ids[next]);
        }
        let own = children(Some(index));
        if let (Some(&first), Some(&last)) = (own.first(), own.last()) {
            item.set("First", ids[first]);
            item.set("Last", ids[last]);
            item.set("Count", counts[index]);
        }
        document
            .objects
            .insert(ids[index], Object::Dictionary(item));
    }
    let top = children(None);
    document.objects.insert(
        root,
        Object::Dictionary(dictionary! {
            "Type" => "Outlines",
            "First" => ids[*top.first()?],
            "Last" => ids[*top.last()?],
            "Count" => headings.len() as i64,
        }),
    );
    Some(root)
}
