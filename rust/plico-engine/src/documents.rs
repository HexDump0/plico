use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use lopdf::{Dictionary, Document, LoadOptions, Object, ObjectId, dictionary};
use md5::{Digest, Md5};

use crate::compression::packed_options;

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
pub(crate) const MAX_PAGE_TREE_DEPTH: usize = 256;

pub fn merge_pdf_bytes(files: &[&[u8]]) -> Result<Vec<u8>, String> {
    merge_pdf_bytes_with_passwords(files, &[])
}

/// `passwords` pairs with `files` by index; a missing or empty entry means the
/// file opens without one.
pub fn merge_pdf_bytes_with_passwords(
    files: &[&[u8]],
    passwords: &[&str],
) -> Result<Vec<u8>, String> {
    merge_pdf_bytes_with_options(files, passwords, None)
}

/// `bookmarks`, when given, holds one title per file: the output gets an
/// outline with one entry per input pointing at its first page.
pub fn merge_pdf_bytes_with_options(
    files: &[&[u8]],
    passwords: &[&str],
    bookmarks: Option<&[&str]>,
) -> Result<Vec<u8>, String> {
    if files.len() < 2 {
        return Err("Choose at least two PDFs to merge.".into());
    }
    if bookmarks.is_some_and(|titles| titles.len() != files.len()) {
        return Err("Every PDF needs a bookmark title.".into());
    }

    let mut documents = Vec::with_capacity(files.len());
    for (index, bytes) in files.iter().enumerate() {
        documents.push(load_document(
            bytes,
            index + 1,
            password_at(passwords, index),
        )?);
    }
    let counts = documents
        .iter()
        .map(|document| document.get_pages().len())
        .collect::<Vec<_>>();

    let mut merged = merge_documents(documents)?;
    if let Some(titles) = bookmarks {
        add_file_bookmarks(&mut merged, titles, &counts)?;
    }
    write_document(merged)
}

pub enum SplitMode<'a> {
    Ranges(&'a [(u32, u32)], bool),
    Every(u32),
}

pub fn split_pdf_bytes(input: &[u8], mode: SplitMode<'_>) -> Result<Vec<Vec<u8>>, String> {
    split_pdf_bytes_with_password(input, "", mode)
}

pub fn split_pdf_bytes_with_password(
    input: &[u8],
    password: &str,
    mode: SplitMode<'_>,
) -> Result<Vec<Vec<u8>>, String> {
    let document = load_document(input, 1, password)?;
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
    organize_pdfs_bytes(
        &[input],
        &pages
            .iter()
            .map(|&(number, turn)| (0, number, turn))
            .collect::<Vec<_>>(),
    )
}

pub fn organize_pdfs_bytes(
    files: &[&[u8]],
    pages: &[(usize, u32, i32)],
) -> Result<Vec<u8>, String> {
    organize_pdfs_bytes_with_passwords(files, &[], pages)
}

pub fn organize_pdfs_bytes_with_passwords(
    files: &[&[u8]],
    passwords: &[&str],
    pages: &[(usize, u32, i32)],
) -> Result<Vec<u8>, String> {
    organize_pdf_items(
        files,
        passwords,
        &pages
            .iter()
            .map(|&(source, number, turn)| OrganizeItem::Page {
                source,
                number,
                turn,
            })
            .collect::<Vec<_>>(),
    )
}

/// One entry of an organized output, in output order. A blank page has no
/// source; its size is in points, before `turn`.
#[derive(Clone, Copy, Debug)]
pub enum OrganizeItem {
    Page {
        source: usize,
        number: u32,
        turn: i32,
    },
    Blank {
        width: f32,
        height: f32,
        turn: i32,
    },
}

pub fn organize_pdf_items(
    files: &[&[u8]],
    passwords: &[&str],
    items: &[OrganizeItem],
) -> Result<Vec<u8>, String> {
    if files.is_empty() {
        return Err("Choose at least one PDF to organize.".into());
    }
    if items.is_empty() {
        return Err("Keep at least one page in the PDF.".into());
    }

    let mut documents = files
        .iter()
        .enumerate()
        .map(|(index, bytes)| load_document(bytes, index + 1, password_at(passwords, index)))
        .collect::<Result<Vec<_>, _>>()?;
    let available = documents
        .iter()
        .map(Document::get_pages)
        .collect::<Vec<_>>();
    let mut selections = vec![Vec::new(); files.len()];

    let mut seen = BTreeSet::new();
    let mut order = Vec::with_capacity(items.len());
    let mut blanks = Vec::new();
    for (position, &item) in items.iter().enumerate() {
        let (source, number, turn) = match item {
            OrganizeItem::Page {
                source,
                number,
                turn,
            } => (source, number, turn),
            OrganizeItem::Blank {
                width,
                height,
                turn,
            } => {
                if !(1.0..=14_400.0).contains(&width) || !(1.0..=14_400.0).contains(&height) {
                    return Err("A blank page needs a size between 1 and 14,400 points.".into());
                }
                if !matches!(turn, 0 | 90 | 180 | 270) {
                    return Err("Page rotations must be 0, 90, 180, or 270 degrees.".into());
                }
                blanks.push((position, width, height, turn));
                continue;
            }
        };
        let Some(&page_id) = available.get(source).and_then(|pages| pages.get(&number)) else {
            return Err(format!(
                "Page {number} of PDF {} could not be read.",
                source + 1
            ));
        };
        if !seen.insert((source, number)) {
            return Err(format!(
                "Page {number} of PDF {} was selected more than once.",
                source + 1
            ));
        }
        if !matches!(turn, 0 | 90 | 180 | 270) {
            return Err("Page rotations must be 0, 90, 180, or 270 degrees.".into());
        }
        if turn != 0 {
            let document = &mut documents[source];
            let page = document
                .get_dictionary(page_id)
                .map_err(|error| format!("Page {number} could not be read: {error}"))?;
            let rotation = page
                .get_deref(b"Rotate", document)
                .ok()
                .cloned()
                .or_else(|| {
                    inheritable_attributes(document, page)
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
        selections[source].push(number);
        order.push((source, number));
    }

    let mut organized = assemble_documents_in_order(
        documents
            .into_iter()
            .zip(selections.into_iter().map(Some))
            .collect(),
        Some(&order),
    )?;
    // Ascending positions, so each insert lands at its final index.
    for (position, width, height, turn) in blanks {
        insert_blank_page(&mut organized, position, width, height, turn)?;
    }
    write_document(organized)
}

/// The kids of the single flat page tree `assemble_documents_in_order` builds.
fn root_pages(document: &Document) -> Result<(ObjectId, Vec<Object>), String> {
    let pages_id = document
        .catalog()
        .and_then(|catalog| catalog.get(b"Pages"))
        .and_then(Object::as_reference)
        .map_err(|error| format!("The page tree could not be read: {error}"))?;
    let kids = document
        .get_dictionary(pages_id)
        .and_then(|pages| pages.get(b"Kids"))
        .and_then(Object::as_array)
        .map_err(|error| format!("The page tree could not be read: {error}"))?
        .clone();
    Ok((pages_id, kids))
}

fn insert_blank_page(
    document: &mut Document,
    position: usize,
    width: f32,
    height: f32,
    turn: i32,
) -> Result<(), String> {
    let (pages_id, mut kids) = root_pages(document)?;
    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), Object::Real(width), Object::Real(height)],
        "Resources" => dictionary! {},
    };
    if turn != 0 {
        page.set("Rotate", i64::from(turn));
    }
    let page_id = document.add_object(page);
    kids.insert(position.min(kids.len()), Object::Reference(page_id));
    let count = kids.len() as i64;
    let pages = document
        .get_dictionary_mut(pages_id)
        .map_err(|error| format!("The page tree could not be read: {error}"))?;
    pages.set("Kids", kids);
    pages.set("Count", count);
    Ok(())
}

/// One outline entry per input, in input order, each opening at that input's
/// first page. `counts` are the inputs' page counts, which is how their first
/// pages are found in the merged, flat page tree.
fn add_file_bookmarks(
    document: &mut Document,
    titles: &[&str],
    counts: &[usize],
) -> Result<(), String> {
    let (_, kids) = root_pages(document)?;
    let outlines_id = document.new_object_id();
    let item_ids = titles
        .iter()
        .map(|_| document.new_object_id())
        .collect::<Vec<_>>();

    let mut first_page = 0;
    for (index, (&title, &count)) in titles.iter().zip(counts).enumerate() {
        let page = kids
            .get(first_page)
            .cloned()
            .ok_or("A bookmark points past the last page.")?;
        first_page += count;
        let mut item = dictionary! {
            "Title" => text_string(title),
            "Parent" => outlines_id,
            "Dest" => vec![page, "Fit".into()],
        };
        if index > 0 {
            item.set("Prev", item_ids[index - 1]);
        }
        if let Some(&next) = item_ids.get(index + 1) {
            item.set("Next", next);
        }
        document
            .objects
            .insert(item_ids[index], Object::Dictionary(item));
    }

    let (Some(&first), Some(&last)) = (item_ids.first(), item_ids.last()) else {
        return Ok(());
    };
    document.objects.insert(
        outlines_id,
        Object::Dictionary(dictionary! {
            "Type" => "Outlines",
            "First" => first,
            "Last" => last,
            "Count" => item_ids.len() as i64,
        }),
    );
    let catalog = document
        .catalog_mut()
        .map_err(|error| format!("The catalog could not be read: {error}"))?;
    catalog.set("Outlines", outlines_id);
    // Opens with the bookmarks showing, since asking for them is the point.
    catalog.set("PageMode", "UseOutlines");
    Ok(())
}

/// PDF text strings are PDFDocEncoding or UTF-16BE with a byte order mark.
/// ASCII is valid PDFDocEncoding as-is; anything else goes out as UTF-16.
pub(crate) fn text_string(text: &str) -> Object {
    if text.is_ascii() {
        return Object::string_literal(text);
    }
    let mut bytes = Vec::new();
    lopdf::encode_utf16_be(text, &mut bytes);
    Object::String(bytes, lopdf::StringFormat::Hexadecimal)
}

/// Lossless stream recompression is always safe, so `reflate` is not optional.
/// `image_quality` trades image fidelity for size: 0 keeps every image byte.
/// `max_image_dimension` downscales images wider or taller than that many
/// pixels, preserving aspect; 0 keeps every size.
pub(crate) fn password_at<'a>(passwords: &[&'a str], index: usize) -> &'a str {
    passwords.get(index).copied().unwrap_or("")
}

/// Decryption happens inside the load, before any renumbering: below /V 5 the
/// encryption key is derived from each object's number and generation. lopdf
/// tries the empty user password first, because most "encrypted" PDFs only
/// restrict permissions and viewers open them silently. A successful load drops
/// /Encrypt, so everything written from it is unlocked.
///
/// A document lopdf cannot decrypt loads as nothing but its /Encrypt
/// dictionary, so there is no decrypting after the fact: a password has to go
/// into the load itself, which means loading a protected file twice.
pub(crate) fn load_document(
    bytes: &[u8],
    position: usize,
    password: &str,
) -> Result<Document, String> {
    let load = |password: Option<String>| {
        Document::load_mem_with_options(
            bytes,
            LoadOptions {
                max_decompressed_size: Some(MAX_DECOMPRESSED_STREAM),
                password,
                ..Default::default()
            },
        )
    };
    let unreadable = |error| format!("PDF {position} could not be read: {error}");
    // pdf.js verifies passwords before they get here, so a failure also covers
    // encryption lopdf cannot handle; this wording is true either way.
    let rejected = || format!("PDF {position} could not be unlocked with that password.");

    let mut document = load(None).map_err(unreadable)?;
    // A successful load removes /Encrypt from the trailer. One that is still
    // there but not a reference, which `is_encrypted` misses, means lopdf
    // decrypted nothing (pdf.js corpus: issue6010_1.pdf).
    if document.is_encrypted() || document.trailer.get(b"Encrypt").is_ok() {
        if password.is_empty() {
            return Err(format!(
                "PDF {position} is password protected. Unlock it before continuing."
            ));
        }
        document =
            load(Some(user_password_for(&document, password))).map_err(|error| match error {
                lopdf::Error::InvalidPassword | lopdf::Error::Decryption(_) => rejected(),
                error => unreadable(error),
            })?;
        if document.is_encrypted() || document.trailer.get(b"Encrypt").is_ok() {
            return Err(rejected());
        }
    }

    if document.get_pages().is_empty() {
        return Err(format!("PDF {position} has no pages."));
    }
    reserve_referenced_ids(&mut document);

    Ok(document)
}

/// lopdf gives each new object the id after `max_id`, which only counts objects
/// that exist. A reference to one that does not can name an id past all of
/// them, and an object added there answers it: annotating gave two popups whose
/// parents were missing the new highlight and its appearance as parents
/// (pdf.js corpus: ZapfDingbats.pdf). Every tool that edits in place adds
/// objects, so new ids start past every id referenced, keeping such references
/// dangling. Merge renumbers instead and sets its own `max_id`.
fn reserve_referenced_ids(document: &mut Document) {
    let mut highest = document.max_id;
    let mut pending = document
        .objects
        .values()
        .chain(document.trailer.iter().map(|(_, value)| value))
        .collect::<Vec<_>>();
    while let Some(object) = pending.pop() {
        match object {
            Object::Reference((id, _)) => highest = highest.max(*id),
            Object::Array(items) => pending.extend(items),
            Object::Dictionary(dictionary) => {
                pending.extend(dictionary.iter().map(|(_, value)| value))
            }
            Object::Stream(stream) => pending.extend(stream.dict.iter().map(|(_, value)| value)),
            _ => {}
        }
    }
    document.max_id = highest;
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
    assemble_documents_in_order(documents, None)
}

/// `output_order` reorders the assembled pages across every input. Organize
/// needs it because its page sequence interleaves documents; Merge and Split
/// emit each document's selection consecutively, so they pass `None`.
fn assemble_documents_in_order(
    documents: Vec<(Document, Option<Vec<u32>>)>,
    output_order: Option<&[(usize, u32)]>,
) -> Result<Document, String> {
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
    let mut pages: Vec<((usize, u32), ObjectId, Dictionary)> = Vec::new();
    let mut catalog: Option<Dictionary> = None;

    for (source, (mut document, selection)) in documents.into_iter().enumerate() {
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
                        .map(|id| (number, id))
                        .ok_or_else(|| format!("Page {number} could not be read."))
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            available.into_iter().collect::<Vec<_>>()
        };
        share_inherited_resources(&mut document);
        let inherited = page_order
            .iter()
            .map(|(_, page_id)| {
                let page = document
                    .get_dictionary(*page_id)
                    .map_err(|error| format!("A PDF page could not be read: {error}"))?;
                Ok(inheritable_attributes(&document, page))
            })
            .collect::<Result<Vec<_>, String>>()?;

        for ((_, page_id), attributes) in page_order.iter().zip(inherited) {
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

        for &(number, page_id) in &page_order {
            let page_id = moved[&page_id];
            let page = document
                .get_dictionary(page_id)
                .map_err(|error| format!("A PDF page could not be read: {error}"))?
                .clone();
            pages.push(((source, number), page_id, page));
        }

        output.objects.extend(document.objects);
    }

    let pages = if let Some(order) = output_order {
        let mut by_source = pages
            .into_iter()
            .map(|(key, id, page)| (key, (id, page)))
            .collect::<BTreeMap<_, _>>();
        order
            .iter()
            .map(|key| {
                by_source.remove(key).ok_or_else(|| {
                    format!("Page {} of PDF {} could not be read.", key.1, key.0 + 1)
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        pages.into_iter().map(|(_, id, page)| (id, page)).collect()
    };

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
pub(crate) fn inheritable_attributes(
    document: &Document,
    page: &Dictionary,
) -> Vec<(&'static [u8], Object)> {
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

/// A page tree node's direct /Resources would be copied onto every page under
/// it once pages take their inherited attributes, so it is moved into an
/// object of its own first and the pages share a reference to it.
fn share_inherited_resources(document: &mut Document) {
    let nodes = document
        .objects
        .iter()
        .filter_map(|(id, object)| {
            let node = object.as_dict().ok()?;
            (node.has(b"Kids") && node.get(b"Resources").ok()?.as_dict().is_ok()).then_some(*id)
        })
        .collect::<Vec<_>>();
    for id in nodes {
        let Some(resources) = document
            .get_dictionary_mut(id)
            .ok()
            .and_then(|node| node.remove(b"Resources"))
        else {
            continue;
        };
        let shared = document.add_object(resources);
        if let Ok(node) = document.get_dictionary_mut(id) {
            node.set("Resources", shared);
        }
    }
}

/// Dictionaries that mean the same wherever they are used, so identical
/// copies can be one object. Pages, annotations, fields and the like have an
/// identity of their own and never qualify.
const SHAREABLE_TYPES: [&[u8]; 4] = [b"Font", b"FontDescriptor", b"ExtGState", b"Encoding"];

/// Two fonts only look identical once the font files they point at have
/// become one, so sharing repeats, a level deeper each time.
const MAX_SHARING_PASSES: usize = 8;

const MAX_FINGERPRINT_DEPTH: usize = 32;

/// Makes identical streams and shareable dictionaries one object, so merging
/// ten invoices made from one template embeds its fonts and logo once.
pub(crate) fn share_identical_objects(document: &mut Document) {
    for _ in 0..MAX_SHARING_PASSES {
        let mut keepers = HashMap::<(Vec<u8>, [u8; 16]), ObjectId>::new();
        let mut replaced = BTreeMap::new();
        for (&id, object) in &document.objects {
            let Some(key) = fingerprint(object) else {
                continue;
            };
            match keepers.entry(key) {
                Entry::Occupied(keeper) => {
                    // A digest match is checked byte for byte, so a crafted
                    // collision cannot swap one image for another.
                    let same = match (object, document.objects.get(keeper.get())) {
                        (Object::Stream(found), Some(Object::Stream(kept))) => {
                            found.content == kept.content
                        }
                        _ => true,
                    };
                    if same {
                        replaced.insert(id, *keeper.get());
                    }
                }
                Entry::Vacant(slot) => {
                    slot.insert(id);
                }
            }
        }
        if replaced.is_empty() {
            return;
        }
        for id in replaced.keys() {
            document.objects.remove(id);
        }
        rewrite_references(
            document
                .objects
                .values_mut()
                .chain(document.trailer.iter_mut().map(|(_, value)| value)),
            |id| {
                if let Some(&keeper) = replaced.get(id) {
                    *id = keeper;
                }
            },
        );
    }
}

/// What makes `object` identical to another that can stand in for it: its
/// dictionary written out, references included, and a digest of any stream
/// content. `None` for objects that are not shared.
fn fingerprint(object: &Object) -> Option<(Vec<u8>, [u8; 16])> {
    let (dictionary, content) = match object {
        Object::Stream(stream) => (&stream.dict, Some(&stream.content)),
        Object::Dictionary(dictionary) => (dictionary, None),
        _ => return None,
    };
    let kind = dictionary.get(b"Type").and_then(Object::as_name).ok();
    let shareable = match content {
        Some(_) => !matches!(kind, Some(b"XRef" | b"ObjStm")),
        None => kind.is_some_and(|kind| SHAREABLE_TYPES.contains(&kind)),
    };
    if !shareable {
        return None;
    }
    let mut written = Vec::new();
    encode_dictionary(dictionary, &mut written, 0)?;
    let digest = content.map_or([0; 16], |content| Md5::digest(content).into());
    Some((written, digest))
}

/// Keys in sorted order, since order means nothing in a dictionary. /Length
/// is left out: stream content is compared on its own, and the same length
/// can be written directly or through a reference.
fn encode_dictionary(dictionary: &Dictionary, out: &mut Vec<u8>, depth: usize) -> Option<()> {
    let mut entries = dictionary
        .iter()
        .filter(|(key, _)| key.as_slice() != b"Length")
        .collect::<Vec<_>>();
    entries.sort_by_key(|(key, _)| *key);
    out.push(b'<');
    for (key, value) in entries {
        out.extend((key.len() as u32).to_le_bytes());
        out.extend(key);
        encode(value, out, depth + 1)?;
    }
    out.push(b'>');
    Some(())
}

fn encode(object: &Object, out: &mut Vec<u8>, depth: usize) -> Option<()> {
    if depth > MAX_FINGERPRINT_DEPTH {
        return None;
    }
    match object {
        Object::Null => out.push(b'n'),
        Object::Boolean(value) => out.extend([b'b', u8::from(*value)]),
        Object::Integer(value) => {
            out.push(b'i');
            out.extend(value.to_le_bytes());
        }
        Object::Real(value) => {
            out.push(b'r');
            out.extend(value.to_bits().to_le_bytes());
        }
        Object::Name(name) => {
            out.push(b'/');
            out.extend((name.len() as u32).to_le_bytes());
            out.extend(name);
        }
        Object::String(text, _) => {
            out.push(b's');
            out.extend((text.len() as u32).to_le_bytes());
            out.extend(text);
        }
        Object::Reference((number, generation)) => {
            out.push(b'R');
            out.extend(number.to_le_bytes());
            out.extend(generation.to_le_bytes());
        }
        Object::Array(items) => {
            out.push(b'[');
            for item in items {
                encode(item, out, depth + 1)?;
            }
            out.push(b']');
        }
        Object::Dictionary(dictionary) => encode_dictionary(dictionary, out, depth)?,
        Object::Stream(_) => return None,
    }
    Some(())
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

    rewrite_references(
        document
            .objects
            .values_mut()
            .chain(document.trailer.iter_mut().map(|(_, value)| value)),
        |id| *id = moved.get(id).copied().unwrap_or(unresolved),
    );
    document.objects = std::mem::take(&mut document.objects)
        .into_iter()
        .map(|(id, object)| (moved[&id], object))
        .collect();
    document.max_id = start + moved.len() as u32;

    moved
}

/// Calls `rewrite` on every reference inside `roots`. Walks with a stack
/// rather than recursion: a deeply nested array would otherwise overflow the
/// wasm stack, which is a trap rather than an error.
pub(crate) fn rewrite_references<'a>(
    roots: impl IntoIterator<Item = &'a mut Object>,
    mut rewrite: impl FnMut(&mut ObjectId),
) {
    let mut pending = roots.into_iter().collect::<Vec<_>>();
    while let Some(object) = pending.pop() {
        match object {
            Object::Reference(id) => rewrite(id),
            Object::Array(items) => pending.extend(items.iter_mut()),
            Object::Dictionary(dictionary) => {
                pending.extend(dictionary.iter_mut().map(|(_, value)| value))
            }
            Object::Stream(stream) => {
                pending.extend(stream.dict.iter_mut().map(|(_, value)| value))
            }
            _ => {}
        }
    }
}

/// Orders (major, minor) properly. Comparing the strings happens to work across
/// today's versions but would rank "1.10" below "1.5".
/// lopdf derives the file key from whatever password it is given as if it were
/// the user password. For revisions 2 to 4 an owner password therefore
/// "authenticates" and then decrypts every object into garbage that still
/// parses. Hand lopdf the user password instead, recovered from the owner
/// password (ISO 32000-1, 7.6.3.4, algorithm 7). Revisions 5 and 6 derive the
/// key from either password correctly.
///
/// Passwords below revision 5 are PDFDocEncoding, which matches Latin-1 for
/// everything but a few punctuation bytes. A byte that does not survive the
/// round trip fails authentication in the load; it cannot produce noise.
fn user_password_for(document: &Document, password: &str) -> String {
    let revision = document
        .get_encrypted()
        .and_then(|dict| dict.get(b"R"))
        .and_then(Object::as_i64)
        .unwrap_or(0);
    if revision >= 5
        || document.authenticate_user_password(password).is_ok()
        || document.authenticate_owner_password(password).is_err()
    {
        return password.to_owned();
    }
    let owner = password
        .chars()
        .filter_map(|char| u8::try_from(u32::from(char)).ok())
        .collect::<Vec<_>>();
    let Some(padded) = recover_user_password(document, &owner, revision) else {
        return password.to_owned();
    };
    let length = (0..=32)
        .find(|&length| padded[length..] == PASSWORD_PADDING[..32 - length])
        .unwrap_or(32);
    padded[..length]
        .iter()
        .map(|&byte| char::from(byte))
        .collect()
}

const PASSWORD_PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

fn recover_user_password(document: &Document, owner: &[u8], revision: i64) -> Option<Vec<u8>> {
    let encrypt = document.get_encrypted().ok()?;
    let owner_value = encrypt.get(b"O").ok()?.as_str().ok()?.get(..32)?;
    let key_length = if revision == 2 {
        5
    } else {
        let bits = encrypt
            .get(b"Length")
            .and_then(Object::as_i64)
            .unwrap_or(40);
        (bits as usize / 8).clamp(5, 16)
    };

    let used = owner.len().min(32);
    let mut hasher = Md5::new();
    hasher.update(&owner[..used]);
    hasher.update(&PASSWORD_PADDING[..32 - used]);
    let mut hash = hasher.finalize().to_vec();
    if revision >= 3 {
        for _ in 0..50 {
            hash = Md5::digest(&hash[..key_length]).to_vec();
        }
    }
    let key = &hash[..key_length];

    let mut user = owner_value.to_vec();
    if revision == 2 {
        rc4(key, &mut user);
    } else {
        for round in (0..=19u8).rev() {
            let round_key = key.iter().map(|byte| byte ^ round).collect::<Vec<_>>();
            rc4(&round_key, &mut user);
        }
    }
    Some(user)
}

fn rc4(key: &[u8], data: &mut [u8]) {
    let mut state: [u8; 256] = std::array::from_fn(|index| index as u8);
    let mut j = 0u8;
    for i in 0..256 {
        j = j.wrapping_add(state[i]).wrapping_add(key[i % key.len()]);
        state.swap(i, j as usize);
    }
    let (mut i, mut j) = (0u8, 0u8);
    for byte in data {
        i = i.wrapping_add(1);
        j = j.wrapping_add(state[i as usize]);
        state.swap(i as usize, j as usize);
        *byte ^= state[state[i as usize].wrapping_add(state[j as usize]) as usize];
    }
}

pub(crate) fn parse_version(version: &str) -> (u32, u32) {
    let (major, minor) = version.split_once('.').unwrap_or((version, "0"));
    (major.parse().unwrap_or(1), minor.parse().unwrap_or(0))
}

fn write_document(mut document: Document) -> Result<Vec<u8>, String> {
    // Reachability sweep from the trailer, so it has to run after /Root is set.
    // Collects each input's superseded catalog and page tree nodes, along with
    // the outline items and name trees that only those referenced.
    document.prune_objects();
    share_identical_objects(&mut document);
    // Compacts the ids pruning left gaps in. Not lopdf's `renumber_objects()`:
    // it would slide a real object onto the id `renumber()` reserved for
    // references to missing objects, and those would then resolve to it.
    renumber(&mut document, 1);
    document.compress();

    // Packed only, unlike `write_compressed`, which also writes a plain copy and
    // keeps the smaller: that clones the document, and merges are where inputs
    // are largest.
    let mut bytes = Vec::new();
    document
        .save_with_options(&mut bytes, packed_options())
        .map_err(|error| format!("The PDF could not be created: {error}"))?;
    Ok(bytes)
}
