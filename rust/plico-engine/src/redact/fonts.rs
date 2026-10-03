//! What redaction needs to know about a font: how a string splits into
//! glyphs, how far each moves the pen, how tall a glyph stands, and which
//! characters it shows, for finding text.
//!
//! A width that cannot be known is `None`, never a guess: a glyph placed by a
//! guessed width can sit beside the box that should have removed it.

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object};

use super::geometry::Matrix;
use super::lexer::{self, Value};
use crate::archive::{MAC_ROMAN, WIN_ANSI, standard_equivalent};
use crate::documents::MAX_DECOMPRESSED_STREAM;
use crate::stamps::{standard_width, win_ansi_char};

pub(super) struct Glyph {
    /// Where it sits in the string.
    pub(super) start: usize,
    pub(super) end: usize,
    /// How far it moves the pen in text space for a 1 point font, before
    /// character and word spacing: across, or down for vertical text.
    pub(super) advance: Option<f64>,
    /// Its extent around the pen position, in the same units: left, bottom,
    /// right, top.
    pub(super) bounds: [f64; 4],
    /// A single-byte code 32, which word spacing applies to.
    pub(super) space: bool,
    pub(super) text: String,
}

enum Codes {
    /// One byte each: every simple font.
    Single,
    /// Lengths from a CMap's code space ranges.
    Ranges(Vec<CodeRange>),
    /// A CMap this engine cannot read, so strings cannot be split.
    Unknown,
}

struct CodeRange {
    low: Vec<u8>,
    high: Vec<u8>,
}

enum Widths {
    /// Codes from `first`, with `missing` for anything outside.
    Listed {
        first: u32,
        widths: Vec<f64>,
        missing: f64,
    },
    /// A standard 14 font with no widths given, by its name and glyph names.
    Standard(&'static str),
    /// Composite: by CID, with a default.
    Cids {
        widths: HashMap<u32, f64>,
        default: f64,
    },
    Unknown,
}

/// Vertical metrics of a composite font (ISO 32000-1, 9.7.4.3), in glyph
/// units: the advance down, and where the glyph hangs from the pen.
struct Vertical {
    metrics: HashMap<u32, [f64; 3]>,
    default: [f64; 2],
}

enum Cids {
    Identity,
    Mapped(Vec<(usize, u32, u32, u32)>),
}

pub(super) struct Font {
    codes: Codes,
    cids: Cids,
    widths: Widths,
    vertical: Option<Vertical>,
    /// Glyph space to text space, horizontally: 1/1000 except for Type 3.
    scale: f64,
    /// The glyph's extent above and below the baseline, in text space units.
    ascent: f64,
    descent: f64,
    /// For a simple font, each code's glyph name.
    names: Option<Vec<Option<String>>>,
    unicode: HashMap<(usize, u32), String>,
}

/// Fonts with absurd bounding boxes would make every glyph so tall that a box
/// covering it would never cover a quarter of it. These limits, in ems, keep
/// a glyph about as tall as its line.
const MAX_ASCENT: f64 = 1.1;
const MIN_ASCENT: f64 = 0.5;
const MAX_DESCENT: f64 = 0.35;

const MAX_UNICODE_ENTRIES: usize = 1 << 20;

fn resolve<'a>(document: &'a Document, value: &'a Object) -> Option<&'a Object> {
    match value {
        Object::Reference(id) => document.get_object(*id).ok(),
        value => Some(value),
    }
}

fn dictionary<'a>(document: &'a Document, value: &'a Object) -> Option<&'a Dictionary> {
    resolve(document, value)?.as_dict().ok()
}

fn number(document: &Document, value: &Object) -> Option<f64> {
    resolve(document, value)?
        .as_float()
        .ok()
        .map(f64::from)
        .filter(|number| number.is_finite())
}

fn numbers(document: &Document, value: &Object) -> Option<Vec<f64>> {
    resolve(document, value)?
        .as_array()
        .ok()?
        .iter()
        .map(|item| number(document, item))
        .collect()
}

fn stream_bytes(document: &Document, value: &Object) -> Option<Vec<u8>> {
    let stream = resolve(document, value)?.as_stream().ok()?;
    stream
        .decompressed_content_with_limit(MAX_DECOMPRESSED_STREAM)
        .ok()
        .or_else(|| (!stream.dict.has(b"Filter")).then(|| stream.content.clone()))
}

impl Font {
    pub(super) fn load(document: &Document, font: &Dictionary) -> Font {
        let subtype = font
            .get(b"Subtype")
            .and_then(Object::as_name)
            .unwrap_or_default();
        let mut loaded = if subtype == b"Type0" {
            Font::composite(document, font)
        } else {
            Font::simple(document, font, subtype == b"Type3")
        };
        if let Some(bytes) = font
            .get(b"ToUnicode")
            .ok()
            .and_then(|value| stream_bytes(document, value))
        {
            loaded.unicode = to_unicode(&bytes);
        }
        loaded
    }

    fn simple(document: &Document, font: &Dictionary, type3: bool) -> Font {
        let descriptor = font
            .get(b"FontDescriptor")
            .ok()
            .and_then(|value| dictionary(document, value));
        let matrix = type3
            .then(|| font.get(b"FontMatrix").ok())
            .flatten()
            .and_then(|value| numbers(document, value))
            .and_then(|values| <Matrix>::try_from(values).ok())
            .filter(|matrix| matrix[0] != 0.0 || matrix[1] != 0.0);
        let scale = matrix.map_or(0.001, |matrix| matrix[0]);
        let names = glyph_names(document, font);

        let missing = descriptor
            .and_then(|descriptor| descriptor.get(b"MissingWidth").ok())
            .and_then(|value| number(document, value))
            .unwrap_or(0.0);
        let listed = font
            .get(b"Widths")
            .ok()
            .and_then(|value| numbers(document, value));
        let widths = match listed {
            Some(widths) => Widths::Listed {
                first: font
                    .get(b"FirstChar")
                    .ok()
                    .and_then(|value| number(document, value))
                    .map_or(0, |first| first.max(0.0) as u32),
                widths,
                missing,
            },
            None => match standard_equivalent(font) {
                Some(name) if !type3 => Widths::Standard(name),
                _ => Widths::Unknown,
            },
        };

        let (ascent, descent) = match matrix {
            Some(matrix) => font
                .get(b"FontBBox")
                .ok()
                .and_then(|value| numbers(document, value))
                .filter(|values| values.len() == 4 && values[3] > values[1])
                .map(|values| {
                    let corners = [(values[0], values[1]), (values[2], values[3])]
                        .map(|(x, y)| matrix[1] * x + matrix[3] * y);
                    (corners[0].max(corners[1]), corners[0].min(corners[1]))
                })
                .unwrap_or((0.8, -0.2)),
            None => vertical_extent(document, descriptor),
        };
        Font {
            codes: Codes::Single,
            cids: Cids::Identity,
            widths,
            vertical: None,
            scale,
            ascent: ascent.clamp(MIN_ASCENT, MAX_ASCENT),
            descent: descent.clamp(-MAX_DESCENT, 0.0),
            names: Some(names),
            unicode: HashMap::new(),
        }
    }

    fn composite(document: &Document, font: &Dictionary) -> Font {
        let descendant = font
            .get(b"DescendantFonts")
            .ok()
            .and_then(|value| resolve(document, value))
            .and_then(|value| value.as_array().ok())
            .and_then(|fonts| fonts.first())
            .and_then(|value| dictionary(document, value));
        let descriptor = descendant
            .and_then(|cid_font| cid_font.get(b"FontDescriptor").ok())
            .and_then(|value| dictionary(document, value));
        let (codes, cids, vertical) = match font
            .get(b"Encoding")
            .ok()
            .and_then(|value| resolve(document, value))
        {
            Some(Object::Name(name)) => match name.as_slice() {
                b"Identity-H" => (identity_codes(), Cids::Identity, false),
                b"Identity-V" => (identity_codes(), Cids::Identity, true),
                _ => (Codes::Unknown, Cids::Identity, name.ends_with(b"-V")),
            },
            Some(Object::Stream(stream)) => {
                let vertical = stream
                    .dict
                    .get(b"WMode")
                    .and_then(Object::as_i64)
                    .is_ok_and(|mode| mode == 1);
                match stream
                    .decompressed_content_with_limit(MAX_DECOMPRESSED_STREAM)
                    .ok()
                    .or_else(|| (!stream.dict.has(b"Filter")).then(|| stream.content.clone()))
                {
                    Some(bytes) => {
                        let (codes, cids, mode) = cid_cmap(&bytes);
                        (codes, cids, vertical || mode)
                    }
                    None => (Codes::Unknown, Cids::Identity, vertical),
                }
            }
            _ => (Codes::Unknown, Cids::Identity, false),
        };

        let widths = match descendant {
            Some(cid_font) => {
                let default = cid_font
                    .get(b"DW")
                    .ok()
                    .and_then(|value| number(document, value))
                    .unwrap_or(1000.0);
                let mut widths = HashMap::new();
                if let Some(list) = cid_font
                    .get(b"W")
                    .ok()
                    .and_then(|value| resolve(document, value))
                    .and_then(|value| value.as_array().ok())
                {
                    cid_metrics(document, list, 1, |cid, values| {
                        widths.insert(cid, values[0]);
                    });
                }
                Widths::Cids { widths, default }
            }
            None => Widths::Unknown,
        };
        let vertical = vertical.then(|| {
            let default = descendant
                .and_then(|cid_font| cid_font.get(b"DW2").ok())
                .and_then(|value| numbers(document, value))
                .and_then(|values| <[f64; 2]>::try_from(values).ok())
                .unwrap_or([880.0, -1000.0]);
            let mut metrics = HashMap::new();
            if let Some(list) = descendant
                .and_then(|cid_font| cid_font.get(b"W2").ok())
                .and_then(|value| resolve(document, value))
                .and_then(|value| value.as_array().ok())
            {
                cid_metrics(document, list, 3, |cid, values| {
                    metrics.insert(cid, [values[0], values[1], values[2]]);
                });
            }
            Vertical { metrics, default }
        });
        let (ascent, descent) = vertical_extent(document, descriptor);
        Font {
            codes,
            cids,
            widths,
            vertical,
            scale: 0.001,
            ascent: ascent.clamp(MIN_ASCENT, MAX_ASCENT),
            descent: descent.clamp(-MAX_DESCENT, 0.0),
            names: None,
            unicode: HashMap::new(),
        }
    }

    pub(super) fn vertical(&self) -> bool {
        self.vertical.is_some()
    }

    /// Splits `string` into glyphs. With codes this engine cannot split, the
    /// whole string is one glyph with no advance.
    pub(super) fn glyphs(&self, string: &[u8]) -> Vec<Glyph> {
        let mut glyphs = Vec::new();
        let mut at = 0;
        while at < string.len() {
            let length = match &self.codes {
                Codes::Single => 1,
                Codes::Ranges(ranges) => code_length(ranges, &string[at..]),
                Codes::Unknown => string.len() - at,
            };
            let end = (at + length).min(string.len());
            let bytes = &string[at..end];
            let code = bytes
                .iter()
                .fold(0u32, |code, &byte| code.wrapping_shl(8) | u32::from(byte));
            let known = !matches!(self.codes, Codes::Unknown);
            glyphs.push(self.glyph(at, end, code, known));
            at = end;
        }
        glyphs
    }

    fn glyph(&self, start: usize, end: usize, code: u32, known: bool) -> Glyph {
        let length = end - start;
        let cid = self.cid(length, code);
        let width = known.then(|| self.width(code, cid)).flatten();
        let text = self.text(length, code);
        let space = length == 1 && code == 32;
        match &self.vertical {
            None => {
                let width = width.map(|width| width * self.scale);
                Glyph {
                    start,
                    end,
                    advance: width,
                    bounds: [0.0, self.descent, width.unwrap_or(0.0), self.ascent],
                    space,
                    text,
                }
            }
            Some(vertical) => {
                let horizontal = width.unwrap_or(1000.0);
                let [down, across, _] = vertical.metrics.get(&cid).copied().unwrap_or([
                    vertical.default[1],
                    horizontal / 2.0,
                    vertical.default[0],
                ]);
                let [down, across, horizontal] =
                    [down, across, horizontal].map(|value| value * self.scale);
                // The glyph hangs from its position vector, centred on the pen
                // across, and fills the cell its advance moves down through.
                // Ascent and descent describe horizontal lines, and would make
                // the box taller than the cell and overlap its neighbours.
                Glyph {
                    start,
                    end,
                    advance: known.then_some(down),
                    bounds: [-across, down.min(0.0), horizontal - across, down.max(0.0)],
                    space,
                    text,
                }
            }
        }
    }

    fn cid(&self, length: usize, code: u32) -> u32 {
        match &self.cids {
            Cids::Identity => code,
            Cids::Mapped(ranges) => ranges
                .iter()
                .find(|(found, low, high, _)| *found == length && (*low..=*high).contains(&code))
                .map_or(0, |(_, low, _, first)| first + (code - low)),
        }
    }

    fn width(&self, code: u32, cid: u32) -> Option<f64> {
        match &self.widths {
            Widths::Listed {
                first,
                widths,
                missing,
            } => Some(
                code.checked_sub(*first)
                    .and_then(|index| widths.get(index as usize))
                    .copied()
                    .unwrap_or(*missing),
            ),
            Widths::Standard(name) => {
                let glyph = self.names.as_ref()?.get(code as usize)?.as_deref()?;
                let win_ansi = WIN_ANSI.iter().position(|found| *found == Some(glyph))?;
                standard_width(name, win_ansi as u8).map(f64::from)
            }
            Widths::Cids { widths, default } => Some(widths.get(&cid).copied().unwrap_or(*default)),
            Widths::Unknown => None,
        }
    }

    fn text(&self, length: usize, code: u32) -> String {
        if let Some(text) = self.unicode.get(&(length, code)) {
            return text.clone();
        }
        let Some(names) = &self.names else {
            return String::new();
        };
        names
            .get(code as usize)
            .and_then(|name| name.as_deref())
            .and_then(glyph_text)
            .or_else(|| {
                u8::try_from(code)
                    .ok()
                    .and_then(win_ansi_char)
                    .map(String::from)
            })
            .unwrap_or_default()
    }
}

fn identity_codes() -> Codes {
    Codes::Ranges(vec![CodeRange {
        low: vec![0, 0],
        high: vec![0xFF, 0xFF],
    }])
}

/// The longest code space range the bytes fall in, as readers do; failing
/// that, the length of the first range whose first byte matches, then one.
fn code_length(ranges: &[CodeRange], bytes: &[u8]) -> usize {
    for length in 1..=4.min(bytes.len()) {
        let candidate = &bytes[..length];
        if ranges.iter().any(|range| {
            range.low.len() == length
                && candidate
                    .iter()
                    .zip(range.low.iter().zip(&range.high))
                    .all(|(byte, (low, high))| (low..=high).contains(&byte))
        }) {
            return length;
        }
    }
    ranges
        .iter()
        .find(|range| {
            range
                .low
                .first()
                .zip(range.high.first())
                .is_some_and(|(low, high)| (low..=high).contains(&&bytes[0]))
        })
        .map_or(1, |range| range.low.len())
        .min(bytes.len())
        .max(1)
}

/// Code space and CID ranges of an embedded CMap. One that builds on another
/// CMap by name can only be read when that one is Identity.
fn cid_cmap(bytes: &[u8]) -> (Codes, Cids, bool) {
    let Ok(operations) = lexer::operations(bytes) else {
        return (Codes::Unknown, Cids::Identity, false);
    };
    let mut ranges = Vec::new();
    let mut cids = Vec::new();
    let mut identity = false;
    let mut vertical = false;
    let code = |value: &Value| match value {
        Value::String(bytes) if (1..=4).contains(&bytes.len()) => Some((
            bytes.len(),
            bytes
                .iter()
                .fold(0u32, |code, &byte| code << 8 | u32::from(byte)),
        )),
        _ => None,
    };
    for operation in &operations {
        match operation.operator.as_slice() {
            b"usecmap" => match operation.operands.last().and_then(Value::name) {
                Some(b"Identity-H" | b"Identity-V") => identity = true,
                _ => return (Codes::Unknown, Cids::Identity, false),
            },
            b"def" if operation.operands.first().and_then(Value::name) == Some(b"WMode") => {
                vertical = operation.operands.get(1).and_then(Value::number) == Some(1.0);
            }
            b"endcodespacerange" => {
                for pair in operation.operands.as_chunks::<2>().0 {
                    if let (Value::String(low), Value::String(high)) = (&pair[0], &pair[1])
                        && low.len() == high.len()
                        && (1..=4).contains(&low.len())
                    {
                        ranges.push(CodeRange {
                            low: low.clone(),
                            high: high.clone(),
                        });
                    }
                }
            }
            b"endcidrange" => {
                for triple in operation.operands.as_chunks::<3>().0 {
                    if let (Some((length, low)), Some((_, high)), Some(cid)) =
                        (code(&triple[0]), code(&triple[1]), triple[2].number())
                        && high >= low
                    {
                        cids.push((length, low, high, cid.max(0.0) as u32));
                    }
                }
            }
            b"endcidchar" => {
                for pair in operation.operands.as_chunks::<2>().0 {
                    if let (Some((length, value)), Some(cid)) = (code(&pair[0]), pair[1].number()) {
                        cids.push((length, value, value, cid.max(0.0) as u32));
                    }
                }
            }
            // Some encoding CMaps map codes to CIDs with bfchar and bfrange,
            // the ToUnicode operators, and readers take the hex destination as
            // the CID (pdf.js corpus: bug920426.pdf).
            b"endbfchar" => {
                for pair in operation.operands.as_chunks::<2>().0 {
                    if let (Some((length, value)), Some((_, cid))) =
                        (code(&pair[0]), code(&pair[1]))
                    {
                        cids.push((length, value, value, cid));
                    }
                }
            }
            b"endbfrange" => {
                for triple in operation.operands.as_chunks::<3>().0 {
                    if let (Some((length, low)), Some((_, high)), Some((_, cid))) =
                        (code(&triple[0]), code(&triple[1]), code(&triple[2]))
                        && high >= low
                    {
                        cids.push((length, low, high, cid));
                    }
                }
            }
            _ => {}
        }
    }
    if identity {
        if ranges.is_empty() {
            return (identity_codes(), Cids::Identity, vertical);
        }
        // Codes it does not map itself fall through to Identity.
        cids.push((2, 0, 0xFFFF, 0));
    }
    if ranges.is_empty() {
        return (Codes::Unknown, Cids::Identity, vertical);
    }
    (Codes::Ranges(ranges), Cids::Mapped(cids), vertical)
}

/// `/W` or `/W2`: `c [v v v ...]` gives consecutive CIDs from c, and
/// `first last v...` gives each CID in a range the same `group` values.
fn cid_metrics(
    document: &Document,
    list: &[Object],
    group: usize,
    mut add: impl FnMut(u32, &[f64]),
) {
    let mut index = 0;
    let mut budget = 1usize << 20;
    while index < list.len() {
        let Some(first) = number(document, &list[index]) else {
            return;
        };
        let first = first.max(0.0) as u32;
        match list
            .get(index + 1)
            .and_then(|value| resolve(document, value))
        {
            Some(Object::Array(values)) => {
                let values = values
                    .iter()
                    .filter_map(|value| number(document, value))
                    .collect::<Vec<_>>();
                for (offset, chunk) in values.chunks_exact(group).enumerate() {
                    add(first + offset as u32, chunk);
                }
                index += 2;
            }
            Some(_) => {
                let Some(last) = list
                    .get(index + 1)
                    .and_then(|value| number(document, value))
                else {
                    return;
                };
                let values = (0..group)
                    .map(|offset| {
                        list.get(index + 2 + offset)
                            .and_then(|value| number(document, value))
                    })
                    .collect::<Option<Vec<_>>>();
                let Some(values) = values else { return };
                let last = last.max(0.0) as u32;
                let count = last.saturating_sub(first) as usize + 1;
                if count > budget {
                    return;
                }
                budget -= count;
                for cid in first..=last {
                    add(cid, &values);
                }
                index += 2 + group;
            }
            None => return,
        }
    }
}

/// Ascent and descent in ems from a font descriptor, or from its bounding
/// box when those are missing or nonsense, or a typical pair.
fn vertical_extent(document: &Document, descriptor: Option<&Dictionary>) -> (f64, f64) {
    let Some(descriptor) = descriptor else {
        return (0.8, -0.2);
    };
    let read = |key: &[u8]| {
        descriptor
            .get(key)
            .ok()
            .and_then(|value| number(document, value))
    };
    if let (Some(ascent), Some(descent)) = (read(b"Ascent"), read(b"Descent"))
        && ascent > 0.0
        && descent <= 0.0
    {
        return (ascent / 1000.0, descent / 1000.0);
    }
    descriptor
        .get(b"FontBBox")
        .ok()
        .and_then(|value| numbers(document, value))
        .filter(|values| values.len() == 4 && values[3] > 0.0)
        .map(|values| (values[3] / 1000.0, values[1].min(0.0) / 1000.0))
        .unwrap_or((0.8, -0.2))
}

/// Each code's glyph name: the base encoding the font names, or the standard
/// one, with its /Differences applied.
fn glyph_names(document: &Document, font: &Dictionary) -> Vec<Option<String>> {
    let encoding = font
        .get(b"Encoding")
        .ok()
        .and_then(|value| resolve(document, value));
    let base = match encoding {
        Some(Object::Name(name)) => Some(name.as_slice()),
        Some(Object::Dictionary(encoding)) => {
            encoding.get(b"BaseEncoding").and_then(Object::as_name).ok()
        }
        _ => None,
    };
    let mut names = (0..256)
        .map(|code| {
            let name = match base {
                Some(b"WinAnsiEncoding") => WIN_ANSI[code],
                Some(b"MacRomanEncoding") => MAC_ROMAN[code],
                // StandardEncoding agrees with WinAnsi on printable ASCII
                // apart from the two quotes; above it, this engine has no table.
                _ => match code {
                    0x27 => Some("quoteright"),
                    0x60 => Some("quoteleft"),
                    0x20..=0x7E => WIN_ANSI[code],
                    _ => None,
                },
            };
            name.map(str::to_owned)
        })
        .collect::<Vec<_>>();
    if let Some(Object::Dictionary(encoding)) = encoding
        && let Some(differences) = encoding
            .get(b"Differences")
            .ok()
            .and_then(|value| resolve(document, value))
            .and_then(|value| value.as_array().ok())
    {
        let mut code = 0usize;
        for item in differences {
            match resolve(document, item) {
                Some(Object::Integer(number)) => code = (*number).clamp(0, 255) as usize,
                Some(Object::Name(name)) => {
                    if let Some(slot) = names.get_mut(code) {
                        *slot = Some(String::from_utf8_lossy(name).into_owned());
                    }
                    code += 1;
                }
                _ => {}
            }
        }
    }
    names
}

/// The text a glyph name stands for, by the Adobe Glyph List's rules for the
/// names PDFs mostly use.
fn glyph_text(name: &str) -> Option<String> {
    let name = name.split('.').next().unwrap_or(name);
    if let Some(code) = WIN_ANSI.iter().position(|found| *found == Some(name)) {
        return win_ansi_char(code as u8).map(String::from);
    }
    let decoded = |hex: &str| {
        u32::from_str_radix(hex, 16)
            .ok()
            .and_then(char::from_u32)
            .map(String::from)
    };
    if let Some(hex) = name.strip_prefix("uni")
        && hex.len() >= 4
        && hex.len() % 4 == 0
    {
        return hex
            .as_bytes()
            .chunks(4)
            .map(|chunk| decoded(std::str::from_utf8(chunk).ok()?))
            .collect();
    }
    if let Some(hex) = name.strip_prefix('u')
        && (4..=6).contains(&hex.len())
    {
        return decoded(hex);
    }
    Some(
        match name {
            "fi" => "fi",
            "fl" => "fl",
            "ff" => "ff",
            "ffi" => "ffi",
            "ffl" => "ffl",
            "quoteright" => "’",
            "quoteleft" => "‘",
            "minus" => "−",
            "dotlessi" => "ı",
            "Lslash" => "Ł",
            "lslash" => "ł",
            "nbspace" => "\u{a0}",
            "sfthyphen" => "\u{ad}",
            _ => return None,
        }
        .to_owned(),
    )
}

/// A ToUnicode CMap's mappings, by code length and code.
fn to_unicode(bytes: &[u8]) -> HashMap<(usize, u32), String> {
    let mut map = HashMap::new();
    let Ok(operations) = lexer::operations(bytes) else {
        return map;
    };
    let code = |value: &Value| match value {
        Value::String(bytes) if (1..=4).contains(&bytes.len()) => Some((
            bytes.len(),
            bytes
                .iter()
                .fold(0u32, |code, &byte| code << 8 | u32::from(byte)),
        )),
        _ => None,
    };
    let text = |value: &Value| match value {
        Value::String(bytes) => Some(utf16(bytes)),
        Value::Name(name) => glyph_text(&String::from_utf8_lossy(name)),
        _ => None,
    };
    for operation in &operations {
        match operation.operator.as_slice() {
            b"endbfchar" => {
                for pair in operation.operands.as_chunks::<2>().0 {
                    if let (Some(key), Some(value)) = (code(&pair[0]), text(&pair[1])) {
                        map.insert(key, value);
                    }
                }
            }
            b"endbfrange" => {
                for triple in operation.operands.as_chunks::<3>().0 {
                    let (Some((length, low)), Some((_, high))) =
                        (code(&triple[0]), code(&triple[1]))
                    else {
                        continue;
                    };
                    if high < low || (high - low) as usize > 0xFFFF {
                        continue;
                    }
                    for (offset, value) in (low..=high).enumerate() {
                        if map.len() >= MAX_UNICODE_ENTRIES {
                            return map;
                        }
                        let found = match &triple[2] {
                            // The last unit counts up through the range.
                            Value::String(bytes) if bytes.len() >= 2 => {
                                let mut bytes = bytes.clone();
                                let at = bytes.len() - 2;
                                let last = u16::from_be_bytes([bytes[at], bytes[at + 1]])
                                    .wrapping_add(offset as u16);
                                bytes[at..].copy_from_slice(&last.to_be_bytes());
                                Some(utf16(&bytes))
                            }
                            Value::String(bytes) if bytes.len() == 1 => {
                                char::from_u32(u32::from(bytes[0]) + offset as u32)
                                    .map(String::from)
                            }
                            Value::Array(items) => items.get(offset).and_then(text),
                            _ => None,
                        };
                        if let Some(found) = found {
                            map.insert((length, value), found);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    map
}

fn utf16(bytes: &[u8]) -> String {
    let units = bytes
        .chunks(2)
        .map(|pair| u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)]))
        .collect::<Vec<_>>();
    String::from_utf16_lossy(&units)
        .chars()
        .filter(|character| !character.is_control() || *character == ' ')
        .collect()
}
