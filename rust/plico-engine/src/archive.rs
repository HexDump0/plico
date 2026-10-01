//! PDF/A (ISO 19005), the archival subset of PDF.
//!
//! The file is changed in place like stamping: no page tree rebuild and no
//! renumbering, so forms, tags, links and outlines survive. What PDF/A forbids
//! is removed or neutralised, what it requires (an output intent, XMP
//! identification, a file ID) is added. A file that would still not conform for
//! a reason we can detect is refused with that reason, never written out
//! claiming a conformance it lacks.

mod encodings;
mod fonts;

use std::collections::{BTreeSet, HashMap};

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, decode_text_string, dictionary};

use crate::compression::{filter_matches, write_compressed};
use crate::documents::{MAX_DECOMPRESSED_STREAM, load_document, parse_version};
use crate::security::ensure_file_id;

pub use fonts::StandardFont;

/// Level B ("basic") only promises that pages look the same in future, which
/// is what can be reached from an arbitrary PDF. Level A needs a tag tree and
/// level U a Unicode mapping for every glyph, neither of which can be invented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfALevel {
    /// PDF/A-2b, what archives usually mean by "PDF/A". No attachments
    /// unless they are PDF/A themselves, which cannot be checked here.
    A2b,
    /// PDF/A-3b: PDF/A-2b plus attachments of any kind.
    A3b,
}

impl PdfALevel {
    fn part(self) -> u8 {
        match self {
            PdfALevel::A2b => 2,
            PdfALevel::A3b => 3,
        }
    }
}

const SRGB_PROFILE: &[u8] = include_bytes!("../assets/icc/srgb.icc");
const CMYK_PROFILE: &[u8] = include_bytes!("../assets/icc/ps_cmyk.icc");

/// Deeper than any real document nests direct objects; keeps a hostile one
/// from overflowing the wasm stack.
const MAX_NESTING: usize = 64;

/// Annotation flags (ISO 32000-1, 12.5.3).
const INVISIBLE: i64 = 1;
const HIDDEN: i64 = 2;
const PRINT: i64 = 4;
const NO_VIEW: i64 = 32;
const TOGGLE_NO_VIEW: i64 = 256;

/// Annotation types ISO 32000-1 defines, less the four PDF/A-2 6.3.1 forbids
/// (3D, Sound, Screen, Movie). Anything else is not allowed either.
const ALLOWED_ANNOTATIONS: [&[u8]; 22] = [
    b"Text",
    b"Link",
    b"FreeText",
    b"Line",
    b"Square",
    b"Circle",
    b"Polygon",
    b"PolyLine",
    b"Highlight",
    b"Underline",
    b"Squiggly",
    b"StrikeOut",
    b"Stamp",
    b"Caret",
    b"Ink",
    b"Popup",
    b"FileAttachment",
    b"Widget",
    b"PrinterMark",
    b"TrapNet",
    b"Watermark",
    b"Redact",
];

const STANDARD_BLEND_MODES: [&[u8]; 16] = [
    b"Normal",
    b"Compatible",
    b"Multiply",
    b"Screen",
    b"Overlay",
    b"Darken",
    b"Lighten",
    b"ColorDodge",
    b"ColorBurn",
    b"HardLight",
    b"SoftLight",
    b"Difference",
    b"Exclusion",
    b"Hue",
    b"Saturation",
    b"Color",
];

const RENDERING_INTENTS: [&[u8]; 4] = [
    b"AbsoluteColorimetric",
    b"RelativeColorimetric",
    b"Saturation",
    b"Perceptual",
];

/// `fonts` are substitutes for unembedded standard fonts; a font without one
/// is refused. [`standard_fonts_for_pdfa`] says which are needed.
pub fn convert_to_pdfa_bytes(
    input: &[u8],
    password: &str,
    level: PdfALevel,
    fonts: &[StandardFont<'_>],
) -> Result<Vec<u8>, String> {
    let mut document = load_document(input, 1, password)?;
    // Loading unpacks object streams; their containers are unreachable now and
    // would otherwise be read as content below.
    document.prune_objects();
    let catalog_id = document
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|_| "The PDF has no document catalog.".to_string())?;

    refuse_what_cannot_be_fixed(&document, catalog_id, level, fonts)?;

    if parse_version(&document.version) > (1, 7) {
        document.version = "1.7".into();
    }
    // 6.1.2: at least four bytes above 127 after the header. lopdf keeps the
    // input's mark, which can be shorter (pdf.js corpus: bitmap-halftone.pdf).
    document.binary_mark = vec![0xE2, 0xE3, 0xCF, 0xD3];
    clean_catalog(&mut document, catalog_id);
    clean_streams(&mut document)?;
    embed_standard_fonts(&mut document, fonts)?;
    clean_dictionaries(&mut document);
    for object in document.objects.values_mut() {
        clamp_integers(object, 0);
    }
    clean_annotations(&mut document)?;
    clean_optional_content(&mut document, catalog_id);
    give_streams_their_resources(&mut document, catalog_id)?;
    if level == PdfALevel::A3b {
        associate_attachments(&mut document, catalog_id);
    }
    set_color_spaces(&mut document, catalog_id)?;
    set_metadata(&mut document, catalog_id, level)?;
    ensure_file_id(&mut document)?;

    document.prune_objects();
    document.compress();
    write_compressed(document)
}

/// Everything detected here would need content this engine cannot produce:
/// font programs, appearance streams, a rendered XFA form.
fn refuse_what_cannot_be_fixed(
    document: &Document,
    catalog_id: ObjectId,
    level: PdfALevel,
    supplied: &[StandardFont<'_>],
) -> Result<(), String> {
    let catalog = document
        .get_dictionary(catalog_id)
        .map_err(|_| "The PDF has no document catalog.".to_string())?;
    if catalog
        .get(b"NeedsRendering")
        .and_then(Object::as_bool)
        .unwrap_or(false)
    {
        return Err("This PDF is a dynamic XFA form, which PDF/A cannot hold.".into());
    }
    if level == PdfALevel::A2b && has_attachments(document) {
        return Err(
            "This PDF has attached files, which PDF/A-2 does not allow. Choose PDF/A-3 to keep them."
                .into(),
        );
    }
    let missing = unembedded_fonts(document)
        .into_iter()
        .filter(|font| {
            !(font.id.is_some()
                && font
                    .standard
                    .is_some_and(|standard| supplied.iter().any(|font| font.name == standard)))
        })
        .map(|font| font.name)
        .collect::<Vec<_>>();
    if let Some(first) = missing.first() {
        return Err(match missing.len() {
            1 => format!(
                "The font “{first}” is not embedded in this PDF. PDF/A needs every font embedded."
            ),
            count => format!(
                "The font “{first}” and {} others are not embedded in this PDF. PDF/A needs every font embedded.",
                count - 1
            ),
        });
    }
    if let Some(page) = annotation_without_appearance(document) {
        return Err(format!(
            "An annotation on page {page} has no stored appearance, which PDF/A needs."
        ));
    }
    Ok(())
}

fn resolve<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Object> {
    match object {
        Object::Reference(id) => document.get_object(*id).ok(),
        object => Some(object),
    }
}

fn resolve_dict<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    match resolve(document, object)? {
        Object::Dictionary(dictionary) => Some(dictionary),
        Object::Stream(stream) => Some(&stream.dict),
        _ => None,
    }
}

fn name_of<'a>(dictionary: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    dictionary.get(key).and_then(Object::as_name).ok()
}

/// Calls `visit` on every dictionary inside `object`, outermost first.
fn each_dictionary(object: &mut Object, depth: usize, visit: &mut dyn FnMut(&mut Dictionary)) {
    if depth > MAX_NESTING {
        return;
    }
    let dictionary = match object {
        Object::Dictionary(dictionary) => dictionary,
        Object::Stream(stream) => &mut stream.dict,
        Object::Array(items) => {
            for item in items {
                each_dictionary(item, depth + 1, visit);
            }
            return;
        }
        _ => return,
    };
    visit(dictionary);
    for (_, value) in dictionary.iter_mut() {
        each_dictionary(value, depth + 1, visit);
    }
}

fn each_dictionary_ref(object: &Object, depth: usize, visit: &mut dyn FnMut(&Dictionary)) {
    if depth > MAX_NESTING {
        return;
    }
    let dictionary = match object {
        Object::Dictionary(dictionary) => dictionary,
        Object::Stream(stream) => &stream.dict,
        Object::Array(items) => {
            for item in items {
                each_dictionary_ref(item, depth + 1, visit);
            }
            return;
        }
        _ => return,
    };
    visit(dictionary);
    for (_, value) in dictionary.iter() {
        each_dictionary_ref(value, depth + 1, visit);
    }
}

fn has_attachments(document: &Document) -> bool {
    let mut found = false;
    for object in document.objects.values() {
        each_dictionary_ref(object, 0, &mut |dictionary| {
            found |=
                dictionary.has(b"EF") || name_of(dictionary, b"Subtype") == Some(b"FileAttachment");
        });
    }
    found
}

/// Fonts that resources point at with no font program. Fonts that only form
/// fields' default resources name are left out: those are for generating
/// appearances, not for drawing.
/// A font drawn with but not embedded: its object, if it has one, its name,
/// and the standard font it stands for, if any.
struct Unembedded {
    id: Option<ObjectId>,
    name: String,
    standard: Option<&'static str>,
}

fn unembedded_fonts(document: &Document) -> Vec<Unembedded> {
    let mut seen = BTreeSet::new();
    let mut missing = Vec::new();
    let embedded = |descriptor: Option<&Dictionary>| {
        descriptor.is_some_and(|descriptor| {
            [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"]
                .iter()
                .any(|key| descriptor.has(key))
        })
    };
    for (_, resources) in resource_dictionaries(document) {
        let Some(fonts) = resources
            .get(b"Font")
            .ok()
            .and_then(|value| resolve_dict(document, value))
        else {
            continue;
        };
        for (_, entry) in fonts.iter() {
            let id = entry.as_reference().ok();
            if id.is_some_and(|id| !seen.insert(id)) {
                continue;
            }
            let Some(font) = resolve_dict(document, entry) else {
                continue;
            };
            let program = match name_of(font, b"Subtype") {
                Some(b"Type3") => continue,
                Some(b"Type0") => font
                    .get(b"DescendantFonts")
                    .ok()
                    .and_then(|value| resolve(document, value))
                    .and_then(|value| value.as_array().ok())
                    .and_then(|fonts| fonts.first())
                    .and_then(|value| resolve_dict(document, value))
                    .and_then(|descendant| font_descriptor(document, descendant)),
                _ => font_descriptor(document, font),
            };
            if embedded(program) {
                continue;
            }
            missing.push(Unembedded {
                id,
                name: font
                    .get(b"BaseFont")
                    .and_then(Object::as_name)
                    .map(|name| String::from_utf8_lossy(name).into_owned())
                    .unwrap_or_else(|_| "unnamed".into()),
                standard: fonts::standard_equivalent(font),
            });
        }
    }
    missing.sort_by(|a, b| a.name.cmp(&b.name));
    missing
}

/// The standard 14 substitutes converting `input` would embed, so a caller can
/// fetch only those for [`convert_to_pdfa_bytes`].
pub fn standard_fonts_for_pdfa(input: &[u8], password: &str) -> Result<Vec<&'static str>, String> {
    let document = load_document(input, 1, password)?;
    let mut names = unembedded_fonts(&document)
        .into_iter()
        .filter(|font| font.id.is_some())
        .filter_map(|font| font.standard)
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    Ok(names)
}

/// Replaces every unembedded font a supplied substitute covers.
fn embed_standard_fonts(
    document: &mut Document,
    supplied: &[StandardFont<'_>],
) -> Result<(), String> {
    let mut programs: HashMap<&str, (ObjectId, fonts::Metrics)> = HashMap::new();
    for font in unembedded_fonts(document) {
        let (Some(id), Some(standard)) = (font.id, font.standard) else {
            continue;
        };
        let Some(substitute) = supplied
            .iter()
            .find(|substitute| substitute.name == standard)
        else {
            continue;
        };
        if !programs.contains_key(standard) {
            let metrics = fonts::Metrics::parse(substitute.metrics)?;
            let program = document.add_object(fonts::program_stream(substitute.program));
            programs.insert(standard, (program, metrics));
        }
        let (program, metrics) = &programs[standard];
        fonts::embed(document, id, metrics, *program)?;
    }
    Ok(())
}

fn font_descriptor<'a>(document: &'a Document, font: &'a Dictionary) -> Option<&'a Dictionary> {
    resolve_dict(document, font.get(b"FontDescriptor").ok()?)
}

/// Every resource dictionary something draws with, by the object holding it:
/// pages and page tree nodes, forms, patterns, appearance streams and Type 3
/// fonts.
fn resource_dictionaries(document: &Document) -> Vec<(ObjectId, &Dictionary)> {
    document
        .objects
        .iter()
        .filter_map(|(id, object)| {
            let holder = match object {
                Object::Dictionary(dictionary) => dictionary,
                Object::Stream(stream) => &stream.dict,
                _ => return None,
            };
            let resources = resolve_dict(document, holder.get(b"Resources").ok()?)?;
            Some((*id, resources))
        })
        .collect()
}

fn page_annotations(document: &Document) -> Vec<(u32, ObjectId, Vec<Object>)> {
    document
        .get_pages()
        .into_iter()
        .filter_map(|(number, page_id)| {
            let page = document.get_dictionary(page_id).ok()?;
            let annotations = resolve(document, page.get(b"Annots").ok()?)?
                .as_array()
                .ok()?
                .clone();
            Some((number, page_id, annotations))
        })
        .collect()
}

/// PDF/A-2 6.3.3: every annotation but popups and links draws from a stored
/// appearance. A viewer otherwise invents one, and that is exactly the kind of
/// rendering that differs between viewers and over time. Hidden annotations
/// get an empty one, so only visible ones count here.
fn annotation_without_appearance(document: &Document) -> Option<u32> {
    for (number, _, annotations) in page_annotations(document) {
        for annotation in &annotations {
            let Some(annotation) = resolve_dict(document, annotation) else {
                continue;
            };
            let subtype = name_of(annotation, b"Subtype").unwrap_or_default();
            if matches!(subtype, b"Popup" | b"Link")
                || !ALLOWED_ANNOTATIONS.contains(&subtype)
                || is_hidden(annotation)
                || has_zero_area(document, annotation)
            {
                continue;
            }
            let draws = annotation
                .get(b"AP")
                .ok()
                .and_then(|value| resolve_dict(document, value))
                .and_then(|appearance| appearance.get(b"N").ok())
                .is_some_and(|normal| has_appearance_stream(document, normal));
            if !draws {
                return Some(number);
            }
        }
    }
    None
}

/// A normal appearance is a stream, or a dictionary of states at least one of
/// which is a stream (pdf.js corpus: issue15557.pdf has states that are all
/// null).
fn has_appearance_stream(document: &Document, normal: &Object) -> bool {
    match resolve(document, normal) {
        Some(Object::Stream(_)) => true,
        Some(Object::Dictionary(states)) => states
            .iter()
            .any(|(_, state)| matches!(resolve(document, state), Some(Object::Stream(_)))),
        _ => false,
    }
}

fn flags(annotation: &Dictionary) -> i64 {
    annotation.get(b"F").and_then(Object::as_i64).unwrap_or(0)
}

fn is_hidden(annotation: &Dictionary) -> bool {
    flags(annotation) & (INVISIBLE | HIDDEN | NO_VIEW | TOGGLE_NO_VIEW) != 0
}

fn rect(document: &Document, annotation: &Dictionary) -> [f32; 4] {
    let numbers = annotation
        .get(b"Rect")
        .ok()
        .and_then(|value| resolve(document, value))
        .and_then(|value| value.as_array().ok())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| resolve(document, item)?.as_float().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    match numbers[..] {
        [x0, y0, x1, y1] => [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)],
        _ => [0.0; 4],
    }
}

fn has_zero_area(document: &Document, annotation: &Dictionary) -> bool {
    let [x0, y0, x1, y1] = rect(document, annotation);
    x1 - x0 == 0.0 && y1 - y0 == 0.0
}

fn clean_catalog(document: &mut Document, catalog_id: ObjectId) {
    let names = document
        .get_dictionary(catalog_id)
        .ok()
        .and_then(|catalog| catalog.get(b"Names").ok())
        .and_then(|names| names.as_reference().ok());
    let Ok(catalog) = document.get_dictionary_mut(catalog_id) else {
        return;
    };
    // Requirements and alternate presentations describe things a PDF/A reader
    // need not do; Perms carries signatures that this rewrite invalidates.
    for key in [b"Requirements".as_slice(), b"Perms", b"NeedsRendering"] {
        catalog.remove(key);
    }
    if catalog
        .get(b"Version")
        .and_then(Object::as_name)
        .is_ok_and(|version| parse_version(&String::from_utf8_lossy(version)) > (1, 7))
    {
        catalog.remove(b"Version");
    }
    let names = match names {
        Some(id) => document.get_dictionary_mut(id).ok(),
        None => catalog
            .get_mut(b"Names")
            .ok()
            .and_then(|names| names.as_dict_mut().ok()),
    };
    if let Some(names) = names {
        names.remove(b"JavaScript");
        names.remove(b"AlternatePresentations");
    }
}

/// PDF/A-2 6.1.7: no external stream data and no LZW. LZW streams are decoded
/// here and deflated with everything else on write.
fn clean_streams(document: &mut Document) -> Result<(), String> {
    for object in document.objects.values_mut() {
        let Object::Stream(stream) = object else {
            continue;
        };
        for key in [b"F".as_slice(), b"FFilter", b"FDecodeParms"] {
            stream.dict.remove(key);
        }
        if filter_matches(&stream.dict, b"LZWDecode") {
            let content = stream
                .decompressed_content_with_limit(MAX_DECOMPRESSED_STREAM)
                .map_err(|_| "A part of this PDF compressed with LZW could not be read.")?;
            stream.dict.remove(b"Filter");
            stream.dict.remove(b"DecodeParms");
            stream.set_content(content);
            stream.allows_compression = true;
        }
    }
    Ok(())
}

fn is_forbidden_action(dictionary: &Dictionary) -> bool {
    match name_of(dictionary, b"S") {
        Some(
            b"Launch" | b"Sound" | b"Movie" | b"ResetForm" | b"ImportData" | b"Hide"
            | b"SetOCGState" | b"Rendition" | b"Trans" | b"GoTo3DView" | b"JavaScript",
        ) => true,
        Some(b"Named") => !matches!(
            name_of(dictionary, b"N"),
            Some(b"NextPage" | b"PrevPage" | b"FirstPage" | b"LastPage")
        ),
        _ => false,
    }
}

/// Per-dictionary fixes that do not depend on where the dictionary sits.
fn clean_dictionaries(document: &mut Document) {
    let forbidden = document
        .objects
        .iter()
        .filter(|(_, object)| object.as_dict().is_ok_and(is_forbidden_action))
        .map(|(id, _)| *id)
        .collect::<BTreeSet<_>>();
    let is_forbidden = |value: &Object| match value {
        Object::Reference(id) => forbidden.contains(id),
        Object::Dictionary(dictionary) => is_forbidden_action(dictionary),
        _ => false,
    };

    for object in document.objects.values_mut() {
        each_dictionary(object, 0, &mut |dictionary| {
            // 6.5: no additional actions anywhere, and only the actions that
            // cannot change what the document shows.
            dictionary.remove(b"AA");
            for key in [b"A".as_slice(), b"OpenAction"] {
                if dictionary.get(key).is_ok_and(is_forbidden) {
                    dictionary.remove(key);
                }
            }
            match dictionary.get_mut(b"Next") {
                Ok(Object::Array(next)) => next.retain(|action| !is_forbidden(action)),
                Ok(next) if is_forbidden(next) => {
                    dictionary.remove(b"Next");
                }
                _ => {}
            }
            // 6.4.1: a widget performs no action at all; 6.4.2: no XFA, and
            // appearances come from the file rather than being regenerated.
            if name_of(dictionary, b"Subtype") == Some(b"Widget") {
                dictionary.remove(b"A");
            }
            // 6.4.3 requires a signature to cover the file byte for byte,
            // which no rewrite can keep true. The field stays, unsigned.
            if name_of(dictionary, b"FT") == Some(b"Sig") {
                dictionary.remove(b"V");
            }
            dictionary.remove(b"XFA");
            dictionary.remove(b"NeedAppearances");
            dictionary.remove(b"PresSteps");
            // 6.6.2.3.1 holds every metadata stream to the XMP schemas PDF/A
            // knows. Only the catalog needs one, and it gets a fresh one.
            dictionary.remove(b"Metadata");
            // 6.2.11.4.2: these must list every glyph exactly when present;
            // leaving them out is allowed.
            dictionary.remove(b"CIDSet");
            dictionary.remove(b"CharSet");
            clean_graphics_state(dictionary);
            match name_of(dictionary, b"Subtype") {
                Some(b"Image") => {
                    dictionary.remove(b"Alternates");
                    dictionary.remove(b"OPI");
                    if dictionary
                        .get(b"Interpolate")
                        .and_then(Object::as_bool)
                        .unwrap_or(false)
                    {
                        dictionary.set("Interpolate", false);
                    }
                }
                Some(b"Form") => {
                    for key in [b"OPI".as_slice(), b"Ref", b"Subtype2", b"PS"] {
                        dictionary.remove(key);
                    }
                }
                // 6.2.11.3.2. Absent already means Identity.
                Some(b"CIDFontType2") if !dictionary.has(b"CIDToGIDMap") => {
                    dictionary.set("CIDToGIDMap", "Identity");
                }
                _ => {}
            }
        });
    }
}

/// 6.2.5 and 6.2.10. Transfer functions and halftones only matter to printing
/// devices; overprint mode 1 is refused with ICC-based CMYK, which DeviceCMYK
/// becomes once DefaultCMYK is set.
fn clean_graphics_state(dictionary: &mut Dictionary) {
    dictionary.remove(b"TR");
    if name_of(dictionary, b"TR2") != Some(b"Default") {
        dictionary.remove(b"TR2");
    }
    dictionary.remove(b"HTP");
    dictionary.remove(b"HT");
    if dictionary.get(b"RI").is_ok_and(|intent| {
        !intent
            .as_name()
            .is_ok_and(|intent| RENDERING_INTENTS.contains(&intent))
    }) {
        dictionary.set("RI", "RelativeColorimetric");
    }
    let blend_mode = match dictionary.get(b"BM") {
        Ok(Object::Name(mode)) => Some(mode.as_slice()),
        // An array lists modes in order of preference; take the first standard one.
        Ok(Object::Array(modes)) => Some(
            modes
                .iter()
                .filter_map(|mode| mode.as_name().ok())
                .find(|mode| STANDARD_BLEND_MODES.contains(mode))
                .unwrap_or(b"Normal"),
        ),
        _ => None,
    }
    .map(|mode| {
        if STANDARD_BLEND_MODES.contains(&mode) {
            mode.to_vec()
        } else {
            b"Normal".to_vec()
        }
    });
    if let Some(mode) = blend_mode {
        dictionary.set("BM", Object::Name(mode));
    }
    if dictionary.get(b"OPM").and_then(Object::as_i64).ok() == Some(1) {
        dictionary.set("OPM", 0);
    }
}

/// 6.3: only annotation types ISO 32000-1 defines, each printable, visible and
/// drawn from a single normal appearance.
fn clean_annotations(document: &mut Document) -> Result<(), String> {
    for (_, page_id, annotations) in page_annotations(document) {
        let mut kept = Vec::with_capacity(annotations.len());
        for annotation in annotations {
            let Object::Reference(id) = annotation else {
                // A direct annotation cannot be pointed at by its popup or
                // field; moving it into an object keeps it the same.
                if let Object::Dictionary(dictionary) = annotation {
                    let id = document.add_object(dictionary);
                    if clean_annotation(document, id)? {
                        kept.push(Object::Reference(id));
                    }
                }
                continue;
            };
            if clean_annotation(document, id)? {
                kept.push(Object::Reference(id));
            }
        }
        let page = document
            .get_dictionary_mut(page_id)
            .map_err(|error| format!("A PDF page could not be read: {error}"))?;
        if kept.is_empty() {
            page.remove(b"Annots");
        } else {
            page.set("Annots", kept);
        }
    }
    Ok(())
}

/// Whether the annotation stays.
fn clean_annotation(document: &mut Document, id: ObjectId) -> Result<bool, String> {
    let Ok(annotation) = document.get_dictionary(id) else {
        return Ok(false);
    };
    let subtype = name_of(annotation, b"Subtype").unwrap_or_default().to_vec();
    if !ALLOWED_ANNOTATIONS.contains(&subtype.as_slice()) {
        return Ok(false);
    }
    if subtype == b"Popup" {
        return Ok(true);
    }
    let hidden = is_hidden(annotation);
    let [x0, y0, x1, y1] = rect(document, annotation);
    let is_button = field_type(document, annotation).as_deref() == Some(b"Btn".as_slice());
    let state = name_of(annotation, b"AS").map(<[u8]>::to_vec);
    let normal = annotation
        .get(b"AP")
        .ok()
        .and_then(|value| resolve_dict(document, value))
        .and_then(|appearance| appearance.get(b"N").ok())
        .cloned();

    // A hidden annotation keeps its place, and whatever a form field stores,
    // but draws nothing, which is what it did before.
    let normal = if hidden {
        let empty = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), Object::Real(x1 - x0), Object::Real(y1 - y0)],
            },
            Vec::new(),
        ));
        Some(if is_button {
            let mut states = Dictionary::new();
            let names = normal
                .as_ref()
                .and_then(|normal| resolve_dict(document, normal))
                .map(|states| {
                    states
                        .iter()
                        .map(|(name, _)| name.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            for name in names {
                states.set(name, empty);
            }
            if states.is_empty() {
                states.set(state.clone().unwrap_or_else(|| b"Off".to_vec()), empty);
            }
            Object::Dictionary(states)
        } else {
            Object::Reference(empty)
        })
    } else {
        // 6.3.3: a button's normal appearance is a dictionary of states, any
        // other annotation's a single stream.
        normal.map(|normal| match (is_button, resolve(document, &normal)) {
            (true, Some(Object::Stream(_))) => {
                let name = state.clone().unwrap_or_else(|| b"On".to_vec());
                Object::Dictionary(Dictionary::from_iter([(name, normal)]))
            }
            (false, Some(Object::Dictionary(states))) => state
                .as_ref()
                .and_then(|state| states.get(state).ok())
                .or_else(|| states.iter().map(|(_, stream)| stream).next())
                .cloned()
                .unwrap_or(normal),
            _ => normal,
        })
    };
    let button_state = match (is_button, &normal) {
        (true, Some(Object::Dictionary(states))) if state.is_none() => {
            states.iter().map(|(name, _)| name.clone()).next()
        }
        _ => None,
    };

    let annotation = document
        .get_dictionary_mut(id)
        .map_err(|error| format!("An annotation could not be read: {error}"))?;
    let flags = annotation.get(b"F").and_then(Object::as_i64).unwrap_or(0);
    annotation.set(
        "F",
        (flags | PRINT) & !(INVISIBLE | HIDDEN | NO_VIEW | TOGGLE_NO_VIEW),
    );
    match normal {
        Some(normal) => annotation.set("AP", dictionary! { "N" => normal }),
        None => {
            annotation.remove(b"AP");
        }
    }
    if let Some(state) = button_state {
        annotation.set("AS", Object::Name(state));
    }
    Ok(true)
}

/// A field's type is inherited from its parents (ISO 32000-1, 12.7.3.1).
fn field_type(document: &Document, annotation: &Dictionary) -> Option<Vec<u8>> {
    let mut node = annotation;
    for _ in 0..MAX_NESTING {
        if let Some(kind) = name_of(node, b"FT") {
            return Some(kind.to_vec());
        }
        node = resolve_dict(document, node.get(b"Parent").ok()?)?;
    }
    None
}

/// 6.9: every optional content configuration is named, none sets usage
/// application automatically, and each order lists every group.
fn clean_optional_content(document: &mut Document, catalog_id: ObjectId) {
    let Some(properties) = document
        .get_dictionary(catalog_id)
        .ok()
        .and_then(|catalog| catalog.get(b"OCProperties").ok())
        .and_then(|value| resolve_dict(document, value))
        .cloned()
    else {
        return;
    };
    let groups = properties
        .get(b"OCGs")
        .ok()
        .and_then(|value| resolve(document, value))
        .and_then(|value| value.as_array().ok())
        .map(|groups| {
            groups
                .iter()
                .filter_map(|group| group.as_reference().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut configurations = Vec::new();
    if let Ok(default) = properties.get(b"D") {
        configurations.push(default.clone());
    }
    if let Some(Object::Array(others)) = properties
        .get(b"Configs")
        .ok()
        .and_then(|value| resolve(document, value))
    {
        configurations.extend(others.iter().cloned());
    }

    let mut names = BTreeSet::new();
    let mut fixed = Vec::new();
    for (index, configuration) in configurations.iter().enumerate() {
        let Some(mut dictionary) = resolve_dict(document, configuration).cloned() else {
            continue;
        };
        dictionary.remove(b"AS");
        let name = dictionary
            .get(b"Name")
            .ok()
            .and_then(|name| decode_text_string(name).ok())
            .filter(|name| !name.is_empty() && !names.contains(name))
            .unwrap_or_else(|| {
                (0..)
                    .map(|copy| match (index, copy) {
                        (0, 0) => "Default".to_owned(),
                        (_, 0) => format!("Configuration {index}"),
                        (_, copy) => format!("Configuration {index}.{copy}"),
                    })
                    .find(|name| !names.contains(name))
                    .unwrap()
            });
        dictionary.set("Name", lopdf::text_string(&name));
        names.insert(name);
        if let Some(order) = dictionary
            .get(b"Order")
            .ok()
            .and_then(|value| resolve(document, value))
            .and_then(|value| value.as_array().ok())
        {
            let mut listed = BTreeSet::new();
            collect_references(document, order, 0, &mut listed);
            let mut order = order.clone();
            order.extend(
                groups
                    .iter()
                    .filter(|group| !listed.contains(*group))
                    .map(|group| Object::Reference(*group)),
            );
            dictionary.set("Order", order);
        }
        fixed.push((configuration.clone(), dictionary));
    }

    let mut properties = properties;
    let mut others = Vec::new();
    for (index, (original, dictionary)) in fixed.into_iter().enumerate() {
        let value = match original {
            Object::Reference(id) => {
                document.objects.insert(id, Object::Dictionary(dictionary));
                Object::Reference(id)
            }
            _ => Object::Dictionary(dictionary),
        };
        if index == 0 && properties.has(b"D") {
            properties.set("D", value);
        } else {
            others.push(value);
        }
    }
    if properties.has(b"Configs") {
        properties.set("Configs", others);
    }
    let holder = document
        .get_dictionary(catalog_id)
        .ok()
        .and_then(|catalog| catalog.get(b"OCProperties").ok())
        .and_then(|value| value.as_reference().ok());
    match holder {
        Some(id) => {
            document.objects.insert(id, Object::Dictionary(properties));
        }
        None => {
            if let Ok(catalog) = document.get_dictionary_mut(catalog_id) {
                catalog.set("OCProperties", properties);
            }
        }
    }
}

fn collect_references(
    document: &Document,
    items: &[Object],
    depth: usize,
    found: &mut BTreeSet<ObjectId>,
) {
    if depth > MAX_NESTING {
        return;
    }
    for item in items {
        match item {
            Object::Reference(id) => {
                if let Ok(Object::Array(nested)) = document.get_object(*id) {
                    collect_references(document, nested, depth + 1, found);
                } else {
                    found.insert(*id);
                }
            }
            Object::Array(nested) => collect_references(document, nested, depth + 1, found),
            _ => {}
        }
    }
}

/// PDF/A-3 6.8: each attachment states its file name, media type and
/// relationship to the document, and is associated with it.
fn associate_attachments(document: &mut Document, catalog_id: ObjectId) {
    let mut specifications = Vec::new();
    for (id, object) in &document.objects {
        if object
            .as_dict()
            .is_ok_and(|dictionary| dictionary.has(b"EF"))
        {
            specifications.push(*id);
        }
    }
    // Direct file specifications cannot be listed in /AF; move them out.
    let mut direct = Vec::new();
    for (id, object) in &document.objects {
        if let Ok(holder) = object.as_dict()
            && holder
                .get(b"FS")
                .and_then(Object::as_dict)
                .is_ok_and(|fs| fs.has(b"EF"))
        {
            direct.push(*id);
        }
    }
    for holder in direct {
        let Some(specification) = document
            .get_dictionary_mut(holder)
            .ok()
            .and_then(|holder| holder.remove(b"FS"))
        else {
            continue;
        };
        let id = document.add_object(specification);
        if let Ok(holder) = document.get_dictionary_mut(holder) {
            holder.set("FS", id);
        }
        specifications.push(id);
    }

    for &id in &specifications {
        let streams = {
            let Ok(specification) = document.get_dictionary_mut(id) else {
                continue;
            };
            let file_name = specification
                .get(b"UF")
                .or_else(|_| specification.get(b"F"))
                .cloned()
                .unwrap_or_else(|_| lopdf::text_string("attachment"));
            for key in [b"F".as_slice(), b"UF"] {
                if !specification.has(key) {
                    specification.set(key, file_name.clone());
                }
            }
            if !specification.has(b"AFRelationship") {
                specification.set("AFRelationship", "Unspecified");
            }
            specification
                .get(b"EF")
                .and_then(Object::as_dict)
                .map(|files| {
                    files
                        .iter()
                        .filter_map(|(_, stream)| stream.as_reference().ok())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        for stream in streams {
            if let Ok(Object::Stream(stream)) = document.get_object_mut(stream)
                && !stream.dict.has(b"Subtype")
            {
                stream.dict.set("Subtype", "application/octet-stream");
            }
        }
    }

    if specifications.is_empty() {
        return;
    }
    let Ok(catalog) = document.get_dictionary_mut(catalog_id) else {
        return;
    };
    let mut associated = catalog
        .get(b"AF")
        .and_then(Object::as_array)
        .cloned()
        .unwrap_or_default();
    for id in specifications {
        if !associated.contains(&Object::Reference(id)) {
            associated.push(Object::Reference(id));
        }
    }
    catalog.set("AF", associated);
}

/// 6.2.2: every content stream names its resources itself. A form or tiling
/// pattern without them borrows those of whatever draws it, which is what
/// readers fall back to (pdf.js corpus: bug1873345.pdf); an appearance stream
/// borrows the form's default resources.
fn give_streams_their_resources(
    document: &mut Document,
    catalog_id: ObjectId,
) -> Result<(), String> {
    let lacks_resources = |document: &Document, id: ObjectId| {
        document.get_object(id).is_ok_and(|object| {
            object.as_stream().is_ok_and(|stream| {
                !stream.dict.has(b"Resources")
                    && (name_of(&stream.dict, b"Subtype") == Some(b"Form")
                        || stream.dict.has(b"PatternType")
                        || stream.dict.has(b"BBox"))
            })
        })
    };

    let defaults = document
        .get_dictionary(catalog_id)
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|value| resolve_dict(document, value))
        .and_then(|form| form.get(b"DR").ok())
        .cloned();
    if let Some(defaults) = defaults {
        let mut appearances = Vec::new();
        for (_, _, annotations) in page_annotations(document) {
            for annotation in &annotations {
                let Some(appearance) = resolve_dict(document, annotation)
                    .and_then(|annotation| annotation.get(b"AP").ok())
                    .and_then(|value| resolve_dict(document, value))
                else {
                    continue;
                };
                for (_, kind) in appearance.iter() {
                    match resolve(document, kind) {
                        Some(Object::Dictionary(states)) => appearances.extend(
                            states
                                .iter()
                                .filter_map(|(_, state)| state.as_reference().ok()),
                        ),
                        _ => appearances.extend(kind.as_reference().ok()),
                    }
                }
            }
        }
        for id in appearances {
            if lacks_resources(document, id)
                && let Ok(Object::Stream(stream)) = document.get_object_mut(id)
            {
                stream.dict.set("Resources", defaults.clone());
            }
        }
    }

    // Repeated so a form inside a form that just got resources gets them too.
    loop {
        let mut borrowed: Vec<(ObjectId, Object)> = Vec::new();
        for holder in document.objects.values() {
            let holder = match holder {
                Object::Dictionary(dictionary) => dictionary,
                Object::Stream(stream) => &stream.dict,
                _ => continue,
            };
            let Ok(resources) = holder.get(b"Resources") else {
                continue;
            };
            let Some(dictionary) = resolve_dict(document, resources) else {
                continue;
            };
            for key in [b"XObject".as_slice(), b"Pattern"] {
                let Some(entries) = dictionary
                    .get(key)
                    .ok()
                    .and_then(|value| resolve_dict(document, value))
                else {
                    continue;
                };
                for (_, entry) in entries.iter() {
                    if let Ok(id) = entry.as_reference()
                        && lacks_resources(document, id)
                    {
                        borrowed.push((id, resources.clone()));
                    }
                }
            }
        }
        if borrowed.is_empty() {
            return Ok(());
        }
        borrowed.sort_by_key(|(id, _)| *id);
        // Two different lenders would mean one of them draws it with names
        // the borrowed set does not define.
        for pair in borrowed.windows(2) {
            if pair[0].0 == pair[1].0 && pair[0].1 != pair[1].1 {
                return Err(
                    "A drawing in this PDF is shared by pages with different resources, which PDF/A cannot express."
                        .into(),
                );
            }
        }
        borrowed.dedup_by_key(|(id, _)| *id);
        for (id, resources) in borrowed {
            if let Ok(Object::Stream(stream)) = document.get_object_mut(id) {
                stream.dict.set("Resources", resources);
            }
        }
    }
}

/// 6.2.4: colour must be device independent. The output intent makes the
/// device space it describes so; other device spaces in use get a Default
/// colour space in every resource dictionary, which reinterprets them through
/// an ICC profile without touching the content.
fn set_color_spaces(document: &mut Document, catalog_id: ObjectId) -> Result<(), String> {
    let components = match existing_output_intent(document, catalog_id) {
        Some((intent, components)) => {
            set_catalog(document, catalog_id, "OutputIntents", vec![intent.into()])?;
            components
        }
        None => {
            let profile =
                document.add_object(Stream::new(dictionary! { "N" => 3 }, SRGB_PROFILE.to_vec()));
            let intent = document.add_object(dictionary! {
                "Type" => "OutputIntent",
                "S" => "GTS_PDFA1",
                "OutputConditionIdentifier" => Object::string_literal("sRGB"),
                "Info" => Object::string_literal("sRGB IEC61966-2.1"),
                "RegistryName" => Object::string_literal("http://www.color.org"),
                "DestOutputProfile" => profile,
            });
            set_catalog(document, catalog_id, "OutputIntents", vec![intent.into()])?;
            3
        }
    };

    let (uses_rgb, uses_cmyk) = device_colors_in_use(document);
    let mut defaults = Vec::new();
    if uses_cmyk && components != 4 {
        let profile =
            document.add_object(Stream::new(dictionary! { "N" => 4 }, CMYK_PROFILE.to_vec()));
        defaults.push((b"DefaultCMYK".as_slice(), profile));
    }
    if uses_rgb && components != 3 {
        let profile =
            document.add_object(Stream::new(dictionary! { "N" => 3 }, SRGB_PROFILE.to_vec()));
        defaults.push((b"DefaultRGB".as_slice(), profile));
    }
    if defaults.is_empty() {
        return Ok(());
    }

    // Default colour spaces only reach what content streams select. veraPDF
    // does not apply them to a shading reached through a pattern (pdf.js
    // corpus: issue20513.pdf) or to a DeviceN alternate there
    // (bug1703683_page2_reduced.pdf), so wherever an object names a device
    // space it names the profile instead. It is the same colour either way.
    let replacements = defaults
        .iter()
        .map(|(name, profile)| {
            let device = match *name {
                b"DefaultCMYK" => b"DeviceCMYK".as_slice(),
                _ => b"DeviceRGB",
            };
            (device.to_vec(), *profile)
        })
        .collect::<Vec<_>>();
    for object in document.objects.values_mut() {
        name_profiles(object, &replacements, false, 0);
    }

    // A page with no resources anywhere up its tree gets an empty set, so it
    // has somewhere to hold the defaults. A form without resources borrows
    // its page's, so it must stay without.
    for page_id in document.get_pages().into_values().collect::<Vec<_>>() {
        let has_resources = {
            let page = document.get_dictionary(page_id).ok();
            page.is_some_and(|page| {
                page.has(b"Resources")
                    || crate::documents::inheritable_attributes(document, page)
                        .iter()
                        .any(|(key, _)| *key == b"Resources")
            })
        };
        if !has_resources && let Ok(page) = document.get_dictionary_mut(page_id) {
            page.set("Resources", Dictionary::new());
        }
    }

    let holders = document
        .objects
        .iter()
        .filter(|(_, object)| match object {
            Object::Dictionary(dictionary) => dictionary.has(b"Resources"),
            Object::Stream(stream) => stream.dict.has(b"Resources"),
            _ => false,
        })
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    let mut done = BTreeSet::new();
    for holder in holders {
        let resources = holder_dictionary(document, holder)
            .and_then(|holder| holder.get(b"Resources").ok())
            .cloned();
        let resources_id = match resources {
            Some(Object::Reference(id)) => id,
            Some(Object::Dictionary(resources)) => {
                // Moved into an object of its own so it can be edited below
                // the same way as a shared one.
                let id = document.add_object(resources);
                if let Some(holder) = holder_dictionary_mut(document, holder) {
                    holder.set("Resources", id);
                }
                id
            }
            _ => continue,
        };
        if !done.insert(resources_id) {
            continue;
        }
        let color_spaces = document
            .get_dictionary(resources_id)
            .ok()
            .and_then(|resources| resources.get(b"ColorSpace").ok())
            .cloned();
        let mut entries = match &color_spaces {
            Some(Object::Reference(id)) => {
                document.get_dictionary(*id).cloned().unwrap_or_default()
            }
            Some(Object::Dictionary(entries)) => entries.clone(),
            _ => Dictionary::new(),
        };
        for (name, profile) in &defaults {
            if !entries.has(name) {
                entries.set(
                    *name,
                    vec![Object::Name(b"ICCBased".to_vec()), (*profile).into()],
                );
            }
        }
        match color_spaces {
            Some(Object::Reference(id)) if !done.contains(&id) => {
                done.insert(id);
                document.objects.insert(id, Object::Dictionary(entries));
            }
            Some(Object::Reference(_)) => {}
            _ => {
                if let Ok(resources) = document.get_dictionary_mut(resources_id) {
                    resources.set("ColorSpace", entries);
                }
            }
        }
    }
    Ok(())
}

/// Replaces device colour space names with ICC-based spaces where they define
/// a colour space: under /ColorSpace and /CS, in a resource /ColorSpace map,
/// and as the alternate or base of Separation, DeviceN and Indexed spaces.
fn name_profiles(
    object: &mut Object,
    replacements: &[(Vec<u8>, ObjectId)],
    defines_space: bool,
    depth: usize,
) {
    if depth > MAX_NESTING {
        return;
    }
    let replacement = |name: &[u8]| {
        replacements
            .iter()
            .find(|(device, _)| device == name)
            .map(|(_, profile)| {
                Object::Array(vec![Object::Name(b"ICCBased".to_vec()), (*profile).into()])
            })
    };
    match object {
        Object::Name(name) if defines_space => {
            if let Some(space) = replacement(name) {
                *object = space;
            }
        }
        Object::Array(items) => {
            let based_at = match items.first().and_then(|first| first.as_name().ok()) {
                Some(b"Separation" | b"DeviceN" | b"NChannel") => Some(2),
                Some(b"Indexed" | b"Pattern") => Some(1),
                _ => None,
            };
            for (index, item) in items.iter_mut().enumerate() {
                name_profiles(item, replacements, Some(index) == based_at, depth + 1);
            }
        }
        Object::Dictionary(_) | Object::Stream(_) => {
            let dictionary = match object {
                Object::Dictionary(dictionary) => dictionary,
                Object::Stream(stream) => &mut stream.dict,
                _ => unreachable!(),
            };
            let is_map = defines_space;
            for (key, value) in dictionary.iter_mut() {
                let defines = is_map || key == b"ColorSpace" || key == b"CS";
                // A resource /ColorSpace map holds spaces, not a space.
                let map = key == b"ColorSpace" && matches!(value, Object::Dictionary(_));
                if map {
                    if let Object::Dictionary(spaces) = value {
                        for (_, space) in spaces.iter_mut() {
                            name_profiles(space, replacements, true, depth + 2);
                        }
                    }
                } else {
                    name_profiles(value, replacements, defines, depth + 1);
                }
            }
        }
        _ => {}
    }
}

/// 6.1.13: integers fit in 32 bits. Larger ones are garbage that no reader
/// interprets the same way (pdf.js corpus: issue3928.pdf).
fn clamp_integers(object: &mut Object, depth: usize) {
    if depth > MAX_NESTING {
        return;
    }
    match object {
        Object::Integer(value) => *value = (*value).clamp(i32::MIN as i64, i32::MAX as i64),
        Object::Array(items) => {
            for item in items {
                clamp_integers(item, depth + 1);
            }
        }
        Object::Dictionary(dictionary) => {
            for (_, value) in dictionary.iter_mut() {
                clamp_integers(value, depth + 1);
            }
        }
        Object::Stream(stream) => {
            for (_, value) in stream.dict.iter_mut() {
                clamp_integers(value, depth + 1);
            }
        }
        _ => {}
    }
}

fn holder_dictionary(document: &Document, id: ObjectId) -> Option<&Dictionary> {
    match document.get_object(id).ok()? {
        Object::Dictionary(dictionary) => Some(dictionary),
        Object::Stream(stream) => Some(&stream.dict),
        _ => None,
    }
}

fn holder_dictionary_mut(document: &mut Document, id: ObjectId) -> Option<&mut Dictionary> {
    match document.get_object_mut(id).ok()? {
        Object::Dictionary(dictionary) => Some(dictionary),
        Object::Stream(stream) => Some(&mut stream.dict),
        _ => None,
    }
}

fn set_catalog(
    document: &mut Document,
    catalog_id: ObjectId,
    key: &str,
    value: impl Into<Object>,
) -> Result<(), String> {
    document
        .get_dictionary_mut(catalog_id)
        .map_err(|_| "The PDF has no document catalog.".to_string())?
        .set(key, value);
    Ok(())
}

/// A PDF/A output intent the file already has, kept so a document made for a
/// print condition keeps it. Only one may stay: 6.2.3 lets every intent with a
/// profile name only the same profile.
fn existing_output_intent(document: &Document, catalog_id: ObjectId) -> Option<(ObjectId, i64)> {
    let intents = document
        .get_dictionary(catalog_id)
        .ok()?
        .get(b"OutputIntents")
        .ok()
        .and_then(|value| resolve(document, value))?
        .as_array()
        .ok()?;
    intents.iter().find_map(|intent| {
        let id = intent.as_reference().ok()?;
        let dictionary = document.get_dictionary(id).ok()?;
        if name_of(dictionary, b"S") != Some(b"GTS_PDFA1") {
            return None;
        }
        let profile = dictionary
            .get(b"DestOutputProfile")
            .ok()
            .and_then(|value| resolve(document, value))?
            .as_stream()
            .ok()?;
        let components = profile.dict.get(b"N").and_then(Object::as_i64).ok()?;
        matches!(components, 1 | 3 | 4).then_some((id, components))
    })
}

/// Whether anything paints in DeviceRGB or DeviceCMYK. Errs towards yes: a
/// needless Default colour space costs a few kilobytes, a missed one makes
/// the file non-conforming.
fn device_colors_in_use(document: &Document) -> (bool, bool) {
    let mut rgb = false;
    let mut cmyk = false;
    let note_space = |name: &[u8], rgb: &mut bool, cmyk: &mut bool| match name {
        b"DeviceRGB" | b"RGB" => *rgb = true,
        b"DeviceCMYK" | b"CMYK" => *cmyk = true,
        _ => {}
    };
    for object in document.objects.values() {
        names_in(object, 0, &mut |name| note_space(name, &mut rgb, &mut cmyk));
    }
    for stream in content_streams(document) {
        let Ok(bytes) = stream.decompressed_content_with_limit(MAX_DECOMPRESSED_STREAM) else {
            // Undecodable content draws nothing in most viewers, but it might
            // in some.
            return (true, true);
        };
        let Ok(content) = Content::decode(&bytes) else {
            return (true, true);
        };
        for operation in &content.operations {
            match operation.operator.as_str() {
                "rg" | "RG" => rgb = true,
                "k" | "K" => cmyk = true,
                "BI" => {
                    for operand in &operation.operands {
                        names_in(operand, 0, &mut |name| {
                            note_space(name, &mut rgb, &mut cmyk)
                        });
                    }
                }
                _ => {}
            }
        }
    }
    (rgb, cmyk)
}

fn names_in(object: &Object, depth: usize, found: &mut dyn FnMut(&[u8])) {
    if depth > MAX_NESTING {
        return;
    }
    match object {
        Object::Name(name) => found(name),
        Object::Array(items) => {
            for item in items {
                names_in(item, depth + 1, found);
            }
        }
        Object::Dictionary(dictionary) => {
            for (_, value) in dictionary.iter() {
                names_in(value, depth + 1, found);
            }
        }
        Object::Stream(stream) => {
            for (_, value) in stream.dict.iter() {
                names_in(value, depth + 1, found);
            }
        }
        _ => {}
    }
}

/// Page contents, forms, patterns, appearance streams and Type 3 glyphs.
fn content_streams(document: &Document) -> Vec<&Stream> {
    let mut ids = BTreeSet::new();
    for page_id in document.get_pages().into_values() {
        ids.extend(document.get_page_contents(page_id));
    }
    for (id, object) in &document.objects {
        match object {
            Object::Stream(stream) if stream.dict.has(b"BBox") => {
                ids.insert(*id);
            }
            Object::Dictionary(font) if name_of(font, b"Subtype") == Some(b"Type3") => {
                if let Some(procedures) = font
                    .get(b"CharProcs")
                    .ok()
                    .and_then(|value| resolve_dict(document, value))
                {
                    ids.extend(
                        procedures
                            .iter()
                            .filter_map(|(_, procedure)| procedure.as_reference().ok()),
                    );
                }
            }
            _ => {}
        }
    }
    ids.into_iter()
        .filter_map(|id| document.get_object(id).ok()?.as_stream().ok())
        .collect()
}

/// 6.6.2: XMP in the catalog identifying the PDF/A part and level, carrying
/// what the document information dictionary says. Written uncompressed, as
/// 6.6.2.1 asks, so tools that do not parse PDF can still find it.
fn set_metadata(
    document: &mut Document,
    catalog_id: ObjectId,
    level: PdfALevel,
) -> Result<(), String> {
    let info_id = document
        .trailer
        .get(b"Info")
        .and_then(Object::as_reference)
        .ok();
    let mut info = info_id
        .and_then(|id| document.get_dictionary(id).ok())
        .cloned()
        .unwrap_or_default();

    let text = |key: &[u8]| {
        info.get(key)
            .ok()
            .and_then(|value| decode_text_string(value).ok())
            .map(|value| value.trim_end_matches('\0').to_owned())
            .filter(|value| !value.is_empty())
    };
    let title = text(b"Title");
    let author = text(b"Author");
    let subject = text(b"Subject");
    let keywords = text(b"Keywords");
    let creator = text(b"Creator");
    let producer = text(b"Producer");
    let created = text(b"CreationDate");
    let modified = text(b"ModDate");
    let created_xmp = created.as_deref().and_then(xmp_date);
    let modified_xmp = modified.as_deref().and_then(xmp_date);
    // A date XMP cannot express is dropped from both, so the two agree.
    if created.is_some() && created_xmp.is_none() {
        info.remove(b"CreationDate");
    }
    if modified.is_some() && modified_xmp.is_none() {
        info.remove(b"ModDate");
    }

    let mut xmp = String::from(
        "<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n",
    );
    xmp.push_str(&format!(
        "<rdf:Description rdf:about=\"\" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\">\n\
         <pdfaid:part>{}</pdfaid:part>\n<pdfaid:conformance>B</pdfaid:conformance>\n\
         </rdf:Description>\n",
        level.part()
    ));
    let mut dc = String::new();
    if let Some(title) = &title {
        dc.push_str(&format!(
            "<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title>\n",
            escape(title)
        ));
    }
    if let Some(author) = &author {
        dc.push_str(&format!(
            "<dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator>\n",
            escape(author)
        ));
    }
    if let Some(subject) = &subject {
        dc.push_str(&format!(
            "<dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:description>\n",
            escape(subject)
        ));
    }
    if !dc.is_empty() {
        xmp.push_str(&format!(
            "<rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n{dc}</rdf:Description>\n"
        ));
    }
    let mut basic = String::new();
    if let Some(creator) = &creator {
        basic.push_str(&format!(
            "<xmp:CreatorTool>{}</xmp:CreatorTool>\n",
            escape(creator)
        ));
    }
    if let Some(date) = &created_xmp {
        basic.push_str(&format!("<xmp:CreateDate>{date}</xmp:CreateDate>\n"));
    }
    if let Some(date) = &modified_xmp {
        basic.push_str(&format!("<xmp:ModifyDate>{date}</xmp:ModifyDate>\n"));
    }
    if !basic.is_empty() {
        xmp.push_str(&format!(
            "<rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">\n{basic}</rdf:Description>\n"
        ));
    }
    let mut pdf = String::new();
    if let Some(producer) = &producer {
        pdf.push_str(&format!(
            "<pdf:Producer>{}</pdf:Producer>\n",
            escape(producer)
        ));
    }
    if let Some(keywords) = &keywords {
        pdf.push_str(&format!(
            "<pdf:Keywords>{}</pdf:Keywords>\n",
            escape(keywords)
        ));
    }
    if !pdf.is_empty() {
        xmp.push_str(&format!(
            "<rdf:Description rdf:about=\"\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\">\n{pdf}</rdf:Description>\n"
        ));
    }
    xmp.push_str("</rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>");

    let metadata = document.add_object(
        Stream::new(
            dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
            xmp.into_bytes(),
        )
        .with_compression(false),
    );
    set_catalog(document, catalog_id, "Metadata", metadata)?;
    match info_id {
        Some(id) => {
            document.objects.insert(id, Object::Dictionary(info));
        }
        None => {
            document.trailer.remove(b"Info");
        }
    }
    Ok(())
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            // XML 1.0 cannot hold most control characters, even escaped.
            character if (character as u32) < 0x20 && !matches!(character, '\t' | '\n' | '\r') => {}
            character => escaped.push(character),
        }
    }
    escaped
}

/// `D:YYYYMMDDHHmmSSOHH'mm'` (ISO 32000-1, 7.9.4), every part after the year
/// optional, to the ISO 8601 form XMP uses.
pub(crate) fn xmp_date(text: &str) -> Option<String> {
    let text = text.trim();
    let text = text.strip_prefix("D:").unwrap_or(text);
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let (numbers, zone) = text.split_at(digits);
    if numbers.len() < 4 || numbers.len() > 14 || numbers.len() % 2 != 0 {
        return None;
    }
    let field = |from: usize, default: u32| {
        numbers
            .get(from..from + 2)
            .map_or(Some(default), |part| part.parse::<u32>().ok())
    };
    let year = numbers[..4].parse::<u32>().ok()?;
    let month = field(4, 1)?;
    let day = field(6, 1)?;
    let hour = field(8, 0)?;
    let minute = field(10, 0)?;
    let second = field(12, 0)?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let mut date = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}");
    let zone = zone.trim_end_matches('\'');
    match zone.as_bytes().first() {
        None => {}
        Some(b'Z') => date.push('Z'),
        Some(sign @ (b'+' | b'-')) => {
            let parts = zone[1..]
                .split('\'')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            let hours = parts.first()?.parse::<u32>().ok()?;
            let minutes = parts
                .get(1)
                .map_or(Some(0), |part| part.parse::<u32>().ok())?;
            if hours > 23 || minutes > 59 || parts.len() > 2 {
                return None;
            }
            date.push_str(&format!("{}{hours:02}:{minutes:02}", *sign as char));
        }
        Some(_) => return None,
    }
    Some(date)
}
