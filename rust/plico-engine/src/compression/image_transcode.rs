use std::collections::BTreeSet;

use jpeg_encoder::{ChromaSubsamplingMethod, ColorType, Encoder as JpegEncoder};
use lopdf::{Dictionary, Document, Object, ObjectId};
use zune_core::bytestream::ZCursor;
use zune_jpeg::{JpegDecoder, zune_core::colorspace::ColorSpace};

use super::filter_matches;

/// Below this size a re-encoded JPEG rarely wins enough to justify the churn.
const MIN_TRANSCODE_JPEG: usize = 20 * 1024;
const MAX_DECODED_JPEG: usize = 256 * 1024 * 1024;

fn collect_mask_ids(document: &Document) -> BTreeSet<ObjectId> {
    document
        .objects
        .values()
        .filter_map(|obj| {
            let Object::Stream(stream) = obj else {
                return None;
            };
            let smask = stream
                .dict
                .get(b"SMask")
                .and_then(Object::as_reference)
                .ok();
            let mask = stream.dict.get(b"Mask").and_then(Object::as_reference).ok();
            Some([smask, mask])
        })
        .flatten()
        .flatten()
        .collect()
}

/// DCTDecode sources are plain JPEG files, so they can be decoded, re-encoded
/// at a chosen quality and swapped back in without touching page content.
/// Anything else (CMYK, inverted /Decode values, predictor chains) is left as
/// it is; a wrong guess here would shift colours, not just sizes.
pub(super) fn transcode_jpeg_images(document: &mut Document, quality: u8, max_dimension: u32) {
    let mask_ids = collect_mask_ids(document);
    let candidates: Vec<(ObjectId, PixelColors)> = document
        .objects
        .iter()
        .filter_map(|(id, object)| {
            let Object::Stream(stream) = object else {
                return None;
            };
            is_jpeg_image(&stream.dict)
                .then_some(*id)
                .zip(pixel_colors(document, &stream.dict))
        })
        .collect();

    for (id, colors) in candidates {
        let Some(Object::Stream(stream)) = document.objects.get_mut(&id) else {
            continue;
        };
        if stream.content.len() < MIN_TRANSCODE_JPEG {
            continue;
        }
        let Ok(pdf_width) = stream.dict.get(b"Width").and_then(Object::as_i64) else {
            continue;
        };
        let Ok(pdf_height) = stream.dict.get(b"Height").and_then(Object::as_i64) else {
            continue;
        };
        // A separately sized mask must stay aligned with its image.
        let image_max_dimension =
            if stream.dict.has(b"SMask") || stream.dict.has(b"Mask") || mask_ids.contains(&id) {
                0
            } else {
                max_dimension
            };
        if let Some((encoded, width, height)) = transcode_jpeg(
            &stream.content,
            colors,
            quality,
            image_max_dimension,
            pdf_width,
            pdf_height,
        ) && encoded.len() < stream.content.len()
        {
            stream.set_content(encoded);
            stream.dict.set("Width", width as i64);
            stream.dict.set("Height", height as i64);
        }
    }
}

/// Large eight-bit RGB and grayscale images are often stored with Flate even
/// when photographic JPEG would be much smaller. Keep the replacement only
/// when it beats the original stream. Other bit depths, masks and colour
/// transforms need separate handling to preserve their appearance.
pub(super) fn transcode_flate_images(document: &mut Document, quality: u8, max_dimension: u32) {
    let mask_ids = collect_mask_ids(document);
    let candidates: Vec<(ObjectId, PixelColors)> = document
        .objects
        .iter()
        .filter_map(|(id, object)| {
            let Object::Stream(stream) = object else {
                return None;
            };
            (!mask_ids.contains(id)
                && is_flate_image(&stream.dict)
                && supports_flate_image_decompression(&stream.dict, document)
                && stream.content.len() >= MIN_TRANSCODE_JPEG)
                .then_some(*id)
                .zip(pixel_colors(document, &stream.dict))
        })
        .collect();

    for (id, colors) in candidates {
        let Some(Object::Stream(stream)) = document.objects.get_mut(&id) else {
            continue;
        };
        let (Ok(width), Ok(height)) = (
            stream.dict.get(b"Width").and_then(Object::as_i64),
            stream.dict.get(b"Height").and_then(Object::as_i64),
        ) else {
            continue;
        };
        let (Ok(width), Ok(height)) = (u16::try_from(width), u16::try_from(height)) else {
            continue;
        };
        if width == 0 || height == 0 {
            continue;
        }
        let components = colors.components();
        let Some(decoded_size) = usize::from(width)
            .checked_mul(usize::from(height))
            .and_then(|pixels| pixels.checked_mul(components))
        else {
            continue;
        };
        if decoded_size > MAX_DECODED_JPEG {
            continue;
        }
        let Ok(mut pixels) = stream.decompressed_content_with_limit(decoded_size) else {
            continue;
        };
        if pixels.len() != decoded_size {
            continue;
        }
        let target = if stream.dict.has(b"SMask") || stream.dict.has(b"Mask") {
            0
        } else {
            max_dimension
        };
        let (new_width, new_height) = downscale_dimensions(width, height, target);
        if (new_width, new_height) != (width, height) {
            pixels = box_downscale(
                &pixels,
                usize::from(width),
                usize::from(height),
                components,
                usize::from(new_width),
                usize::from(new_height),
            );
        }
        let Some(encoded) = encode_jpeg(&pixels, new_width, new_height, colors, quality) else {
            continue;
        };
        if encoded.len() < stream.content.len() {
            stream.set_content(encoded);
            stream.dict.set("Filter", "DCTDecode");
            stream.dict.remove(b"DecodeParms");
            stream.dict.set("Width", i64::from(new_width));
            stream.dict.set("Height", i64::from(new_height));
        }
    }
}

fn supports_flate_image_decompression(dict: &Dictionary, document: &Document) -> bool {
    let Ok(params) = dict.get(b"DecodeParms") else {
        return true;
    };
    let params_dict = match params {
        Object::Dictionary(d) => Some(d),
        Object::Reference(id) => document
            .get_object(*id)
            .ok()
            .and_then(|obj| obj.as_dict().ok()),
        _ => None,
    };
    let Some(d) = params_dict else {
        return false;
    };
    let predictor = d.get(b"Predictor").and_then(Object::as_i64).unwrap_or(1);
    predictor == 1 || (10..=15).contains(&predictor)
}

#[derive(Clone, Copy)]
pub(crate) enum PixelColors {
    Gray,
    Rgb,
}

impl PixelColors {
    fn components(self) -> usize {
        match self {
            Self::Gray => 1,
            Self::Rgb => 3,
        }
    }
}

fn is_flate_image(dict: &Dictionary) -> bool {
    dict.get(b"Subtype")
        .is_ok_and(|value| value.as_name().is_ok_and(|name| name == b"Image"))
        && filter_matches(dict, b"FlateDecode")
        && !dict.has(b"Decode")
        && !dict.has(b"ImageMask")
        && dict
            .get(b"BitsPerComponent")
            .is_ok_and(|bits| bits.as_i64().is_ok_and(|value| value == 8))
}

pub(crate) fn is_jpeg_image(dict: &Dictionary) -> bool {
    let subtype_image = dict
        .get(b"Subtype")
        .is_ok_and(|value| value.as_name().is_ok_and(|name| name == b"Image"));
    subtype_image
        && filter_matches(dict, b"DCTDecode")
        && !dict.has(b"DecodeParms")
        && !dict.has(b"Decode")
        && dict
            .get(b"BitsPerComponent")
            .is_ok_and(|bits| bits.as_i64().is_ok_and(|value| value == 8))
}

/// Maps a PDF colour space onto the JPEG component layout it implies. Only
/// well-known 1- and 3-component spaces pass; ICC profiles carry arbitrary
/// transforms that a re-encode would silently reinterpret.
fn pixel_colors(document: &Document, dict: &Dictionary) -> Option<PixelColors> {
    let colorspace = match dict.get(b"ColorSpace").ok()? {
        Object::Reference(id) => document.get_object(*id).ok()?,
        other => other,
    };
    match colorspace {
        Object::Name(name) => match name.as_slice() {
            b"DeviceGray" | b"CalGray" => Some(PixelColors::Gray),
            b"DeviceRGB" | b"CalRGB" => Some(PixelColors::Rgb),
            _ => None,
        },
        Object::Array(items) => {
            let first = items.first()?.as_name().ok()?;
            if first != b"ICCBased" {
                return None;
            }
            let reference = items.get(1)?.as_reference().ok()?;
            let Object::Stream(stream) = document.get_object(reference).ok()? else {
                return None;
            };
            match stream.dict.get(b"N").and_then(Object::as_i64) {
                Ok(1) => Some(PixelColors::Gray),
                Ok(3) => Some(PixelColors::Rgb),
                _ => None,
            }
        }
        _ => None,
    }
}

fn transcode_jpeg(
    source: &[u8],
    colors: PixelColors,
    quality: u8,
    max_dimension: u32,
    pdf_width: i64,
    pdf_height: i64,
) -> Option<(Vec<u8>, u16, u16)> {
    // zune's reader trait needs an in-memory cursor; wrapping the slice avoids
    // an owned copy of the JPEG source.
    let mut decoder = JpegDecoder::new(ZCursor::new(source));
    if matches!(colors, PixelColors::Gray) {
        decoder.set_options(decoder.options().jpeg_set_out_colorspace(ColorSpace::Luma));
    }
    decoder.decode_headers().ok()?;
    let info = decoder.info()?;
    let expected = match colors {
        PixelColors::Gray => ColorSpace::Luma,
        PixelColors::Rgb => ColorSpace::RGB,
    };
    if decoder.output_colorspace()? != expected
        || info.width == 0
        || info.height == 0
        || usize::from(info.components) != colors.components()
        || i64::from(info.width) != pdf_width
        || i64::from(info.height) != pdf_height
        || decoder.output_buffer_size()? > MAX_DECODED_JPEG
    {
        return None;
    }
    let mut pixels = decoder.decode().ok()?;

    let (width, height) = (info.width, info.height);
    let components = colors.components();
    let (new_width, new_height) = downscale_dimensions(width, height, max_dimension);
    if (new_width, new_height) != (width, height) {
        pixels = box_downscale(
            &pixels,
            width as usize,
            height as usize,
            components,
            usize::from(new_width),
            usize::from(new_height),
        );
    }

    Some((
        encode_jpeg(&pixels, new_width, new_height, colors, quality)?,
        new_width,
        new_height,
    ))
}

fn downscale_dimensions(width: u16, height: u16, max_dimension: u32) -> (u16, u16) {
    if max_dimension == 0 || u32::from(width.max(height)) <= max_dimension {
        return (width, height);
    }
    let long = u32::from(width.max(height));
    let new_width = (u32::from(width) * max_dimension / long).max(1);
    let new_height = (u32::from(height) * max_dimension / long).max(1);
    (new_width as u16, new_height as u16)
}

pub(crate) fn encode_jpeg(
    pixels: &[u8],
    width: u16,
    height: u16,
    colors: PixelColors,
    quality: u8,
) -> Option<Vec<u8>> {
    let mut encoded = Vec::new();
    let mut encoder = JpegEncoder::new(&mut encoded, quality.clamp(1, 100));
    encoder.set_chroma_subsampling_method(ChromaSubsamplingMethod::Average);
    encoder.set_optimized_huffman_tables(true);
    encoder
        .encode(
            pixels,
            width,
            height,
            match colors {
                PixelColors::Gray => ColorType::Luma,
                PixelColors::Rgb => ColorType::Rgb,
            },
        )
        .ok()?;
    Some(encoded)
}

/// Area-average reduction. Every destination pixel maps onto at least one
/// source pixel because each target dimension is at most the source's.
fn box_downscale(
    data: &[u8],
    width: usize,
    height: usize,
    components: usize,
    new_width: usize,
    new_height: usize,
) -> Vec<u8> {
    let mut out = vec![0u8; new_width * new_height * components];
    for (dy, row) in out.chunks_exact_mut(new_width * components).enumerate() {
        let y0 = (dy * height) / new_height;
        let y1 = (((dy + 1) * height) / new_height).clamp(y0 + 1, height);
        for (dx, pixel) in row.chunks_exact_mut(components).enumerate() {
            let x0 = (dx * width) / new_width;
            let x1 = (((dx + 1) * width) / new_width).clamp(x0 + 1, width);
            let mut sums = [0u64; 3];
            let mut count = 0u64;
            for y in y0..y1 {
                let source = &data[y * width * components..][..width * components];
                for x in x0..x1 {
                    for (c, sum) in sums.iter_mut().take(components).enumerate() {
                        *sum += source[x * components + c] as u64;
                    }
                    count += 1;
                }
            }
            if count == 0 {
                continue;
            }
            for (c, sum) in sums.iter_mut().take(components).enumerate() {
                pixel[c] = (*sum / count) as u8;
            }
        }
    }
    out
}
