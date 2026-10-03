//! Redaction: what lies under each box is taken out of the file, then the box
//! is painted where it was.
//!
//! Covering is not removing. A black rectangle over text leaves the text in
//! the file, where anyone can select, copy or search it, so each page's
//! content is rewritten without it (see `content.rs`), and annotations and
//! form fields under a box go too. Text that describes the removed content
//! for accessibility goes with it.
//!
//! When something under a box cannot be removed exactly, such as an image in
//! a format the engine cannot decode or text in a font it cannot measure, the
//! page is never written with it still there. The caller is told which pages
//! need a picture, renders them with the boxes painted in, and passes the
//! pictures back; those pages are replaced by them.

mod content;
mod decode;
mod fonts;
mod geometry;
mod lexer;
mod pixels;

use std::collections::{BTreeMap, BTreeSet};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

use content::{Context, NeedsImage, open_states, own_subdictionary, rewrite};
use decode::decode;
use geometry::{IDENTITY, Rect};
use lexer::format_number;

use crate::compression::strip_metadata;
use crate::documents::{MAX_PAGE_TREE_DEPTH, load_document};
use crate::flatten::{self, prune_fields, remove_orphaned_popups};
use crate::images::{image_matrix, prepare_image};
use crate::stamps::{Frame, Stamper, finish, page_frame, rectangle, resolve, selected_pages};

pub struct Redaction {
    /// From 1.
    pub page: u32,
    /// Left, top, right and bottom edges as fractions of the visible page's
    /// width and height, measured from its top left as the reader sees it.
    pub area: [f32; 4],
}

/// A picture of a whole page as the reader sees it, with its boxes already
/// painted in, to draw instead of the page's content. JPG or PNG.
pub struct PageImage<'a> {
    pub page: u32,
    pub image: &'a [u8],
}

pub struct RedactOptions<'a> {
    pub redactions: &'a [Redaction],
    /// Red, green and blue, each 0 to 1.
    pub color: [f32; 3],
    /// Also removes the document's properties and XMP metadata, which can
    /// name what was redacted.
    pub remove_metadata: bool,
    pub page_images: &'a [PageImage<'a>],
}

/// Why a page has to be drawn from a picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unremovable {
    /// Text under a box is in a font whose widths cannot be worked out, so
    /// which glyphs lie under it is unknown.
    Text,
    /// An image under a box is in a format the engine cannot decode.
    Image,
    /// The page's content cannot be read.
    Content,
}

pub enum Redacted {
    Done {
        bytes: Vec<u8>,
        /// Pages drawn from a picture.
        imaged: Vec<u32>,
    },
    /// These pages have something under a box that cannot be removed in
    /// place. Nothing was written; call again with a picture of each.
    NeedsImages(Vec<(u32, Unremovable)>),
}

/// Where each glyph on a page sits and what it reads, for finding text.
pub struct PageText {
    /// Left, top, right and bottom as fractions of the visible page, the same
    /// way as [`Redaction::area`].
    pub boxes: Vec<[f32; 4]>,
    pub text: Vec<String>,
}

pub fn redact_pdf_bytes(
    input: &[u8],
    password: &str,
    options: RedactOptions<'_>,
) -> Result<Redacted, String> {
    if options.redactions.is_empty() {
        return Err("Mark at least one area to redact.".into());
    }
    if !options
        .color
        .iter()
        .all(|channel| (0.0..=1.0).contains(channel))
    {
        return Err("Choose a valid fill color.".into());
    }
    let mut areas = BTreeMap::<u32, Vec<[f32; 4]>>::new();
    for redaction in options.redactions {
        let [left, top, right, bottom] = redaction.area;
        if !redaction.area.iter().all(|edge| (0.0..=1.0).contains(edge))
            || left >= right
            || top >= bottom
        {
            return Err("Mark areas inside the page.".into());
        }
        areas
            .entry(redaction.page)
            .or_default()
            .push(redaction.area);
    }
    let mut images = BTreeMap::new();
    for picture in options.page_images {
        if !areas.contains_key(&picture.page) {
            return Err(format!("Page {} has nothing to redact.", picture.page));
        }
        images.insert(picture.page, picture.image);
    }
    let fill = options.color.map(f64::from);

    let mut document = load_document(input, 1, password)?;
    let numbers = areas.keys().copied().collect::<Vec<_>>();
    let pages = selected_pages(&document, &numbers)?;
    let mut stamper = Stamper::new(&mut document, 1.0);
    let mut needs = Vec::new();
    let mut touched = Vec::new();

    for &(page, page_id) in &pages {
        let frame = page_frame(&document, page_id)?;
        let boxes = areas[&page]
            .iter()
            .filter_map(|area| page_area(&frame, *area))
            .collect::<Vec<_>>();
        if let Some(picture) = images.get(&page) {
            draw_picture(&mut document, page_id, &frame, picture, &boxes, fill)?;
            touched.push((page_id, None));
            continue;
        }
        match redact_page(&mut document, &mut stamper, page_id, &boxes, fill) {
            Ok(mcids) => touched.push((page_id, Some(mcids))),
            Err(NeedsImage(reason)) => needs.push((page, reason)),
        }
    }
    if !needs.is_empty() {
        return Ok(Redacted::NeedsImages(needs));
    }

    let catalog_id = document
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|_| "This PDF has no catalog.".to_string())?;
    let mut removed = BTreeSet::new();
    let mut widgets = BTreeSet::new();
    for &(page, page_id) in &pages {
        let frame = page_frame(&document, page_id)?;
        let boxes = areas[&page]
            .iter()
            .filter_map(|area| page_area(&frame, *area))
            .collect::<Vec<_>>();
        remove_annotations(
            &mut document,
            page_id,
            &boxes,
            images.contains_key(&page),
            &mut removed,
            &mut widgets,
        )?;
        let dictionary = document
            .get_dictionary_mut(page_id)
            .map_err(|error| format!("A PDF page could not be read: {error}"))?;
        // Each of these can still show or hold the page as it was: an
        // embedded thumbnail, an editor's private copy, and page metadata.
        for key in [b"Thumb".as_slice(), b"PieceInfo", b"Metadata"] {
            dictionary.remove(key);
        }
    }
    remove_orphaned_popups(&mut document, &removed)?;
    if !widgets.is_empty() {
        prune_fields(&mut document, catalog_id, &widgets);
    }
    // Deleted outright, so nothing that still points at them (a structure
    // element, another annotation's reply) keeps what they said.
    for id in &removed {
        document.objects.remove(id);
    }
    strip_descriptions(&mut document, catalog_id, &touched);
    if options.remove_metadata {
        strip_metadata(&mut document);
    }
    Ok(Redacted::Done {
        bytes: finish(document, 1.0)?,
        imaged: images.keys().copied().collect(),
    })
}

/// Every page's glyphs, for finding text and showing what a box removes.
/// Pages the engine cannot read come back empty.
pub fn page_texts(input: &[u8], password: &str) -> Result<Vec<PageText>, String> {
    let mut document = load_document(input, 1, password)?;
    let pages = selected_pages(&document, &[])?;
    let mut texts = Vec::with_capacity(pages.len());
    for (_, page_id) in pages {
        let mut found = PageText {
            boxes: Vec::new(),
            text: Vec::new(),
        };
        let frame = page_frame(&document, page_id)?;
        let frame_matrix = frame.matrix.map(f64::from);
        let to_frame = geometry::invert(frame_matrix);
        let (Some(to_frame), Ok(content)) = (to_frame, page_content(&document, page_id)) else {
            texts.push(found);
            continue;
        };
        let resources = effective_resources(&document, page_id);
        let mut context = Context::new(&[], [0.0; 3]);
        context.found = Some(Vec::new());
        let _ = rewrite(
            &mut document,
            &mut context,
            &content,
            resources,
            IDENTITY,
            0,
        );
        let (width, height) = (f64::from(frame.width), f64::from(frame.height));
        for glyph in context.found.unwrap_or_default() {
            let Some(shown) = glyph.bounds.transformed(to_frame) else {
                continue;
            };
            let area = [
                shown.x0 / width,
                1.0 - shown.y1 / height,
                shown.x1 / width,
                1.0 - shown.y0 / height,
            ];
            // Off the visible page: nobody can see it to look for it.
            if area[2] <= 0.0 || area[0] >= 1.0 || area[3] <= 0.0 || area[1] >= 1.0 {
                continue;
            }
            found.boxes.push(area.map(|value| value as f32));
            found.text.push(glyph.text);
        }
        texts.push(found);
    }
    Ok(texts)
}

/// `area`, given as fractions of the visible page, in the page's own space.
fn page_area(frame: &Frame, [left, top, right, bottom]: [f32; 4]) -> Option<Rect> {
    let (width, height) = (f64::from(frame.width), f64::from(frame.height));
    let [left, top, right, bottom] = [left, top, right, bottom].map(f64::from);
    // The frame's origin is the bottom left, so top and bottom swap.
    let shown = Rect {
        x0: left * width,
        y0: (1.0 - bottom) * height,
        x1: right * width,
        y1: (1.0 - top) * height,
    };
    // f32 fractions land a hair off round numbers (50.0000016 for 50), so
    // edges are kept to a thousandth of a point, as cropping does.
    let round = |value: f64| (value * 1000.0).round() / 1000.0;
    shown
        .transformed(frame.matrix.map(f64::from))
        .map(|area| Rect {
            x0: round(area.x0),
            y0: round(area.y0),
            x1: round(area.x1),
            y1: round(area.y1),
        })
}

/// The page's content streams decoded and joined, or `NeedsImage` when one
/// cannot be decoded and so cannot be checked.
fn page_content(document: &Document, page_id: ObjectId) -> Result<Vec<u8>, NeedsImage> {
    let page = document
        .get_dictionary(page_id)
        .map_err(|_| NeedsImage(Unremovable::Content))?;
    let streams = match page.get(b"Contents") {
        Ok(Object::Reference(id)) => match document.get_object(*id) {
            Ok(Object::Array(items)) => items.clone(),
            Ok(_) => vec![Object::Reference(*id)],
            Err(_) => Vec::new(),
        },
        Ok(Object::Array(items)) => items.clone(),
        Ok(Object::Stream(stream)) => vec![Object::Stream(stream.clone())],
        _ => Vec::new(),
    };
    let mut content = Vec::new();
    for item in &streams {
        let Some(Object::Stream(stream)) = resolve(document, item) else {
            continue;
        };
        let Some((decoded, false)) = decode(document, stream) else {
            return Err(NeedsImage(Unremovable::Content));
        };
        content.extend_from_slice(&decoded);
        // Streams split only between tokens, but may not end in white space.
        content.push(b'\n');
    }
    Ok(content)
}

/// The page's resources, its own or the nearest ancestor's.
fn effective_resources(document: &Document, page_id: ObjectId) -> Dictionary {
    let mut node = Some(page_id);
    for _ in 0..MAX_PAGE_TREE_DEPTH {
        let Some(dictionary) = node.and_then(|id| document.get_dictionary(id).ok()) else {
            break;
        };
        if let Ok(resources) = dictionary.get(b"Resources") {
            return resolve(document, resources)
                .and_then(|value| value.as_dict().ok())
                .cloned()
                .unwrap_or_default();
        }
        node = dictionary
            .get(b"Parent")
            .and_then(Object::as_reference)
            .ok();
    }
    Dictionary::new()
}

/// Rewrites the page without what lies under `boxes` and paints them, or
/// leaves it untouched and says it needs a picture. Returns the marked
/// content whose content changed.
fn redact_page(
    document: &mut Document,
    stamper: &mut Stamper,
    page_id: ObjectId,
    boxes: &[Rect],
    fill: [f64; 3],
) -> Result<BTreeSet<i64>, NeedsImage> {
    let mut content = page_content(document, page_id)?;
    let mut resources = stamper.own_resources(document, page_id);
    // Annotations under a box are drawn into the page first, as flattening
    // would, so only the part of them under the box goes rather than all of
    // a form field that merely touches it.
    let mut drawing = String::new();
    for (index, annotation) in annotations_under(document, page_id, boxes)
        .iter()
        .enumerate()
    {
        let Some(appearance) = flatten::appearance(document, page_id, annotation) else {
            continue;
        };
        let name = format!("Annotation{index}");
        own_subdictionary(document, &mut resources, b"XObject")
            .set(name.as_bytes(), appearance.form);
        let matrix = appearance
            .matrix
            .map(|value| format_number(f64::from(value)));
        let draw = format!("{} cm\n/{name} Do\n", matrix.join(" "));
        // Optional content still decides whether it shows (8.11.3.2).
        match appearance.optional {
            Some(optional) => {
                let tag = format!("AnnotationContent{index}");
                own_subdictionary(document, &mut resources, b"Properties")
                    .set(tag.as_bytes(), optional);
                drawing.push_str(&format!("q\n/OC /{tag} BDC\n{draw}EMC\nQ\n"));
            }
            None => drawing.push_str(&format!("q\n{draw}Q\n")),
        }
    }
    if !drawing.is_empty() {
        let open = open_states(&content);
        let mut combined = b"q\n".to_vec();
        combined.extend_from_slice(&content);
        combined.extend(b"\nQ\n".repeat(open + 1));
        combined.extend_from_slice(drawing.as_bytes());
        content = combined;
    }
    let mut context = Context::new(boxes, fill);
    let rewritten = rewrite(document, &mut context, &content, resources, IDENTITY, 0)?;
    // Wrapped so the boxes are drawn in the page's own space whatever the
    // content left behind, including states it saved and never restored.
    let mut stream = b"q\n".to_vec();
    stream.extend_from_slice(&rewritten.content);
    stream.extend(b"Q\n".repeat(rewritten.open + 1));
    stream.extend(fill_boxes(boxes, fill));
    let contents = document.add_object(Stream::new(dictionary! {}, stream));
    let page = document
        .get_dictionary_mut(page_id)
        .map_err(|_| NeedsImage(Unremovable::Content))?;
    page.set("Contents", contents);
    page.set("Resources", rewritten.resources);
    Ok(rewritten.touched)
}

fn fill_boxes(boxes: &[Rect], [red, green, blue]: [f64; 3]) -> Vec<u8> {
    let mut drawing = format!(
        "q\n{} {} {} rg\n",
        format_number(red),
        format_number(green),
        format_number(blue)
    );
    for area in boxes {
        drawing.push_str(&format!(
            "{} {} {} {} re\n",
            format_number(area.x0),
            format_number(area.y0),
            format_number(area.width()),
            format_number(area.height())
        ));
    }
    drawing.push_str("f\nQ\n");
    drawing.into_bytes()
}

/// Replaces the page's content with `picture`, drawn over the visible page,
/// and paints the boxes again so their edges stay sharp.
fn draw_picture(
    document: &mut Document,
    page_id: ObjectId,
    frame: &Frame,
    picture: &[u8],
    boxes: &[Rect],
    fill: [f64; 3],
) -> Result<(), String> {
    let mut image = prepare_image(picture)
        .map_err(|error| format!("A page picture could not be used: {error}"))?;
    if let Some(mask) = image.mask.take() {
        let mask_id = document.add_object(mask);
        image.stream.dict.set("SMask", mask_id);
    }
    let image_id = document.add_object(image.stream);
    let number = |values: [f32; 6]| {
        values
            .map(|value| format_number(f64::from(value)))
            .join(" ")
    };
    let mut content = format!(
        "q\n{} cm\n{} cm\n/Page Do\nQ\n",
        number(frame.matrix),
        number(image_matrix(
            image.orientation,
            0.0,
            0.0,
            frame.width,
            frame.height
        ))
    )
    .into_bytes();
    content.extend(fill_boxes(boxes, fill));
    let contents = document.add_object(Stream::new(dictionary! {}, content));
    let page = document
        .get_dictionary_mut(page_id)
        .map_err(|error| format!("A PDF page could not be read: {error}"))?;
    page.set("Contents", contents);
    page.set(
        "Resources",
        dictionary! { "XObject" => dictionary! { "Page" => image_id } },
    );
    Ok(())
}

/// Whether a box overlaps what the annotation shows: its rectangle, and the
/// points readers draw it from when it has no stored appearance, which need
/// not lie inside it (pdf.js corpus: issue13447.pdf, whose ink is drawn well
/// outside its /Rect). One with nothing to place it by counts as under, since
/// where it shows cannot be known.
fn under_boxes(document: &Document, annotation: &Dictionary, boxes: &[Rect]) -> bool {
    let rect = annotation
        .get(b"Rect")
        .ok()
        .and_then(|value| rectangle(document, value))
        .map(|[x0, y0, x1, y1]| Rect {
            x0: f64::from(x0),
            y0: f64::from(y0),
            x1: f64::from(x1),
            y1: f64::from(y1),
        });
    let numbers = |value: &Object| -> Vec<f64> {
        match resolve(document, value) {
            Some(Object::Array(items)) => items
                .iter()
                .flat_map(|item| match resolve(document, item) {
                    Some(Object::Array(inner)) => inner
                        .iter()
                        .filter_map(|value| resolve(document, value)?.as_float().ok())
                        .map(f64::from)
                        .collect(),
                    Some(value) => value.as_float().ok().map(f64::from).into_iter().collect(),
                    None => Vec::new(),
                })
                .collect(),
            _ => Vec::new(),
        }
    };
    let points = [b"InkList".as_slice(), b"QuadPoints", b"Vertices", b"L"]
        .iter()
        .filter_map(|key| annotation.get(key).ok())
        .flat_map(numbers)
        .collect::<Vec<_>>();
    let drawn = Rect::around(
        points
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[x, y]| (x, y))
            .filter(|(x, y)| x.is_finite() && y.is_finite()),
    );
    if rect.is_none() && drawn.is_none() {
        return true;
    }
    [rect, drawn].into_iter().flatten().any(|area| {
        boxes.iter().any(|found| {
            found.overlaps(area) || (area.area() == 0.0 && found.contains((area.x0, area.y0)))
        })
    })
}

/// The page's annotations a box overlaps, in the order readers draw them:
/// form fields after everything else, as pdf.js does (see `flatten.rs`).
fn annotations_under(document: &Document, page_id: ObjectId, boxes: &[Rect]) -> Vec<Dictionary> {
    let Some(entries) = document
        .get_dictionary(page_id)
        .ok()
        .and_then(|page| page.get(b"Annots").ok())
        .and_then(|value| resolve(document, value))
        .and_then(|value| value.as_array().ok())
    else {
        return Vec::new();
    };
    let (fields, others): (Vec<Dictionary>, Vec<Dictionary>) = entries
        .iter()
        .filter_map(|entry| resolve(document, entry)?.as_dict().ok())
        .filter(|annotation| under_boxes(document, annotation, boxes))
        .cloned()
        .partition(|annotation| {
            annotation.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Widget".as_slice())
        });
    others.into_iter().chain(fields).collect()
}

/// Takes annotations under a box off the page: comments, links, form fields.
/// On a page drawn from a picture, everything but links outside the boxes
/// goes, since the picture already shows them.
fn remove_annotations(
    document: &mut Document,
    page_id: ObjectId,
    boxes: &[Rect],
    pictured: bool,
    removed: &mut BTreeSet<ObjectId>,
    widgets: &mut BTreeSet<ObjectId>,
) -> Result<(), String> {
    let Some(annotations) = document
        .get_dictionary(page_id)
        .ok()
        .and_then(|page| page.get(b"Annots").ok())
        .and_then(|value| resolve(document, value))
        .and_then(|value| value.as_array().ok())
        .cloned()
    else {
        return Ok(());
    };
    let count = annotations.len();
    let mut kept = Vec::new();
    for entry in annotations {
        let Some(annotation) = resolve(document, &entry).and_then(|value| value.as_dict().ok())
        else {
            kept.push(entry);
            continue;
        };
        let subtype = annotation
            .get(b"Subtype")
            .and_then(Object::as_name)
            .unwrap_or_default();
        let under = under_boxes(document, annotation, boxes);
        let goes = if pictured {
            subtype != b"Link" || under
        } else {
            under && subtype != b"Popup"
        };
        if !goes {
            kept.push(entry);
            continue;
        }
        if let Object::Reference(id) = entry {
            removed.insert(id);
            if subtype == b"Widget" {
                widgets.insert(id);
            }
        }
    }
    if kept.len() == count {
        return Ok(());
    }
    let page = document
        .get_dictionary_mut(page_id)
        .map_err(|error| format!("A PDF page could not be read: {error}"))?;
    if kept.is_empty() {
        page.remove(b"Annots");
    } else {
        page.set("Annots", kept);
    }
    Ok(())
}

/// Removes /ActualText, /Alt and /E from structure elements around content
/// that changed, and every element on a page drawn from a picture: each can
/// spell out what was removed.
fn strip_descriptions(
    document: &mut Document,
    catalog_id: ObjectId,
    pages: &[(ObjectId, Option<BTreeSet<i64>>)],
) {
    let pages = pages
        .iter()
        .filter(|(_, mcids)| mcids.as_ref().is_none_or(|mcids| !mcids.is_empty()))
        .map(|(id, mcids)| (*id, mcids.clone()))
        .collect::<BTreeMap<_, _>>();
    if pages.is_empty() {
        return;
    }
    let Some(root) = document
        .get_dictionary(catalog_id)
        .ok()
        .and_then(|catalog| catalog.get(b"StructTreeRoot").ok())
        .and_then(|value| value.as_reference().ok())
    else {
        return;
    };
    let mut marked = BTreeSet::new();
    let mut seen = BTreeSet::new();
    // Element, the page it inherits, and the elements above it.
    let mut stack = vec![(root, None::<ObjectId>, Vec::<ObjectId>::new())];
    while let Some((id, page, ancestors)) = stack.pop() {
        if ancestors.len() > MAX_PAGE_TREE_DEPTH || !seen.insert(id) {
            continue;
        }
        let Ok(element) = document.get_dictionary(id) else {
            continue;
        };
        let page = element
            .get(b"Pg")
            .and_then(Object::as_reference)
            .ok()
            .or(page);
        let mut lineage = ancestors.clone();
        if id != root {
            lineage.push(id);
        }
        let mut kids = match element.get(b"K") {
            Ok(Object::Array(items)) => items.clone(),
            Ok(item) => vec![item.clone()],
            Err(_) => Vec::new(),
        };
        while let Some(kid) = kids.pop() {
            let content = match &kid {
                Object::Integer(mcid) => Some((page, *mcid)),
                Object::Dictionary(reference) if reference.has(b"MCID") => Some((
                    reference
                        .get(b"Pg")
                        .and_then(Object::as_reference)
                        .ok()
                        .or(page),
                    reference
                        .get(b"MCID")
                        .and_then(Object::as_i64)
                        .unwrap_or(-1),
                )),
                Object::Reference(child) => match document.get_object(*child) {
                    Ok(Object::Dictionary(child_dict)) if child_dict.has(b"MCID") => Some((
                        child_dict
                            .get(b"Pg")
                            .and_then(Object::as_reference)
                            .ok()
                            .or(page),
                        child_dict
                            .get(b"MCID")
                            .and_then(Object::as_i64)
                            .unwrap_or(-1),
                    )),
                    Ok(Object::Array(items)) => {
                        kids.extend(items.iter().cloned());
                        None
                    }
                    _ => {
                        stack.push((*child, page, lineage.clone()));
                        None
                    }
                },
                _ => None,
            };
            if let Some((Some(on), mcid)) = content
                && let Some(mcids) = pages.get(&on)
                && mcids.as_ref().is_none_or(|mcids| mcids.contains(&mcid))
            {
                marked.extend(lineage.iter().copied());
            }
        }
    }
    for id in marked {
        if let Ok(element) = document.get_dictionary_mut(id) {
            for key in [b"ActualText".as_slice(), b"Alt", b"E"] {
                element.remove(key);
            }
        }
    }
}
