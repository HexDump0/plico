use js_sys::{Array, Uint8Array};
use wasm_bindgen::prelude::*;

use crate::text::Typesetter;
use crate::{
    Annotation, AnnotationKind, Area, Clip, CompressOptions, Condition, Dash, EditOptions, Erasure,
    FieldFill, FieldValue, FlattenScope, FontFamily, Heading, ImageMove, ImagePdfOptions,
    LinkTarget, Markup, OcrPage, OcrWord, OrganizeItem, PageCrop, PageImage, PageNumberOptions,
    PageOrientation, PdfALevel, Position, PrintDocument, PrintItem, PrintMark, PrintPage,
    ProtectOptions, Protection, RedactOptions, Redacted, Redaction, ScanLook, ScanPaper, Shape,
    SignaturePlacement, SplitMode, StandardFont, SuppliedFont, TextAlign, TextRemoval, TextStyle,
    Unremovable, WatermarkContent, WatermarkOptions, add_page_numbers_bytes, add_signature_bytes,
    add_text_layer_bytes, add_watermark_bytes, annotate_pdf_bytes,
    compress_pdf_bytes_with_password, condition_of, convert_to_pdfa_bytes, crop_pdf_bytes,
    edit_pdf_bytes, fill_form_bytes, find_page, flatten_pdf_bytes, images_to_pdf_bytes,
    merge_pdf_bytes_with_options, organize_pdf_items, page_texts, print_pdf_bytes,
    protect_pdf_bytes, protection_of, redact_pdf_bytes, scan_image_bytes, scans_to_pdf_bytes,
    split_pdf_bytes_with_password, standard_fonts_for_pdfa, unlock_pdf_bytes,
};

/// Marks an organize instruction as a blank page; its page number then indexes
/// the width and height pairs in `blanks`.
const BLANK_PAGE: u32 = u32::MAX;

fn as_strs(passwords: &[String]) -> Vec<&str> {
    passwords.iter().map(String::as_str).collect()
}

#[wasm_bindgen]
/// An empty `bookmarks` merges without an outline.
pub fn merge_pdfs(
    input: &[u8],
    lengths: &[u32],
    passwords: Vec<String>,
    bookmarks: Vec<String>,
) -> Result<Vec<u8>, JsValue> {
    let expected_length = lengths
        .iter()
        .try_fold(0usize, |total, length| total.checked_add(*length as usize));
    if expected_length != Some(input.len()) {
        return Err(JsValue::from_str("The PDF input was incomplete."));
    }

    let mut offset = 0;
    let files = lengths
        .iter()
        .map(|length| {
            let end = offset + *length as usize;
            let bytes = &input[offset..end];
            offset = end;
            bytes
        })
        .collect::<Vec<_>>();

    let titles = as_strs(&bookmarks);
    merge_pdf_bytes_with_options(
        &files,
        &as_strs(&passwords),
        (!titles.is_empty()).then_some(&titles[..]),
    )
    .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn images_to_pdf(
    input: &[u8],
    lengths: &[u32],
    page_width: f32,
    page_height: f32,
    margin: f32,
    orientation: u8,
) -> Result<Vec<u8>, JsValue> {
    let expected_length = lengths
        .iter()
        .try_fold(0usize, |total, length| total.checked_add(*length as usize));
    if expected_length != Some(input.len()) {
        return Err(JsValue::from_str("The image input was incomplete."));
    }
    let mut offset = 0;
    let images = lengths
        .iter()
        .map(|length| {
            let end = offset + *length as usize;
            let bytes = &input[offset..end];
            offset = end;
            bytes
        })
        .collect::<Vec<_>>();
    images_to_pdf_bytes(
        &images,
        ImagePdfOptions {
            page_width,
            page_height,
            margin,
            orientation: match orientation {
                1 => PageOrientation::Portrait,
                2 => PageOrientation::Landscape,
                _ => PageOrientation::Auto,
            },
        },
    )
    .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
/// The sheet of paper in an RGBA photo: its top left, top right, bottom right
/// and bottom left corners as x and y fractions, or nothing when none is found.
pub fn find_scan_page(rgba: &[u8], width: u32, height: u32) -> Vec<f32> {
    find_page(rgba, width, height).map_or_else(Vec::new, |corners| corners.to_vec())
}

#[wasm_bindgen]
/// Straightens the area inside `corners` (as `find_scan_page` gives them),
/// turned `turns` quarter turns clockwise. `look` is 0 as taken, 1 document,
/// 2 grayscale, 3 black and white. Returns a JPEG, or a one-bit PNG.
pub fn scan_image(
    rgba: &[u8],
    width: u32,
    height: u32,
    corners: &[f32],
    turns: u8,
    look: u8,
    max_side: u32,
) -> Result<Vec<u8>, JsValue> {
    let corners = <[f32; 8]>::try_from(corners)
        .map_err(|_| JsValue::from_str("Choose the corners of the page."))?;
    let look = match look {
        0 => ScanLook::Original,
        1 => ScanLook::Document,
        2 => ScanLook::Grayscale,
        _ => ScanLook::BlackWhite,
    };
    scan_image_bytes(rgba, width, height, corners, turns, look, max_side)
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
/// One page per scanned image. A `paper_height` of 0 shapes each page like
/// its scan, `paper_width` points across its short side.
pub fn scans_to_pdf(
    input: &[u8],
    lengths: &[u32],
    paper_width: f32,
    paper_height: f32,
) -> Result<Vec<u8>, JsValue> {
    let expected_length = lengths
        .iter()
        .try_fold(0usize, |total, length| total.checked_add(*length as usize));
    if expected_length != Some(input.len()) {
        return Err(JsValue::from_str("The scanned pages were incomplete."));
    }
    let mut offset = 0;
    let images = lengths
        .iter()
        .map(|length| {
            let end = offset + *length as usize;
            let bytes = &input[offset..end];
            offset = end;
            bytes
        })
        .collect::<Vec<_>>();
    let paper = if paper_height == 0.0 {
        ScanPaper::Fit(paper_width)
    } else {
        ScanPaper::Sheet(paper_width, paper_height)
    };
    scans_to_pdf_bytes(&images, paper).map_err(|error| JsValue::from_str(&error))
}

fn split_outputs_to_js(outputs: Vec<Vec<u8>>) -> Array {
    let result = Array::new();
    for bytes in outputs {
        result.push(&Uint8Array::from(bytes.as_slice()));
    }
    result
}

#[wasm_bindgen]
pub fn split_pdf_ranges(
    input: &[u8],
    password: &str,
    bounds: &[u32],
    combine: bool,
) -> Result<Array, JsValue> {
    let (pairs, remainder) = bounds.as_chunks::<2>();
    if !remainder.is_empty() {
        return Err(JsValue::from_str("A page range is incomplete."));
    }
    let ranges = pairs
        .iter()
        .map(|&[from, to]| (from, to))
        .collect::<Vec<_>>();
    split_pdf_bytes_with_password(input, password, SplitMode::Ranges(&ranges, combine))
        .map(split_outputs_to_js)
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn split_pdf_every(input: &[u8], password: &str, interval: u32) -> Result<Array, JsValue> {
    split_pdf_bytes_with_password(input, password, SplitMode::Every(interval))
        .map(split_outputs_to_js)
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn organize_pdfs(
    input: &[u8],
    lengths: &[u32],
    passwords: Vec<String>,
    instructions: &[u32],
    blanks: &[f32],
) -> Result<Vec<u8>, JsValue> {
    let expected_length = lengths
        .iter()
        .try_fold(0usize, |total, length| total.checked_add(*length as usize));
    if expected_length != Some(input.len()) {
        return Err(JsValue::from_str("The PDF input was incomplete."));
    }

    let (chunks, remainder) = instructions.as_chunks::<3>();
    if !remainder.is_empty() {
        return Err(JsValue::from_str("A page instruction is incomplete."));
    }

    let mut offset = 0;
    let files = lengths
        .iter()
        .map(|length| {
            let end = offset + *length as usize;
            let bytes = &input[offset..end];
            offset = end;
            bytes
        })
        .collect::<Vec<_>>();
    let items = chunks
        .iter()
        .map(|chunk| {
            let turn = chunk[2] as i32;
            if chunk[0] != BLANK_PAGE {
                return Ok(OrganizeItem::Page {
                    source: chunk[0] as usize,
                    number: chunk[1],
                    turn,
                });
            }
            let size = blanks
                .get(chunk[1] as usize * 2..chunk[1] as usize * 2 + 2)
                .ok_or("A blank page has no size.")?;
            Ok(OrganizeItem::Blank {
                width: size[0],
                height: size[1],
                turn,
            })
        })
        .collect::<Result<Vec<_>, &str>>()
        .map_err(JsValue::from_str)?;

    organize_pdf_items(&files, &as_strs(&passwords), &items)
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn unlock_pdf(input: &[u8], password: &str) -> Result<Vec<u8>, JsValue> {
    unlock_pdf_bytes(input, password).map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
/// The standard font substitutes `convert_to_pdfa` will need for this file.
pub fn pdfa_standard_fonts(input: &[u8], password: &str) -> Result<Vec<String>, JsValue> {
    standard_fonts_for_pdfa(input, password)
        .map(|names| names.into_iter().map(str::to_owned).collect())
        .map_err(|error: String| JsValue::from_str(&error))
}

#[wasm_bindgen]
/// `part` is 2 for PDF/A-2b or 3 for PDF/A-3b. `font_names`,
/// `font_programs` and `font_metrics` describe the substitutes, one each per
/// font; the programs are concatenated with their lengths in
/// `program_lengths`.
pub fn convert_to_pdfa(
    input: &[u8],
    password: &str,
    part: u8,
    font_names: Vec<String>,
    font_programs: &[u8],
    program_lengths: &[u32],
    font_metrics: Vec<String>,
) -> Result<Vec<u8>, JsValue> {
    let level = match part {
        2 => PdfALevel::A2b,
        3 => PdfALevel::A3b,
        _ => return Err(JsValue::from_str("Choose PDF/A-2 or PDF/A-3.")),
    };
    let total = program_lengths
        .iter()
        .try_fold(0usize, |total, length| total.checked_add(*length as usize));
    if total != Some(font_programs.len())
        || font_names.len() != program_lengths.len()
        || font_names.len() != font_metrics.len()
    {
        return Err(JsValue::from_str("The PDF/A fonts were incomplete."));
    }
    let mut offset = 0;
    let fonts = font_names
        .iter()
        .zip(program_lengths)
        .zip(&font_metrics)
        .map(|((name, length), metrics)| {
            let end = offset + *length as usize;
            let program = &font_programs[offset..end];
            offset = end;
            StandardFont {
                name,
                program,
                metrics,
            }
        })
        .collect::<Vec<_>>();
    convert_to_pdfa_bytes(input, password, level, &fonts).map_err(|error| JsValue::from_str(&error))
}

/// `areas` holds four numbers for each page in `pages`: its left, top, right
/// and bottom edges as fractions of the visible page, from its top left.
#[wasm_bindgen]
pub fn crop_pdf(
    input: &[u8],
    password: &str,
    pages: &[u32],
    areas: &[f32],
) -> Result<Vec<u8>, JsValue> {
    let (areas, remainder) = areas.as_chunks::<4>();
    if !remainder.is_empty() || areas.len() != pages.len() {
        return Err(JsValue::from_str("A crop area is incomplete."));
    }
    let crops = pages
        .iter()
        .zip(areas)
        .map(|(&page, &area)| PageCrop { page, area })
        .collect::<Vec<_>>();
    crop_pdf_bytes(input, password, &crops).map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn compress_pdf(
    input: &[u8],
    password: &str,
    image_quality: u32,
    max_image_dimension: u32,
    remove_metadata: bool,
    remove_thumbnails: bool,
) -> Result<Vec<u8>, JsValue> {
    compress_pdf_bytes_with_password(
        input,
        password,
        CompressOptions {
            reflate: true,
            image_quality: image_quality.min(100) as u8,
            max_image_dimension,
            remove_metadata,
            remove_thumbnails,
        },
    )
    .map_err(|error| JsValue::from_str(&error))
}

/// The page count when the engine can open the file, -1 when it needs a
/// password it was not given, 0 when it is damaged.
#[wasm_bindgen]
pub fn pdf_condition(input: &[u8], password: &str) -> i32 {
    match condition_of(input, password) {
        Condition::Readable { pages } => pages as i32,
        Condition::Locked => -1,
        Condition::Damaged => 0,
    }
}

/// 0 unprotected, 1 restricted but opens without a password, 2 needs a password.
#[wasm_bindgen]
pub fn pdf_protection(input: &[u8]) -> Result<u8, JsValue> {
    protection_of(input)
        .map(|protection| match protection {
            Protection::None => 0,
            Protection::Restricted => 1,
            Protection::Password => 2,
        })
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn protect_pdf(
    input: &[u8],
    password: &str,
    user_password: &str,
    owner_password: &str,
    allow_printing: bool,
    allow_copying: bool,
    allow_editing: bool,
) -> Result<Vec<u8>, JsValue> {
    protect_pdf_bytes(
        input,
        password,
        ProtectOptions {
            user_password,
            owner_password,
            allow_printing,
            allow_copying,
            allow_editing,
        },
    )
    .map_err(|error| JsValue::from_str(&error))
}

/// 0 Helvetica, 1 Times, 2 Courier. `color` is 0xRRGGBB.
/// Fonts for text the standard ones cannot draw: their programs back to
/// back, each one's length, and its role, a group name with `-bold` for the
/// bold weight. The groups `helvetica`, `times` and `courier` serve only text
/// in that family; any other serves every family.
fn supplied_fonts<'a>(
    programs: &'a [u8],
    lengths: &[u32],
    roles: &'a [String],
) -> Result<Vec<SuppliedFont<'a>>, JsValue> {
    let total = lengths
        .iter()
        .try_fold(0usize, |total, length| total.checked_add(*length as usize));
    if total != Some(programs.len()) || lengths.len() != roles.len() {
        return Err(JsValue::from_str("The fonts were incomplete."));
    }
    let mut offset = 0;
    Ok(roles
        .iter()
        .zip(lengths)
        .map(|(role, &length)| {
            let end = offset + length as usize;
            let bytes = &programs[offset..end];
            offset = end;
            let (group, bold) = match role.strip_suffix("-bold") {
                Some(group) => (group, true),
                None => (role.as_str(), false),
            };
            let family = match group {
                "helvetica" => Some(FontFamily::Helvetica),
                "times" => Some(FontFamily::Times),
                "courier" => Some(FontFamily::Courier),
                _ => None,
            };
            SuppliedFont {
                group,
                family,
                bold,
                bytes,
            }
        })
        .collect())
}

fn text_style(font: u8, bold: bool, size: f32, color: u32) -> TextStyle {
    TextStyle {
        family: match font {
            1 => FontFamily::Times,
            2 => FontFamily::Courier,
            _ => FontFamily::Helvetica,
        },
        bold,
        size,
        color: [16, 8, 0].map(|shift| ((color >> shift) & 0xFF) as f32 / 255.0),
    }
}

/// Row by row from the top left: 0 top left, 4 centre, 8 bottom right.
fn position(index: u8) -> Position {
    match index {
        0 => Position::TopLeft,
        1 => Position::Top,
        2 => Position::TopRight,
        3 => Position::Left,
        5 => Position::Right,
        6 => Position::BottomLeft,
        7 => Position::Bottom,
        8 => Position::BottomRight,
        _ => Position::Center,
    }
}

/// An empty `pages` numbers every page.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn add_page_numbers(
    input: &[u8],
    password: &str,
    pages: &[u32],
    first_number: u32,
    template: &str,
    position_index: u8,
    margin: f32,
    font: u8,
    bold: bool,
    size: f32,
    color: u32,
    opacity: f32,
    font_programs: &[u8],
    font_lengths: &[u32],
    font_roles: Vec<String>,
) -> Result<Vec<u8>, JsValue> {
    let fonts = supplied_fonts(font_programs, font_lengths, &font_roles)?;
    add_page_numbers_bytes(
        input,
        password,
        PageNumberOptions {
            template,
            first_number,
            pages,
            position: position(position_index),
            margin,
            style: text_style(font, bold, size, color),
            opacity,
            fonts: &fonts,
        },
    )
    .map_err(|error| JsValue::from_str(&error))
}

/// A non-empty `image` is the watermark and the text settings are ignored;
/// otherwise `text` is. An empty `pages` marks every page.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn add_watermark(
    input: &[u8],
    password: &str,
    pages: &[u32],
    text: &str,
    font: u8,
    bold: bool,
    size: f32,
    color: u32,
    image: &[u8],
    image_width: f32,
    position_index: u8,
    margin: f32,
    rotation: f32,
    opacity: f32,
    behind: bool,
    tile: bool,
    font_programs: &[u8],
    font_lengths: &[u32],
    font_roles: Vec<String>,
) -> Result<Vec<u8>, JsValue> {
    let fonts = supplied_fonts(font_programs, font_lengths, &font_roles)?;
    let content = if image.is_empty() {
        WatermarkContent::Text {
            text,
            style: text_style(font, bold, size, color),
        }
    } else {
        WatermarkContent::Image {
            bytes: image,
            width: image_width,
        }
    };
    add_watermark_bytes(
        input,
        password,
        WatermarkOptions {
            content,
            pages,
            position: position(position_index),
            margin,
            rotation,
            opacity,
            behind,
            tile,
            fonts: &fonts,
        },
    )
    .map_err(|error| JsValue::from_str(&error))
}

/// `places` holds three numbers for each page in `pages`: the signature's
/// left edge, top edge and width as fractions of the visible page, from its
/// top left. A page may be listed more than once.
#[wasm_bindgen]
pub fn sign_pdf(
    input: &[u8],
    password: &str,
    image: &[u8],
    pages: &[u32],
    places: &[f32],
) -> Result<Vec<u8>, JsValue> {
    let (places, remainder) = places.as_chunks::<3>();
    if !remainder.is_empty() || places.len() != pages.len() {
        return Err(JsValue::from_str("A signature placement is incomplete."));
    }
    let placements = pages
        .iter()
        .zip(places)
        .map(|(&page, &place)| SignaturePlacement { page, place })
        .collect::<Vec<_>>();
    add_signature_bytes(input, password, image, &placements)
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
/// Writes recognized words into their pages as invisible text. One entry per
/// word: `pages` is its page from 1, `texts` what it says, `boxes` five
/// numbers each (left, width, baseline, size and angle, as `OcrWord` measures
/// them), and `spaces` whether a space follows it on its line.
pub fn add_text_layer(
    input: &[u8],
    password: &str,
    pages: &[u32],
    texts: Vec<String>,
    boxes: &[f32],
    spaces: &[u8],
) -> Result<Vec<u8>, JsValue> {
    let (boxes, remainder) = boxes.as_chunks::<5>();
    if !remainder.is_empty()
        || boxes.len() != pages.len()
        || texts.len() != pages.len()
        || spaces.len() != pages.len()
    {
        return Err(JsValue::from_str("The recognized text is incomplete."));
    }
    let mut by_page = Vec::<OcrPage<'_>>::new();
    for (index, &page) in pages.iter().enumerate() {
        let [left, width, baseline, size, angle] = boxes[index];
        let word = OcrWord {
            text: &texts[index],
            left,
            width,
            baseline,
            size,
            angle,
            space: spaces[index] != 0,
        };
        match by_page.last_mut() {
            Some(last) if last.page == page => last.words.push(word),
            _ => by_page.push(OcrPage {
                page,
                words: vec![word],
            }),
        }
    }
    add_text_layer_bytes(input, password, &by_page).map_err(|error| JsValue::from_str(&error))
}

/// Returns the flattened PDF, then how many annotations in scope were left as
/// they were. `forms_only` flattens form fields and nothing else.
#[wasm_bindgen]
pub fn flatten_pdf(input: &[u8], password: &str, forms_only: bool) -> Result<Array, JsValue> {
    let scope = if forms_only {
        FlattenScope::FormFields
    } else {
        FlattenScope::Everything
    };
    let flattened =
        flatten_pdf_bytes(input, password, scope).map_err(|error| JsValue::from_str(&error))?;
    let result = Array::new();
    result.push(&Uint8Array::from(flattened.bytes.as_slice()));
    result.push(&JsValue::from(flattened.kept as u32));
    Ok(result)
}

#[wasm_bindgen]
/// One entry per field: `widgets` names a widget of it by object number,
/// `kinds` says what its value is (0 text, 1 choices, 2 turn that widget on,
/// 3 turn it off), and `counts` how many of `values` it takes, in order: one
/// text, any number of export values, or none for a button. Returns the PDF
/// and, when flattened, how many fields were left as they were.
#[allow(clippy::too_many_arguments)]
pub fn fill_form(
    input: &[u8],
    password: &str,
    widgets: &[u32],
    kinds: &[u8],
    counts: &[u32],
    values: Vec<String>,
    flatten: bool,
    font_programs: &[u8],
    font_lengths: &[u32],
    font_roles: Vec<String>,
) -> Result<Array, JsValue> {
    let fonts = supplied_fonts(font_programs, font_lengths, &font_roles)?;
    let incomplete = || JsValue::from_str("A field to fill is incomplete.");
    if kinds.len() != widgets.len()
        || counts.len() != widgets.len()
        || counts.iter().map(|&count| count as usize).sum::<usize>() != values.len()
    {
        return Err(incomplete());
    }
    let mut offset = 0;
    let mut fills = Vec::with_capacity(widgets.len());
    for index in 0..widgets.len() {
        let count = counts[index] as usize;
        let range = offset..offset + count;
        offset += count;
        let value = match (kinds[index], count) {
            (0, 1) => FieldValue::Text(&values[range.start]),
            (1, _) => FieldValue::Choices(values[range].iter().map(String::as_str).collect()),
            (2, 0) => FieldValue::Button(true),
            (3, 0) => FieldValue::Button(false),
            _ => return Err(incomplete()),
        };
        fills.push(FieldFill {
            widget: (widgets[index], 0),
            value,
        });
    }
    let filled = fill_form_bytes(input, password, &fills, flatten, &fonts)
        .map_err(|error| JsValue::from_str(&error))?;
    let result = Array::new();
    result.push(&Uint8Array::from(filled.bytes.as_slice()));
    result.push(&JsValue::from(filled.kept as u32));
    Ok(result)
}

/// Redacts `areas` (left, top, right, bottom fractions, four per entry of
/// `pages`) and paints them `color` (0xRRGGBB). `images` holds one JPG or PNG
/// for each page in `image_pages`, drawn instead of that page.
///
/// Returns the PDF and the pages drawn from a picture, or, when some page
/// needs a picture it was not given, `undefined`, those pages, and why each
/// does: 0 for text in a font that cannot be measured, 1 for an image that
/// cannot be decoded, 2 for content that cannot be read.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn redact_pdf(
    input: &[u8],
    password: &str,
    pages: &[u32],
    areas: &[f32],
    color: u32,
    remove_metadata: bool,
    image_pages: &[u32],
    images: Array,
) -> Result<Array, JsValue> {
    let (areas, remainder) = areas.as_chunks::<4>();
    if !remainder.is_empty() || areas.len() != pages.len() {
        return Err(JsValue::from_str("A redaction area is incomplete."));
    }
    let redactions = pages
        .iter()
        .zip(areas)
        .map(|(&page, &area)| Redaction { page, area })
        .collect::<Vec<_>>();
    let pictures = images
        .iter()
        .map(|image| Uint8Array::new(&image).to_vec())
        .collect::<Vec<_>>();
    if pictures.len() != image_pages.len() {
        return Err(JsValue::from_str("A page picture is missing."));
    }
    let page_images = image_pages
        .iter()
        .zip(&pictures)
        .map(|(&page, image)| PageImage { page, image })
        .collect::<Vec<_>>();
    let channel = |shift: u32| ((color >> shift) & 0xFF) as f32 / 255.0;
    let redacted = redact_pdf_bytes(
        input,
        password,
        RedactOptions {
            redactions: &redactions,
            color: [channel(16), channel(8), channel(0)],
            remove_metadata,
            page_images: &page_images,
        },
    )
    .map_err(|error| JsValue::from_str(&error))?;
    let result = Array::new();
    match redacted {
        Redacted::Done { bytes, imaged } => {
            result.push(&Uint8Array::from(bytes.as_slice()));
            result.push(&js_sys::Uint32Array::from(imaged.as_slice()));
        }
        Redacted::NeedsImages(needed) => {
            let pages = needed.iter().map(|(page, _)| *page).collect::<Vec<_>>();
            let reasons = needed
                .iter()
                .map(|(_, reason)| match reason {
                    Unremovable::Text => 0u8,
                    Unremovable::Image => 1,
                    Unremovable::Content => 2,
                })
                .collect::<Vec<_>>();
            result.push(&JsValue::UNDEFINED);
            result.push(&js_sys::Uint32Array::from(pages.as_slice()));
            result.push(&Uint8Array::from(reasons.as_slice()));
        }
    }
    Ok(result)
}

/// Each page's glyphs as `[boxes, text, ends, metrics, colors, looks,
/// images]`: four fractions per glyph as in `redact_pdf`, the page's text,
/// where each glyph's text ends in it in UTF-16 units, then each glyph's
/// font size in points and baseline as a fraction of the page's height from
/// its top, its fill as 0xRRGGBB or 0xFFFFFFFF when unknown, and its look:
/// the nearest standard family (0 Helvetica, 1 Times, 2 Courier), plus 4 for
/// bold, 8 for italic, 16 when it paints nothing, and 32 when it runs
/// upright. Last, where each image is drawn, four fractions each like the
/// glyphs' boxes.
#[wasm_bindgen]
pub fn redaction_text(input: &[u8], password: &str) -> Result<Array, JsValue> {
    let pages = page_texts(input, password).map_err(|error| JsValue::from_str(&error))?;
    let result = Array::new();
    for page in pages {
        let boxes = page.boxes.concat();
        let mut text = String::new();
        let mut ends = Vec::with_capacity(page.text.len());
        let mut length = 0u32;
        for glyph in &page.text {
            text.push_str(glyph);
            length += glyph.encode_utf16().count() as u32;
            ends.push(length);
        }
        let metrics = page
            .styles
            .iter()
            .flat_map(|style| [style.size, style.baseline])
            .collect::<Vec<_>>();
        let colors = page
            .styles
            .iter()
            .map(|style| {
                style.color.map_or(NO_FILL, |channels| {
                    let [red, green, blue] =
                        channels.map(|channel| (channel * 255.0).round() as u32);
                    (red << 16) | (green << 8) | blue
                })
            })
            .collect::<Vec<_>>();
        let looks = page
            .styles
            .iter()
            .map(|style| {
                let family = match style.family {
                    FontFamily::Helvetica => 0,
                    FontFamily::Times => 1,
                    FontFamily::Courier => 2,
                };
                family
                    | u8::from(style.bold) << 2
                    | u8::from(style.italic) << 3
                    | u8::from(style.invisible) << 4
                    | u8::from(style.upright) << 5
            })
            .collect::<Vec<_>>();
        let entry = Array::new();
        entry.push(&js_sys::Float32Array::from(boxes.as_slice()));
        entry.push(&JsValue::from_str(&text));
        entry.push(&js_sys::Uint32Array::from(ends.as_slice()));
        entry.push(&js_sys::Float32Array::from(metrics.as_slice()));
        entry.push(&js_sys::Uint32Array::from(colors.as_slice()));
        entry.push(&Uint8Array::from(looks.as_slice()));
        let images = page.images.iter().flatten().copied().collect::<Vec<_>>();
        entry.push(&js_sys::Float32Array::from(images.as_slice()));
        result.push(&entry);
    }
    Ok(result)
}

/// Sets an annotation's `fill` to nothing.
const NO_FILL: u32 = u32::MAX;

/// Adds annotations, one per entry of `kinds`: 0 highlight, 1 underline,
/// 2 strike out, 3 squiggly, 4 ink, 5 rectangle, 6 ellipse, 7 line, 8 arrow,
/// 9 text box, 10 note, 11 image. Each takes `lengths[i]` numbers from
/// `points`, fractions of the page as displayed from its top left: four per
/// box for markup, x and y pairs for ink with a NaN pair between strokes,
/// a box for shapes and text, two points for lines, a point for notes, and
/// left, top and width for images. `colors` and `fills` are 0xRRGGBB, with
/// `fills` 0xFFFFFFFF for none. `sizes` is the stroke width in points, or the
/// font size for text. `fonts` is the family (0 Helvetica, 1 Times, 2
/// Courier) times two, plus one for bold, plus eight for centred text or
/// sixteen for text set against the right edge. `texts` and `comments` hold one
/// string each. Images take their bytes from `images` in order, split by
/// `image_lengths`. `remove` deletes the annotations already in the file with
/// those object numbers (pdf.js reports them as "41R"). `flatten` draws
/// everything into the pages instead.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn annotate_pdf(
    input: &[u8],
    password: &str,
    kinds: &[u8],
    pages: &[u32],
    colors: &[u32],
    opacities: &[f32],
    sizes: &[f32],
    fills: &[u32],
    fonts: &[u8],
    lengths: &[u32],
    points: &[f32],
    texts: Vec<String>,
    comments: Vec<String>,
    images: &[u8],
    image_lengths: &[u32],
    remove: &[u32],
    flatten: bool,
    font_programs: &[u8],
    font_lengths: &[u32],
    font_roles: Vec<String>,
) -> Result<Vec<u8>, JsValue> {
    let supplied = supplied_fonts(font_programs, font_lengths, &font_roles)?;
    let annotations = annotations(
        kinds,
        pages,
        colors,
        opacities,
        sizes,
        fills,
        fonts,
        lengths,
        points,
        &texts,
        &comments,
        images,
        image_lengths,
    )?;
    let remove = remove.iter().map(|&number| (number, 0)).collect::<Vec<_>>();
    annotate_pdf_bytes(input, password, &annotations, &remove, flatten, &supplied)
        .map_err(|error| JsValue::from_str(&error))
}

/// Edits pages. Text under each of `replace_areas` (four fractions per area,
/// on `replace_pages`) is taken out with nothing else under it; when it
/// cannot be, its `replace_shown` area is painted in its `replace_covers`
/// colour, 0xRRGGBB.
/// Everything under `erase_areas` is taken out and the area painted in its
/// `erase_fills` colour, 0xRRGGBB.
/// Before either, each image drawn at `image_froms` (four fractions each,
/// as `redaction_text` reports them, on `image_pages`) is drawn at
/// `image_tos` instead, or taken away where `image_kept` is 0.
/// The rest are annotations as `annotate_pdf` takes them, drawn into the
/// pages afterwards. Returns `[bytes, pages, reasons]`: the pages where an
/// area was painted over rather than taken out, with reasons numbered as
/// `redact_pdf` numbers them.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn edit_pdf(
    input: &[u8],
    password: &str,
    image_pages: &[u32],
    image_froms: &[f32],
    image_tos: &[f32],
    image_kept: &[u8],
    replace_pages: &[u32],
    replace_areas: &[f32],
    replace_shown: &[f32],
    replace_covers: &[u32],
    erase_pages: &[u32],
    erase_areas: &[f32],
    erase_fills: &[u32],
    kinds: &[u8],
    pages: &[u32],
    colors: &[u32],
    opacities: &[f32],
    sizes: &[f32],
    fills: &[u32],
    fonts: &[u8],
    lengths: &[u32],
    points: &[f32],
    texts: Vec<String>,
    images: &[u8],
    image_lengths: &[u32],
    font_programs: &[u8],
    font_lengths: &[u32],
    font_roles: Vec<String>,
) -> Result<Array, JsValue> {
    let supplied = supplied_fonts(font_programs, font_lengths, &font_roles)?;
    let incomplete = || JsValue::from_str("An edit is incomplete.");
    let (replace_areas, remainder) = replace_areas.as_chunks::<4>();
    let (replace_shown, rest) = replace_shown.as_chunks::<4>();
    if !remainder.is_empty()
        || !rest.is_empty()
        || replace_areas.len() != replace_pages.len()
        || replace_shown.len() != replace_pages.len()
        || replace_covers.len() != replace_pages.len()
    {
        return Err(incomplete());
    }
    let (erase_areas, remainder) = erase_areas.as_chunks::<4>();
    if !remainder.is_empty()
        || erase_areas.len() != erase_pages.len()
        || erase_fills.len() != erase_pages.len()
    {
        return Err(incomplete());
    }
    let (image_froms, remainder) = image_froms.as_chunks::<4>();
    let (image_tos, rest) = image_tos.as_chunks::<4>();
    if !remainder.is_empty()
        || !rest.is_empty()
        || image_froms.len() != image_pages.len()
        || image_tos.len() != image_pages.len()
        || image_kept.len() != image_pages.len()
    {
        return Err(incomplete());
    }
    let moves = image_pages
        .iter()
        .zip(image_froms)
        .zip(image_tos)
        .zip(image_kept)
        .map(|(((&page, &from), &to), &kept)| ImageMove {
            page,
            from,
            to: (kept != 0).then_some(to),
        })
        .collect::<Vec<_>>();
    let rgb = |color: u32| [16, 8, 0].map(|shift| ((color >> shift) & 0xFF) as f32 / 255.0);
    let replace = replace_pages
        .iter()
        .zip(replace_areas)
        .zip(replace_shown)
        .zip(replace_covers)
        .map(|(((&page, &area), &shown), &cover)| TextRemoval {
            page,
            area,
            shown,
            cover: rgb(cover),
        })
        .collect::<Vec<_>>();
    let erase = erase_pages
        .iter()
        .zip(erase_areas)
        .zip(erase_fills)
        .map(|((&page, &area), &fill)| Erasure {
            page,
            area,
            fill: rgb(fill),
        })
        .collect::<Vec<_>>();
    let comments = vec![String::new(); kinds.len()];
    let additions = annotations(
        kinds,
        pages,
        colors,
        opacities,
        sizes,
        fills,
        fonts,
        lengths,
        points,
        &texts,
        &comments,
        images,
        image_lengths,
    )?;
    let edited = edit_pdf_bytes(
        input,
        password,
        EditOptions {
            images: &moves,
            replace: &replace,
            erase: &erase,
            additions: &additions,
            fonts: &supplied,
        },
    )
    .map_err(|error| JsValue::from_str(&error))?;
    let (covered, reasons): (Vec<u32>, Vec<u8>) = edited
        .covered
        .iter()
        .map(|&(page, reason)| {
            (
                page,
                match reason {
                    Unremovable::Text => 0u8,
                    Unremovable::Image => 1,
                    Unremovable::Content => 2,
                },
            )
        })
        .unzip();
    let result = Array::new();
    result.push(&Uint8Array::from(edited.bytes.as_slice()));
    result.push(&js_sys::Uint32Array::from(covered.as_slice()));
    result.push(&Uint8Array::from(reasons.as_slice()));
    Ok(result)
}

/// Unpacks the flat arrays `annotate_pdf` and `edit_pdf` take.
#[allow(clippy::too_many_arguments)]
fn annotations<'a>(
    kinds: &[u8],
    pages: &[u32],
    colors: &[u32],
    opacities: &[f32],
    sizes: &[f32],
    fills: &[u32],
    fonts: &[u8],
    lengths: &[u32],
    points: &[f32],
    texts: &'a [String],
    comments: &'a [String],
    images: &'a [u8],
    image_lengths: &[u32],
) -> Result<Vec<Annotation<'a>>, JsValue> {
    let count = kinds.len();
    let incomplete = || JsValue::from_str("An annotation is incomplete.");
    if [
        pages.len(),
        colors.len(),
        opacities.len(),
        sizes.len(),
        fills.len(),
        fonts.len(),
        lengths.len(),
        texts.len(),
        comments.len(),
    ]
    .iter()
    .any(|&length| length != count)
        || lengths.iter().map(|&length| length as usize).sum::<usize>() != points.len()
        || image_lengths
            .iter()
            .map(|&length| length as usize)
            .sum::<usize>()
            != images.len()
    {
        return Err(incomplete());
    }
    let rgb = |color: u32| [16, 8, 0].map(|shift| ((color >> shift) & 0xFF) as f32 / 255.0);
    let (mut offset, mut image_offset, mut image_index) = (0, 0, 0);
    let mut annotations = Vec::with_capacity(count);
    for index in 0..count {
        let values = &points[offset..offset + lengths[index] as usize];
        offset += values.len();
        let size = sizes[index];
        let fill = (fills[index] != NO_FILL).then(|| rgb(fills[index]));
        let pair = |values: &[f32]| <[f32; 2]>::try_from(values).map_err(|_| incomplete());
        let quad = |values: &[f32]| <[f32; 4]>::try_from(values).map_err(|_| incomplete());
        let kind = match kinds[index] {
            kind @ 0..=3 => {
                let (boxes, remainder) = values.as_chunks::<4>();
                if boxes.is_empty() || !remainder.is_empty() {
                    return Err(incomplete());
                }
                AnnotationKind::Markup {
                    style: [
                        Markup::Highlight,
                        Markup::Underline,
                        Markup::StrikeOut,
                        Markup::Squiggly,
                    ][kind as usize],
                    boxes: boxes.to_vec(),
                }
            }
            4 => {
                let (pairs, remainder) = values.as_chunks::<2>();
                if !remainder.is_empty() {
                    return Err(incomplete());
                }
                let strokes = pairs
                    .split(|point| point.iter().any(|value| value.is_nan()))
                    .filter(|stroke| !stroke.is_empty())
                    .map(<[_]>::to_vec)
                    .collect();
                AnnotationKind::Ink {
                    strokes,
                    width: size,
                }
            }
            kind @ 5..=6 => AnnotationKind::Shape {
                shape: if kind == 5 {
                    Shape::Rectangle
                } else {
                    Shape::Ellipse
                },
                area: quad(values)?,
                width: size,
                fill,
            },
            kind @ 7..=8 => {
                let [x0, y0, x1, y1] = quad(values)?;
                AnnotationKind::Line {
                    from: [x0, y0],
                    to: [x1, y1],
                    width: size,
                    arrow: kind == 8,
                }
            }
            9 => AnnotationKind::Text {
                area: quad(values)?,
                text: &texts[index],
                family: match fonts[index] / 2 % 4 {
                    1 => FontFamily::Times,
                    2 => FontFamily::Courier,
                    _ => FontFamily::Helvetica,
                },
                bold: fonts[index] % 2 == 1,
                size,
                fill,
                align: match fonts[index] / 8 {
                    1 => TextAlign::Center,
                    2 => TextAlign::Right,
                    _ => TextAlign::Left,
                },
            },
            10 => AnnotationKind::Note { at: pair(values)? },
            11 => {
                let length = *image_lengths.get(image_index).ok_or_else(incomplete)? as usize;
                let bytes = &images[image_offset..image_offset + length];
                image_offset += length;
                image_index += 1;
                let [left, top, width] = <[f32; 3]>::try_from(values).map_err(|_| incomplete())?;
                AnnotationKind::Image {
                    place: [left, top, width],
                    bytes,
                }
            }
            _ => {
                return Err(JsValue::from_str(
                    "This kind of annotation is not supported.",
                ));
            }
        };
        annotations.push(Annotation {
            page: pages[index],
            kind,
            color: rgb(colors[index]),
            opacity: opacities[index],
            comment: &comments[index],
        });
    }
    Ok(annotations)
}

/// Numbers per item in `print_pdf`'s `numbers`.
const PRINT_STRIDE: usize = 20;

#[wasm_bindgen]
/// Draws new pages from a layout the browser measured (see `print.rs`).
/// `pages` holds each page's width and height in points. Each item has a
/// kind (0 fill, 1 line, 2 outline, 3 text, 4 image, 5 link), a page from 0,
/// a colour, a text (what a text item says, a link's address) and twenty
/// numbers: 0 to 3 its box (left, top, width, height; a line's two ends; a
/// text's left, baseline, size and measured width), 4 to 7 corner radii, 8
/// opacity, 9 to 12 a clip box (NaN for none) and 13 to 16 its radii, 17 a
/// stroke width, a text's letter spacing or the top a link goes to, 18 a
/// dash (0 solid, 1 dashed, 2 dotted) or a text's family (0 sans, 1 serif,
/// 2 mono), and 19 a text's style (1 bold, 2 italic), an image's index or
/// the page a link goes to (NaN for an address). Images arrive end to end in
/// `images`, cut by `image_lengths`; headings become bookmarks.
#[allow(clippy::too_many_arguments)]
pub fn print_pdf(
    pages: &[f32],
    kinds: &[u8],
    item_pages: &[u32],
    colors: &[u32],
    texts: Vec<String>,
    numbers: &[f32],
    images: &[u8],
    image_lengths: &[u32],
    heading_levels: &[u8],
    heading_pages: &[u32],
    heading_tops: &[f32],
    heading_titles: Vec<String>,
    title: &str,
    language: &str,
    font_programs: &[u8],
    font_lengths: &[u32],
    font_roles: Vec<String>,
) -> Result<Vec<u8>, JsValue> {
    let incomplete = || JsValue::from_str("The document's layout could not be read.");
    let (sizes, remainder) = pages.as_chunks::<2>();
    let count = kinds.len();
    if !remainder.is_empty()
        || item_pages.len() != count
        || colors.len() != count
        || texts.len() != count
        || numbers.len() != count * PRINT_STRIDE
        || heading_pages.len() != heading_levels.len()
        || heading_tops.len() != heading_levels.len()
        || heading_titles.len() != heading_levels.len()
    {
        return Err(incomplete());
    }
    let page_sizes = sizes
        .iter()
        .map(|&[width, height]| PrintPage { width, height })
        .collect::<Vec<_>>();
    let mut cut = Vec::with_capacity(image_lengths.len());
    let mut offset = 0usize;
    for &length in image_lengths {
        let end = offset
            .checked_add(length as usize)
            .filter(|&end| end <= images.len())
            .ok_or_else(incomplete)?;
        cut.push(&images[offset..end]);
        offset = end;
    }
    let area = |values: &[f32]| Area {
        left: values[0],
        top: values[1],
        width: values[2],
        height: values[3],
    };
    let radii = |values: &[f32]| [values[0], values[1], values[2], values[3]];
    let dash = |value: f32| match value as u8 {
        1 => Dash::Dashed,
        2 => Dash::Dotted,
        _ => Dash::Solid,
    };
    let mut items = Vec::with_capacity(count);
    for (index, values) in numbers.as_chunks::<PRINT_STRIDE>().0.iter().enumerate() {
        let color = colors[index];
        let geometry = &values[0..4];
        let mark = match kinds[index] {
            0 => PrintMark::Fill {
                area: area(geometry),
                radii: radii(&values[4..8]),
                color,
            },
            1 => PrintMark::Line {
                from: (values[0], values[1]),
                to: (values[2], values[3]),
                width: values[17],
                dash: dash(values[18]),
                color,
            },
            2 => PrintMark::Outline {
                area: area(geometry),
                radii: radii(&values[4..8]),
                width: values[17],
                dash: dash(values[18]),
                color,
            },
            3 => {
                let style = values[19] as u8;
                PrintMark::Text {
                    text: &texts[index],
                    left: values[0],
                    baseline: values[1],
                    size: values[2],
                    width: values[3],
                    spacing: values[17],
                    family: match values[18] as u8 {
                        1 => FontFamily::Times,
                        2 => FontFamily::Courier,
                        _ => FontFamily::Helvetica,
                    },
                    bold: style & 1 != 0,
                    italic: style & 2 != 0,
                    color,
                }
            }
            4 => PrintMark::Image {
                area: area(geometry),
                image: if values[19] >= 0.0 {
                    values[19] as usize
                } else {
                    usize::MAX
                },
            },
            5 => PrintMark::Link {
                area: area(geometry),
                target: if values[19].is_nan() {
                    LinkTarget::Uri(&texts[index])
                } else if values[19] >= 0.0 {
                    LinkTarget::Page {
                        page: values[19] as usize,
                        top: values[17],
                    }
                } else {
                    return Err(incomplete());
                },
            },
            _ => return Err(incomplete()),
        };
        let clip = (!values[9].is_nan()).then(|| Clip {
            area: area(&values[9..13]),
            radii: radii(&values[13..17]),
        });
        items.push(PrintItem {
            page: item_pages[index] as usize,
            mark,
            opacity: values[8],
            clip,
        });
    }
    let headings = heading_levels
        .iter()
        .enumerate()
        .map(|(index, &level)| Heading {
            level,
            title: &heading_titles[index],
            page: heading_pages[index] as usize,
            top: heading_tops[index],
        })
        .collect::<Vec<_>>();
    let fonts = supplied_fonts(font_programs, font_lengths, &font_roles)?;
    print_pdf_bytes(
        &PrintDocument {
            pages: &page_sizes,
            items: &items,
            images: &cut,
            headings: &headings,
            title,
            language,
        },
        &fonts,
    )
    .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
/// The characters of `text` none of the fonts has, each once.
pub fn missing_characters(
    text: &str,
    font_programs: &[u8],
    font_lengths: &[u32],
    font_roles: Vec<String>,
) -> Result<String, JsValue> {
    let fonts = supplied_fonts(font_programs, font_lengths, &font_roles)?;
    let typesetter = Typesetter::new(&fonts).map_err(|error| JsValue::from_str(&error))?;
    Ok(typesetter.missing(text))
}
