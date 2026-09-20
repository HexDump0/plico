use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use jpeg_encoder::{ChromaSubsamplingMethod, ColorType, Encoder as JpegEncoder};
use js_sys::{Array, Uint8Array};
use lopdf::{Dictionary, Document, LoadOptions, Object, ObjectId, SaveOptions, dictionary};
use wasm_bindgen::prelude::*;
use zune_core::bytestream::ZCursor;
use zune_jpeg::{JpegDecoder, zune_core::colorspace::ColorSpace};

/// The attributes a page may omit and inherit from an ancestor node in the page tree.
/// (ISO 32000-1, 7.7.3.4 lists these four)
const INHERITABLE_PAGE_KEYS: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// Catalog entries that describe the first document's page set and would be
/// wrong, not merely incomplete, once the other documents' pages are appended.
const STALE_CATALOG_KEYS: [&[u8]; 6] = [
    // One outline tree per input. Re-rooting them under a synthetic parent is a
    // separate feature; a tree covering only the first input is worse than none.
    b"Outlines",
    // Appended pages carry /StructParents pointing into structure trees that did
    // not survive, so the merged file is untagged whatever this claims.
    b"StructTreeRoot",
    b"MarkInfo",
    // Numbers only the first input's page range.
    b"PageLabels",
    // XMP describing the first input, including PDF/A conformance claims that
    // merging invalidates.
    b"Metadata",
    // Would override the header version computed below.
    b"Version",
];

/// Ceiling on how far any single stream may inflate while a document loads.
/// Object and cross-reference streams are decoded eagerly, so without a bound a
/// small file can exhaust wasm's address space before any of this code runs.
const MAX_DECOMPRESSED_STREAM: usize = 256 * 1024 * 1024;

/// Matches lopdf's own page tree traversal limit. Guards against a /Parent cycle
/// in a malformed file.
const MAX_PAGE_TREE_DEPTH: usize = 256;

pub fn merge_pdf_bytes(files: &[&[u8]]) -> Result<Vec<u8>, String> {
    if files.len() < 2 {
        return Err("Choose at least two PDFs to merge.".into());
    }

    let mut documents = Vec::with_capacity(files.len());
    for (index, bytes) in files.iter().enumerate() {
        documents.push(load_document(bytes, index + 1)?);
    }

    write_document(merge_documents(documents)?)
}

pub enum SplitMode<'a> {
    Ranges(&'a [(u32, u32)], bool),
    Every(u32),
}

pub fn split_pdf_bytes(input: &[u8], mode: SplitMode<'_>) -> Result<Vec<Vec<u8>>, String> {
    let document = load_document(input, 1)?;
    let page_count = document.get_pages().len() as u32;

    let groups = match mode {
        SplitMode::Ranges(ranges, combine) => {
            if ranges.is_empty() {
                return Err("Add at least one page range.".into());
            }
            let mut groups = Vec::with_capacity(ranges.len());
            for &(from, to) in ranges {
                if from == 0 || to < from || to > page_count {
                    return Err(format!("Page ranges must be between 1 and {page_count}."));
                }
                groups.push((from..=to).collect::<Vec<_>>());
            }
            if combine {
                let mut seen = BTreeSet::new();
                vec![
                    groups
                        .into_iter()
                        .flatten()
                        .filter(|page| seen.insert(*page))
                        .collect(),
                ]
            } else {
                groups
            }
        }
        SplitMode::Every(interval) => {
            if interval == 0 {
                return Err("Choose at least one page per PDF.".into());
            }
            (1..=page_count)
                .collect::<Vec<_>>()
                .chunks(interval as usize)
                .map(|chunk| chunk.to_vec())
                .collect()
        }
    };

    let mut source = Some(document);
    let mut outputs = Vec::with_capacity(groups.len());
    let count = groups.len();
    for (index, pages) in groups.into_iter().enumerate() {
        let document = if index + 1 == count {
            source.take().unwrap()
        } else {
            source.as_ref().unwrap().clone()
        };
        outputs.push(write_document(assemble_documents(vec![(
            document,
            Some(pages),
        )])?)?);
    }
    Ok(outputs)
}

/// Lossless stream recompression is always safe, so `reflate` is not optional.
/// `image_quality` trades image fidelity for size: 0 keeps every image byte.
/// `max_image_dimension` downscales images wider or taller than that many
/// pixels, preserving aspect; 0 keeps every size.
#[derive(Clone, Copy)]
pub struct CompressOptions {
    pub reflate: bool,
    pub image_quality: u8,
    pub max_image_dimension: u32,
    pub remove_metadata: bool,
    pub remove_thumbnails: bool,
}

/// Below this size a re-encoded JPEG rarely wins enough to justify the churn.
const MIN_TRANSCODE_JPEG: usize = 20 * 1024;
const MAX_DECODED_JPEG: usize = 256 * 1024 * 1024;

pub fn compress_pdf_bytes(input: &[u8], options: CompressOptions) -> Result<Vec<u8>, String> {
    let mut document = load_document(input, 1)?;

    let removed_metadata = options.remove_metadata && strip_metadata(&mut document);
    let removed_thumbnails = options.remove_thumbnails && strip_thumbnails(&mut document);

    // Reachability sweep from the trailer, so it has to run after /Root is set.
    // Collects everything the stripping above orphaned along with the source's
    // own object streams, which the writer re-packs into fresh ones.
    document.prune_objects();
    // Packs streams that carry no filter at all. lopdf only does this at its
    // best level and only when it saves over 19 bytes, which is the guard
    // against inflating incompressible data.
    document.compress();
    if options.reflate {
        recompress_flate_streams(&mut document);
    }
    if options.image_quality > 0 {
        transcode_jpeg_images(
            &mut document,
            options.image_quality,
            options.max_image_dimension,
        );
        transcode_flate_images(
            &mut document,
            options.image_quality,
            options.max_image_dimension,
        );
    }

    let output = write_compressed(document)?;
    // A rewrite that only repacks bytes should not make a file larger. When a
    // user explicitly removes metadata or thumbnails, keep that removal even
    // if the writer's overhead exceeds the bytes it saved.
    if output.len() < input.len() || removed_metadata || removed_thumbnails {
        Ok(output)
    } else {
        Ok(input.to_vec())
    }
}

fn write_compressed(mut document: Document) -> Result<Vec<u8>, String> {
    // Object streams pack non-stream objects (page dictionaries, annotations,
    // font metadata) into one compressed blob, the main size win after stream
    // recompression. Object streams need PDF 1.5; the writer raises the header
    // version itself. For small documents the added structure can outweigh the
    // savings, so both layouts are written and the smaller one ships.
    let modern = SaveOptions::builder()
        .use_object_streams(true)
        .use_xref_streams(true)
        .build();

    let mut packed = Vec::new();
    let mut plain = Vec::new();
    document
        .save_with_options(&mut packed, modern)
        .and_then(|_| document.save_to(&mut plain))
        .map_err(|error| format!("The PDF could not be created: {error}"))?;
    Ok(if packed.len() <= plain.len() {
        packed
    } else {
        plain
    })
}

fn strip_metadata(document: &mut Document) -> bool {
    let mut removed = false;
    // /Info carries document properties; removing the whole entry lets the
    // reachability sweep collect it. Trailing producer marks are not preserved.
    removed |= document.trailer.remove(b"Info").is_some();
    // XMP can be attached to the catalog, pages, images, or other objects.
    for object in document.objects.values_mut() {
        match object {
            Object::Dictionary(dict) => removed |= dict.remove(b"Metadata").is_some(),
            Object::Stream(stream) => removed |= stream.dict.remove(b"Metadata").is_some(),
            _ => {}
        }
    }
    removed
}

fn strip_thumbnails(document: &mut Document) -> bool {
    let mut removed = false;
    for page_id in document.get_pages().into_values() {
        if let Ok(page) = document.get_dictionary_mut(page_id) {
            removed |= page.remove(b"Thumb").is_some();
        }
    }
    removed
}

/// Re-deflates streams that are already FlateDecode-compressed, keeping the
/// result only when it is smaller. Streams with predictors are skipped: their
/// decoded bytes need the predictor reapplied to stay a legal re-encoding.
fn recompress_flate_streams(document: &mut Document) {
    let candidates: Vec<ObjectId> = document
        .objects
        .iter()
        .filter_map(|(id, object)| {
            let Object::Stream(stream) = object else {
                return None;
            };
            is_single_flate(&stream.dict).then_some(*id)
        })
        .collect();

    for id in candidates {
        let Some(Object::Stream(stream)) = document.objects.get_mut(&id) else {
            continue;
        };
        if !stream.allows_compression || stream.content.len() < 1024 {
            continue;
        }
        let Ok(decoded) = stream.decompressed_content_with_limit(MAX_DECOMPRESSED_STREAM) else {
            continue;
        };
        let Ok(packed) = deflate_best(&decoded) else {
            continue;
        };
        if packed.len() < stream.content.len() {
            stream.set_content(packed);
        }
    }
}

fn deflate_best(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder
        .write_all(data)
        .and_then(|_| encoder.finish())
        .map_err(|error| format!("A stream could not be compressed: {error}"))
}

fn is_single_flate(dict: &Dictionary) -> bool {
    filter_matches(dict, b"FlateDecode") && !dict.has(b"DecodeParms")
}

/// Matches /Filter when it names the filter directly or lists it alone.
fn filter_matches(dict: &Dictionary, name: &[u8]) -> bool {
    match dict.get(b"Filter") {
        Ok(Object::Name(filter)) => filter == name,
        Ok(Object::Array(items)) => {
            items.len() == 1 && items[0].as_name().is_ok_and(|filter| filter == name)
        }
        _ => false,
    }
}

/// DCTDecode sources are plain JPEG files, so they can be decoded, re-encoded
/// at a chosen quality and swapped back in without touching page content.
/// Anything else (CMYK, inverted /Decode values, predictor chains) is left as
/// it is; a wrong guess here would shift colours, not just sizes.
fn transcode_jpeg_images(document: &mut Document, quality: u8, max_dimension: u32) {
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
        let image_max_dimension = if stream.dict.has(b"SMask") || stream.dict.has(b"Mask") {
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
fn transcode_flate_images(document: &mut Document, quality: u8, max_dimension: u32) {
    let candidates: Vec<(ObjectId, PixelColors)> = document
        .objects
        .iter()
        .filter_map(|(id, object)| {
            let Object::Stream(stream) = object else {
                return None;
            };
            (is_flate_image(&stream.dict) && stream.content.len() >= MIN_TRANSCODE_JPEG)
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

#[derive(Clone, Copy)]
enum PixelColors {
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

fn is_jpeg_image(dict: &Dictionary) -> bool {
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

fn encode_jpeg(
    pixels: &[u8],
    width: u16,
    height: u16,
    colors: PixelColors,
    quality: u8,
) -> Option<Vec<u8>> {
    let mut encoded = Vec::new();
    let mut encoder = JpegEncoder::new(&mut encoded, quality.clamp(1, 100));
    encoder.set_chroma_subsampling_method(ChromaSubsamplingMethod::Average);
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

fn load_document(bytes: &[u8], position: usize) -> Result<Document, String> {
    let options = LoadOptions {
        max_decompressed_size: Some(MAX_DECOMPRESSED_STREAM),
        ..Default::default()
    };
    let mut document = Document::load_mem_with_options(bytes, options)
        .map_err(|error| format!("PDF {position} could not be read: {error}"))?;

    // Most "encrypted" PDFs only restrict permissions and open under an empty
    // user password, which is what viewers do silently. Only a document that
    // fails that is genuinely password protected.
    //
    // This has to happen before any renumbering: below /V 5 the encryption key
    // is derived from each object's number and generation.
    if document.is_encrypted() {
        document.decrypt("").map_err(|_| {
            format!("PDF {position} is password protected. Unlock it before continuing.")
        })?;
    }

    if document.get_pages().is_empty() {
        return Err(format!("PDF {position} has no pages."));
    }

    Ok(document)
}

fn merge_documents(documents: Vec<Document>) -> Result<Document, String> {
    assemble_documents(
        documents
            .into_iter()
            .map(|document| (document, None))
            .collect(),
    )
}

/// Rebuilds one page tree from the requested pages of each input. Merge selects
/// every page; Split selects only the pages in its output group.
fn assemble_documents(documents: Vec<(Document, Option<Vec<u32>>)>) -> Result<Document, String> {
    let version = documents
        .iter()
        .map(|(document, _)| document.version.as_str())
        .max_by_key(|version| parse_version(version))
        .unwrap_or("1.5")
        .to_owned();

    let mut output = Document::with_version(version);
    let mut next_id = 1;
    // A Vec, not a map keyed by object id: page order is the concatenation of
    // each input's page order, which is not the same sequence as its object id
    // order. Nothing requires a page tree to list its pages in id order.
    let mut pages: Vec<(ObjectId, Dictionary)> = Vec::new();
    let mut catalog: Option<Dictionary> = None;

    for (mut document, selection) in documents {
        // Page order and inherited attributes both have to be read while this
        // document's own page tree is still intact, so before any renumbering.
        let available = document.get_pages();
        let page_order = if let Some(numbers) = selection {
            numbers
                .into_iter()
                .map(|number| {
                    available
                        .get(&number)
                        .copied()
                        .ok_or_else(|| format!("Page {number} could not be read."))
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            available.into_values().collect::<Vec<_>>()
        };
        let inherited = page_order
            .iter()
            .map(|page_id| {
                let page = document
                    .get_dictionary(*page_id)
                    .map_err(|error| format!("A PDF page could not be read: {error}"))?;
                Ok(inheritable_attributes(&document, page))
            })
            .collect::<Result<Vec<_>, String>>()?;

        for (page_id, attributes) in page_order.iter().zip(inherited) {
            let Ok(page) = document.get_dictionary_mut(*page_id) else {
                continue;
            };
            for (key, value) in attributes {
                page.set(key, value);
            }
        }

        // Moves this document into an id range disjoint from every document
        // already merged, rewriting its internal references to match.
        let moved = renumber(&mut document, next_id);
        // One past the reserved id that dangling references were pointed at.
        next_id += moved.len() as u32 + 1;

        if catalog.is_none() {
            catalog = document.catalog().ok().cloned();
        }

        for page_id in &page_order {
            let page_id = moved[page_id];
            let page = document
                .get_dictionary(page_id)
                .map_err(|error| format!("A PDF page could not be read: {error}"))?
                .clone();
            pages.push((page_id, page));
        }

        output.objects.extend(document.objects);
    }

    let pages_id = (next_id, 0);
    let catalog_id = (next_id + 1, 0);
    let info_id = (next_id + 2, 0);
    output.max_id = next_id + 2;

    let kids: Vec<Object> = pages.iter().map(|(id, _)| Object::Reference(*id)).collect();
    let count = kids.len() as u32;

    // Applied after the bulk copy above, which inserted the unmodified originals.
    for (page_id, mut page) in pages {
        page.set("Parent", pages_id);
        output.objects.insert(page_id, Object::Dictionary(page));
    }

    output.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => count,
        }),
    );

    // The first input's catalog carries document-wide settings worth keeping,
    // such as /Lang and /ViewerPreferences. The entries that described only its
    // own page set are dropped rather than left dangling.
    let mut catalog = catalog.unwrap_or_default();
    for key in STALE_CATALOG_KEYS {
        catalog.remove(key);
    }
    catalog.set("Type", "Catalog");
    catalog.set("Pages", pages_id);
    output
        .objects
        .insert(catalog_id, Object::Dictionary(catalog));

    output.objects.insert(
        info_id,
        Object::Dictionary(dictionary! {
            "Producer" => Object::string_literal("Plico"),
        }),
    );

    output.trailer.set("Root", catalog_id);
    output.trailer.set("Info", info_id);

    Ok(output)
}

/// Resolves the inheritable attributes a page does not define itself by walking
/// the page's /Parent chain, so they can be written onto the page.
///
/// Merging reparents every page under one synthetic root, which puts the
/// ancestors these attributes would have been read from out of reach. Without
/// this, a page that inherited /MediaBox silently adopts whatever the merged
/// root carries instead, or loses it entirely and becomes non-conformant.
fn inheritable_attributes(document: &Document, page: &Dictionary) -> Vec<(&'static [u8], Object)> {
    let mut found: Vec<(&'static [u8], Object)> = Vec::new();
    let mut ancestor = page.get(b"Parent").and_then(Object::as_reference).ok();

    for _ in 0..MAX_PAGE_TREE_DEPTH {
        let Some(id) = ancestor else { break };
        let Ok(node) = document.get_dictionary(id) else {
            break;
        };

        for key in INHERITABLE_PAGE_KEYS {
            let known = page.has(key) || found.iter().any(|(seen, _)| *seen == key);
            if !known && let Ok(value) = node.get(key) {
                found.push((key, value.clone()));
            }
        }

        if INHERITABLE_PAGE_KEYS
            .iter()
            .all(|key| page.has(key) || found.iter().any(|(seen, _)| seen == key))
        {
            break;
        }

        ancestor = node.get(b"Parent").and_then(Object::as_reference).ok();
    }

    found
}

/// Moves every object into `[start, start + count)`, rewriting references to
/// match, and reports where each one landed.
///
/// lopdf's `renumber_objects_with` leaves references to objects that do not
/// exist alone, so once the surviving ids are compacted a dangling reference can
/// come to point at an unrelated object. A corpus file whose page tree lists a
/// missing kid turns into a page tree that lists *itself*, and the cycle costs
/// every page in the document. Here an id that resolves to nothing is pointed at
/// a reserved id that is never populated, leaving it dangling, which is what it
/// already was and what the spec reads as null.
fn renumber(document: &mut Document, start: u32) -> BTreeMap<ObjectId, ObjectId> {
    let moved = document
        .objects
        .keys()
        .enumerate()
        .map(|(index, id)| (*id, (start + index as u32, 0)))
        .collect::<BTreeMap<_, _>>();
    let unresolved = (start + moved.len() as u32, 0);

    let mut objects = std::mem::take(&mut document.objects);
    for object in objects.values_mut() {
        remap_object(object, &moved, unresolved);
    }
    document.objects = objects
        .into_iter()
        .map(|(id, object)| (moved[&id], object))
        .collect();

    if let Ok(root) = document.trailer.get(b"Root").and_then(Object::as_reference)
        && let Some(id) = moved.get(&root)
    {
        document.trailer.set("Root", *id);
    }
    document.max_id = start + moved.len() as u32;

    moved
}

fn remap_object(object: &mut Object, moved: &BTreeMap<ObjectId, ObjectId>, unresolved: ObjectId) {
    match object {
        Object::Reference(id) => *id = moved.get(id).copied().unwrap_or(unresolved),
        Object::Array(items) => {
            for item in items {
                remap_object(item, moved, unresolved);
            }
        }
        Object::Dictionary(dictionary) => remap_dictionary(dictionary, moved, unresolved),
        Object::Stream(stream) => remap_dictionary(&mut stream.dict, moved, unresolved),
        _ => {}
    }
}

fn remap_dictionary(
    dictionary: &mut Dictionary,
    moved: &BTreeMap<ObjectId, ObjectId>,
    unresolved: ObjectId,
) {
    for (_, value) in dictionary.iter_mut() {
        remap_object(value, moved, unresolved);
    }
}

/// Orders (major, minor) properly. Comparing the strings happens to work across
/// today's versions but would rank "1.10" below "1.5".
fn parse_version(version: &str) -> (u32, u32) {
    let (major, minor) = version.split_once('.').unwrap_or((version, "0"));
    (major.parse().unwrap_or(1), minor.parse().unwrap_or(0))
}

fn write_document(mut document: Document) -> Result<Vec<u8>, String> {
    // Reachability sweep from the trailer, so it has to run after /Root is set.
    // Collects each input's superseded catalog and page tree nodes, along with
    // the outline items and name trees that only those referenced.
    document.prune_objects();
    document.renumber_objects();
    document.compress();

    let mut bytes = Vec::new();
    document
        .save_to(&mut bytes)
        .map_err(|error| format!("The PDF could not be created: {error}"))?;
    Ok(bytes)
}

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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lopdf::{Document, Object, Stream, dictionary};

    use super::{
        CompressOptions, SplitMode, compress_pdf_bytes, filter_matches, is_jpeg_image,
        merge_pdf_bytes, parse_version, split_pdf_bytes,
    };

    fn one_page_pdf(label: &str) -> Vec<u8> {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let content_id =
            document.add_object(Stream::new(dictionary! {}, label.as_bytes().to_vec()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        bytes
    }

    /// Puts /MediaBox and /Resources on the page tree node rather than on the
    /// page, which is legal and is what most report generators emit.
    fn inheriting_pdf(label: &str, width: i64) -> Vec<u8> {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let content_id =
            document.add_object(Stream::new(dictionary! {}, label.as_bytes().to_vec()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
                "MediaBox" => vec![0.into(), 0.into(), width.into(), 800.into()],
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        bytes
    }

    fn numbered_pdf(count: u32) -> Vec<u8> {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let mut kids = Vec::new();
        for number in 1..=count {
            let content_id =
                document.add_object(Stream::new(dictionary! {}, number.to_string().into_bytes()));
            let page_id = document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
            });
            kids.push(Object::Reference(page_id));
        }
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => count,
                "MediaBox" => vec![0.into(), 0.into(), 400.into(), 600.into()],
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        bytes
    }

    fn page_contents(bytes: &[u8]) -> Vec<String> {
        let document = Document::load_mem(bytes).unwrap();
        document
            .get_pages()
            .into_values()
            .map(|page| {
                String::from_utf8_lossy(&document.get_page_content(page))
                    .trim()
                    .to_owned()
            })
            .collect()
    }

    fn page_widths(document: &Document) -> Vec<i64> {
        document
            .get_pages()
            .into_values()
            .map(|id| {
                document
                    .get_dictionary(id)
                    .and_then(|page| page.get(b"MediaBox"))
                    .and_then(Object::as_array)
                    .map(|media_box| media_box[2].as_i64().unwrap_or(-1))
                    .unwrap_or(-1)
            })
            .collect()
    }

    #[test]
    fn merges_pages_in_input_order() {
        let first = one_page_pdf("first");
        let second = one_page_pdf("second");
        let merged = merge_pdf_bytes(&[&first, &second]).unwrap();
        let document = Document::load_mem(&merged).unwrap();
        let pages = document.get_pages();

        assert_eq!(pages.len(), 2);
        let content = pages
            .into_values()
            .map(|page| document.get_page_content(page))
            .collect::<Vec<_>>();
        assert_eq!(content, [b"first\n".to_vec(), b"second\n".to_vec()]);
    }

    #[test]
    fn rejects_invalid_input() {
        let error = merge_pdf_bytes(&[b"not a pdf", b"also not a pdf"]).unwrap_err();
        assert!(error.starts_with("PDF 1 could not be read:"));
    }

    #[test]
    fn requires_two_documents() {
        let pdf = one_page_pdf("only");
        assert_eq!(
            merge_pdf_bytes(&[&pdf]).unwrap_err(),
            "Choose at least two PDFs to merge."
        );
    }

    #[test]
    fn keeps_each_page_geometry_when_inherited() {
        let narrow = inheriting_pdf("narrow", 200);
        let wide = inheriting_pdf("wide", 600);
        let merged = merge_pdf_bytes(&[&narrow, &wide]).unwrap();
        let document = Document::load_mem(&merged).unwrap();

        assert_eq!(page_widths(&document), vec![200, 600]);
    }

    #[test]
    fn keeps_inherited_resources() {
        let merged = merge_pdf_bytes(&[&one_page_pdf("a"), &inheriting_pdf("b", 300)]).unwrap();
        let document = Document::load_mem(&merged).unwrap();

        let second = document.get_pages()[&2];
        let resources = document
            .get_dictionary(second)
            .and_then(|page| page.get_deref(b"Resources", &document))
            .and_then(Object::as_dict)
            .expect("appended page kept the resources it inherited");
        assert!(resources.has(b"Font"));
    }

    /// /MediaBox is required. The merged root carries no default, so every page
    /// has to resolve one on its own.
    #[test]
    fn every_page_defines_its_own_mediabox() {
        let merged = merge_pdf_bytes(&[&one_page_pdf("a"), &inheriting_pdf("b", 612)]).unwrap();
        let document = Document::load_mem(&merged).unwrap();

        assert_eq!(page_widths(&document), vec![200, 612]);
    }

    #[test]
    fn drops_unreachable_objects() {
        let merged = merge_pdf_bytes(&[&one_page_pdf("a"), &one_page_pdf("b")]).unwrap();
        let mut document = Document::load_mem(&merged).unwrap();

        let types = document
            .objects
            .iter()
            .map(|(id, object)| (*id, object.type_name().unwrap_or(b"").to_vec()))
            .collect::<BTreeMap<_, _>>();
        // startxref reaches the cross-reference stream, not the object graph, so
        // the reader always leaves that one looking unreferenced.
        let leaked = document
            .prune_objects()
            .into_iter()
            .filter(|id| types[id] != b"XRef")
            .collect::<Vec<_>>();

        assert!(leaked.is_empty(), "unreachable objects kept: {leaked:?}");
    }

    /// Page order has to come from the page tree, not from object numbering.
    #[test]
    fn keeps_page_order_when_ids_run_backwards() {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        // Allocate the second page first, so it takes the lower object number.
        let second_content = document.add_object(Stream::new(dictionary! {}, b"second".to_vec()));
        let second = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => second_content,
            "MediaBox" => vec![0.into(), 0.into(), 300.into(), 300.into()],
        });
        let first_content = document.add_object(Stream::new(dictionary! {}, b"first".to_vec()));
        let first = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => first_content,
            "MediaBox" => vec![0.into(), 0.into(), 300.into(), 300.into()],
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![first.into(), second.into()],
                "Count" => 2,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        let mut reversed = Vec::new();
        document.save_to(&mut reversed).unwrap();

        let merged = merge_pdf_bytes(&[&reversed, &one_page_pdf("third")]).unwrap();
        let document = Document::load_mem(&merged).unwrap();
        let content = document
            .get_pages()
            .into_values()
            .map(|page| {
                String::from_utf8_lossy(&document.get_page_content(page))
                    .trim()
                    .to_owned()
            })
            .collect::<Vec<_>>();

        assert_eq!(content, ["first", "second", "third"]);
    }

    /// A page tree listing a kid that was never written. Compacting object ids
    /// must not let that reference drift onto a real object: pointed at the page
    /// tree itself it forms a cycle, and every page in the document disappears.
    #[test]
    fn survives_a_page_tree_kid_that_does_not_exist() {
        let mut document = Document::with_version("1.4");
        let pages_id = document.new_object_id();
        let content_id = document.add_object(Stream::new(dictionary! {}, b"real".to_vec()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                // (9, 0) is never written.
                "Kids" => vec![Object::Reference((9, 0)), page_id.into()],
                "Count" => 2,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        let mut broken = Vec::new();
        document.save_to(&mut broken).unwrap();

        let merged = merge_pdf_bytes(&[&broken, &one_page_pdf("second")]).unwrap();
        let document = Document::load_mem(&merged).unwrap();

        assert_eq!(document.get_pages().len(), 2);
    }

    #[test]
    fn merges_more_than_two_documents() {
        let files = [
            one_page_pdf("a"),
            one_page_pdf("b"),
            one_page_pdf("c"),
            one_page_pdf("d"),
        ];
        let refs = files.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let merged = merge_pdf_bytes(&refs).unwrap();

        assert_eq!(Document::load_mem(&merged).unwrap().get_pages().len(), 4);
    }

    #[test]
    fn splits_ranges_into_separate_pdfs() {
        let outputs = split_pdf_bytes(
            &numbered_pdf(5),
            SplitMode::Ranges(&[(2, 3), (5, 5)], false),
        )
        .unwrap();

        assert_eq!(outputs.len(), 2);
        assert_eq!(page_contents(&outputs[0]), ["2", "3"]);
        assert_eq!(page_contents(&outputs[1]), ["5"]);

        let document = Document::load_mem(&outputs[0]).unwrap();
        assert_eq!(page_widths(&document), vec![400, 400]);
        let first = document.get_pages()[&1];
        let resources = document
            .get_dictionary(first)
            .and_then(|page| page.get_deref(b"Resources", &document))
            .and_then(Object::as_dict)
            .unwrap();
        assert!(resources.has(b"Font"));
    }

    #[test]
    fn combined_ranges_keep_order_and_include_overlaps_once() {
        let outputs =
            split_pdf_bytes(&numbered_pdf(5), SplitMode::Ranges(&[(3, 4), (1, 3)], true)).unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(page_contents(&outputs[0]), ["3", "4", "1", "2"]);
    }

    #[test]
    fn splits_every_n_pages_with_a_short_final_part() {
        let outputs = split_pdf_bytes(&numbered_pdf(5), SplitMode::Every(2)).unwrap();

        assert_eq!(outputs.len(), 3);
        assert_eq!(page_contents(&outputs[0]), ["1", "2"]);
        assert_eq!(page_contents(&outputs[1]), ["3", "4"]);
        assert_eq!(page_contents(&outputs[2]), ["5"]);
    }

    #[test]
    fn split_rejects_invalid_ranges_and_intervals() {
        let input = numbered_pdf(3);
        assert!(split_pdf_bytes(&input, SplitMode::Ranges(&[], false)).is_err());
        assert!(split_pdf_bytes(&input, SplitMode::Ranges(&[(0, 2)], false)).is_err());
        assert!(split_pdf_bytes(&input, SplitMode::Ranges(&[(3, 2)], false)).is_err());
        assert!(split_pdf_bytes(&input, SplitMode::Ranges(&[(1, 4)], false)).is_err());
        assert!(split_pdf_bytes(&input, SplitMode::Every(0)).is_err());
    }

    #[test]
    fn split_output_drops_unreachable_pages_and_objects() {
        let outputs =
            split_pdf_bytes(&numbered_pdf(5), SplitMode::Ranges(&[(3, 3)], false)).unwrap();
        let mut document = Document::load_mem(&outputs[0]).unwrap();
        let types = document
            .objects
            .iter()
            .map(|(id, object)| (*id, object.type_name().unwrap_or(b"").to_vec()))
            .collect::<BTreeMap<_, _>>();
        let leaked = document
            .prune_objects()
            .into_iter()
            .filter(|id| types[id] != b"XRef")
            .collect::<Vec<_>>();

        assert_eq!(page_contents(&outputs[0]), ["3"]);
        assert!(leaked.is_empty(), "unreachable objects kept: {leaked:?}");
    }

    fn jpeg_bytes(width: u16, height: u16, quality: u8) -> Vec<u8> {
        // A smooth gradient encodes compactly; scale it to spread the data.
        let components = 3usize;
        let mut pixels = Vec::with_capacity(width as usize * height as usize * components);
        for y in 0..height {
            for x in 0..width {
                pixels.push((x as u32 * 3 % 256) as u8);
                pixels.push((y as u32 * 7 % 256) as u8);
                pixels.push((x as u32 + y as u32) as u8);
            }
        }
        let mut encoded = Vec::new();
        let mut encoder = jpeg_encoder::Encoder::new(&mut encoded, quality);
        encoder.set_chroma_subsampling_method(jpeg_encoder::ChromaSubsamplingMethod::Nearest);
        encoder
            .encode(&pixels, width, height, jpeg_encoder::ColorType::Rgb)
            .unwrap();
        encoded
    }

    /// A page whose only content is one image XObject holding the given JPEG.
    fn jpeg_pdf(jpeg: &[u8], width: i64, height: i64) -> Vec<u8> {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let image_id = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => width,
                "Height" => height,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            jpeg.to_vec(),
        ));
        let content_id = document.add_object(Stream::new(
            dictionary! {},
            format!("q {width} 0 0 {height} 0 0 cm /Im1 Do Q").into_bytes(),
        ));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
            "Resources" => dictionary! { "XObject" => dictionary! { "Im1" => image_id } },
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        bytes
    }

    fn compress_options(
        image_quality: u8,
        max_image_dimension: u32,
        remove_metadata: bool,
        remove_thumbnails: bool,
    ) -> CompressOptions {
        CompressOptions {
            reflate: true,
            image_quality,
            max_image_dimension,
            remove_metadata,
            remove_thumbnails,
        }
    }

    /// The stream holding the first page's decoded content after a compress.
    fn image_stream(document: &Document) -> (i64, i64, &[u8]) {
        let page = document.get_pages()[&1];
        let image = document
            .get_dictionary(page)
            .and_then(|page| page.get_deref(b"Resources", document))
            .and_then(|resources| resources.as_dict())
            .and_then(|resources| resources.get_deref(b"XObject", document))
            .and_then(|xobjects| xobjects.as_dict())
            .and_then(|xobjects| xobjects.get_deref(b"Im1", document))
            .and_then(|object| object.as_stream())
            .unwrap();
        let width = image.dict.get(b"Width").and_then(Object::as_i64).unwrap();
        let height = image.dict.get(b"Height").and_then(Object::as_i64).unwrap();
        (width, height, &image.content)
    }
    #[test]
    fn compress_packs_uncompressed_streams() {
        // lopdf only packs a stream when it saves meaningfully, so give the
        // content some bulk.
        let input = one_page_pdf(&"sample".repeat(400));
        let compressed = compress_pdf_bytes(&input, compress_options(0, 0, false, false)).unwrap();
        let document = Document::load_mem(&compressed).unwrap();
        let pages = document.get_pages();

        assert_eq!(pages.len(), 1);
        assert_eq!(
            document.get_page_content(pages[&1]),
            format!("{}\n", "sample".repeat(400)).into_bytes()
        );
        let has_flate = document.objects.values().any(|object| match object {
            Object::Stream(stream) => filter_is_flate(stream),
            _ => false,
        });
        assert!(
            has_flate,
            "content stream did not gain a FlateDecode filter"
        );
        assert!(compressed.len() <= input.len(), "a bulky PDF inflated");
    }

    fn filter_is_flate(stream: &Stream) -> bool {
        matches!(stream.dict.get(b"Filter"), Ok(Object::Name(filter)) if filter == b"FlateDecode")
    }

    #[test]
    fn compress_strips_metadata_and_thumbnails() {
        let mut document = Document::load_mem(&one_page_pdf("body")).unwrap();
        let metadata_id = document.add_object(Stream::new(
            dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
            b"<x/>".to_vec(),
        ));
        let thumb_id = document.add_object(Stream::new(dictionary! {}, vec![0u8; 512]));
        document.catalog_mut().unwrap().set("Metadata", metadata_id);
        let page_id = document.get_pages()[&1];
        document
            .get_dictionary_mut(page_id)
            .unwrap()
            .set("Thumb", thumb_id);
        let mut with_extras = Vec::new();
        document.save_to(&mut with_extras).unwrap();

        let compressed =
            compress_pdf_bytes(&with_extras, compress_options(0, 0, true, true)).unwrap();
        let mut document = Document::load_mem(&compressed).unwrap();

        assert!(document.catalog().unwrap().get(b"Metadata").is_err());
        assert!(document.trailer.get(b"Info").is_err());
        let page = document.get_pages()[&1];
        assert!(
            document
                .get_dictionary(page)
                .unwrap()
                .get(b"Thumb")
                .is_err()
        );

        let types = document
            .objects
            .iter()
            .map(|(id, object)| (*id, object.type_name().unwrap_or(b"").to_vec()))
            .collect::<BTreeMap<_, _>>();
        // startxref reaches the cross-reference stream, not the object graph,
        // so the reader always leaves the output's own XRef and ObjStm looking
        // unreferenced.
        let leaked = document
            .prune_objects()
            .into_iter()
            .filter(|id| types[id] != b"XRef" && types[id] != b"ObjStm")
            .collect::<Vec<_>>();
        assert!(leaked.is_empty(), "stripped streams kept: {leaked:?}");
    }

    #[test]
    fn compress_transcodes_jpeg_images() {
        let original = jpeg_bytes(600, 400, 100);
        let input = jpeg_pdf(&original, 600, 400);
        let compressed = compress_pdf_bytes(&input, compress_options(60, 0, false, false)).unwrap();
        let document = Document::load_mem(&compressed).unwrap();

        let (width, height, content) = image_stream(&document);
        assert_eq!((width, height), (600, 400));
        assert!(
            content.len() < original.len(),
            "re-encoded JPEG not smaller"
        );
        // The replacement still decodes as a JPEG with the same dimensions.
        let mut decoder = zune_jpeg::JpegDecoder::new(zune_core::bytestream::ZCursor::new(content));
        decoder.decode().unwrap();
        let info = decoder.info().unwrap();
        assert_eq!((info.width, info.height), (600, 400));
    }

    #[test]
    fn compress_downscales_oversized_images() {
        let original = jpeg_bytes(3000, 800, 95);
        let input = jpeg_pdf(&original, 3000, 800);
        let compressed =
            compress_pdf_bytes(&input, compress_options(80, 2000, false, false)).unwrap();
        let document = Document::load_mem(&compressed).unwrap();

        let (width, height, content) = image_stream(&document);
        assert_eq!((width, height), (2000, 533));
        let mut decoder = zune_jpeg::JpegDecoder::new(zune_core::bytestream::ZCursor::new(content));
        let pixels = decoder.decode().unwrap();
        assert_eq!(pixels.len(), 2000 * 533 * 3);
        assert_eq!(document.get_pages().len(), 1);
    }

    #[test]
    fn compress_downscales_grayscale_jpeg_without_panicking() {
        let width = 1500u16;
        let height = 1000u16;
        let pixels = (0..usize::from(width) * usize::from(height))
            .map(|index| ((index / usize::from(width) + index) % 256) as u8)
            .collect::<Vec<_>>();
        let mut jpeg = Vec::new();
        jpeg_encoder::Encoder::new(&mut jpeg, 95)
            .encode(&pixels, width, height, jpeg_encoder::ColorType::Luma)
            .unwrap();
        let mut source = Document::load_mem(&jpeg_pdf(&jpeg, width.into(), height.into())).unwrap();
        let image_id = source
            .objects
            .iter()
            .find_map(|(id, object)| {
                object
                    .as_stream()
                    .ok()
                    .filter(|stream| is_jpeg_image(&stream.dict))
                    .map(|_| *id)
            })
            .unwrap();
        source
            .get_object_mut(image_id)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .dict
            .set("ColorSpace", "DeviceGray");
        let mut input = Vec::new();
        source.save_to(&mut input).unwrap();

        let output = compress_pdf_bytes(&input, compress_options(60, 750, false, false)).unwrap();
        let result = Document::load_mem(&output).unwrap();
        assert_eq!(image_stream(&result).0, 750);
        assert_eq!(image_stream(&result).1, 500);
    }

    #[test]
    fn compress_converts_large_flate_photo_to_jpeg() {
        let jpeg = jpeg_bytes(600, 400, 100);
        let mut decoder = zune_jpeg::JpegDecoder::new(zune_core::bytestream::ZCursor::new(&jpeg));
        let pixels = decoder.decode().unwrap();
        let mut source = Document::load_mem(&jpeg_pdf(&jpeg, 600, 400)).unwrap();
        let image = source
            .objects
            .values_mut()
            .find_map(|object| {
                object
                    .as_stream_mut()
                    .ok()
                    .filter(|stream| is_jpeg_image(&stream.dict))
            })
            .unwrap();
        image.dict.remove(b"Filter");
        image.set_content(pixels);
        image.compress().unwrap();
        let original_stream_size = image.content.len();
        let mut input = Vec::new();
        source.save_to(&mut input).unwrap();

        let output = compress_pdf_bytes(&input, compress_options(60, 0, false, false)).unwrap();
        let result = Document::load_mem(&output).unwrap();
        let (_, _, content) = image_stream(&result);
        assert!(content.len() < original_stream_size);
        assert!(result.objects.values().any(|object| {
            object.as_stream().ok().is_some_and(|stream| {
                stream
                    .dict
                    .get(b"Subtype")
                    .is_ok_and(|value| value.as_name().is_ok_and(|name| name == b"Image"))
                    && filter_matches(&stream.dict, b"DCTDecode")
            })
        }));
    }

    #[test]
    fn compress_output_uses_object_streams() {
        // Enough dictionaries that packing them into object streams wins over
        // writing each one directly.
        let compressed =
            compress_pdf_bytes(&numbered_pdf(8), compress_options(0, 0, false, false)).unwrap();
        let document = Document::load_mem(&compressed).unwrap();

        assert!(
            document
                .objects
                .values()
                .any(|object| object.type_name().is_ok_and(|name| name == b"ObjStm")),
            "no object stream in output"
        );
        let (major, minor) = parse_version(&document.version);
        assert!((major, minor) >= (1, 5));
    }
    #[test]
    fn compress_never_inflates_a_small_document() {
        let input = one_page_pdf("small");
        let compressed = compress_pdf_bytes(&input, compress_options(0, 0, false, false)).unwrap();
        assert!(compressed.len() <= input.len());
    }

    #[test]
    fn compress_rejects_unreadable_input() {
        let error =
            compress_pdf_bytes(b"not a pdf", compress_options(0, 0, false, false)).unwrap_err();
        assert!(error.starts_with("PDF 1 could not be read:"));
    }

    #[test]
    fn orders_versions_numerically() {
        assert!(parse_version("1.10") > parse_version("1.5"));
        assert!(parse_version("2.0") > parse_version("1.7"));
        assert_eq!(parse_version("nonsense"), (1, 0));
    }
}
