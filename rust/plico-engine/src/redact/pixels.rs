//! Images under a box: the pixels it hides are overwritten in a copy of the
//! image, so what they showed is gone from the file rather than covered.
//!
//! Only pixels wholly under the boxes change. One that still shows outside
//! them keeps its colour, which the reader can see anyway, so nothing beyond a
//! box changes: a one-pixel image stretched into a rule across the page stays
//! a rule (pdf.js corpus: issue4436r.pdf).

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use zune_core::bytestream::ZCursor;
use zune_jpeg::{JpegDecoder, zune_core::colorspace::ColorSpace};

use super::decode::decode;
use super::geometry::{Matrix, Point, Rect, apply, invert};
use jpeg_encoder::{ColorType, Encoder as JpegEncoder, SamplingFactor};

use crate::compression::PixelColors;
use crate::documents::MAX_DECOMPRESSED_STREAM;

pub(super) enum ImageEdit {
    /// Nothing of it lies under a box.
    Keep,
    /// All of it lies under one box, so it need not be drawn at all.
    Drop,
    /// A redacted copy to draw instead.
    Replace(ObjectId),
    /// It cannot be decoded here, so the page has to become a picture.
    Unsupported,
}

/// What the overwritten samples become.
#[derive(Clone, Copy)]
enum Paint {
    /// The fill colour, as near as the colour space allows.
    Colour([f64; 3]),
    /// A stencil mask that leaves the page alone there.
    Unpainted,
    /// A soft or explicit mask that lets the image show there, so the
    /// overwritten pixels read as the fill colour rather than a hole.
    Shown,
}

/// The quality redacted JPEGs are written at: high enough that the rest of
/// the picture does not visibly change.
const JPEG_QUALITY: u8 = 92;

/// Redacts an image drawn with `ctm` mapping its unit square onto the page.
pub(super) fn redact_image(
    document: &mut Document,
    image: &Stream,
    ctm: Matrix,
    boxes: &[Rect],
    fill: [f64; 3],
) -> ImageEdit {
    let corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].map(|corner| apply(ctm, corner));
    let Some(bounds) = Rect::around(corners) else {
        return ImageEdit::Keep;
    };
    if !boxes.iter().any(|area| area.overlaps(bounds)) {
        return ImageEdit::Keep;
    }
    if boxes
        .iter()
        .any(|area| corners.iter().all(|corner| area.contains(*corner)))
    {
        return ImageEdit::Drop;
    }
    let Some(inverse) = invert(ctm) else {
        // A flattened image draws nothing, so it goes.
        return ImageEdit::Drop;
    };
    let mask = image
        .dict
        .get(b"ImageMask")
        .and_then(Object::as_bool)
        .unwrap_or(false);
    match redact_samples(
        document,
        image,
        inverse,
        boxes,
        if mask {
            Paint::Unpainted
        } else {
            Paint::Colour(fill)
        },
    ) {
        Some(Some(id)) => ImageEdit::Replace(id),
        Some(None) => ImageEdit::Keep,
        None => ImageEdit::Unsupported,
    }
}

/// A redacted copy of `image`, `Some(None)` when no pixel lies wholly under
/// the boxes, or `None` when the image cannot be read. Its masks follow it.
fn redact_samples(
    document: &mut Document,
    image: &Stream,
    inverse: Matrix,
    boxes: &[Rect],
    paint: Paint,
) -> Option<Option<ObjectId>> {
    let dict = &image.dict;
    let width = positive(dict, b"Width")?;
    let height = positive(dict, b"Height")?;
    let rows = covered_rows(inverse, boxes, width, height);
    if rows.iter().all(Vec::is_empty) {
        return Some(None);
    }

    let mask = dict
        .get(b"ImageMask")
        .and_then(Object::as_bool)
        .unwrap_or(false);
    let bits = if mask {
        1
    } else {
        dict.get(b"BitsPerComponent")
            .ok()
            .and_then(|value| resolve(document, value))
            .and_then(|value| value.as_i64().ok())?
    };
    if !matches!(bits, 1 | 2 | 4 | 8 | 16) {
        return None;
    }
    let space = if mask {
        Space::Mask
    } else {
        colour_space(document, dict.get(b"ColorSpace").ok()?)?
    };
    let components = space.components();
    let decode = dict
        .get(b"Decode")
        .ok()
        .and_then(|value| resolve(document, value))
        .and_then(|value| value.as_array().ok())
        .and_then(|items| {
            items
                .iter()
                .map(|item| item.as_float().ok().map(f64::from))
                .collect::<Option<Vec<_>>>()
        })
        .filter(|values| values.len() == 2 * components);
    let values = sample_values(space, paint, decode.as_deref(), bits as u32);

    let (mut samples, jpeg) = decode_samples(document, image, width, height, components)?;
    let row_bytes = (width * components * bits as usize).div_ceil(8);
    if samples.len() < row_bytes * height {
        return None;
    }
    for (row, spans) in rows.iter().enumerate() {
        let line = &mut samples[row * row_bytes..(row + 1) * row_bytes];
        for &(from, to) in spans {
            for pixel in from..to {
                for (component, &value) in values.iter().enumerate() {
                    write_sample(
                        line,
                        (pixel * components + component) * bits as usize,
                        bits as u32,
                        value,
                    );
                }
            }
        }
    }

    let mut copy = dict.clone();
    copy.remove(b"Filter");
    copy.remove(b"DecodeParms");
    copy.remove(b"Length");
    let content = match jpeg {
        Some(colors) => {
            copy.set("Filter", "DCTDecode");
            encode_jpeg(&samples, width as u16, height as u16, colors)?
        }
        // Written raw; saving compresses it.
        None => samples,
    };
    // Masks are images of their own, often at another resolution, and a
    // signature's soft mask is its shape, so they are redacted too.
    for key in [b"SMask".as_slice(), b"Mask"] {
        let Some(Object::Reference(mask_id)) = dict.get(key).ok() else {
            continue;
        };
        let Ok(Object::Stream(mask)) = document.get_object(*mask_id).cloned() else {
            continue;
        };
        if let Some(redacted) = redact_samples(document, &mask, inverse, boxes, Paint::Shown)? {
            copy.set(key, redacted);
        }
    }
    Some(Some(document.add_object(Stream::new(copy, content))))
}

fn resolve<'a>(document: &'a Document, value: &'a Object) -> Option<&'a Object> {
    match value {
        Object::Reference(id) => document.get_object(*id).ok(),
        value => Some(value),
    }
}

fn positive(dict: &Dictionary, key: &[u8]) -> Option<usize> {
    let value = dict.get(key).and_then(Object::as_i64).ok()?;
    (1..=1 << 16).contains(&value).then_some(value as usize)
}

/// How far inside a pixel's edges the boxes have to reach for it to count as
/// hidden, in pixels. Float noise at a box edge that lines up with pixel
/// boundaries would otherwise leave a row of hidden pixels untouched.
const SLACK: f64 = 0.1;

/// For each row from the top, the pixel ranges hidden under the boxes.
fn covered_rows(
    inverse: Matrix,
    boxes: &[Rect],
    width: usize,
    height: usize,
) -> Vec<Vec<(usize, usize)>> {
    // Each box as a quadrilateral in pixel space, rows counted from the top.
    let quads = boxes
        .iter()
        .map(|area| {
            area.corners().map(|corner| {
                let (u, v) = apply(inverse, corner);
                (u * width as f64, (1.0 - v) * height as f64)
            })
        })
        .collect::<Vec<[Point; 4]>>();
    (0..height)
        .map(|row| {
            // A pixel lies inside a convex box when its corners do, so a row
            // of pixels is covered where the boxes cover both its edges.
            let top = sections(&quads, row as f64 + SLACK);
            let bottom = sections(&quads, row as f64 + 1.0 - SLACK);
            let mut spans = Vec::new();
            for &(a0, a1) in &top {
                for &(b0, b1) in &bottom {
                    let (low, high) = (a0.max(b0), a1.min(b1));
                    let from = (low - SLACK).ceil().max(0.0);
                    let to = ((high + SLACK).floor()).min(width as f64);
                    if to > from {
                        spans.push((from as usize, to as usize));
                    }
                }
            }
            spans.sort_unstable();
            spans
        })
        .collect()
}

/// Where the boxes cross the line at height `y`, merged.
fn sections(quads: &[[Point; 4]], y: f64) -> Vec<(f64, f64)> {
    let mut found = quads
        .iter()
        .filter_map(|quad| {
            let mut low = f64::INFINITY;
            let mut high = f64::NEG_INFINITY;
            for index in 0..4 {
                let (a, b) = (quad[index], quad[(index + 1) % 4]);
                if y < a.1.min(b.1) || y > a.1.max(b.1) {
                    continue;
                }
                let xs = if a.1 == b.1 {
                    [a.0, b.0]
                } else {
                    let x = a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0);
                    [x, x]
                };
                for x in xs {
                    low = low.min(x);
                    high = high.max(x);
                }
            }
            (low <= high).then_some((low, high))
        })
        .collect::<Vec<_>>();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (low, high) in found {
        match merged.last_mut() {
            Some(last) if low <= last.1 => last.1 = last.1.max(high),
            _ => merged.push((low, high)),
        }
    }
    merged
}

#[derive(Clone, Copy)]
enum Space {
    Gray,
    Rgb,
    Cmyk,
    Lab,
    Indexed,
    /// Separation, DeviceN, or an ICC profile of another size: tints, left
    /// at zero.
    Tints(usize),
    Mask,
}

impl Space {
    fn components(self) -> usize {
        match self {
            Space::Gray | Space::Indexed | Space::Mask => 1,
            Space::Rgb | Space::Lab => 3,
            Space::Cmyk => 4,
            Space::Tints(count) => count,
        }
    }
}

fn colour_space(document: &Document, value: &Object) -> Option<Space> {
    match resolve(document, value)? {
        Object::Name(name) => match name.as_slice() {
            b"DeviceGray" | b"CalGray" => Some(Space::Gray),
            b"DeviceRGB" | b"CalRGB" => Some(Space::Rgb),
            b"DeviceCMYK" => Some(Space::Cmyk),
            _ => None,
        },
        Object::Array(items) => match items.first()?.as_name().ok()? {
            b"CalGray" => Some(Space::Gray),
            b"CalRGB" => Some(Space::Rgb),
            b"Lab" => Some(Space::Lab),
            b"Indexed" | b"I" => Some(Space::Indexed),
            b"Separation" => Some(Space::Tints(1)),
            b"DeviceN" => Some(Space::Tints(
                resolve(document, items.get(1)?)?
                    .as_array()
                    .ok()?
                    .len()
                    .max(1),
            )),
            b"ICCBased" => {
                let profile = resolve(document, items.get(1)?)?.as_stream().ok()?;
                match profile.dict.get(b"N").and_then(Object::as_i64).ok()? {
                    1 => Some(Space::Gray),
                    3 => Some(Space::Rgb),
                    4 => Some(Space::Cmyk),
                    count @ 2..=32 => Some(Space::Tints(count as usize)),
                    _ => None,
                }
            }
            _ => None,
        },
        _ => None,
    }
}

/// The integer sample for each component that reads as `paint` through
/// `decode`, or the default decode for the space.
fn sample_values(space: Space, paint: Paint, decode: Option<&[f64]>, bits: u32) -> Vec<u32> {
    let maximum = ((1u64 << bits) - 1) as f64;
    let (targets, defaults): (Vec<f64>, Vec<[f64; 2]>) = match (space, paint) {
        (Space::Mask, Paint::Unpainted) => (vec![1.0], vec![[0.0, 1.0]]),
        // In a mask image, as in a stencil, 0 marks where the image shows;
        // in a soft mask, 1 is opaque.
        (Space::Mask, _) => (vec![0.0], vec![[0.0, 1.0]]),
        (Space::Gray, Paint::Shown) => (vec![1.0], vec![[0.0, 1.0]]),
        (space, Paint::Colour([red, green, blue])) => match space {
            Space::Gray => (
                vec![0.299 * red + 0.587 * green + 0.114 * blue],
                vec![[0.0, 1.0]],
            ),
            Space::Rgb => (vec![red, green, blue], vec![[0.0, 1.0]; 3]),
            Space::Cmyk => {
                let black = 1.0 - red.max(green).max(blue);
                let ink = |channel: f64| {
                    if black >= 1.0 {
                        0.0
                    } else {
                        (1.0 - channel - black) / (1.0 - black)
                    }
                };
                (
                    vec![ink(red), ink(green), ink(blue), black],
                    vec![[0.0, 1.0]; 4],
                )
            }
            Space::Lab => (
                vec![
                    100.0 * (0.299 * red + 0.587 * green + 0.114 * blue),
                    0.0,
                    0.0,
                ],
                vec![[0.0, 100.0], [-100.0, 100.0], [-100.0, 100.0]],
            ),
            Space::Indexed => (vec![0.0], vec![[0.0, maximum]]),
            Space::Tints(count) => (vec![0.0; count], vec![[0.0, 1.0]; count]),
            Space::Mask => unreachable!(),
        },
        (space, _) => (
            vec![0.0; space.components()],
            vec![[0.0, 1.0]; space.components()],
        ),
    };
    targets
        .iter()
        .enumerate()
        .map(|(index, target)| {
            let [low, high] = decode
                .map(|decode| [decode[2 * index], decode[2 * index + 1]])
                .unwrap_or(defaults[index]);
            if (high - low).abs() < 1e-12 {
                return 0;
            }
            ((target - low) / (high - low) * maximum)
                .round()
                .clamp(0.0, maximum) as u32
        })
        .collect()
}

fn write_sample(line: &mut [u8], bit: usize, bits: u32, value: u32) {
    match bits {
        8 => line[bit / 8] = value as u8,
        16 => line[bit / 8..bit / 8 + 2].copy_from_slice(&(value as u16).to_be_bytes()),
        _ => {
            let byte = bit / 8;
            let shift = 8 - bits as usize - bit % 8;
            let mask = (((1u32 << bits) - 1) << shift) as u8;
            line[byte] = (line[byte] & !mask) | (((value << shift) as u8) & mask);
        }
    }
}

/// The image's samples, and for a JPEG the layout to write it back in.
fn decode_samples(
    document: &Document,
    image: &Stream,
    width: usize,
    height: usize,
    components: usize,
) -> Option<(Vec<u8>, Option<PixelColors>)> {
    let (bytes, jpeg) = decode(document, image)?;
    if !jpeg {
        return Some((bytes, None));
    }
    let colors = match components {
        1 => PixelColors::Gray,
        3 => PixelColors::Rgb,
        _ => return None,
    };
    // Readers skip anything before the start of the image.
    let start = bytes.windows(2).position(|pair| pair == [0xFF, 0xD8])?;
    let mut decoder = JpegDecoder::new(ZCursor::new(&bytes[start..]));
    if components == 1 {
        decoder.set_options(decoder.options().jpeg_set_out_colorspace(ColorSpace::Luma));
    }
    decoder.decode_headers().ok()?;
    let info = decoder.info()?;
    let expected = if components == 1 {
        ColorSpace::Luma
    } else {
        ColorSpace::RGB
    };
    if decoder.output_colorspace()? != expected
        || usize::from(info.width) != width
        || usize::from(info.height) != height
        || usize::from(info.components) != components
        || decoder.output_buffer_size()? > MAX_DECOMPRESSED_STREAM
    {
        return None;
    }
    Some((decoder.decode().ok()?, Some(colors)))
}

/// An unfiltered inline image's data with its hidden pixels overwritten,
/// `Some(None)` when no pixel lies wholly under the boxes, or `None` when its
/// layout cannot be worked out here.
pub(super) fn redact_inline(
    dictionary: &super::lexer::Value,
    data: &[u8],
    ctm: Matrix,
    boxes: &[Rect],
    fill: [f64; 3],
) -> Option<Option<Vec<u8>>> {
    use super::lexer::{Value, inline_components};
    let get = |short: &[u8], long: &[u8]| dictionary.get(short).or_else(|| dictionary.get(long));
    if get(b"F", b"Filter")
        .is_some_and(|filter| !matches!(filter, Value::Array(items) if items.is_empty()))
    {
        return None;
    }
    let size = |value: Option<&Value>| {
        value
            .and_then(Value::number)
            .filter(|value| (1.0..=65536.0).contains(value))
            .map(|value| value as usize)
    };
    let width = size(get(b"W", b"Width"))?;
    let height = size(get(b"H", b"Height"))?;
    let mask = matches!(get(b"IM", b"ImageMask"), Some(Value::Bool(true)));
    let (space, bits) = if mask {
        (Space::Mask, 1)
    } else {
        let space = match get(b"CS", b"ColorSpace")? {
            Value::Name(name) => match name.as_slice() {
                b"G" | b"DeviceGray" => Space::Gray,
                b"RGB" | b"DeviceRGB" => Space::Rgb,
                b"CMYK" | b"DeviceCMYK" => Space::Cmyk,
                _ => return None,
            },
            other if inline_components(other) == Some(1) => Space::Indexed,
            _ => return None,
        };
        (space, get(b"BPC", b"BitsPerComponent")?.number()? as u32)
    };
    if !matches!(bits, 1 | 2 | 4 | 8 | 16) {
        return None;
    }
    let components = space.components();
    let decode = match get(b"D", b"Decode") {
        Some(Value::Array(items)) => Some(
            items
                .iter()
                .map(Value::number)
                .collect::<Option<Vec<_>>>()?,
        )
        .filter(|values| values.len() == 2 * components),
        _ => None,
    };
    let inverse = invert(ctm)?;
    let rows = covered_rows(inverse, boxes, width, height);
    if rows.iter().all(Vec::is_empty) {
        return Some(None);
    }
    let paint = if mask {
        Paint::Unpainted
    } else {
        Paint::Colour(fill)
    };
    let values = sample_values(space, paint, decode.as_deref(), bits);
    let row_bytes = (width * components * bits as usize).div_ceil(8);
    if data.len() < row_bytes * height {
        return None;
    }
    let mut samples = data.to_vec();
    for (row, spans) in rows.iter().enumerate() {
        let line = &mut samples[row * row_bytes..(row + 1) * row_bytes];
        for &(from, to) in spans {
            for pixel in from..to {
                for (component, &value) in values.iter().enumerate() {
                    write_sample(
                        line,
                        (pixel * components + component) * bits as usize,
                        bits,
                        value,
                    );
                }
            }
        }
    }
    Some(Some(samples))
}

/// Without chroma subsampling, so the box's edge does not bleed colour into
/// the pixels beside it.
fn encode_jpeg(pixels: &[u8], width: u16, height: u16, colors: PixelColors) -> Option<Vec<u8>> {
    let mut encoded = Vec::new();
    let mut encoder = JpegEncoder::new(&mut encoded, JPEG_QUALITY);
    encoder.set_sampling_factor(SamplingFactor::F_1_1);
    encoder.set_optimized_huffman_tables(true);
    let layout = match colors {
        PixelColors::Gray => ColorType::Luma,
        PixelColors::Rgb => ColorType::Rgb,
    };
    encoder.encode(pixels, width, height, layout).ok()?;
    Some(encoded)
}
