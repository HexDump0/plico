use std::io::{Cursor, Write};

use crate::compression::{PixelColors, encode_jpeg, write_compressed};
use flate2::{Compression, write::ZlibEncoder};
use lopdf::{Document, Object, Stream, dictionary};
use zune_core::bytestream::ZCursor;
use zune_jpeg::{JpegDecoder, zune_core::colorspace::ColorSpace};

const MAX_DECODED_IMAGE: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy)]
pub struct ImagePdfOptions {
    pub page_width: f32,
    pub page_height: f32,
    pub margin: f32,
}

struct PreparedImage {
    stream: Stream,
    mask: Option<Stream>,
    width: u32,
    height: u32,
    orientation: u8,
}

pub fn images_to_pdf_bytes(images: &[&[u8]], options: ImagePdfOptions) -> Result<Vec<u8>, String> {
    if images.is_empty() {
        return Err("Choose at least one JPG or PNG image.".into());
    }
    if !options.page_width.is_finite()
        || !options.page_height.is_finite()
        || !options.margin.is_finite()
        || options.page_width <= 0.0
        || options.page_height <= 0.0
        || options.margin < 0.0
        || options.margin * 2.0 >= options.page_width.min(options.page_height)
        || options.page_width.max(options.page_height) > 14_400.0
    {
        return Err("Choose a valid page size and margin.".into());
    }

    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let mut kids = Vec::with_capacity(images.len());

    for (index, bytes) in images.iter().enumerate() {
        let mut image = prepare_image(bytes)
            .map_err(|error| format!("Image {} could not be added: {error}", index + 1))?;
        let landscape = matches!(image.orientation, 5..=8)
            .then_some(image.height > image.width)
            .unwrap_or(image.width > image.height);
        let (page_width, page_height) = if landscape {
            (options.page_height, options.page_width)
        } else {
            (options.page_width, options.page_height)
        };

        if let Some(mask) = image.mask.take() {
            let mask_id = document.add_object(mask);
            image.stream.dict.set("SMask", mask_id);
        }
        let image_id = document.add_object(image.stream);
        let (logical_width, logical_height) = if matches!(image.orientation, 5..=8) {
            (image.height as f32, image.width as f32)
        } else {
            (image.width as f32, image.height as f32)
        };
        let scale = ((page_width - 2.0 * options.margin) / logical_width)
            .min((page_height - 2.0 * options.margin) / logical_height);
        let draw_width = logical_width * scale;
        let draw_height = logical_height * scale;
        let x = (page_width - draw_width) / 2.0;
        let y = (page_height - draw_height) / 2.0;
        let [a, b, c, d, e, f] = image_matrix(image.orientation, x, y, draw_width, draw_height);
        let content = format!("q\n{a:.4} {b:.4} {c:.4} {d:.4} {e:.4} {f:.4} cm\n/Im0 Do\nQ\n");
        let content_id = document.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![Object::Integer(0), Object::Integer(0), Object::Real(page_width), Object::Real(page_height)],
            "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image_id } },
            "Contents" => content_id,
        });
        kids.push(Object::Reference(page_id));
    }

    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => images.len() as i64,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document.compress();
    write_compressed(document)
}

fn image_matrix(orientation: u8, x: f32, y: f32, width: f32, height: f32) -> [f32; 6] {
    match orientation {
        2 => [-width, 0.0, 0.0, height, x + width, y],
        3 => [-width, 0.0, 0.0, -height, x + width, y + height],
        4 => [width, 0.0, 0.0, -height, x, y + height],
        5 => [0.0, -height, -width, 0.0, x + width, y + height],
        6 => [0.0, -height, width, 0.0, x, y + height],
        7 => [0.0, height, width, 0.0, x, y],
        8 => [0.0, height, -width, 0.0, x + width, y],
        _ => [width, 0.0, 0.0, height, x, y],
    }
}

fn prepare_image(bytes: &[u8]) -> Result<PreparedImage, String> {
    if bytes.starts_with(b"\xFF\xD8") {
        prepare_jpeg(bytes)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        prepare_png(bytes)
    } else {
        Err("Only JPG and PNG images are supported.".into())
    }
}

fn prepare_jpeg(bytes: &[u8]) -> Result<PreparedImage, String> {
    let mut decoder = JpegDecoder::new(ZCursor::new(bytes));
    decoder
        .decode_headers()
        .map_err(|_| "The JPG header is invalid.")?;
    let info = decoder.info().ok_or("The JPG has no image data.")?;
    if info.width == 0 || info.height == 0 {
        return Err("The JPG has no image data.".into());
    }
    let (content, colorspace) = match info.components {
        1 => (bytes.to_vec(), "DeviceGray"),
        3 => (bytes.to_vec(), "DeviceRGB"),
        4 => {
            decoder.set_options(decoder.options().jpeg_set_out_colorspace(ColorSpace::RGB));
            if decoder.output_buffer_size().unwrap_or(usize::MAX) > MAX_DECODED_IMAGE {
                return Err("The JPG is too large to decode safely.".into());
            }
            let pixels = decoder
                .decode()
                .map_err(|_| "The JPG could not be decoded.")?;
            let encoded = encode_jpeg(&pixels, info.width, info.height, PixelColors::Rgb, 95)
                .ok_or("The JPG could not be converted to RGB.")?;
            (encoded, "DeviceRGB")
        }
        _ => return Err("The JPG color format is unsupported.".into()),
    };
    Ok(PreparedImage {
        stream: Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => info.width as i64,
                "Height" => info.height as i64,
                "ColorSpace" => colorspace,
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            content,
        ),
        mask: None,
        width: info.width.into(),
        height: info.height.into(),
        orientation: jpeg_orientation(bytes),
    })
}

fn prepare_png(bytes: &[u8]) -> Result<PreparedImage, String> {
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: MAX_DECODED_IMAGE,
        },
    );
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|_| "The PNG is invalid.")?;
    let size = reader
        .output_buffer_size()
        .filter(|size| *size <= MAX_DECODED_IMAGE)
        .ok_or("The PNG is too large to decode safely.")?;
    let mut pixels = vec![0; size];
    let frame = reader
        .next_frame(&mut pixels)
        .map_err(|_| "The PNG could not be decoded.")?;
    let pixels = &pixels[..frame.buffer_size()];
    let (colorspace, components, alpha) = match frame.color_type {
        png::ColorType::Grayscale => ("DeviceGray", 1, false),
        png::ColorType::Rgb => ("DeviceRGB", 3, false),
        png::ColorType::GrayscaleAlpha => ("DeviceGray", 1, true),
        png::ColorType::Rgba => ("DeviceRGB", 3, true),
        png::ColorType::Indexed => return Err("The PNG palette could not be expanded.".into()),
    };
    let (image_pixels, mask) = if alpha {
        let mut image_pixels = Vec::with_capacity(pixels.len() / (components + 1) * components);
        let mut mask = Vec::with_capacity(pixels.len() / (components + 1));
        for pixel in pixels.chunks_exact(components + 1) {
            image_pixels.extend_from_slice(&pixel[..components]);
            mask.push(pixel[components]);
        }
        let mask = mask.iter().any(|alpha| *alpha != 255).then_some(mask);
        (image_pixels, mask)
    } else {
        (pixels.to_vec(), None)
    };
    let mask = mask
        .map(|bytes| flate_image_stream(bytes, frame.width, frame.height, "DeviceGray", true))
        .transpose()?;
    Ok(PreparedImage {
        stream: flate_image_stream(image_pixels, frame.width, frame.height, colorspace, false)?,
        mask,
        width: frame.width,
        height: frame.height,
        orientation: 1,
    })
}

fn flate_image_stream(
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    colorspace: &str,
    mask: bool,
) -> Result<Stream, String> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&pixels)
        .map_err(|_| "The PNG could not be compressed.")?;
    let compressed = encoder
        .finish()
        .map_err(|_| "The PNG could not be compressed.")?;
    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => width as i64,
        "Height" => height as i64,
        "ColorSpace" => colorspace,
        "BitsPerComponent" => 8,
        "Filter" => "FlateDecode",
    };
    if mask {
        dict.set("Decode", vec![Object::Integer(0), Object::Integer(1)]);
    }
    Ok(Stream::new(dict, compressed))
}

pub(crate) fn jpeg_orientation(bytes: &[u8]) -> u8 {
    let mut offset = 2;
    while offset + 4 <= bytes.len() && bytes[offset] == 0xFF {
        let marker = bytes[offset + 1];
        if marker == 0xDA || marker == 0xD9 {
            break;
        }
        if marker == 0x00 || marker == 0xFF {
            offset += 1;
            continue;
        }
        let length = u16::from_be_bytes([bytes[offset + 2], bytes[offset + 3]]) as usize;
        if length < 2 || offset + 2 + length > bytes.len() {
            break;
        }
        if marker == 0xE1 {
            let segment = &bytes[offset + 4..offset + 2 + length];
            if let Some(orientation) = exif_orientation(segment) {
                return orientation;
            }
        }
        offset += 2 + length;
    }
    1
}

fn exif_orientation(segment: &[u8]) -> Option<u8> {
    let tiff = segment.strip_prefix(b"Exif\0\0")?;
    let little_endian = match tiff.get(..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let read_u16 = |offset: usize| {
        let bytes: [u8; 2] = tiff.get(offset..offset.checked_add(2)?)?.try_into().ok()?;
        Some(if little_endian {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        })
    };
    let read_u32 = |offset: usize| {
        let bytes: [u8; 4] = tiff.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
        Some(if little_endian {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    };
    if read_u16(2)? != 42 {
        return None;
    }
    let ifd = usize::try_from(read_u32(4)?).ok()?;
    let count = usize::from(read_u16(ifd)?);
    for index in 0..count {
        let entry = ifd.checked_add(2)?.checked_add(index.checked_mul(12)?)?;
        if read_u16(entry)? == 0x0112 && read_u16(entry + 2)? == 3 && read_u32(entry + 4)? == 1 {
            let value = read_u16(entry + 8)?;
            return (1..=8).contains(&value).then_some(value as u8);
        }
    }
    None
}
