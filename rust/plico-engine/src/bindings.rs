use js_sys::{Array, Uint8Array};
use wasm_bindgen::prelude::*;

use crate::{
    CompressOptions, FontFamily, ImagePdfOptions, OrganizeItem, PageCrop, PageNumberOptions,
    PageOrientation, PdfALevel, Position, ProtectOptions, Protection, SignaturePlacement,
    SplitMode, StandardFont, TextStyle, WatermarkContent, WatermarkOptions, add_page_numbers_bytes,
    add_signature_bytes, add_watermark_bytes, compress_pdf_bytes_with_password,
    convert_to_pdfa_bytes, crop_pdf_bytes, images_to_pdf_bytes, merge_pdf_bytes_with_options,
    organize_pdf_items, protect_pdf_bytes, protection_of, split_pdf_bytes_with_password,
    standard_fonts_for_pdfa, unlock_pdf_bytes,
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
) -> Result<Vec<u8>, JsValue> {
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
) -> Result<Vec<u8>, JsValue> {
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
