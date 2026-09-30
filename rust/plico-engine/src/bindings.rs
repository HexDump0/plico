use js_sys::{Array, Uint8Array};
use wasm_bindgen::prelude::*;

use crate::{
    CompressOptions, ImagePdfOptions, OrganizeItem, PageOrientation, SplitMode,
    compress_pdf_bytes_with_password, images_to_pdf_bytes, merge_pdf_bytes_with_options,
    organize_pdf_items, split_pdf_bytes_with_password, unlock_pdf_bytes,
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
    let chunks = bounds.chunks_exact(2);
    if !chunks.remainder().is_empty() {
        return Err(JsValue::from_str("A page range is incomplete."));
    }
    let ranges = chunks.map(|pair| (pair[0], pair[1])).collect::<Vec<_>>();
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

    let chunks = instructions.chunks_exact(3);
    if !chunks.remainder().is_empty() {
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
