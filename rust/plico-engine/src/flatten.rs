//! Flattening: what annotations and form fields show is drawn into the page
//! itself, and the annotation goes, so nothing on the page can be edited and
//! every viewer shows it the same way.
//!
//! Only stored appearances are drawn. An annotation without one is left as it
//! was rather than given a look the engine made up, and so is a text or choice
//! field in a form that asks viewers to redraw its fields, because its stored
//! appearance may not be what they show. Either is counted in `kept`. An empty
//! field with nothing stored and nothing for a viewer to draw goes.

use std::collections::BTreeSet;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

use crate::documents::load_document;
use crate::stamps::{Stamper, finish, page_frame, rectangle, resolve, selected_pages};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlattenScope {
    /// Every annotation whose look is all it is.
    Everything,
    /// Form field widgets only; comments, highlights and the like stay.
    FormFields,
}

pub struct Flattened {
    pub bytes: Vec<u8>,
    /// Annotations in scope left as they were: nothing stored to draw, or
    /// a form field whose stored look may not show its value.
    pub kept: usize,
}

/// Annotation flags (ISO 32000-1, 12.5.3).
const HIDDEN: i64 = 2;
const NO_ROTATE: i64 = 16;
const NO_VIEW: i64 = 32;

/// Annotations that carry more than their look: a file, a sound or a movie
/// would be lost with the annotation.
const CARRIERS: [&[u8]; 6] = [
    b"FileAttachment",
    b"Sound",
    b"Movie",
    b"Screen",
    b"3D",
    b"RichMedia",
];

const MAX_NESTING: usize = 64;

pub fn flatten_pdf_bytes(
    input: &[u8],
    password: &str,
    scope: FlattenScope,
) -> Result<Flattened, String> {
    let mut document = load_document(input, 1, password)?;
    let pages = selected_pages(&document, &[])?;
    let catalog_id = document
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|_| "This PDF has no catalog.".to_string())?;
    let form = document
        .get_dictionary(catalog_id)
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|value| resolve_dict(&document, value))
        .cloned();
    let redraws_fields = form
        .as_ref()
        .and_then(|form| form.get(b"NeedAppearances").ok())
        .and_then(|value| value.as_bool().ok())
        .unwrap_or(false);
    let defaults = form.as_ref().and_then(|form| form.get(b"DR").ok()).cloned();

    let mut stamper = Stamper::new(&mut document, 1.0);
    let mut kept = 0;
    // Annotations taken off their pages, drawn or not, and widgets among them.
    let mut removed = BTreeSet::new();
    let mut widgets = BTreeSet::new();

    for (_, page_id) in pages {
        let Some(annotations) = document
            .get_dictionary(page_id)
            .ok()
            .and_then(|page| page.get(b"Annots").ok())
            .and_then(|value| resolve(&document, value))
            .and_then(|value| value.as_array().ok())
            .cloned()
        else {
            continue;
        };
        let rotation = page_rotation(&document, page_id);
        let mut remaining = Vec::new();
        let mut draws = String::new();
        // Viewers draw form fields over every other annotation whatever the
        // /Annots order, so flattening does too (pdf.js corpus:
        // issue13003.pdf, whose checkbox sits under a square drawn after it).
        let mut field_draws = String::new();
        let mut xobjects = Dictionary::new();
        let mut properties = Dictionary::new();

        for entry in annotations {
            let (id, annotation) = match &entry {
                Object::Reference(id) => match document.get_dictionary(*id) {
                    Ok(annotation) => (Some(*id), annotation.clone()),
                    Err(_) => {
                        remaining.push(entry);
                        continue;
                    }
                },
                Object::Dictionary(annotation) => (None, annotation.clone()),
                _ => {
                    remaining.push(entry);
                    continue;
                }
            };
            let subtype = name_of(&annotation, b"Subtype").unwrap_or_default();
            let is_widget = subtype == b"Widget";
            let in_scope = match scope {
                FlattenScope::Everything => {
                    !matches!(subtype, b"Link" | b"Popup") && !CARRIERS.contains(&subtype)
                }
                FlattenScope::FormFields => is_widget,
            };
            let flags = annotation.get(b"F").and_then(Object::as_i64).unwrap_or(0);
            if !in_scope || (flags & NO_VIEW != 0 && flags & HIDDEN == 0) {
                // Out of scope, or shown only on paper: drawing it would show
                // it on screen too.
                remaining.push(entry);
                continue;
            }
            let field = if is_widget {
                field_type(&document, &annotation)
            } else {
                None
            };
            let text = matches!(field.as_deref(), Some(b"Tx" | b"Ch"));
            let filled = text && has_value(&document, &annotation);
            // A form that asks viewers to redraw its fields makes every stored
            // text or choice appearance suspect, empty ones too: viewers redraw
            // them with the field's own colours (pdf.js corpus: bug1669099.pdf,
            // whose stored boxes lack the grey a viewer gives a read-only one).
            let stale = redraws_fields && text;
            let appearance = annotation
                .get(b"AP")
                .ok()
                .and_then(|value| resolve_dict(&document, value))
                .and_then(|appearance| appearance.get(b"N").ok())
                .cloned();
            // An empty field with nothing stored and no box to draw shows
            // nothing in any viewer, so it simply goes. Blank forms are full
            // of them: 292 of the 297 such text fields in the pdf.js corpus.
            let invisible =
                appearance.is_none() && text && !filled && !has_box(&document, &annotation);
            if flags & HIDDEN == 0 && !invisible && (stale || appearance.is_none()) {
                kept += 1;
                remaining.push(entry);
                continue;
            }

            // From here the annotation leaves the page, drawn if it shows.
            if let Some(id) = id {
                removed.insert(id);
                if is_widget {
                    widgets.insert(id);
                }
            }
            if flags & HIDDEN != 0 || invisible {
                continue;
            }
            let Some(stream_id) = appearance
                .and_then(|normal| chosen_state(&document, &annotation, normal))
                .and_then(|normal| as_stream_id(&mut document, normal))
            else {
                continue;
            };
            let Some(placement) = placement(&document, &annotation, stream_id, flags, rotation)
            else {
                continue;
            };
            // A widget's appearance may lean on the form's default resources
            // rather than carry its own.
            if let (Some(defaults), Ok(Object::Stream(stream))) =
                (&defaults, document.get_object_mut(stream_id))
                && !stream.dict.has(b"Resources")
            {
                stream.dict.set("Resources", defaults.clone());
            }

            let name = format!("A{}", xobjects.len());
            xobjects.set(name.as_bytes(), stream_id);
            let draws = if is_widget {
                &mut field_draws
            } else {
                &mut draws
            };
            draws.push_str("q\n");
            // Optional content still decides whether it shows (8.11.3.2).
            let optional = annotation.get(b"OC").ok().cloned();
            if let Some(optional) = &optional {
                let tag = format!("C{}", properties.len());
                properties.set(tag.as_bytes(), optional.clone());
                draws.push_str(&format!("/OC /{tag} BDC\n"));
            }
            draws.push_str(&format!("{} cm\n/{name} Do\n", matrix(placement)));
            if optional.is_some() {
                draws.push_str("EMC\n");
            }
            draws.push_str("Q\n");
        }
        draws.push_str(&field_draws);

        let unchanged = remaining.len()
            == document
                .get_dictionary(page_id)
                .ok()
                .and_then(|page| page.get(b"Annots").ok())
                .and_then(|value| resolve(&document, value))
                .and_then(|value| value.as_array().ok())
                .map_or(0, Vec::len);
        if unchanged {
            continue;
        }
        set_annotations(&mut document, page_id, remaining)?;
        if draws.is_empty() {
            continue;
        }
        let frame = page_frame(&document, page_id)?;
        let mut resources = dictionary! { "XObject" => xobjects };
        if !properties.is_empty() {
            resources.set("Properties", properties);
        }
        let flattened = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => frame.media_box.iter().map(|&value| Object::Real(value)).collect::<Vec<_>>(),
                "Resources" => resources,
            },
            draws.into_bytes(),
        ));
        stamper.place(&mut document, page_id, flattened, false)?;
    }

    remove_orphaned_popups(&mut document, &removed)?;
    if !widgets.is_empty() {
        prune_fields(&mut document, catalog_id, &widgets);
    }
    Ok(Flattened {
        bytes: finish(document, 1.0)?,
        kept,
    })
}

/// What an annotation shows, ready to draw into its page.
pub(crate) struct Appearance {
    /// Its stored normal appearance, a form XObject.
    pub(crate) form: ObjectId,
    /// Where that lands on the page.
    pub(crate) matrix: [f32; 6],
    /// Optional content that still decides whether it shows.
    pub(crate) optional: Option<Object>,
}

/// What readers show for `annotation` on page `page_id`, by the rules
/// flattening follows: nothing for hidden, print-only or popup annotations,
/// nothing without a stored appearance, and nothing for a text or choice
/// field in a form that asks readers to redraw its fields, since its stored
/// look may not be what they show.
pub(crate) fn appearance(
    document: &mut Document,
    page_id: ObjectId,
    annotation: &Dictionary,
) -> Option<Appearance> {
    let flags = annotation.get(b"F").and_then(Object::as_i64).unwrap_or(0);
    let subtype = name_of(annotation, b"Subtype").unwrap_or_default();
    if flags & (HIDDEN | NO_VIEW) != 0 || subtype == b"Popup" {
        return None;
    }
    let form = document
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|value| resolve_dict(document, value))
        .cloned();
    let redraws_fields = form
        .as_ref()
        .and_then(|form| form.get(b"NeedAppearances").ok())
        .and_then(|value| value.as_bool().ok())
        .unwrap_or(false);
    if subtype == b"Widget"
        && redraws_fields
        && matches!(
            field_type(document, annotation).as_deref(),
            Some(b"Tx" | b"Ch")
        )
    {
        return None;
    }
    let normal = annotation
        .get(b"AP")
        .ok()
        .and_then(|value| resolve_dict(document, value))
        .and_then(|appearance| appearance.get(b"N").ok())
        .cloned()?;
    let state = chosen_state(document, annotation, normal)?;
    let form_id = as_stream_id(document, state)?;
    let rotation = page_rotation(document, page_id);
    let matrix = placement(document, annotation, form_id, flags, rotation)?;
    // A widget's appearance may lean on the form's default resources.
    if let (Some(defaults), Ok(Object::Stream(stream))) = (
        form.as_ref().and_then(|form| form.get(b"DR").ok()).cloned(),
        document.get_object_mut(form_id),
    ) && !stream.dict.has(b"Resources")
    {
        stream.dict.set("Resources", defaults);
    }
    Some(Appearance {
        form: form_id,
        matrix,
        optional: annotation.get(b"OC").ok().cloned(),
    })
}

fn resolve_dict<'a>(document: &'a Document, value: &'a Object) -> Option<&'a Dictionary> {
    resolve(document, value)?.as_dict().ok()
}

fn name_of<'a>(dictionary: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    dictionary.get(key).ok()?.as_name().ok()
}

/// A field's type is inherited from its parents (12.7.3.1).
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

/// Whether a field holds anything to show. Values are inherited from parent
/// fields (12.7.3.1).
fn has_value(document: &Document, annotation: &Dictionary) -> bool {
    let mut node = annotation;
    for _ in 0..MAX_NESTING {
        if let Ok(value) = node.get(b"V") {
            return match resolve(document, value) {
                Some(Object::String(text, _)) => !text.is_empty(),
                Some(Object::Array(items)) => !items.is_empty(),
                Some(Object::Null) | None => false,
                Some(_) => true,
            };
        }
        let Some(parent) = node
            .get(b"Parent")
            .ok()
            .and_then(|parent| resolve_dict(document, parent))
        else {
            return false;
        };
        node = parent;
    }
    false
}

/// Whether a viewer would draw a background or border for the field from its
/// /MK colours, which an empty array leaves transparent (12.5.6.19).
fn has_box(document: &Document, annotation: &Dictionary) -> bool {
    annotation
        .get(b"MK")
        .ok()
        .and_then(|value| resolve_dict(document, value))
        .is_some_and(|characteristics| {
            [b"BG".as_slice(), b"BC"].iter().any(|key| {
                characteristics
                    .get(key)
                    .ok()
                    .and_then(|value| resolve(document, value))
                    .and_then(|value| value.as_array().ok())
                    .is_some_and(|colour| !colour.is_empty())
            })
        })
}

/// The normal appearance shown now: the stream itself, or the state /AS
/// names. A dictionary of states with no /AS, or an /AS it lacks, shows
/// nothing (12.5.5).
fn chosen_state(document: &Document, annotation: &Dictionary, normal: Object) -> Option<Object> {
    match resolve(document, &normal)? {
        Object::Stream(_) => Some(normal),
        Object::Dictionary(states) => states.get(name_of(annotation, b"AS")?).ok().cloned(),
        _ => None,
    }
}

/// Streams are always indirect, so a direct one is malformed; moving it into
/// an object keeps what it draws.
fn as_stream_id(document: &mut Document, object: Object) -> Option<ObjectId> {
    match object {
        Object::Reference(id) => document
            .get_object(id)
            .ok()?
            .as_stream()
            .is_ok()
            .then_some(id),
        Object::Stream(stream) => Some(document.add_object(stream)),
        _ => None,
    }
}

fn page_rotation(document: &Document, page_id: ObjectId) -> i64 {
    let mut node = Some(page_id);
    for _ in 0..MAX_NESTING {
        let Some(dictionary) = node.and_then(|id| document.get_dictionary(id).ok()) else {
            break;
        };
        if let Some(rotation) = dictionary
            .get(b"Rotate")
            .ok()
            .and_then(|value| resolve(document, value))
            .and_then(|value| value.as_float().ok())
            .filter(|rotation| rotation.is_finite())
        {
            return ((rotation / 90.0).round() as i64 * 90).rem_euclid(360);
        }
        node = dictionary
            .get(b"Parent")
            .and_then(Object::as_reference)
            .ok();
    }
    0
}

/// Where the appearance lands (12.5.5): its bounding box, turned by its own
/// matrix, is fitted onto the annotation's rectangle. A NoRotate annotation on
/// a turned page is turned back about its top left corner, so it stays upright
/// as the reader sees it.
fn placement(
    document: &Document,
    annotation: &Dictionary,
    stream_id: ObjectId,
    flags: i64,
    rotation: i64,
) -> Option<[f32; 6]> {
    let [x0, y0, x1, y1] = rectangle(document, annotation.get(b"Rect").ok()?)?;
    let stream = document.get_object(stream_id).ok()?.as_stream().ok()?;
    let bbox = rectangle(document, stream.dict.get(b"BBox").ok()?)?;
    let form_matrix = stream
        .dict
        .get(b"Matrix")
        .ok()
        .and_then(|value| resolve(document, value))
        .and_then(|value| value.as_array().ok())
        .and_then(|items| {
            items
                .iter()
                .map(|item| resolve(document, item)?.as_float().ok())
                .collect::<Option<Vec<_>>>()
        })
        .and_then(|values| <[f32; 6]>::try_from(values).ok())
        .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let corners = [
        (bbox[0], bbox[1]),
        (bbox[2], bbox[1]),
        (bbox[0], bbox[3]),
        (bbox[2], bbox[3]),
    ]
    .map(|(x, y)| apply(form_matrix, x, y));
    let left = corners
        .iter()
        .map(|corner| corner.0)
        .fold(f32::INFINITY, f32::min);
    let right = corners
        .iter()
        .map(|corner| corner.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let bottom = corners
        .iter()
        .map(|corner| corner.1)
        .fold(f32::INFINITY, f32::min);
    let top = corners
        .iter()
        .map(|corner| corner.1)
        .fold(f32::NEG_INFINITY, f32::max);
    let (width, height) = (right - left, top - bottom);
    if !(width > 0.0 && height > 0.0) || !(x1 > x0 && y1 > y0) {
        return None;
    }
    let (scale_x, scale_y) = ((x1 - x0) / width, (y1 - y0) / height);
    let fit = [
        scale_x,
        0.0,
        0.0,
        scale_y,
        x0 - left * scale_x,
        y0 - bottom * scale_y,
    ];
    if flags & NO_ROTATE == 0 || rotation == 0 {
        return Some(fit);
    }
    // Counterclockwise by the page's turn, about the top left corner.
    let (sin, cos) = (rotation as f32).to_radians().sin_cos();
    let turn = [
        cos,
        sin,
        -sin,
        cos,
        x0 - x0 * cos + y1 * sin,
        y1 - x0 * sin - y1 * cos,
    ];
    Some(multiply(fit, turn))
}

fn apply([a, b, c, d, e, f]: [f32; 6], x: f32, y: f32) -> (f32, f32) {
    (a * x + c * y + e, b * x + d * y + f)
}

/// `first` then `second`, as PDF concatenates matrices.
fn multiply(first: [f32; 6], second: [f32; 6]) -> [f32; 6] {
    let [a, b, c, d, e, f] = first;
    let [p, q, r, s, t, u] = second;
    [
        a * p + b * r,
        a * q + b * s,
        c * p + d * r,
        c * q + d * s,
        e * p + f * r + t,
        e * q + f * s + u,
    ]
}

fn matrix(values: [f32; 6]) -> String {
    values
        .map(|value| {
            let text = format!("{value:.4}");
            let text = text.trim_end_matches('0').trim_end_matches('.');
            if text == "-0" || text.is_empty() {
                "0".to_owned()
            } else {
                text.to_owned()
            }
        })
        .join(" ")
}

fn set_annotations(
    document: &mut Document,
    page_id: ObjectId,
    remaining: Vec<Object>,
) -> Result<(), String> {
    let page = document
        .get_dictionary_mut(page_id)
        .map_err(|error| format!("A PDF page could not be read: {error}"))?;
    if remaining.is_empty() {
        page.remove(b"Annots");
    } else {
        page.set("Annots", remaining);
    }
    Ok(())
}

/// A popup only shows its parent's text, so it goes with its parent.
pub(crate) fn remove_orphaned_popups(
    document: &mut Document,
    removed: &BTreeSet<ObjectId>,
) -> Result<(), String> {
    if removed.is_empty() {
        return Ok(());
    }
    for (_, page_id) in document.get_pages() {
        let Some(annotations) = document
            .get_dictionary(page_id)
            .ok()
            .and_then(|page| page.get(b"Annots").ok())
            .and_then(|value| resolve(document, value))
            .and_then(|value| value.as_array().ok())
            .cloned()
        else {
            continue;
        };
        let count = annotations.len();
        let remaining = annotations
            .into_iter()
            .filter(|entry| {
                let parent = resolve_dict(document, entry)
                    .filter(|annotation| name_of(annotation, b"Subtype") == Some(b"Popup"))
                    .and_then(|popup| popup.get(b"Parent").ok())
                    .and_then(|parent| parent.as_reference().ok());
                !parent.is_some_and(|parent| removed.contains(&parent))
            })
            .collect::<Vec<_>>();
        if remaining.len() != count {
            set_annotations(document, page_id, remaining)?;
        }
    }
    Ok(())
}

/// Takes flattened widgets out of the form's field tree, and fields left with
/// no widgets with them. A form with no fields left goes entirely, and so does
/// XFA, which would otherwise show the old form in viewers that read it.
pub(crate) fn prune_fields(
    document: &mut Document,
    catalog_id: ObjectId,
    widgets: &BTreeSet<ObjectId>,
) {
    let Some(form) = document
        .get_dictionary(catalog_id)
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .cloned()
    else {
        return;
    };
    let fields = resolve_dict(document, &form)
        .and_then(|form| form.get(b"Fields").ok())
        .and_then(|value| resolve(document, value))
        .and_then(|value| value.as_array().ok())
        .cloned()
        .unwrap_or_default();
    let mut seen = BTreeSet::new();
    let fields = fields
        .into_iter()
        .filter(|field| keep_field(document, field, widgets, &mut seen, 0))
        .collect::<Vec<_>>();

    if fields.is_empty() {
        if let Ok(catalog) = document.get_dictionary_mut(catalog_id) {
            catalog.remove(b"AcroForm");
        }
        return;
    }
    let holder = match form {
        Object::Reference(id) => document.get_dictionary_mut(id).ok(),
        _ => document
            .get_dictionary_mut(catalog_id)
            .ok()
            .and_then(|catalog| catalog.get_mut(b"AcroForm").ok())
            .and_then(|form| form.as_dict_mut().ok()),
    };
    if let Some(form) = holder {
        form.set("Fields", fields);
        form.remove(b"XFA");
    }
}

/// Whether `field` still has a widget on some page, pruning its kids to the
/// ones that do.
fn keep_field(
    document: &mut Document,
    field: &Object,
    widgets: &BTreeSet<ObjectId>,
    seen: &mut BTreeSet<ObjectId>,
    depth: usize,
) -> bool {
    let Ok(id) = field.as_reference() else {
        return true;
    };
    if widgets.contains(&id) {
        return false;
    }
    if depth > MAX_NESTING || !seen.insert(id) {
        return true;
    }
    let Some(kids) = document
        .get_dictionary(id)
        .ok()
        .and_then(|node| node.get(b"Kids").ok())
        .and_then(|value| resolve(document, value))
        .and_then(|value| value.as_array().ok())
        .cloned()
    else {
        return true;
    };
    let count = kids.len();
    let kids = kids
        .into_iter()
        .filter(|kid| keep_field(document, kid, widgets, seen, depth + 1))
        .collect::<Vec<_>>();
    if kids.is_empty() && count > 0 {
        return false;
    }
    if kids.len() != count
        && let Ok(node) = document.get_dictionary_mut(id)
    {
        node.set("Kids", kids);
    }
    true
}
