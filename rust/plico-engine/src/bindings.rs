use js_sys::{Array, Uint8Array};
use wasm_bindgen::prelude::*;

use crate::{
    CompressOptions, ImagePdfOptions, SplitMode, compress_pdf_bytes, images_to_pdf_bytes,
    merge_pdf_bytes, organize_pdf_bytes, split_pdf_bytes,
};

#[wasm_bindgen]
pub fn merge_pdfs(input: &[u8], lengths: &[u32]) -> Result<Vec<u8>, JsValue> {
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

    merge_pdf_bytes(&files).map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn images_to_pdf(
    input: &[u8],
    lengths: &[u32],
    page_width: f32,
    page_height: f32,
    margin: f32,
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
pub fn split_pdf_ranges(input: &[u8], bounds: &[u32], combine: bool) -> Result<Array, JsValue> {
    let chunks = bounds.chunks_exact(2);
    if !chunks.remainder().is_empty() {
        return Err(JsValue::from_str("A page range is incomplete."));
    }
    let ranges = chunks.map(|pair| (pair[0], pair[1])).collect::<Vec<_>>();
    split_pdf_bytes(input, SplitMode::Ranges(&ranges, combine))
        .map(split_outputs_to_js)
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn split_pdf_every(input: &[u8], interval: u32) -> Result<Array, JsValue> {
    split_pdf_bytes(input, SplitMode::Every(interval))
        .map(split_outputs_to_js)
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn organize_pdf(input: &[u8], page_turns: &[u32]) -> Result<Vec<u8>, JsValue> {
    let chunks = page_turns.chunks_exact(2);
    if !chunks.remainder().is_empty() {
        return Err(JsValue::from_str("A page instruction is incomplete."));
    }
    let pages = chunks
        .map(|pair| (pair[0], pair[1] as i32))
        .collect::<Vec<_>>();
    organize_pdf_bytes(input, &pages).map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn compress_pdf(
    input: &[u8],
    image_quality: u32,
    max_image_dimension: u32,
    remove_metadata: bool,
    remove_thumbnails: bool,
) -> Result<Vec<u8>, JsValue> {
    compress_pdf_bytes(
        input,
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
