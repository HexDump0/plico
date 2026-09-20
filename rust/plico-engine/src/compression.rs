use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use crate::documents::{MAX_DECOMPRESSED_STREAM, load_document};
use flate2::{Compression, write::ZlibEncoder};
use lopdf::{Dictionary, Document, Object, ObjectId, SaveOptions};

pub(crate) mod image_transcode;
pub(crate) use image_transcode::{PixelColors, encode_jpeg};
use image_transcode::{transcode_flate_images, transcode_jpeg_images};

#[derive(Clone, Copy)]
pub struct CompressOptions {
    pub reflate: bool,
    pub image_quality: u8,
    pub max_image_dimension: u32,
    pub remove_metadata: bool,
    pub remove_thumbnails: bool,
}

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
    deduplicate_streams(&mut document);
    document.prune_objects();

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

pub(crate) fn write_compressed(mut document: Document) -> Result<Vec<u8>, String> {
    // Object streams pack non-stream objects (page dictionaries, annotations,
    // font metadata) into one compressed blob, the main size win after stream
    // recompression. Object streams need PDF 1.5; the writer raises the header
    // version itself. For small documents the added structure can outweigh the
    // savings, so both layouts are written and the smaller one ships.
    let modern = SaveOptions::builder()
        .use_object_streams(true)
        .use_xref_streams(true)
        .max_objects_per_stream(200)
        .compression_level(9)
        .build();

    let mut packed = Vec::new();
    let mut plain = Vec::new();
    let mut doc_plain = document.clone();
    document
        .save_with_options(&mut packed, modern)
        .and_then(|_| doc_plain.save_to(&mut plain))
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
/// result only when it is smaller. Standard zlib decoding unpacks only the
/// outer compression layer without undoing any internal predictor filtering,
/// so streams with DecodeParms predictors can be safely recompressed without
/// altering their decoded pixel representation.
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
        if !stream.allows_compression || stream.content.len() < 64 {
            continue;
        }
        let Ok(decoded) = decompress_zlib_stream(&stream.content, MAX_DECOMPRESSED_STREAM) else {
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

fn decompress_zlib_stream(compressed: &[u8], limit: usize) -> Result<Vec<u8>, ()> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;

    if compressed.is_empty() {
        return Err(());
    }

    let mut output = Vec::new();
    let decoder = ZlibDecoder::new(compressed);
    let read_result = decoder.take((limit as u64) + 1).read_to_end(&mut output);

    if read_result.is_ok() && output.len() <= limit && !output.is_empty() {
        return Ok(output);
    }

    Err(())
}

pub(crate) fn deflate_best(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder
        .write_all(data)
        .and_then(|_| encoder.finish())
        .map_err(|error| format!("A stream could not be compressed: {error}"))
}

fn is_single_flate(dict: &Dictionary) -> bool {
    filter_matches(dict, b"FlateDecode")
}

/// Matches /Filter when it names the filter directly or lists it alone.
pub(crate) fn filter_matches(dict: &Dictionary, name: &[u8]) -> bool {
    match dict.get(b"Filter") {
        Ok(Object::Name(filter)) => filter == name,
        Ok(Object::Array(items)) => {
            items.len() == 1 && items[0].as_name().is_ok_and(|filter| filter == name)
        }
        _ => false,
    }
}

/// Finds stream objects with identical dictionary attributes (ignoring /Length)
/// and identical content bytes, replacing references to redundant copies with
/// a single canonical object id.
fn deduplicate_streams(document: &mut Document) -> usize {
    let stream_ids: Vec<ObjectId> = document
        .objects
        .iter()
        .filter_map(|(id, object)| {
            let Object::Stream(stream) = object else {
                return None;
            };
            if stream
                .dict
                .get(b"Type")
                .and_then(Object::as_name)
                .is_ok_and(|name| name == b"ObjStm" || name == b"XRef")
            {
                return None;
            }
            Some(*id)
        })
        .collect();

    let mut by_len: BTreeMap<usize, Vec<ObjectId>> = BTreeMap::new();
    for id in stream_ids {
        if let Some(Object::Stream(stream)) = document.objects.get(&id) {
            by_len.entry(stream.content.len()).or_default().push(id);
        }
    }

    let mut replacements: BTreeMap<ObjectId, ObjectId> = BTreeMap::new();
    for (_len, group) in by_len {
        if group.len() < 2 {
            continue;
        }
        let mut canonical: Vec<ObjectId> = Vec::new();
        for id in group {
            let Some(Object::Stream(current)) = document.objects.get(&id) else {
                continue;
            };
            let mut found_canon = None;
            for &canon_id in &canonical {
                let Some(Object::Stream(canon)) = document.objects.get(&canon_id) else {
                    continue;
                };
                if current.content == canon.content
                    && stream_dicts_match(&current.dict, &canon.dict)
                {
                    found_canon = Some(canon_id);
                    break;
                }
            }
            if let Some(target) = found_canon {
                replacements.insert(id, target);
            } else {
                canonical.push(id);
            }
        }
    }

    if replacements.is_empty() {
        return 0;
    }

    let count = replacements.len();
    for object in document.objects.values_mut() {
        replace_references(object, &replacements);
    }
    replace_references_in_dict(&mut document.trailer, &replacements);

    for id in replacements.keys() {
        document.objects.remove(id);
    }

    count
}

fn stream_dicts_match(a: &Dictionary, b: &Dictionary) -> bool {
    let a_keys: BTreeSet<_> = a
        .iter()
        .map(|(k, _)| k.as_slice())
        .filter(|k| *k != b"Length")
        .collect();
    let b_keys: BTreeSet<_> = b
        .iter()
        .map(|(k, _)| k.as_slice())
        .filter(|k| *k != b"Length")
        .collect();
    if a_keys != b_keys {
        return false;
    }
    for key in a_keys {
        if a.get(key).ok() != b.get(key).ok() {
            return false;
        }
    }
    true
}

fn replace_references(object: &mut Object, replacements: &BTreeMap<ObjectId, ObjectId>) {
    match object {
        Object::Reference(id) => {
            if let Some(target) = replacements.get(id) {
                *id = *target;
            }
        }
        Object::Array(items) => {
            for item in items {
                replace_references(item, replacements);
            }
        }
        Object::Dictionary(dict) => replace_references_in_dict(dict, replacements),
        Object::Stream(stream) => replace_references_in_dict(&mut stream.dict, replacements),
        _ => {}
    }
}

fn replace_references_in_dict(dict: &mut Dictionary, replacements: &BTreeMap<ObjectId, ObjectId>) {
    for (_, value) in dict.iter_mut() {
        replace_references(value, replacements);
    }
}
