use std::collections::{BTreeMap, BTreeSet};

use lopdf::{Dictionary, Document, LoadOptions, Object, ObjectId, dictionary};

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
pub(crate) const MAX_DECOMPRESSED_STREAM: usize = 256 * 1024 * 1024;

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

pub fn organize_pdf_bytes(input: &[u8], pages: &[(u32, i32)]) -> Result<Vec<u8>, String> {
    let mut document = load_document(input, 1)?;
    let available = document.get_pages();
    if pages.is_empty() {
        return Err("Keep at least one page in the PDF.".into());
    }

    let mut seen = BTreeSet::new();
    let mut order = Vec::with_capacity(pages.len());
    for &(number, turn) in pages {
        let Some(&page_id) = available.get(&number) else {
            return Err(format!("Page {number} could not be read."));
        };
        if !seen.insert(number) {
            return Err(format!("Page {number} was selected more than once."));
        }
        if !matches!(turn, 0 | 90 | 180 | 270) {
            return Err("Page rotations must be 0, 90, 180, or 270 degrees.".into());
        }
        if turn != 0 {
            let page = document
                .get_dictionary(page_id)
                .map_err(|error| format!("Page {number} could not be read: {error}"))?;
            let rotation = page
                .get_deref(b"Rotate", &document)
                .ok()
                .cloned()
                .or_else(|| {
                    inheritable_attributes(&document, page)
                        .into_iter()
                        .find(|(key, _)| *key == b"Rotate")
                        .map(|(_, value)| value)
                })
                .and_then(|value| match value {
                    Object::Reference(id) => document.get_object(id).ok()?.as_i64().ok(),
                    value => value.as_i64().ok(),
                })
                .unwrap_or(0);
            document
                .get_dictionary_mut(page_id)
                .map_err(|error| format!("Page {number} could not be read: {error}"))?
                .set("Rotate", (rotation + i64::from(turn)).rem_euclid(360));
        }
        order.push(number);
    }

    write_document(assemble_documents(vec![(document, Some(order))])?)
}

/// Lossless stream recompression is always safe, so `reflate` is not optional.
/// `image_quality` trades image fidelity for size: 0 keeps every image byte.
/// `max_image_dimension` downscales images wider or taller than that many
/// pixels, preserving aspect; 0 keeps every size.
pub(crate) fn load_document(bytes: &[u8], position: usize) -> Result<Document, String> {
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
/// every page; Split and Organize select pages in their output order.
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
pub(crate) fn parse_version(version: &str) -> (u32, u32) {
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
