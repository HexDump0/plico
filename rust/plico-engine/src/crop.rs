//! Cropping: each chosen page keeps only part of what it shows.
//!
//! Pages are changed where they are, like stamping. The area to keep is given
//! as the reader sees the page and mapped back through the page's frame, so
//! /Rotate, an existing /CropBox and /UserUnit all hold. Nothing outside the
//! area is removed from the file, only hidden, which is why cropping is not
//! redaction.

use std::collections::BTreeMap;

use lopdf::Object;

use crate::documents::load_document;
use crate::stamps::{finish, page_frame, rectangle, resolve, selected_pages};

pub struct PageCrop {
    /// From 1.
    pub page: u32,
    /// Left, top, right and bottom edges as fractions of the visible page's
    /// width and height, measured from its top left as the reader sees it.
    pub area: [f32; 4],
}

/// The smallest page the spec lets readers expect (ISO 32000-1, C.2).
const MIN_SIDE: f32 = 3.0;

pub fn crop_pdf_bytes(input: &[u8], password: &str, crops: &[PageCrop]) -> Result<Vec<u8>, String> {
    if crops.is_empty() {
        return Err("Choose at least one page.".into());
    }
    let mut areas = BTreeMap::new();
    for crop in crops {
        let [left, top, right, bottom] = crop.area;
        if !crop.area.iter().all(|edge| (0.0..=1.0).contains(edge))
            || left >= right
            || top >= bottom
        {
            return Err("Choose a crop area inside the page.".into());
        }
        if areas.insert(crop.page, crop.area).is_some() {
            return Err(format!("Page {} was given two crop areas.", crop.page));
        }
    }

    let mut document = load_document(input, 1, password)?;
    let numbers = areas.keys().copied().collect::<Vec<_>>();
    for (page, page_id) in selected_pages(&document, &numbers)? {
        let [left, top, right, bottom] = areas[&page];
        if left <= 0.0 && top <= 0.0 && right >= 1.0 && bottom >= 1.0 {
            continue;
        }
        let frame = page_frame(&document, page_id)?;
        // The frame's origin is the bottom left, so top and bottom swap.
        let (x0, x1) = (left * frame.width, right * frame.width);
        let (y0, y1) = ((1.0 - bottom) * frame.height, (1.0 - top) * frame.height);
        if x1 - x0 < MIN_SIDE || y1 - y0 < MIN_SIDE {
            return Err(format!("The crop area on page {page} is too small."));
        }
        let [a, b, c, d, e, f] = frame.matrix;
        let corner = |x: f32, y: f32| (a * x + c * y + e, b * x + d * y + f);
        let ((px0, py0), (px1, py1)) = (corner(x0, y0), corner(x1, y1));
        let media = frame.media_box;
        // f32 fractions land a hair off round numbers (168.39998 for 168.4),
        // which is enough for a renderer to round an edge sitting on a pixel
        // boundary the other way. A thousandth of a point is far below
        // anything visible. Rounding can also step outside the media box.
        let round = |value: f32| (value * 1000.0).round() / 1000.0;
        let area = [
            round(px0.min(px1)).max(media[0]),
            round(py0.min(py1)).max(media[1]),
            round(px0.max(px1)).min(media[2]),
            round(py0.max(py1)).min(media[3]),
        ];

        let page_dictionary = document
            .get_dictionary(page_id)
            .map_err(|error| format!("A PDF page could not be read: {error}"))?;
        // Bleed, trim and art boxes are clipped to the new area rather than
        // left describing paper that is no longer there.
        let boxes = [b"BleedBox".as_slice(), b"TrimBox", b"ArtBox"].map(|key| {
            let clipped = page_dictionary
                .get(key)
                .ok()
                .and_then(|value| resolve(&document, value))
                .and_then(|value| rectangle(&document, value))
                .map(|found| {
                    [
                        found[0].max(area[0]),
                        found[1].max(area[1]),
                        found[2].min(area[2]),
                        found[3].min(area[3]),
                    ]
                })
                .filter(|[x0, y0, x1, y1]| x1 > x0 && y1 > y0);
            (key, clipped)
        });

        let page_dictionary = document
            .get_dictionary_mut(page_id)
            .map_err(|error| format!("A PDF page could not be read: {error}"))?;
        let rectangle_object = |values: [f32; 4]| {
            values
                .iter()
                .map(|&value| Object::Real(value))
                .collect::<Vec<_>>()
        };
        page_dictionary.set("CropBox", rectangle_object(area));
        // Some print paths only read /MediaBox (Ghostscript without
        // -dUseCropBox, many printer drivers), so it follows the crop. The
        // content outside is still in the stream either way.
        page_dictionary.set("MediaBox", rectangle_object(area));
        for (key, clipped) in boxes {
            match clipped {
                Some(clipped) => page_dictionary.set(key, rectangle_object(clipped)),
                None => {
                    page_dictionary.remove(key);
                }
            }
        }
        // An embedded thumbnail still shows the whole page.
        page_dictionary.remove(b"Thumb");
    }
    finish(document, 1.0)
}
