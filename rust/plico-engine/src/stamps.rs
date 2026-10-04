//! Page numbers and watermarks: something drawn onto pages that already exist.
//!
//! Pages are changed where they are, with no page tree rebuild and no
//! renumbering. Each stamped page gets a Form XObject laid out in the page's
//! visible area as a reader sees it, which is the /CropBox turned by /Rotate
//! and scaled by /UserUnit, not the raw /MediaBox.

mod standard_fonts;

pub(crate) use standard_fonts::{
    base_font, cap_height, standard_width, text_width, win_ansi, win_ansi_char,
};

use std::collections::{BTreeMap, BTreeSet, HashMap};

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

use crate::compression::write_compressed;
use crate::documents::{
    MAX_DECOMPRESSED_STREAM, MAX_PAGE_TREE_DEPTH, inheritable_attributes, load_document,
    parse_version,
};
use crate::images::{image_matrix, prepare_image};

/// The standard fonts every reader has, so nothing is embedded. They cover
/// WinAnsiEncoding only: Western European letters and common punctuation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFamily {
    Helvetica,
    Times,
    Courier,
}

/// The standard font nearest a font, by its name: Arial and most sans faces
/// become Helvetica, Times New Roman and other serifs Times, monospaced faces
/// Courier. Acrobat's own form resource names count too.
pub(crate) fn standard_family(name: &[u8]) -> (FontFamily, bool) {
    let name = String::from_utf8_lossy(name).to_lowercase();
    // A subset's name starts with six capitals and a plus.
    let name = match name.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 => rest,
        _ => name.as_str(),
    };
    let has = |part: &str| name.contains(part);
    let family = if has("cour") || has("mono") || name == "cobo" {
        FontFamily::Courier
    } else if has("times")
        || name == "tiro"
        || name == "tibo"
        || (has("serif") && !has("sans"))
        || ["roman", "georgia", "garamond", "minion", "cambria", "book"]
            .iter()
            .any(|part| has(part))
    {
        FontFamily::Times
    } else {
        FontFamily::Helvetica
    };
    let bold =
        has("bold") || has("black") || has("heavy") || matches!(name, "hebo" | "tibo" | "cobo");
    (family, bold)
}

#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    pub family: FontFamily,
    pub bold: bool,
    /// Points.
    pub size: f32,
    /// Red, green and blue, each 0 to 1.
    pub color: [f32; 3],
}

/// A spot on the page as the reader sees it, after any /Rotate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Position {
    /// -1, 0 or 1 across, then up.
    fn direction(self) -> (i8, i8) {
        match self {
            Position::TopLeft => (-1, 1),
            Position::Top => (0, 1),
            Position::TopRight => (1, 1),
            Position::Left => (-1, 0),
            Position::Center => (0, 0),
            Position::Right => (1, 0),
            Position::BottomLeft => (-1, -1),
            Position::Bottom => (0, -1),
            Position::BottomRight => (1, -1),
        }
    }
}

pub struct PageNumberOptions<'a> {
    /// `{n}` becomes the page's number and `{total}` the last number printed.
    pub template: &'a str,
    /// Printed on the first page in `pages`.
    pub first_number: u32,
    /// Pages to number, from 1; empty numbers every page. Numbers follow the
    /// document's pages, so a page left out between two numbered ones still
    /// counts.
    pub pages: &'a [u32],
    pub position: Position,
    /// Points between the text and the edges of the visible page.
    pub margin: f32,
    pub style: TextStyle,
    pub opacity: f32,
}

pub enum WatermarkContent<'a> {
    /// Lines split at `\n`.
    Text { text: &'a str, style: TextStyle },
    /// A JPG or PNG; `width` is a fraction of the visible page width.
    Image { bytes: &'a [u8], width: f32 },
}

pub struct WatermarkOptions<'a> {
    pub content: WatermarkContent<'a>,
    /// Pages to mark, from 1; empty marks every page.
    pub pages: &'a [u32],
    pub position: Position,
    /// Points from the edges of the visible page, or between repeats when
    /// tiled.
    pub margin: f32,
    /// Degrees counterclockwise, as the reader sees the page.
    pub rotation: f32,
    pub opacity: f32,
    /// Under the page content rather than over it. A page that paints an
    /// opaque background hides it.
    pub behind: bool,
    /// Repeats the mark across the whole page, ignoring `position`.
    pub tile: bool,
}

/// Keeps a tiled mark from turning into an enormous content stream.
const MAX_TILES: usize = 1_000;

/// Line spacing, as a multiple of the font size.
pub(crate) const LEADING: f32 = 1.2;

pub fn add_page_numbers_bytes(
    input: &[u8],
    password: &str,
    options: PageNumberOptions<'_>,
) -> Result<Vec<u8>, String> {
    check_style(&options.style)?;
    check_placement(options.margin, options.opacity)?;
    if options.template.trim().is_empty() {
        return Err("Enter a page number format.".into());
    }

    let mut document = load_document(input, 1, password)?;
    let pages = selected_pages(&document, options.pages)?;
    let (Some(&(first_page, _)), Some(&(last_page, _))) = (pages.first(), pages.last()) else {
        return Err("Choose at least one page.".into());
    };
    let last_number = options
        .first_number
        .checked_add(last_page - first_page)
        .ok_or("The first page number is too large.")?;
    let template = options
        .template
        .replace("{total}", &last_number.to_string());

    let mut stamper = Stamper::new(&mut document, options.opacity);
    let font = stamper.font(&mut document, options.style);
    for (page, page_id) in pages {
        let number = options.first_number + (page - first_page);
        let mark = Mark::text(&template.replace("{n}", &number.to_string()), options.style)?;
        let frame = page_frame(&document, page_id)?;
        let content = stamper.draw(
            &mark,
            &frame,
            Placement {
                position: options.position,
                margin: options.margin,
                rotation: 0.0,
                tile: false,
            },
        )?;
        let form = stamper.form(&mut document, &frame, content, Some(font), None);
        stamper.place(&mut document, page_id, form, false)?;
    }
    finish(document, options.opacity)
}

pub fn add_watermark_bytes(
    input: &[u8],
    password: &str,
    options: WatermarkOptions<'_>,
) -> Result<Vec<u8>, String> {
    check_placement(options.margin, options.opacity)?;
    if !options.rotation.is_finite() {
        return Err("Choose a valid rotation.".into());
    }
    let (mark, image) = match options.content {
        WatermarkContent::Text { text, style } => {
            check_style(&style)?;
            if text.trim().is_empty() {
                return Err("Enter the watermark text.".into());
            }
            (Mark::text(text, style)?, None)
        }
        WatermarkContent::Image { bytes, width } => {
            if !(width > 0.0 && width <= 1.0) {
                return Err("Choose an image width between 1% and 100% of the page.".into());
            }
            let image = prepare_image(bytes)
                .map_err(|error| format!("The watermark image could not be used: {error}"))?;
            let (logical_width, logical_height) = if matches!(image.orientation, 5..=8) {
                (image.height, image.width)
            } else {
                (image.width, image.height)
            };
            let mark = Mark::Image {
                orientation: image.orientation,
                aspect: logical_height as f32 / logical_width as f32,
                width,
            };
            (mark, Some(image))
        }
    };

    let mut document = load_document(input, 1, password)?;
    let pages = selected_pages(&document, options.pages)?;
    let mut stamper = Stamper::new(&mut document, options.opacity);
    let font = match &mark {
        Mark::Text { style, .. } => Some(stamper.font(&mut document, *style)),
        Mark::Image { .. } => None,
    };
    let image = image.map(|mut image| {
        if let Some(mask) = image.mask.take() {
            let mask_id = document.add_object(mask);
            image.stream.dict.set("SMask", mask_id);
        }
        document.add_object(image.stream)
    });
    let placement = Placement {
        position: options.position,
        margin: options.margin,
        rotation: options.rotation,
        tile: options.tile,
    };

    // The mark is the same on every page, so pages that share a visible area
    // share one form.
    let mut forms = HashMap::new();
    for (_, page_id) in pages {
        let frame = page_frame(&document, page_id)?;
        let form = match forms.get(&frame.key()) {
            Some(&form) => form,
            None => {
                let content = stamper.draw(&mark, &frame, placement)?;
                let form = stamper.form(&mut document, &frame, content, font, image);
                forms.insert(frame.key(), form);
                form
            }
        };
        stamper.place(&mut document, page_id, form, options.behind)?;
    }
    finish(document, options.opacity)
}

pub struct SignaturePlacement {
    /// From 1. A page may carry the signature more than once.
    pub page: u32,
    /// Left edge, top edge and width as fractions of the visible page's width
    /// and height, measured from its top left as the reader sees it. The
    /// height follows from the image, so it is never stretched.
    pub place: [f32; 3],
}

/// Draws a JPG or PNG signature over each placement. It is a picture of a
/// signature, not a cryptographic one.
pub fn add_signature_bytes(
    input: &[u8],
    password: &str,
    image: &[u8],
    placements: &[SignaturePlacement],
) -> Result<Vec<u8>, String> {
    if placements.is_empty() {
        return Err("Choose at least one page to sign.".into());
    }
    let mut by_page = BTreeMap::<u32, Vec<[f32; 3]>>::new();
    for placement in placements {
        let [left, top, width] = placement.place;
        if !(left.is_finite() && top.is_finite() && width > 0.0 && width <= 1.0)
            || !(0.0..1.0).contains(&left)
            || !(0.0..1.0).contains(&top)
        {
            return Err("Place the signature on the page.".into());
        }
        by_page
            .entry(placement.page)
            .or_default()
            .push(placement.place);
    }
    let image = prepare_image(image)
        .map_err(|error| format!("The signature image could not be used: {error}"))?;
    let (upright_width, upright_height) = if matches!(image.orientation, 5..=8) {
        (image.height, image.width)
    } else {
        (image.width, image.height)
    };
    let aspect = upright_height as f32 / upright_width as f32;
    let orientation = image.orientation;

    let mut document = load_document(input, 1, password)?;
    let numbers = by_page.keys().copied().collect::<Vec<_>>();
    let pages = selected_pages(&document, &numbers)?;
    let mut stamper = Stamper::new(&mut document, 1.0);
    let mut image = image;
    if let Some(mask) = image.mask.take() {
        let mask_id = document.add_object(mask);
        image.stream.dict.set("SMask", mask_id);
    }
    let image = document.add_object(image.stream);

    for (page, page_id) in pages {
        let frame = page_frame(&document, page_id)?;
        let mut content = format!("q\n{} cm\n", matrix(frame.matrix));
        for &[left, top, width] in &by_page[&page] {
            let width = width * frame.width;
            let height = width * aspect;
            // Kept on the page where it fits: a place chosen on one page can
            // run off a shorter or narrower one.
            let x = (left * frame.width).min(frame.width - width).max(0.0);
            let y = (frame.height - top * frame.height - height)
                .min(frame.height - height)
                .max(0.0);
            content.push_str(&format!(
                "q\n{} cm\n/I0 Do\nQ\n",
                matrix(image_matrix(orientation, x, y, width, height))
            ));
        }
        content.push_str("Q\n");
        let form = stamper.form(&mut document, &frame, content, None, Some(image));
        stamper.place(&mut document, page_id, form, false)?;
    }
    finish(document, 1.0)
}

pub(crate) fn check_style(style: &TextStyle) -> Result<(), String> {
    if !(1.0..=1_000.0).contains(&style.size) {
        return Err("Choose a text size between 1 and 1,000 points.".into());
    }
    if !style
        .color
        .iter()
        .all(|channel| (0.0..=1.0).contains(channel))
    {
        return Err("Choose a valid text color.".into());
    }
    Ok(())
}

fn check_placement(margin: f32, opacity: f32) -> Result<(), String> {
    if !(0.0..=1_000.0).contains(&margin) {
        return Err("Choose a margin between 0 and 1,000 points.".into());
    }
    if !(0.0..=1.0).contains(&opacity) {
        return Err("Choose an opacity between 0 and 100%.".into());
    }
    Ok(())
}

pub(crate) fn selected_pages(
    document: &Document,
    pages: &[u32],
) -> Result<Vec<(u32, ObjectId)>, String> {
    if has_unreadable_page(document) {
        return Err("Some pages of this PDF are damaged and could not be read.".into());
    }
    let available = document.get_pages();
    if pages.is_empty() {
        return Ok(available.into_iter().collect());
    }
    pages
        .iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|number| {
            available
                .get(number)
                .map(|&id| (*number, id))
                .ok_or_else(|| format!("Pages must be between 1 and {}.", available.len()))
        })
        .collect()
}

/// Whether the page tree lists a page lopdf could not read. `get_pages()`
/// skips it, so numbering would count one page fewer than a reader shows, and
/// the tree is written back still listing it. Merge rebuilds the tree and
/// drops such a page instead (pdf.js corpus: issue7229.pdf, whose first page
/// only a repairing reader can find).
fn has_unreadable_page(document: &Document) -> bool {
    let Ok(root) = document
        .catalog()
        .and_then(|catalog| catalog.get(b"Pages"))
        .and_then(Object::as_reference)
    else {
        return true;
    };
    let mut seen = BTreeSet::new();
    let mut nodes = vec![(root, 0)];
    while let Some((id, depth)) = nodes.pop() {
        if depth > MAX_PAGE_TREE_DEPTH || !seen.insert(id) {
            continue;
        }
        let Ok(kids) = document
            .get_dictionary(id)
            .and_then(|node| node.get(b"Kids"))
            .and_then(|kids| match kids {
                Object::Reference(id) => document.get_object(*id).and_then(Object::as_array),
                kids => kids.as_array(),
            })
        else {
            continue;
        };
        for kid in kids {
            let Ok(kid) = kid.as_reference() else {
                return true;
            };
            match document.get_dictionary(kid) {
                Ok(node) if node.has(b"Kids") => nodes.push((kid, depth + 1)),
                Ok(_) => {}
                Err(_) => return true,
            }
        }
    }
    false
}

pub(crate) fn finish(mut document: Document, opacity: f32) -> Result<Vec<u8>, String> {
    // Constant opacity in a graphics state dictionary arrived in PDF 1.4.
    if opacity < 1.0 && parse_version(&document.version) < (1, 4) {
        document.version = "1.4".into();
    }
    // Loading unpacks object streams; their containers are unreachable now.
    document.prune_objects();
    document.compress();
    write_compressed(document)
}

/// The page's visible area in reading orientation, in points, with the matrix
/// that maps it onto the page's own coordinates.
#[derive(Clone, Copy)]
pub(crate) struct Frame {
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) matrix: [f32; 6],
    pub(crate) media_box: [f32; 4],
}

impl Frame {
    fn key(&self) -> [u32; 12] {
        let mut key = [0; 12];
        let values = [self.width, self.height]
            .into_iter()
            .chain(self.matrix)
            .chain(self.media_box);
        for (slot, value) in key.iter_mut().zip(values) {
            *slot = value.to_bits();
        }
        key
    }
}

/// Letter, which is what readers assume for a page with no usable /MediaBox.
const DEFAULT_MEDIA_BOX: [f32; 4] = [0.0, 0.0, 612.0, 792.0];

pub(crate) fn page_frame(document: &Document, page_id: ObjectId) -> Result<Frame, String> {
    let page = document
        .get_dictionary(page_id)
        .map_err(|error| format!("A PDF page could not be read: {error}"))?;
    let inherited = inheritable_attributes(document, page);
    let attribute = |key: &[u8]| {
        let value = page.get(key).ok().or_else(|| {
            inherited
                .iter()
                .find(|(found, _)| *found == key)
                .map(|(_, value)| value)
        })?;
        resolve(document, value)
    };

    let media_box = attribute(b"MediaBox")
        .and_then(|value| rectangle(document, value))
        .unwrap_or(DEFAULT_MEDIA_BOX);
    // Readers clip the crop box to the media box.
    let [x0, y0, x1, y1] = attribute(b"CropBox")
        .and_then(|value| rectangle(document, value))
        .map(|crop| {
            [
                crop[0].max(media_box[0]),
                crop[1].max(media_box[1]),
                crop[2].min(media_box[2]),
                crop[3].min(media_box[3]),
            ]
        })
        .filter(|[x0, y0, x1, y1]| x1 > x0 && y1 > y0)
        .unwrap_or(media_box);
    let rotation = attribute(b"Rotate")
        .and_then(|value| value.as_float().ok())
        .filter(|rotation| rotation.is_finite())
        .map(|rotation| ((rotation / 90.0).round() as i64 * 90).rem_euclid(360))
        .unwrap_or(0);
    // Not inheritable. Each user space unit is this many points.
    let unit = page
        .get(b"UserUnit")
        .ok()
        .and_then(|value| resolve(document, value))
        .and_then(|value| value.as_float().ok())
        .filter(|unit| unit.is_finite() && *unit > 0.0)
        .unwrap_or(1.0);

    let (width, height) = (x1 - x0, y1 - y0);
    // Viewers turn the page clockwise by /Rotate, so reading orientation is
    // the page's own coordinates turned back the other way.
    let (width, height, [a, b, c, d, e, f]) = match rotation {
        90 => (height, width, [0.0, 1.0, -1.0, 0.0, x1, y0]),
        180 => (width, height, [-1.0, 0.0, 0.0, -1.0, x1, y1]),
        270 => (height, width, [0.0, -1.0, 1.0, 0.0, x0, y1]),
        _ => (width, height, [1.0, 0.0, 0.0, 1.0, x0, y0]),
    };
    Ok(Frame {
        width: width * unit,
        height: height * unit,
        matrix: [a / unit, b / unit, c / unit, d / unit, e, f],
        media_box,
    })
}

pub(crate) fn resolve<'a>(document: &'a Document, value: &'a Object) -> Option<&'a Object> {
    match value {
        Object::Reference(id) => document.get_object(*id).ok(),
        value => Some(value),
    }
}

/// A rectangle's corners in either order, normalised to lower left and upper
/// right as the spec says readers should.
pub(crate) fn rectangle(document: &Document, value: &Object) -> Option<[f32; 4]> {
    let numbers = value
        .as_array()
        .ok()?
        .iter()
        .map(|item| resolve(document, item)?.as_float().ok())
        .collect::<Option<Vec<_>>>()?;
    let [left, bottom, right, top] = numbers[..] else {
        return None;
    };
    let rectangle = [
        left.min(right),
        bottom.min(top),
        left.max(right),
        bottom.max(top),
    ];
    (rectangle.iter().all(|value| value.is_finite())
        && rectangle[2] > rectangle[0]
        && rectangle[3] > rectangle[1])
        .then_some(rectangle)
}

enum Mark {
    Text {
        /// Encoded lines with their widths in points.
        lines: Vec<(Vec<u8>, f32)>,
        style: TextStyle,
    },
    Image {
        orientation: u8,
        /// Height over width, upright.
        aspect: f32,
        width: f32,
    },
}

impl Mark {
    fn text(text: &str, style: TextStyle) -> Result<Mark, String> {
        let lines = text
            .split('\n')
            .map(|line| {
                let encoded = line
                    .trim_end_matches('\r')
                    .chars()
                    .map(|character| {
                        standard_fonts::win_ansi(character).ok_or_else(|| {
                            format!("“{character}” cannot be drawn with the built-in PDF fonts.")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let width = standard_fonts::text_width(style.family, style.bold, &encoded) as f32
                    / 1000.0
                    * style.size;
                Ok((encoded, width))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Mark::Text { lines, style })
    }

    /// Width and height in points on a page of this width. Text is measured
    /// from the top of its capitals to the baseline of its last line, which is
    /// what the eye reads as its extent.
    fn size(&self, page_width: f32) -> (f32, f32) {
        match self {
            Mark::Text { lines, style } => (
                lines.iter().map(|(_, width)| *width).fold(0.0, f32::max),
                style.size
                    * (standard_fonts::cap_height(style.family, style.bold)
                        + LEADING * (lines.len() - 1) as f32),
            ),
            Mark::Image { aspect, width, .. } => (page_width * width, page_width * width * aspect),
        }
    }
}

#[derive(Clone, Copy)]
struct Placement {
    position: Position,
    margin: f32,
    rotation: f32,
    tile: bool,
}

/// Objects shared by every stamped page, plus what has been written so far.
pub(crate) struct Stamper {
    font: Option<ObjectId>,
    graphics_state: Option<ObjectId>,
    streams: HashMap<Vec<u8>, ObjectId>,
    promoted: BTreeSet<ObjectId>,
}

impl Stamper {
    pub(crate) fn new(document: &mut Document, opacity: f32) -> Stamper {
        let graphics_state = (opacity < 1.0).then(|| {
            document.add_object(dictionary! {
                "Type" => "ExtGState",
                "ca" => Object::Real(opacity),
                "CA" => Object::Real(opacity),
            })
        });
        Stamper {
            font: None,
            graphics_state,
            streams: HashMap::new(),
            promoted: BTreeSet::new(),
        }
    }

    fn font(&mut self, document: &mut Document, style: TextStyle) -> ObjectId {
        *self.font.get_or_insert_with(|| {
            document.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => standard_fonts::base_font(style.family, style.bold),
                "Encoding" => "WinAnsiEncoding",
            })
        })
    }

    /// The form's content stream, in the frame's reading orientation.
    fn draw(&self, mark: &Mark, frame: &Frame, placement: Placement) -> Result<String, String> {
        let (width, height) = mark.size(frame.width);
        let (sin, cos) = placement.rotation.to_radians().sin_cos();
        // Half the extent of the turned mark, which is what has to stay
        // `margin` away from the edges.
        let half_width = (width / 2.0 * cos).abs() + (height / 2.0 * sin).abs();
        let half_height = (width / 2.0 * sin).abs() + (height / 2.0 * cos).abs();
        let (across, up) = placement.position.direction();
        let centers = if placement.tile {
            tile_centers(frame, half_width, half_height, placement.margin)?
        } else {
            let offset = |direction: i8, length: f32, half: f32| match direction {
                -1 => placement.margin + half,
                1 => length - placement.margin - half,
                _ => length / 2.0,
            };
            vec![(
                offset(across, frame.width, half_width),
                offset(up, frame.height, half_height),
            )]
        };
        let turn =
            |x: f32, y: f32, (cx, cy): (f32, f32)| (cx + x * cos - y * sin, cy + x * sin + y * cos);

        let mut content = String::from("q\n");
        if self.graphics_state.is_some() {
            content.push_str("/G0 gs\n");
        }
        content.push_str(&format!("{} cm\n", matrix(frame.matrix)));
        match mark {
            Mark::Text { lines, style } => {
                let [red, green, blue] = style.color;
                content.push_str(&format!(
                    "{} {} {} rg\nBT\n/F0 {} Tf\n",
                    number(red),
                    number(green),
                    number(blue),
                    number(style.size)
                ));
                let align = if placement.tile { 0 } else { across };
                let cap = standard_fonts::cap_height(style.family, style.bold) * style.size;
                for &center in &centers {
                    for (index, (encoded, line_width)) in lines.iter().enumerate() {
                        let x = match align {
                            -1 => -width / 2.0,
                            1 => width / 2.0 - line_width,
                            _ => -line_width / 2.0,
                        };
                        let y = height / 2.0 - cap - index as f32 * LEADING * style.size;
                        let (x, y) = turn(x, y, center);
                        content.push_str(&format!(
                            "{} Tm\n<{}> Tj\n",
                            matrix([cos, sin, -sin, cos, x, y]),
                            encoded
                                .iter()
                                .map(|byte| format!("{byte:02X}"))
                                .collect::<String>()
                        ));
                    }
                }
                content.push_str("ET\n");
            }
            Mark::Image { orientation, .. } => {
                for &center in &centers {
                    let (x, y) = turn(-width / 2.0, -height / 2.0, center);
                    content.push_str(&format!(
                        "q\n{} cm\n{} cm\n/I0 Do\nQ\n",
                        matrix([cos, sin, -sin, cos, x, y]),
                        matrix(image_matrix(*orientation, 0.0, 0.0, width, height))
                    ));
                }
            }
        }
        content.push_str("Q\n");
        Ok(content)
    }

    pub(crate) fn form(
        &self,
        document: &mut Document,
        frame: &Frame,
        content: String,
        font: Option<ObjectId>,
        image: Option<ObjectId>,
    ) -> ObjectId {
        let mut resources = Dictionary::new();
        if let Some(font) = font {
            resources.set("Font", dictionary! { "F0" => font });
        }
        if let Some(image) = image {
            resources.set("XObject", dictionary! { "I0" => image });
        }
        if let Some(graphics_state) = self.graphics_state {
            resources.set("ExtGState", dictionary! { "G0" => graphics_state });
        }
        document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => frame.media_box.iter().map(|&value| Object::Real(value)).collect::<Vec<_>>(),
                "Resources" => resources,
            },
            content.into_bytes(),
        ))
    }

    /// Draws `form` on the page, over or under what is already there. The
    /// page's own streams are left untouched; only its /Contents array and
    /// /Resources change.
    pub(crate) fn place(
        &mut self,
        document: &mut Document,
        page_id: ObjectId,
        form: ObjectId,
        behind: bool,
    ) -> Result<(), String> {
        let name = self.add_xobject(document, page_id, form)?;
        let draw = format!("/{name} Do\n");
        let existing = content_streams(document, page_id);
        let contents = if behind || existing.is_empty() {
            let mut contents = vec![Object::Reference(self.stream(document, draw))];
            contents.extend(existing);
            contents
        } else {
            // Wrapping the page in q/Q restores the state it started in, so
            // the form is not drawn through whatever transform or clip the
            // page left behind. A page that saves more states than it restores
            // needs those closed too, or its last transform still applies.
            let closing = "Q\n".repeat(1 + open_states(document, page_id));
            let opening = self.stream(document, "q\n".into());
            let ending = self.stream(document, format!("\n{closing}{draw}"));
            let mut contents = vec![Object::Reference(opening)];
            contents.extend(existing);
            contents.push(Object::Reference(ending));
            contents
        };
        document
            .get_dictionary_mut(page_id)
            .map_err(|error| format!("A PDF page could not be read: {error}"))?
            .set("Contents", contents);
        Ok(())
    }

    /// Identical streams are written once, since most pages end the same way.
    fn stream(&mut self, document: &mut Document, content: String) -> ObjectId {
        let content = content.into_bytes();
        *self
            .streams
            .entry(content.clone())
            .or_insert_with(|| document.add_object(Stream::new(dictionary! {}, content)))
    }

    /// Adds `form` to the page's /XObject resources under a free name.
    fn add_xobject(
        &mut self,
        document: &mut Document,
        page_id: ObjectId,
        form: ObjectId,
    ) -> Result<String, String> {
        let mut resources = self.own_resources(document, page_id);
        let mut xobjects = match resources.get(b"XObject") {
            Ok(Object::Reference(id)) => document.get_dictionary(*id).cloned().unwrap_or_default(),
            Ok(Object::Dictionary(xobjects)) => xobjects.clone(),
            _ => Dictionary::new(),
        };
        let name = (0..)
            .map(|index| match index {
                0 => "Plico".to_owned(),
                index => format!("Plico{index}"),
            })
            .find(|name| !xobjects.has(name.as_bytes()))
            .unwrap();
        xobjects.set(name.as_bytes(), form);
        resources.set("XObject", xobjects);
        document
            .get_dictionary_mut(page_id)
            .map_err(|error| format!("A PDF page could not be read: {error}"))?
            .set("Resources", resources);
        Ok(name)
    }

    /// A copy of the page's effective resources that it can own, since adding
    /// the form to resources other pages share would put it in their
    /// resources too.
    ///
    /// The copy is shallow. Shared resources first have their direct
    /// subdictionaries moved into objects of their own, so every copy refers to
    /// one font dictionary rather than duplicating it on each page.
    pub(crate) fn own_resources(
        &mut self,
        document: &mut Document,
        page_id: ObjectId,
    ) -> Dictionary {
        let mut node = Some(page_id);
        let mut found = None;
        for _ in 0..MAX_PAGE_TREE_DEPTH {
            let Some(id) = node else { break };
            let Ok(dictionary) = document.get_dictionary(id) else {
                break;
            };
            if let Ok(resources) = dictionary.get(b"Resources") {
                found = Some((id, resources.clone()));
                break;
            }
            node = dictionary
                .get(b"Parent")
                .and_then(Object::as_reference)
                .ok();
        }

        let shared = match found {
            Some((holder, Object::Dictionary(resources))) if holder == page_id => return resources,
            Some((_, Object::Reference(shared))) => shared,
            // Inherited from a page tree node. Moved into an object of its own
            // so it is promoted once, not once per page.
            Some((holder, Object::Dictionary(resources))) => {
                let shared = document.add_object(resources);
                if let Ok(node) = document.get_dictionary_mut(holder) {
                    node.set("Resources", shared);
                }
                shared
            }
            _ => return Dictionary::new(),
        };
        let Ok(resources) = document.get_dictionary(shared) else {
            return Dictionary::new();
        };
        if self.promoted.insert(shared) {
            let direct = resources
                .iter()
                .filter_map(|(key, value)| Some((key.clone(), value.as_dict().ok()?.clone())))
                .collect::<Vec<_>>();
            for (key, value) in direct {
                let id = document.add_object(value);
                if let Ok(resources) = document.get_dictionary_mut(shared) {
                    resources.set(key, id);
                }
            }
        }
        document.get_dictionary(shared).cloned().unwrap_or_default()
    }
}

/// Centres that cover the page, in rows offset by half a step like brickwork,
/// laid out from the middle so the pattern is symmetric.
fn tile_centers(
    frame: &Frame,
    half_width: f32,
    half_height: f32,
    gap: f32,
) -> Result<Vec<(f32, f32)>, String> {
    let too_many = || "The watermark is too small to repeat across the page.".to_string();
    let step_x = 2.0 * half_width + gap;
    let step_y = 2.0 * half_height + gap;
    if step_x < 1.0 || step_y < 1.0 {
        return Err(too_many());
    }
    let columns = ((frame.width / 2.0 + half_width) / step_x).ceil() + 1.0;
    let rows = ((frame.height / 2.0 + half_height) / step_y).ceil();
    if (2.0 * columns + 1.0) * (2.0 * rows + 1.0) > 4.0 * MAX_TILES as f32 {
        return Err(too_many());
    }
    let (columns, rows) = (columns as i32, rows as i32);
    let (middle_x, middle_y) = (frame.width / 2.0, frame.height / 2.0);
    let mut centers = Vec::new();
    for row in -rows..=rows {
        let shift = if row % 2 == 0 { 0.0 } else { 0.5 };
        for column in -columns..=columns {
            let x = middle_x + (column as f32 + shift) * step_x;
            let y = middle_y + row as f32 * step_y;
            if (x - middle_x).abs() < middle_x + half_width
                && (y - middle_y).abs() < middle_y + half_height
            {
                centers.push((x, y));
            }
        }
    }
    if centers.len() > MAX_TILES {
        return Err(too_many());
    }
    Ok(centers)
}

/// The page's /Contents as a list of stream references.
fn content_streams(document: &mut Document, page_id: ObjectId) -> Vec<Object> {
    let Ok(page) = document.get_dictionary(page_id) else {
        return Vec::new();
    };
    match page.get(b"Contents") {
        Ok(Object::Reference(id)) => match document.get_object(*id) {
            Ok(Object::Array(items)) => items.clone(),
            _ => vec![Object::Reference(*id)],
        },
        Ok(Object::Array(items)) => items.clone(),
        // Streams are always indirect, so a direct one is malformed. Moving it
        // into an object keeps what it draws.
        Ok(Object::Stream(stream)) => {
            let stream = stream.clone();
            vec![Object::Reference(document.add_object(stream))]
        }
        _ => Vec::new(),
    }
}

/// How many graphics states the page saves and never restores. A restore with
/// nothing saved is ignored by readers, so it is ignored here too.
fn open_states(document: &Document, page_id: ObjectId) -> usize {
    let Ok(content) = document.get_page_content_with_limit(page_id, MAX_DECOMPRESSED_STREAM) else {
        return 0;
    };
    let Ok(content) = Content::decode(&content) else {
        return 0;
    };
    content
        .operations
        .iter()
        .fold(0, |depth: usize, operation| {
            match operation.operator.as_str() {
                "q" => depth + 1,
                "Q" => depth.saturating_sub(1),
                _ => depth,
            }
        })
}

pub(crate) fn matrix(values: [f32; 6]) -> String {
    values.map(number).join(" ")
}

pub(crate) fn number(value: f32) -> String {
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    match text {
        "-0" | "" => "0".to_owned(),
        text => text.to_owned(),
    }
}
