//! Embedding substitutes for the standard 14 fonts, which PDFs may use without
//! embedding and PDF/A does not allow.
//!
//! The substitutes are URW base35 (`static/pdfa-fonts`), metric clones of
//! Adobe's. Their CFF programs go in as /FontFile3 /Type1C, so the font stays a
//! Type 1 font and its encoding keeps meaning what it meant. /Widths are
//! rewritten from the same metrics as the program, which is what 6.2.11.5
//! checks.

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

use super::encodings::{MAC_ROMAN, WIN_ANSI};

/// A substitute the caller supplies: `name` is one of the standard 14, as in
/// [`standard_equivalent`].
pub struct StandardFont<'a> {
    pub name: &'a str,
    /// A bare CFF program.
    pub program: &'a [u8],
    /// The metrics file `scripts/build-pdfa-fonts.py` writes beside it.
    pub metrics: &'a str,
}

/// Larger differences mean the font a PDF asked for is not the one its name
/// suggests, so a substitute would visibly misplace its text. In thousandths
/// of the font size.
const MAX_WIDTH_CHANGE: f32 = 50.0;

pub(super) struct Metrics {
    flags: i64,
    bbox: [i64; 4],
    italic_angle: f32,
    ascent: i64,
    descent: i64,
    cap_height: i64,
    stem_v: i64,
    builtin: Vec<Option<String>>,
    widths: HashMap<String, i64>,
}

impl Metrics {
    pub(super) fn parse(text: &str) -> Result<Metrics, String> {
        let broken = || "A PDF/A font file is damaged.".to_string();
        let mut metrics = Metrics {
            flags: 32,
            bbox: [0; 4],
            italic_angle: 0.0,
            ascent: 0,
            descent: 0,
            cap_height: 0,
            stem_v: 80,
            builtin: vec![None; 256],
            widths: HashMap::new(),
        };
        for line in text.lines() {
            let mut fields = line.split_whitespace();
            let (Some(key), values) = (fields.next(), fields.collect::<Vec<_>>()) else {
                continue;
            };
            let number = |index: usize| -> Result<i64, String> {
                values
                    .get(index)
                    .and_then(|value| value.parse().ok())
                    .ok_or_else(broken)
            };
            match key {
                "Flags" => metrics.flags = number(0)?,
                "FontBBox" => {
                    metrics.bbox = [number(0)?, number(1)?, number(2)?, number(3)?];
                }
                "ItalicAngle" => {
                    metrics.italic_angle = values
                        .first()
                        .and_then(|value| value.parse().ok())
                        .ok_or_else(broken)?;
                }
                "Ascent" => metrics.ascent = number(0)?,
                "Descent" => metrics.descent = number(0)?,
                "CapHeight" => metrics.cap_height = number(0)?,
                "StemV" => metrics.stem_v = number(0)?,
                "C" => {
                    let code = usize::try_from(number(0)?).map_err(|_| broken())?;
                    let name = values.get(1).ok_or_else(broken)?;
                    *metrics.builtin.get_mut(code).ok_or_else(broken)? = Some((*name).to_owned());
                }
                "W" => {
                    let name = values.first().ok_or_else(broken)?;
                    metrics.widths.insert((*name).to_owned(), number(1)?);
                }
                _ => {}
            }
        }
        if metrics.widths.is_empty() {
            return Err(broken());
        }
        Ok(metrics)
    }

    fn is_symbolic(&self) -> bool {
        self.flags & 4 != 0
    }
}

/// The standard 14 font an unembedded simple font stands for, from its name,
/// including the aliases readers accept (Arial, Times New Roman, Courier New,
/// with `,Bold` or `-BoldMT` style suffixes).
pub(crate) fn standard_equivalent(font: &Dictionary) -> Option<&'static str> {
    let subtype = font.get(b"Subtype").and_then(Object::as_name).ok()?;
    if !matches!(subtype, b"Type1" | b"MMType1" | b"TrueType") {
        return None;
    }
    let name = font.get(b"BaseFont").and_then(Object::as_name).ok()?;
    let name = String::from_utf8_lossy(name)
        .to_lowercase()
        .replace(' ', "");
    // A subset tag is six capitals and a plus sign.
    let name = match name.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 => rest.to_owned(),
        _ => name,
    };
    let (family, style) = match name.find([',', '-']) {
        Some(at) => (&name[..at], &name[at + 1..]),
        None => (name.as_str(), ""),
    };
    let family = family.trim_end_matches("mt").trim_end_matches("ps");
    let family = match family {
        "helvetica" | "arial" => "Helvetica",
        "times" | "timesroman" | "timesnewroman" => "Times",
        "courier" | "couriernew" => "Courier",
        "symbol" => return Some("Symbol"),
        "zapfdingbats" => return Some("ZapfDingbats"),
        _ => return None,
    };
    let bold = style.contains("bold");
    let italic = style.contains("italic") || style.contains("oblique");
    let rest = [
        "bold", "italic", "oblique", "roman", "regular", "mt", "ps", ",", "-",
    ]
    .iter()
    .fold(style.to_owned(), |rest, word| rest.replace(word, ""));
    if !rest.is_empty() {
        return None;
    }
    Some(match (family, bold, italic) {
        ("Times", false, false) => "Times-Roman",
        ("Times", true, false) => "Times-Bold",
        ("Times", false, true) => "Times-Italic",
        ("Times", true, true) => "Times-BoldItalic",
        ("Helvetica", false, false) => "Helvetica",
        ("Helvetica", true, false) => "Helvetica-Bold",
        ("Helvetica", false, true) => "Helvetica-Oblique",
        ("Helvetica", true, true) => "Helvetica-BoldOblique",
        ("Courier", false, false) => "Courier",
        ("Courier", true, false) => "Courier-Bold",
        ("Courier", false, true) => "Courier-Oblique",
        _ => "Courier-BoldOblique",
    })
}

/// Embeds `metrics`' program (already added as `program`) into the font
/// `font_id`, rewriting what describes it to match.
pub(super) fn embed(
    document: &mut Document,
    font_id: ObjectId,
    metrics: &Metrics,
    program: ObjectId,
) -> Result<(), String> {
    let font = document
        .get_dictionary(font_id)
        .map_err(|error| format!("A font could not be read: {error}"))?
        .clone();
    let base_font = font
        .get(b"BaseFont")
        .and_then(Object::as_name)
        .map(<[u8]>::to_vec)
        .unwrap_or_default();
    let display = String::from_utf8_lossy(&base_font).into_owned();
    let is_true_type = font.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"TrueType");
    let encoding = match font.get(b"Encoding") {
        Ok(Object::Reference(id)) => document.get_object(*id).ok().cloned(),
        Ok(encoding) => Some(encoding.clone()),
        Err(_) => None,
    };

    let named = |name: &[u8]| -> Option<Vec<Option<String>>> {
        let table = match name {
            b"WinAnsiEncoding" => &WIN_ANSI,
            b"MacRomanEncoding" => &MAC_ROMAN,
            b"StandardEncoding" if !metrics.is_symbolic() => return Some(metrics.builtin.clone()),
            _ => return None,
        };
        Some(table.iter().map(|name| name.map(str::to_owned)).collect())
    };
    // An unembedded TrueType font with no encoding is read as WinAnsi by the
    // readers that cope with it at all.
    let implicit = if is_true_type && !metrics.is_symbolic() {
        named(b"WinAnsiEncoding")
    } else {
        Some(metrics.builtin.clone())
    };
    let (mut names, differences, builtin) = match &encoding {
        None => (implicit, None, !is_true_type || metrics.is_symbolic()),
        Some(Object::Name(name)) => (named(name), None, false),
        Some(Object::Dictionary(encoding)) => {
            let base = encoding.get(b"BaseEncoding").and_then(Object::as_name).ok();
            let names = match base {
                Some(base) => named(base),
                None => implicit,
            };
            (
                names,
                encoding
                    .get(b"Differences")
                    .and_then(Object::as_array)
                    .ok()
                    .cloned(),
                base.is_none() && (!is_true_type || metrics.is_symbolic()),
            )
        }
        _ => (None, None, false),
    };
    let Some(names) = names.as_mut() else {
        return Err(format!(
            "The font “{display}” uses an encoding PDF/A cannot keep."
        ));
    };
    if let Some(differences) = &differences {
        let mut code = 0usize;
        for item in differences {
            match item {
                Object::Integer(start) => code = usize::try_from(*start).unwrap_or(256),
                Object::Name(name) => {
                    if let Some(slot) = names.get_mut(code) {
                        *slot = Some(String::from_utf8_lossy(name).into_owned());
                    }
                    code += 1;
                }
                _ => {}
            }
        }
    }

    let widths = names
        .iter()
        .map(|name| {
            name.as_ref()
                .and_then(|name| metrics.widths.get(name))
                .copied()
        })
        .collect::<Vec<_>>();
    let (Some(first), Some(last)) = (
        widths.iter().position(Option::is_some),
        widths.iter().rposition(Option::is_some),
    ) else {
        return Err(format!(
            "The font “{display}” has no characters PDF/A can draw."
        ));
    };
    // Where the PDF stated widths, text was laid out with them; a substitute
    // whose widths differ much would move it.
    if let (Ok(stated), Ok(start)) = (
        font.get(b"Widths").and_then(|value| match value {
            Object::Reference(id) => document.get_object(*id).and_then(Object::as_array),
            value => value.as_array(),
        }),
        font.get(b"FirstChar").and_then(Object::as_i64),
    ) {
        for (offset, stated) in stated.iter().enumerate() {
            let Some(Some(width)) = usize::try_from(start)
                .ok()
                .and_then(|start| widths.get(start + offset))
            else {
                continue;
            };
            let Ok(stated) = stated.as_float() else {
                continue;
            };
            if (stated - *width as f32).abs() > MAX_WIDTH_CHANGE {
                return Err(format!(
                    "The font “{display}” is not embedded, and the closest standard font would move its text."
                ));
            }
        }
    }

    let descriptor = document.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(base_font),
        "Flags" => metrics.flags,
        "FontBBox" => metrics.bbox.iter().map(|&value| Object::Integer(value)).collect::<Vec<_>>(),
        "ItalicAngle" => Object::Real(metrics.italic_angle),
        "Ascent" => metrics.ascent,
        "Descent" => metrics.descent,
        "CapHeight" => metrics.cap_height,
        "StemV" => metrics.stem_v,
        "FontFile3" => program,
    });
    let font = document
        .get_dictionary_mut(font_id)
        .map_err(|error| format!("A font could not be read: {error}"))?;
    font.set("Subtype", "Type1");
    font.set("FontDescriptor", descriptor);
    font.set("FirstChar", first as i64);
    font.set("LastChar", last as i64);
    font.set(
        "Widths",
        widths[first..=last]
            .iter()
            .map(|width| Object::Integer(width.unwrap_or(0)))
            .collect::<Vec<_>>(),
    );
    if builtin && metrics.is_symbolic() {
        // The program's own encoding is empty (see the build script), so the
        // one readers assume for Symbol and ZapfDingbats is spelled out.
        font.set(
            "Encoding",
            dictionary! { "Differences" => differences_for(names) },
        );
    } else if encoding.is_none() && is_true_type {
        font.set("Encoding", "WinAnsiEncoding");
    }
    Ok(())
}

fn differences_for(names: &[Option<String>]) -> Vec<Object> {
    let mut differences = Vec::new();
    let mut previous = None;
    for (code, name) in names.iter().enumerate() {
        let Some(name) = name else { continue };
        if previous != Some(code.wrapping_sub(1)) {
            differences.push(Object::Integer(code as i64));
        }
        differences.push(Object::Name(name.as_bytes().to_vec()));
        previous = Some(code);
    }
    differences
}

pub(super) fn program_stream(program: &[u8]) -> Stream {
    Stream::new(dictionary! { "Subtype" => "Type1C" }, program.to_vec())
}
