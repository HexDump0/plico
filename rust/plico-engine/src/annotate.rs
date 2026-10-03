//! Annotations: highlights, drawing, shapes, text boxes, notes and pictures
//! added to pages that already exist.
//!
//! Each one is a real annotation with a stored appearance, so every reader
//! draws it as written rather than making up a look of its own, and any PDF
//! editor can still move, change or delete it. Pages are changed where they
//! are, like stamping. Geometry is given as the reader sees the page and drawn
//! through the page's frame, so /Rotate, /CropBox and /UserUnit hold: the
//! appearance's bounding box is the annotation's rectangle in page space, and
//! its content turns into the frame first, so readers place it without any
//! fitting (ISO 32000-1, 12.5.5).
//!
//! With `flatten`, the same appearances are drawn into the page content
//! instead of being added as annotations. That is adding text, shapes or
//! pictures to the page itself, which is what editing a PDF mostly means.

use std::collections::{BTreeMap, BTreeSet};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

use crate::documents::{load_document, parse_version, text_string};
use crate::flatten::{prune_fields, remove_orphaned_popups, set_annotations};
use crate::images::{image_matrix, prepare_image};
use crate::stamps::{
    FontFamily, LEADING, Stamper, base_font, cap_height, finish, matrix, number, page_frame,
    resolve, selected_pages, text_width, win_ansi,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Markup {
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Rectangle,
    Ellipse,
}

/// Points and boxes are fractions of the visible page's width and height,
/// measured from its top left as the reader sees it. Boxes are left, top,
/// right and bottom.
pub enum AnnotationKind<'a> {
    /// Boxes over text, usually one per line.
    Markup { style: Markup, boxes: Vec<[f32; 4]> },
    /// Freehand strokes, each a run of points; `width` is in points.
    Ink {
        strokes: Vec<Vec<[f32; 2]>>,
        width: f32,
    },
    Shape {
        shape: Shape,
        area: [f32; 4],
        width: f32,
        fill: Option<[f32; 3]>,
    },
    Line {
        from: [f32; 2],
        to: [f32; 2],
        width: f32,
        arrow: bool,
    },
    /// Text wrapped to the box's width, in the annotation's colour. Lines that
    /// do not fit the box's height are cut off by it.
    Text {
        area: [f32; 4],
        text: &'a str,
        family: FontFamily,
        bold: bool,
        size: f32,
        fill: Option<[f32; 3]>,
    },
    /// A note icon with its top left at `at`; the comment is the note.
    Note { at: [f32; 2] },
    /// A JPG or PNG with its left edge, top edge and width; the height follows
    /// from the image.
    Image { place: [f32; 3], bytes: &'a [u8] },
}

pub struct Annotation<'a> {
    /// From 1.
    pub page: u32,
    pub kind: AnnotationKind<'a>,
    /// Red, green and blue, each 0 to 1.
    pub color: [f32; 3],
    pub opacity: f32,
    /// What readers show when the annotation is opened or pointed at.
    pub comment: &'a str,
}

/// Annotation flags (12.5.3): shown on paper as well as on screen.
const PRINT: i64 = 4;

/// Space between a text box's edges and its text, in points.
pub const TEXT_PADDING: f32 = 2.0;

/// A note icon's side, in points.
pub const NOTE_SIZE: f32 = 20.0;

/// Control point distance for a quarter circle drawn as a cubic curve.
const KAPPA: f32 = 0.552_284_8;

/// Adds `annotations` after deleting the annotations already in the file
/// whose object ids are in `remove`.
pub fn annotate_pdf_bytes(
    input: &[u8],
    password: &str,
    annotations: &[Annotation<'_>],
    remove: &[ObjectId],
    flatten: bool,
) -> Result<Vec<u8>, String> {
    if annotations.is_empty() && remove.is_empty() {
        return Err("Add at least one annotation.".into());
    }
    for annotation in annotations {
        check(annotation, flatten)?;
    }

    let mut document = load_document(input, 1, password)?;
    if !remove.is_empty() {
        remove_annotations(&mut document, remove)?;
    }
    let numbers = annotations
        .iter()
        .map(|annotation| annotation.page)
        .collect::<Vec<_>>();
    let pages = selected_pages(&document, &numbers)?
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let mut drawn = BTreeMap::<ObjectId, Vec<ObjectId>>::new();
    let mut transparent = false;

    for (index, annotation) in annotations.iter().enumerate() {
        let page_id = pages[&annotation.page];
        let frame = page_frame(&document, page_id)?;
        let mut sketch = Sketch::new(frame.width, frame.height);
        let mut resources = Dictionary::new();
        let mut state = dictionary! { "Type" => "ExtGState" };
        if annotation.opacity < 1.0 {
            state.set("ca", Object::Real(annotation.opacity));
            state.set("CA", Object::Real(annotation.opacity));
            transparent = true;
        }
        let mut entries = sketch.draw(&mut document, annotation, &mut resources, &mut state)?;
        if state.len() > 1 {
            transparent = true;
            let state = document.add_object(state);
            resources.set("ExtGState", dictionary! { "G0" => state });
        }

        let [a, b, c, d, e, f] = frame.matrix;
        let to_page = |x: f32, y: f32| (a * x + c * y + e, b * x + d * y + f);
        let (left, bottom, right, top) = sketch.bounds();
        let corners = [(left, bottom), (right, top)].map(|(x, y)| to_page(x, y));
        let rect = [
            corners[0].0.min(corners[1].0),
            corners[0].1.min(corners[1].1),
            corners[0].0.max(corners[1].0),
            corners[0].1.max(corners[1].1),
        ];
        let mut content = String::from("q\n");
        if resources.has(b"ExtGState") {
            content.push_str("/G0 gs\n");
        }
        content.push_str(&format!(
            "{} cm\n{}Q\n",
            matrix(frame.matrix),
            sketch.content
        ));
        let appearance = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => reals(&rect),
                "Resources" => resources,
            },
            content.into_bytes(),
        ));

        if flatten {
            drawn.entry(page_id).or_default().push(appearance);
            continue;
        }
        // Page space, the way /Rect, /QuadPoints, /InkList and /L are read.
        for (key, points) in sketch.page_points.drain(..) {
            let values = points
                .into_iter()
                .map(|(x, y)| to_page(x, y))
                .flat_map(|(x, y)| [x, y])
                .collect::<Vec<_>>();
            entries.set(key, reals(&values));
        }
        if let Some(strokes) = sketch.strokes.take() {
            let list = strokes
                .into_iter()
                .map(|stroke| {
                    let values = stroke
                        .into_iter()
                        .map(|(x, y)| to_page(x, y))
                        .flat_map(|(x, y)| [x, y])
                        .collect::<Vec<_>>();
                    Object::Array(reals(&values))
                })
                .collect::<Vec<_>>();
            entries.set("InkList", list);
        }
        entries.set("Type", "Annot");
        entries.set("Rect", reals(&rect));
        entries.set("P", page_id);
        entries.set("F", PRINT);
        entries.set("NM", Object::string_literal(format!("plico-{index}")));
        entries.set("AP", dictionary! { "N" => appearance });
        if annotation.opacity < 1.0 {
            entries.set("CA", Object::Real(annotation.opacity));
        }
        if !annotation.comment.is_empty() {
            entries.set("Contents", text_string(annotation.comment));
        }
        let id = document.add_object(entries);
        add_to_page(&mut document, page_id, id)?;
    }

    let mut stamper = Stamper::new(&mut document, 1.0);
    for (page_id, appearances) in drawn {
        let frame = page_frame(&document, page_id)?;
        let mut xobjects = Dictionary::new();
        let mut content = String::new();
        for (index, appearance) in appearances.into_iter().enumerate() {
            let name = format!("A{index}");
            content.push_str(&format!("/{name} Do\n"));
            xobjects.set(name.as_bytes(), appearance);
        }
        let form = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => reals(&frame.media_box),
                "Resources" => dictionary! { "XObject" => xobjects },
            },
            content.into_bytes(),
        ));
        stamper.place(&mut document, page_id, form, false)?;
    }
    // Constant opacity and blend modes arrived in PDF 1.4.
    if transparent && parse_version(&document.version) < (1, 4) {
        document.version = "1.4".into();
    }
    finish(document, 1.0)
}

/// Takes the annotations in `remove` off their pages, with their popups, and
/// form fields left with no widget out of the form.
fn remove_annotations(document: &mut Document, remove: &[ObjectId]) -> Result<(), String> {
    let wanted = remove.iter().copied().collect::<BTreeSet<_>>();
    let mut removed = BTreeSet::new();
    for (_, page_id) in selected_pages(document, &[])? {
        let Some(annotations) = document
            .get_dictionary(page_id)
            .ok()
            .and_then(|page| page.get(b"Annots").ok())
            .and_then(|value| resolve(document, value))
            .and_then(|value| value.as_array().ok())
            .cloned()
        else {
            continue;
        };
        let count = annotations.len();
        let remaining = annotations
            .into_iter()
            .filter(|entry| match entry.as_reference() {
                Ok(id) if wanted.contains(&id) => {
                    removed.insert(id);
                    false
                }
                _ => true,
            })
            .collect::<Vec<_>>();
        if remaining.len() != count {
            set_annotations(document, page_id, remaining)?;
        }
    }
    if removed.len() != wanted.len() {
        return Err("Some of the annotations to remove are no longer in this PDF.".into());
    }
    let widgets = removed
        .iter()
        .copied()
        .filter(|&id| {
            document
                .get_dictionary(id)
                .ok()
                .and_then(|annotation| annotation.get(b"Subtype").ok())
                .and_then(|subtype| subtype.as_name().ok())
                == Some(b"Widget")
        })
        .collect::<BTreeSet<_>>();
    remove_orphaned_popups(document, &removed)?;
    if !widgets.is_empty()
        && let Ok(catalog) = document.trailer.get(b"Root").and_then(Object::as_reference)
    {
        prune_fields(document, catalog, &widgets);
    }
    Ok(())
}

fn check(annotation: &Annotation<'_>, flatten: bool) -> Result<(), String> {
    let fraction = |value: &f32| (0.0..=1.0).contains(value);
    let area = |[left, top, right, bottom]: [f32; 4]| {
        [left, top, right, bottom].iter().all(fraction) && left < right && top < bottom
    };
    let colour = |rgb: &[f32; 3]| rgb.iter().all(fraction);
    let width = |width: f32| (0.1..=100.0).contains(&width);
    if !colour(&annotation.color) || !colour_of(&annotation.kind).is_none_or(|fill| colour(&fill)) {
        return Err("Choose a valid annotation color.".into());
    }
    if !(annotation.opacity > 0.0 && annotation.opacity <= 1.0) {
        return Err("Choose an opacity between 1 and 100%.".into());
    }
    let page = annotation.page;
    let valid = match &annotation.kind {
        AnnotationKind::Markup { boxes, .. } => {
            !boxes.is_empty() && boxes.iter().all(|&found| area(found))
        }
        AnnotationKind::Ink { strokes, width: w } => {
            width(*w)
                && !strokes.is_empty()
                && strokes
                    .iter()
                    .all(|stroke| !stroke.is_empty() && stroke.iter().flatten().all(fraction))
        }
        AnnotationKind::Shape {
            area: found,
            width: w,
            ..
        } => width(*w) && area(*found),
        AnnotationKind::Line {
            from, to, width: w, ..
        } => width(*w) && from.iter().chain(to).all(fraction) && from != to,
        AnnotationKind::Text {
            area: found,
            text,
            size,
            ..
        } => {
            if text.trim().is_empty() {
                return Err(format!("A text box on page {page} is empty."));
            }
            if let Some(character) = text.chars().find(|&character| {
                !matches!(character, '\n' | '\r') && win_ansi(character).is_none()
            }) {
                return Err(format!(
                    "“{character}” cannot be drawn with the built-in PDF fonts."
                ));
            }
            (1.0..=1_000.0).contains(size) && area(*found)
        }
        AnnotationKind::Note { at } => {
            if flatten {
                return Err("Notes cannot be drawn into the page.".into());
            }
            if annotation.comment.trim().is_empty() {
                return Err(format!("A note on page {page} is empty."));
            }
            at.iter().all(fraction)
        }
        AnnotationKind::Image {
            place: [left, top, w],
            ..
        } => fraction(left) && fraction(top) && *w > 0.0 && *w <= 1.0,
    };
    if valid {
        Ok(())
    } else {
        Err(format!("An annotation on page {page} is not on the page."))
    }
}

fn colour_of(kind: &AnnotationKind<'_>) -> Option<[f32; 3]> {
    match kind {
        AnnotationKind::Shape { fill, .. } | AnnotationKind::Text { fill, .. } => *fill,
        _ => None,
    }
}

/// An appearance being drawn in the page's frame, and its extent there.
struct Sketch {
    width: f32,
    height: f32,
    content: String,
    extent: Option<(f32, f32, f32, f32)>,
    /// Annotation entries holding points, still in the frame.
    page_points: Vec<(&'static str, Vec<(f32, f32)>)>,
    strokes: Option<Vec<Vec<(f32, f32)>>>,
}

impl Sketch {
    fn new(width: f32, height: f32) -> Sketch {
        Sketch {
            width,
            height,
            content: String::new(),
            extent: None,
            page_points: Vec::new(),
            strokes: None,
        }
    }

    fn point(&self, [x, y]: [f32; 2]) -> (f32, f32) {
        (x * self.width, (1.0 - y) * self.height)
    }

    /// Left, bottom, right and top in the frame.
    fn area(&self, [left, top, right, bottom]: [f32; 4]) -> [f32; 4] {
        [
            left * self.width,
            (1.0 - bottom) * self.height,
            right * self.width,
            (1.0 - top) * self.height,
        ]
    }

    fn include(&mut self, x: f32, y: f32, margin: f32) {
        let (left, bottom, right, top) = self.extent.unwrap_or((x, y, x, y));
        self.extent = Some((
            left.min(x - margin),
            bottom.min(y - margin),
            right.max(x + margin),
            top.max(y + margin),
        ));
    }

    fn bounds(&self) -> (f32, f32, f32, f32) {
        let (left, bottom, right, top) = self.extent.unwrap_or_default();
        // Readers ignore an annotation with an empty rectangle.
        (left, bottom, right.max(left + 0.01), top.max(bottom + 0.01))
    }

    fn push(&mut self, text: &str) {
        self.content.push_str(text);
    }

    /// Draws `annotation` and returns its kind-specific entries.
    fn draw(
        &mut self,
        document: &mut Document,
        annotation: &Annotation<'_>,
        resources: &mut Dictionary,
        state: &mut Dictionary,
    ) -> Result<Dictionary, String> {
        let color = annotation.color;
        let mut entries = dictionary! { "C" => reals(&color) };
        match &annotation.kind {
            AnnotationKind::Markup { style, boxes } => {
                let subtype = match style {
                    Markup::Highlight => "Highlight",
                    Markup::Underline => "Underline",
                    Markup::StrikeOut => "StrikeOut",
                    Markup::Squiggly => "Squiggly",
                };
                entries.set("Subtype", subtype);
                let mut quads = Vec::new();
                if *style == Markup::Highlight {
                    // Multiplied, so the text under it stays readable.
                    state.set("BM", "Multiply");
                    self.push(&format!("{} rg\n", rgb(color)));
                } else {
                    self.push(&format!("{} RG\n", rgb(color)));
                }
                for &found in boxes {
                    let [x0, y0, x1, y1] = self.area(found);
                    // Upper left, upper right, lower left, lower right as the
                    // reader sees them, the order Acrobat writes and readers
                    // expect, whatever the spec's figure suggests.
                    quads.extend([(x0, y1), (x1, y1), (x0, y0), (x1, y0)]);
                    self.include(x0, y0, 0.0);
                    self.include(x1, y1, 0.0);
                    let height = y1 - y0;
                    let thickness = (height / 16.0).max(0.5);
                    match style {
                        Markup::Highlight => self.push(&format!(
                            "{} {} {} {} re\nf\n",
                            number(x0),
                            number(y0),
                            number(x1 - x0),
                            number(height)
                        )),
                        Markup::Underline | Markup::StrikeOut => {
                            let y = if *style == Markup::Underline {
                                y0 + thickness
                            } else {
                                y0 + height * 0.45
                            };
                            self.push(&format!(
                                "{} w\n{} {} m\n{} {} l\nS\n",
                                number(thickness),
                                number(x0),
                                number(y),
                                number(x1),
                                number(y)
                            ));
                        }
                        Markup::Squiggly => {
                            let step = (height / 6.0).max(1.0);
                            let (low, high) = (y0 + thickness, y0 + thickness + step);
                            self.push(&format!(
                                "{} w\n1 j\n{} {} m\n",
                                number(thickness),
                                number(x0),
                                number(low)
                            ));
                            let mut x = x0;
                            let mut up = true;
                            while x < x1 {
                                x = (x + step).min(x1);
                                let y = if up { high } else { low };
                                self.push(&format!("{} {} l\n", number(x), number(y)));
                                up = !up;
                            }
                            self.push("S\n");
                        }
                    }
                }
                self.page_points.push(("QuadPoints", quads));
            }
            AnnotationKind::Ink { strokes, width } => {
                entries.set("Subtype", "Ink");
                entries.set("BS", dictionary! { "W" => Object::Real(*width) });
                self.push(&format!(
                    "{} RG\n{} w\n1 J\n1 j\n",
                    rgb(color),
                    number(*width)
                ));
                let mut list = Vec::new();
                for stroke in strokes {
                    let points = stroke
                        .iter()
                        .map(|&point| self.point(point))
                        .collect::<Vec<_>>();
                    for &(x, y) in &points {
                        self.include(x, y, width / 2.0);
                    }
                    self.push(&smooth(&points));
                    list.push(points);
                }
                self.strokes = Some(list);
            }
            AnnotationKind::Shape {
                shape,
                area,
                width,
                fill,
            } => {
                entries.set(
                    "Subtype",
                    match shape {
                        Shape::Rectangle => "Square",
                        Shape::Ellipse => "Circle",
                    },
                );
                entries.set("BS", dictionary! { "W" => Object::Real(*width) });
                if let Some(fill) = fill {
                    entries.set("IC", reals(fill));
                    self.push(&format!("{} rg\n", rgb(*fill)));
                }
                // The stroke sits inside the box, as the spec puts the border
                // of these annotations.
                let [x0, y0, x1, y1] = self.area(*area);
                self.include(x0, y0, 0.0);
                self.include(x1, y1, 0.0);
                let inset = (width / 2.0).min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
                let [x0, y0, x1, y1] = [x0 + inset, y0 + inset, x1 - inset, y1 - inset];
                self.push(&format!("{} RG\n{} w\n", rgb(color), number(*width)));
                match shape {
                    Shape::Rectangle => self.push(&format!(
                        "{} {} {} {} re\n",
                        number(x0),
                        number(y0),
                        number(x1 - x0),
                        number(y1 - y0)
                    )),
                    Shape::Ellipse => self.push(&ellipse(x0, y0, x1, y1)),
                }
                self.push(if fill.is_some() { "B\n" } else { "S\n" });
            }
            AnnotationKind::Line {
                from,
                to,
                width,
                arrow,
            } => {
                entries.set("Subtype", "Line");
                entries.set("BS", dictionary! { "W" => Object::Real(*width) });
                let ending = if *arrow { "OpenArrow" } else { "None" };
                entries.set(
                    "LE",
                    vec![Object::Name(b"None".to_vec()), Object::Name(ending.into())],
                );
                let ((x0, y0), (x1, y1)) = (self.point(*from), self.point(*to));
                self.page_points.push(("L", vec![(x0, y0), (x1, y1)]));
                self.push(&format!(
                    "{} RG\n{} w\n1 J\n1 j\n{} {} m\n{} {} l\n",
                    rgb(color),
                    number(*width),
                    number(x0),
                    number(y0),
                    number(x1),
                    number(y1)
                ));
                self.include(x0, y0, width / 2.0);
                self.include(x1, y1, width / 2.0);
                if *arrow {
                    let length = ((x1 - x0).hypot(y1 - y0)).max(f32::EPSILON);
                    let (ux, uy) = ((x1 - x0) / length, (y1 - y0) / length);
                    let size = width * 3.0 + 4.0;
                    // Thirty degrees either side of the line.
                    let (sin, cos) = 30f32.to_radians().sin_cos();
                    for side in [1.0, -1.0] {
                        let (dx, dy) =
                            (-(ux * cos - side * uy * sin), -(side * ux * sin + uy * cos));
                        let (x, y) = (x1 + dx * size, y1 + dy * size);
                        self.push(&format!(
                            "{} {} m\n{} {} l\n",
                            number(x1),
                            number(y1),
                            number(x),
                            number(y)
                        ));
                        self.include(x, y, width / 2.0);
                    }
                }
                self.push("S\n");
            }
            AnnotationKind::Text {
                area,
                text,
                family,
                bold,
                size,
                fill,
            } => {
                entries.set("Subtype", "FreeText");
                entries.remove(b"C");
                if let Some(fill) = fill {
                    entries.set("C", reals(fill));
                }
                entries.set("BS", dictionary! { "W" => 0 });
                entries.set(
                    "DA",
                    Object::string_literal(format!("/F0 {} Tf {} rg", number(*size), rgb(color))),
                );
                let font = document.add_object(dictionary! {
                    "Type" => "Font",
                    "Subtype" => "Type1",
                    "BaseFont" => base_font(*family, *bold),
                    "Encoding" => "WinAnsiEncoding",
                });
                resources.set("Font", dictionary! { "F0" => font });
                let [x0, y0, x1, y1] = self.area(*area);
                self.include(x0, y0, 0.0);
                self.include(x1, y1, 0.0);
                if let Some(fill) = fill {
                    self.push(&format!(
                        "{} rg\n{} {} {} {} re\nf\n",
                        rgb(*fill),
                        number(x0),
                        number(y0),
                        number(x1 - x0),
                        number(y1 - y0)
                    ));
                }
                let lines = wrap(text, *family, *bold, *size, x1 - x0 - 2.0 * TEXT_PADDING);
                self.push(&format!(
                    "{} {} {} {} re\nW\nn\n{} rg\nBT\n/F0 {} Tf\n",
                    number(x0),
                    number(y0),
                    number(x1 - x0),
                    number(y1 - y0),
                    rgb(color),
                    number(*size)
                ));
                let first = y1 - TEXT_PADDING - cap_height(*family, *bold) * size;
                for (index, line) in lines.iter().enumerate() {
                    let y = first - index as f32 * LEADING * size;
                    if y < y0 - size {
                        break;
                    }
                    self.push(&format!(
                        "1 0 0 1 {} {} Tm\n<{}> Tj\n",
                        number(x0 + TEXT_PADDING),
                        number(y),
                        hex(line)
                    ));
                }
                self.push("ET\n");
            }
            AnnotationKind::Note { at } => {
                entries.set("Subtype", "Text");
                entries.set("Name", "Comment");
                let (x, top) = self.point(*at);
                // Kept on the page where it fits.
                let x = x.min(self.width - NOTE_SIZE).max(0.0);
                let y = (top - NOTE_SIZE).max(0.0);
                self.include(x, y, 0.0);
                self.include(x + NOTE_SIZE, y + NOTE_SIZE, 0.0);
                let scale = NOTE_SIZE / 20.0;
                let at = |px: f32, py: f32| {
                    format!("{} {}", number(x + px * scale), number(y + py * scale))
                };
                self.push(&format!(
                    "{} rg\n0.2 G\n{} w\n1 j\n{} m\n{} l\n{} l\n{} l\n{} l\n{} l\n{} l\nh\nB\n\
                     {} m\n{} l\n{} m\n{} l\nS\n",
                    rgb(color),
                    number(scale),
                    at(2.0, 18.0),
                    at(18.0, 18.0),
                    at(18.0, 6.0),
                    at(9.0, 6.0),
                    at(5.0, 2.0),
                    at(6.0, 6.0),
                    at(2.0, 6.0),
                    at(5.0, 14.0),
                    at(15.0, 14.0),
                    at(5.0, 10.0),
                    at(12.0, 10.0),
                ));
            }
            AnnotationKind::Image {
                place: [left, top, width],
                bytes,
            } => {
                entries.set("Subtype", "Stamp");
                let mut image = prepare_image(bytes)
                    .map_err(|error| format!("The image could not be used: {error}"))?;
                let (upright_width, upright_height) = if matches!(image.orientation, 5..=8) {
                    (image.height, image.width)
                } else {
                    (image.width, image.height)
                };
                if let Some(mask) = image.mask.take() {
                    let mask = document.add_object(mask);
                    image.stream.dict.set("SMask", mask);
                }
                let orientation = image.orientation;
                let image = document.add_object(image.stream);
                resources.set("XObject", dictionary! { "I0" => image });
                let width = width * self.width;
                let height = width * upright_height as f32 / upright_width as f32;
                let x = (left * self.width).min(self.width - width).max(0.0);
                let y = ((1.0 - top) * self.height - height)
                    .min(self.height - height)
                    .max(0.0);
                self.include(x, y, 0.0);
                self.include(x + width, y + height, 0.0);
                self.push(&format!(
                    "q\n{} cm\n/I0 Do\nQ\n",
                    matrix(image_matrix(orientation, x, y, width, height))
                ));
            }
        }
        Ok(entries)
    }
}

/// A stroke through `points`, rounded by curving through the midpoints
/// between them, the way the signature pad smooths a drawn line. A single
/// point is a dot.
fn smooth(points: &[(f32, f32)]) -> String {
    let at = |(x, y): (f32, f32)| format!("{} {}", number(x), number(y));
    let mut path = format!("{} m\n", at(points[0]));
    if points.len() <= 2 {
        path.push_str(&format!("{} l\nS\n", at(*points.last().unwrap())));
        return path;
    }
    let mut start = points[0];
    for pair in points[1..].windows(2) {
        let (control, next) = (pair[0], pair[1]);
        let end = ((control.0 + next.0) / 2.0, (control.1 + next.1) / 2.0);
        // The quadratic through `control`, as a cubic.
        let first = (
            start.0 + 2.0 / 3.0 * (control.0 - start.0),
            start.1 + 2.0 / 3.0 * (control.1 - start.1),
        );
        let second = (
            end.0 + 2.0 / 3.0 * (control.0 - end.0),
            end.1 + 2.0 / 3.0 * (control.1 - end.1),
        );
        path.push_str(&format!("{} {} {} c\n", at(first), at(second), at(end)));
        start = end;
    }
    path.push_str(&format!("{} l\nS\n", at(*points.last().unwrap())));
    path
}

pub(crate) fn ellipse(x0: f32, y0: f32, x1: f32, y1: f32) -> String {
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let n = number;
    format!(
        "{} {} m\n\
         {} {} {} {} {} {} c\n\
         {} {} {} {} {} {} c\n\
         {} {} {} {} {} {} c\n\
         {} {} {} {} {} {} c\nh\n",
        n(cx + rx),
        n(cy),
        n(cx + rx),
        n(cy + ky),
        n(cx + kx),
        n(cy + ry),
        n(cx),
        n(cy + ry),
        n(cx - kx),
        n(cy + ry),
        n(cx - rx),
        n(cy + ky),
        n(cx - rx),
        n(cy),
        n(cx - rx),
        n(cy - ky),
        n(cx - kx),
        n(cy - ry),
        n(cx),
        n(cy - ry),
        n(cx + kx),
        n(cy - ry),
        n(cx + rx),
        n(cy - ky),
        n(cx + rx),
        n(cy),
    )
}

/// Lines of WinAnsi codes no wider than `width` points, broken at spaces where
/// possible and inside a word only when the word alone is too wide. Line
/// breaks in the text are kept; the spaces a break replaces are not drawn.
pub(crate) fn wrap(
    text: &str,
    family: FontFamily,
    bold: bool,
    size: f32,
    width: f32,
) -> Vec<Vec<u8>> {
    let measure = |codes: &[u8]| text_width(family, bold, codes) as f32 / 1000.0 * size;
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line: Vec<u8> = Vec::new();
        for word in paragraph.trim_end_matches('\r').split(' ') {
            let codes = word.chars().filter_map(win_ansi).collect::<Vec<_>>();
            let mut candidate = line.clone();
            if !candidate.is_empty() {
                candidate.push(b' ');
            }
            candidate.extend(&codes);
            if measure(&candidate) <= width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            for code in codes {
                line.push(code);
                if measure(&line) > width && line.len() > 1 {
                    line.pop();
                    lines.push(std::mem::replace(&mut line, vec![code]));
                }
            }
        }
        lines.push(line);
    }
    lines
}

fn add_to_page(document: &mut Document, page_id: ObjectId, id: ObjectId) -> Result<(), String> {
    let mut annotations = document
        .get_dictionary(page_id)
        .ok()
        .and_then(|page| page.get(b"Annots").ok())
        .and_then(|value| resolve(document, value))
        .and_then(|value| value.as_array().ok())
        .cloned()
        .unwrap_or_default();
    annotations.push(Object::Reference(id));
    document
        .get_dictionary_mut(page_id)
        .map_err(|error| format!("A PDF page could not be read: {error}"))?
        .set("Annots", annotations);
    Ok(())
}

fn reals(values: &[f32]) -> Vec<Object> {
    values.iter().map(|&value| Object::Real(value)).collect()
}

fn rgb([red, green, blue]: [f32; 3]) -> String {
    format!("{} {} {}", number(red), number(green), number(blue))
}

fn hex(codes: &[u8]) -> String {
    codes.iter().map(|code| format!("{code:02X}")).collect()
}
