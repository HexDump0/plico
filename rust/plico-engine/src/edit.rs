//! Editing: replacing text a page already has, erasing parts of a page, and
//! adding text, shapes, drawings and pictures to it.
//!
//! Old text is taken out of the content the way redaction takes it out,
//! glyph by glyph with the rest of the line left where it was, but every
//! path and image under it stays, so the page behind the text is untouched.
//! Its replacement is drawn into the page in the standard font nearest the
//! old one. Erasing takes everything under an area out and paints it, by
//! default in the colour around it, so it disappears into the page.
//! Additions are annotations drawn into the page, as Annotate draws them.
//!
//! When something under an area cannot be taken out in place, such as text
//! in a font the engine cannot measure, the area is painted over instead and
//! the caller is told which pages, since what was there is still in the file.

use std::collections::BTreeMap;

use lopdf::Object;

use crate::annotate::{Annotation, check, draw_annotations};
use crate::documents::load_document;
use crate::redact::{
    Unremovable, cover_on_page, redraw_image, remove_on_page, strip_descriptions, take_images,
};
use crate::stamps::{Stamper, finish, selected_pages};

/// Text to take out: whatever glyphs `area` covers, as redaction decides
/// it. When they cannot be taken out, `shown`, where they reach, is painted
/// over in `cover`, the colour behind them.
pub struct TextRemoval {
    /// From 1.
    pub page: u32,
    /// Left, top, right and bottom as fractions of the visible page, as in
    /// [`Redaction::area`]. A band along the baseline is enough, and safer
    /// than the glyphs' full height: lines set closer than their fonts are
    /// tall overlap, and a glyph goes once a quarter of it is covered.
    pub area: [f32; 4],
    pub shown: [f32; 4],
    /// Red, green and blue, each 0 to 1.
    pub cover: [f32; 3],
}

/// An area to empty: everything under it is taken out, and it is painted in
/// `fill`, as are images under it.
pub struct Erasure {
    /// From 1.
    pub page: u32,
    /// As in [`TextRemoval::area`].
    pub area: [f32; 4],
    /// Red, green and blue, each 0 to 1.
    pub fill: [f32; 3],
}

/// An image the page already draws, moved, resized or taken away.
pub struct ImageMove {
    /// From 1.
    pub page: u32,
    /// Where it is drawn, exactly as [`crate::PageText::images`] reports it.
    pub from: [f32; 4],
    /// Where to draw it instead, as in [`TextRemoval::area`]; `None` takes
    /// it away.
    pub to: Option<[f32; 4]>,
}

pub struct EditOptions<'a> {
    /// Applied first, to the page as it was.
    pub images: &'a [ImageMove],
    pub replace: &'a [TextRemoval],
    pub erase: &'a [Erasure],
    /// Drawn into the pages after the removals, so they can sit where the
    /// old content was. Notes and text markup are not drawn content.
    pub additions: &'a [Annotation<'a>],
}

pub struct Edited {
    pub bytes: Vec<u8>,
    /// Pages where an area was painted over rather than taken out, and why.
    pub covered: Vec<(u32, Unremovable)>,
}

const WHITE: [f64; 3] = [1.0; 3];

pub fn edit_pdf_bytes(
    input: &[u8],
    password: &str,
    options: EditOptions<'_>,
) -> Result<Edited, String> {
    if options.images.is_empty()
        && options.replace.is_empty()
        && options.erase.is_empty()
        && options.additions.is_empty()
    {
        return Err("Make at least one change.".into());
    }
    if options
        .images
        .iter()
        .any(|moved| !moved.from.iter().all(|edge| edge.is_finite()))
    {
        return Err("An image to move is not on the page.".into());
    }
    let areas = options
        .replace
        .iter()
        .flat_map(|removal| [removal.area, removal.shown])
        .chain(options.erase.iter().map(|erased| erased.area))
        .chain(options.images.iter().filter_map(|moved| moved.to));
    for [left, top, right, bottom] in areas {
        if ![left, top, right, bottom]
            .iter()
            .all(|edge| (0.0..=1.0).contains(edge))
            || left >= right
            || top >= bottom
        {
            return Err("Mark areas inside the page.".into());
        }
    }
    let colours = options
        .replace
        .iter()
        .map(|removal| removal.cover)
        .chain(options.erase.iter().map(|erased| erased.fill));
    for colour in colours {
        if !colour.iter().all(|channel| (0.0..=1.0).contains(channel)) {
            return Err("Choose a valid color.".into());
        }
    }
    for addition in options.additions {
        check(addition, true)?;
    }

    let mut document = load_document(input, 1, password)?;
    let mut replaced = BTreeMap::<u32, Vec<&TextRemoval>>::new();
    for removal in options.replace {
        replaced.entry(removal.page).or_default().push(removal);
    }
    // In the order they were made, since a later area paints over an
    // earlier one; neighbours of one colour go together.
    let mut erased = BTreeMap::<u32, Vec<([f64; 3], Vec<[f32; 4]>)>>::new();
    for erase in options.erase {
        let fill = erase.fill.map(f64::from);
        let groups = erased.entry(erase.page).or_default();
        match groups.last_mut() {
            Some((colour, areas)) if *colour == fill => areas.push(erase.area),
            _ => groups.push((fill, vec![erase.area])),
        }
    }
    let mut moved = BTreeMap::<u32, Vec<&ImageMove>>::new();
    for image in options.images {
        moved.entry(image.page).or_default().push(image);
    }
    let numbers = replaced
        .keys()
        .chain(erased.keys())
        .chain(moved.keys())
        .copied()
        .collect::<Vec<_>>();
    let pages = if numbers.is_empty() {
        Vec::new()
    } else {
        selected_pages(&document, &numbers)?
    };

    let mut stamper = Stamper::new(&mut document, 1.0);
    let mut touched = Vec::new();
    let mut covered = Vec::new();
    for (page, page_id) in pages {
        if let Some(images) = moved.get(&page) {
            let froms = images.iter().map(|image| image.from).collect::<Vec<_>>();
            match take_images(&mut document, &mut stamper, page_id, &froms) {
                Ok((taken, mcids)) => {
                    touched.push((page_id, Some(mcids)));
                    for (image, taken) in images.iter().zip(taken) {
                        let taken = taken.ok_or_else(|| {
                            format!("An image on page {page} could not be found to move.")
                        })?;
                        if let Some(to) = image.to {
                            redraw_image(&mut document, &mut stamper, page_id, taken, to)?;
                        }
                    }
                }
                Err(_) => {
                    return Err(format!("The images on page {page} cannot be changed."));
                }
            }
        }
        if let Some(removals) = replaced.get(&page) {
            let areas = removals
                .iter()
                .map(|removal| removal.area)
                .collect::<Vec<_>>();
            match remove_on_page(&mut document, &mut stamper, page_id, &areas, true, WHITE) {
                Ok(mcids) => touched.push((page_id, Some(mcids))),
                Err(reason) => {
                    for removal in removals {
                        let cover = removal.cover.map(f64::from);
                        cover_on_page(
                            &mut document,
                            &mut stamper,
                            page_id,
                            &[removal.shown],
                            cover,
                        )?;
                    }
                    covered.push((page, reason));
                }
            }
        }
        for (fill, areas) in erased.get(&page).into_iter().flatten() {
            match remove_on_page(&mut document, &mut stamper, page_id, areas, false, *fill) {
                Ok(mcids) => touched.push((page_id, Some(mcids))),
                Err(reason) => {
                    cover_on_page(&mut document, &mut stamper, page_id, areas, *fill)?;
                    if !covered.iter().any(|(number, _)| *number == page) {
                        covered.push((page, reason));
                    }
                }
            }
        }
        // An embedded thumbnail would still show the page as it was.
        if let Ok(dictionary) = document.get_dictionary_mut(page_id) {
            dictionary.remove(b"Thumb");
        }
    }
    if !touched.is_empty() {
        let catalog_id = document
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .map_err(|_| "This PDF has no catalog.".to_string())?;
        strip_descriptions(&mut document, catalog_id, &touched);
    }
    if !options.additions.is_empty() {
        draw_annotations(&mut document, options.additions, true)?;
    }
    Ok(Edited {
        bytes: finish(document, 1.0)?,
        covered,
    })
}
