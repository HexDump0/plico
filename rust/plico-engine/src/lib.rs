use std::collections::{BTreeMap, BTreeSet};

use js_sys::{Array, Uint8Array};
use lopdf::{Dictionary, Document, LoadOptions, Object, ObjectId, dictionary};
use wasm_bindgen::prelude::*;

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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lopdf::{Document, Object, Stream, dictionary};

    use super::{SplitMode, merge_pdf_bytes, parse_version, split_pdf_bytes};

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

    #[test]
    fn orders_versions_numerically() {
        assert!(parse_version("1.10") > parse_version("1.5"));
        assert!(parse_version("2.0") > parse_version("1.7"));
        assert_eq!(parse_version("nonsense"), (1, 0));
    }
}
