//! Text the standard fonts cannot draw, set in fonts the caller supplies and
//! embedded as subsets.
//!
//! Text the standard 14 fonts can draw is still drawn in them, so nothing is
//! embedded for it and that output is unchanged. Anything else is decided for
//! a whole piece of text at once (a watermark, a text box, a field's value),
//! so it reads as one face rather than switching fonts mid-word: it is shaped
//! with rustybuzz, ordered for right-to-left scripts, and drawn in Type0 fonts
//! with Identity-H encoding. Each glyph's code is its id in the subset, widths
//! come from the font, and a ToUnicode map keeps the text searchable and
//! copyable.
//!
//! Bold text takes a bold font where one was supplied and the regular one
//! where not (the CJK fonts come in one weight); nothing is drawn thicker to
//! fake it.

use std::collections::BTreeMap;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use md5::{Digest, Md5};
use rustybuzz::ttf_parser::{GlyphId, RawFace, Tag, name_id};
use rustybuzz::{Direction, Face, UnicodeBuffer};
use subsetter::GlyphRemapper;
use unicode_bidi::BidiInfo;
use unicode_script::{Script, UnicodeScript};

use crate::stamps::{FontFamily, cap_height, number, text_width, win_ansi};

pub struct SuppliedFont<'a> {
    /// Fonts of one face in its two weights share a group.
    pub group: &'a str,
    /// Text in another family does not use it; `None` serves every family.
    pub family: Option<FontFamily>,
    pub bold: bool,
    /// An OpenType font with TrueType or CFF outlines.
    pub bytes: &'a [u8],
}

/// How one piece of text is drawn.
#[derive(Clone, Debug)]
pub(crate) enum Setting {
    Standard {
        family: FontFamily,
        bold: bool,
    },
    /// Supplied fonts in the order a character tries them.
    Embedded {
        fonts: Vec<usize>,
        /// Fraction of the font size.
        cap: f32,
    },
}

impl Setting {
    /// Where the tops of capitals sit above the baseline, as a fraction of
    /// the font size.
    pub(crate) fn cap_height(&self) -> f32 {
        match self {
            Setting::Standard { family, bold } => cap_height(*family, *bold),
            Setting::Embedded { cap, .. } => *cap,
        }
    }
}

/// A line ready to draw. Widths and offsets are thousandths of the font size.
#[derive(Clone, Debug)]
pub(crate) enum SetLine {
    /// WinAnsi codes in the standard font, which the caller names `/F0`.
    Standard {
        codes: Vec<u8>,
        width: u32,
    },
    Shaped {
        glyphs: Vec<Glyph>,
        width: f32,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Glyph {
    font: usize,
    id: u16,
    advance: f32,
    offset: (f32, f32),
    /// What the glyph stands for; empty for all but the first glyph of a
    /// cluster.
    text: String,
}

impl SetLine {
    /// Points at `size`.
    pub(crate) fn width(&self, size: f32) -> f32 {
        match self {
            SetLine::Standard { width, .. } => *width as f32 / 1000.0 * size,
            SetLine::Shaped { width, .. } => width / 1000.0 * size,
        }
    }
}

struct Loaded<'a> {
    supplied: &'a SuppliedFont<'a>,
    face: Face<'a>,
    /// Thousandths of the font size per font unit.
    scale: f32,
    /// Reserved when a page first names the font, written by [`Typesetter::write`].
    id: Option<ObjectId>,
    remapper: GlyphRemapper,
    /// By id in the subset: width in thousandths and the text it stands for.
    used: BTreeMap<u16, (f32, String)>,
}

pub(crate) struct Typesetter<'a> {
    fonts: Vec<Loaded<'a>>,
}

impl<'a> Typesetter<'a> {
    pub(crate) fn new(supplied: &'a [SuppliedFont<'a>]) -> Result<Typesetter<'a>, String> {
        let fonts = supplied
            .iter()
            .map(|font| {
                let face = Face::from_slice(font.bytes, 0)
                    .ok_or_else(|| format!("The {} font could not be read.", font.group))?;
                let scale = 1000.0 / face.units_per_em() as f32;
                Ok(Loaded {
                    supplied: font,
                    face,
                    scale,
                    id: None,
                    remapper: GlyphRemapper::new(),
                    used: BTreeMap::new(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Typesetter { fonts })
    }

    /// How `text` is drawn in `family`: in the standard font when it can
    /// draw every character, otherwise in the supplied fonts. Fails with the
    /// first character no font has.
    pub(crate) fn setting(
        &self,
        text: &str,
        family: FontFamily,
        bold: bool,
    ) -> Result<Setting, char> {
        let drawn = || {
            text.chars()
                .filter(|character| !matches!(character, '\n' | '\r'))
        };
        if drawn().all(|character| win_ansi(character).is_some()) {
            return Ok(Setting::Standard { family, bold });
        }
        let mut groups = Vec::<(&str, Vec<usize>)>::new();
        for (index, font) in self.fonts.iter().enumerate() {
            if font.supplied.family.is_some_and(|only| only != family) {
                continue;
            }
            match groups
                .iter_mut()
                .find(|(group, _)| *group == font.supplied.group)
            {
                Some((_, members)) => members.push(index),
                None => groups.push((font.supplied.group, vec![index])),
            }
        }
        let fonts = groups
            .into_iter()
            .map(|(_, members)| {
                members
                    .iter()
                    .copied()
                    .find(|&index| self.fonts[index].supplied.bold == bold)
                    .unwrap_or(members[0])
            })
            .collect::<Vec<_>>();
        if let Some(missing) = drawn().find(|&character| {
            !follows(character) && !fonts.iter().any(|&index| self.has(index, character))
        }) {
            return Err(missing);
        }
        // The family's own font sets the line, whatever script fills it.
        let cap = fonts
            .iter()
            .map(|&index| &self.fonts[index])
            .find(|font| font.supplied.family == Some(family))
            .and_then(|font| Some(font.face.capital_height()? as f32 * font.scale / 1000.0))
            .unwrap_or_else(|| cap_height(family, bold));
        Ok(Setting::Embedded { fonts, cap })
    }

    fn has(&self, font: usize, character: char) -> bool {
        self.fonts[font].face.glyph_index(character).is_some()
    }

    /// `text`, one line with no breaks, shaped as `setting` says.
    pub(crate) fn line(&self, text: &str, setting: &Setting) -> SetLine {
        let fonts = match setting {
            Setting::Standard { family, bold } => {
                let codes = text.chars().filter_map(win_ansi).collect::<Vec<_>>();
                let width = text_width(*family, *bold, &codes);
                return SetLine::Standard { codes, width };
            }
            Setting::Embedded { fonts, .. } => fonts,
        };
        let mut glyphs = Vec::new();
        let bidi = BidiInfo::new(text, None);
        for paragraph in &bidi.paragraphs {
            let (levels, runs) = bidi.visual_runs(paragraph, paragraph.range.clone());
            for run in runs {
                let rtl = levels[run.start].is_rtl();
                let mut pieces = self.pieces(&text[run.clone()], fonts);
                if rtl {
                    pieces.reverse();
                }
                for (range, font) in pieces {
                    let start = run.start + range.start;
                    self.shape(&text[start..run.start + range.end], font, rtl, &mut glyphs);
                }
            }
        }
        let width = glyphs.iter().map(|glyph| glyph.advance).sum();
        SetLine::Shaped { glyphs, width }
    }

    /// `text` cut where the font changes: each character takes the first font
    /// that has it, and marks stay with the character they sit on.
    fn pieces(&self, text: &str, fonts: &[usize]) -> Vec<(std::ops::Range<usize>, usize)> {
        let mut pieces = Vec::<(std::ops::Range<usize>, usize)>::new();
        for (start, character) in text.char_indices() {
            let end = start + character.len_utf8();
            let previous = pieces.last().map(|(_, font)| *font);
            let font = match previous {
                Some(font) if follows(character) => font,
                _ => fonts
                    .iter()
                    .copied()
                    .find(|&font| self.has(font, character))
                    .or(previous)
                    .unwrap_or(fonts[0]),
            };
            match pieces.last_mut() {
                Some((range, last)) if *last == font => range.end = end,
                _ => pieces.push((start..end, font)),
            }
        }
        pieces
    }

    fn shape(&self, text: &str, font: usize, rtl: bool, glyphs: &mut Vec<Glyph>) {
        let loaded = &self.fonts[font];
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        buffer.set_direction(if rtl {
            Direction::RightToLeft
        } else {
            Direction::LeftToRight
        });
        let shaped = rustybuzz::shape(&loaded.face, &[], buffer);
        let infos = shaped.glyph_infos();
        let mut starts = infos
            .iter()
            .map(|info| info.cluster as usize)
            .collect::<Vec<_>>();
        starts.sort_unstable();
        starts.dedup();
        let first = glyphs.len();
        for (info, position) in infos.iter().zip(shaped.glyph_positions()) {
            glyphs.push(Glyph {
                font,
                id: info.glyph_id as u16,
                advance: position.x_advance as f32 * loaded.scale,
                offset: (
                    position.x_offset as f32 * loaded.scale,
                    position.y_offset as f32 * loaded.scale,
                ),
                text: String::new(),
            });
        }
        // Each glyph stands for the character it is the plain form of, so a
        // reordered vowel sign (Devanagari's ि before its consonant) copies
        // as itself; whatever a cluster has left goes to its first glyph.
        for (index, &start) in starts.iter().enumerate() {
            let end = starts.get(index + 1).copied().unwrap_or(text.len());
            let members = (first..glyphs.len())
                .filter(|&glyph| infos[glyph - first].cluster as usize == start)
                .collect::<Vec<_>>();
            let mut left = String::new();
            for character in text[start..end].chars() {
                let plain = loaded.face.glyph_index(character).map(|glyph| glyph.0);
                match members.iter().find(|&&glyph| {
                    glyphs[glyph].text.is_empty() && Some(glyphs[glyph].id) == plain
                }) {
                    Some(&glyph) => glyphs[glyph].text.push(character),
                    None => left.push(character),
                }
            }
            if let Some(&glyph) = members.first() {
                glyphs[glyph].text.insert_str(0, &left);
            }
        }
    }

    /// Adds the fonts `line` uses to a /Font resource dictionary, named
    /// `/P0`, `/P1` and so on by their place in the supplied list.
    pub(crate) fn name_fonts(
        &mut self,
        document: &mut Document,
        line: &SetLine,
        fonts: &mut Dictionary,
    ) {
        let SetLine::Shaped { glyphs, .. } = line else {
            return;
        };
        for glyph in glyphs {
            let loaded = &mut self.fonts[glyph.font];
            let id = *loaded.id.get_or_insert_with(|| document.new_object_id());
            fonts.set(format!("P{}", glyph.font).as_bytes(), id);
        }
    }

    /// Operators drawing `line` with its baseline starting at the origin of
    /// `matrix`, inside BT and ET. A standard line expects `/F0` already set
    /// at `size`.
    pub(crate) fn show(&mut self, line: &SetLine, size: f32, matrix: [f32; 6]) -> String {
        let glyphs = match line {
            SetLine::Standard { codes, .. } => {
                return format!(
                    "{} Tm\n<{}> Tj\n",
                    text_matrix(matrix),
                    codes
                        .iter()
                        .map(|code| format!("{code:02X}"))
                        .collect::<String>()
                );
            }
            SetLine::Shaped { glyphs, .. } => glyphs,
        };
        let [a, b, c, d, e, f] = matrix;
        let at = |x: f32, y: f32| {
            let (x, y) = (x / 1000.0 * size, y / 1000.0 * size);
            [a, b, c, d, e + a * x + c * y, f + b * x + d * y]
        };
        let mut content = String::new();
        let mut current = None;
        let mut pen = 0.0;
        let mut index = 0;
        while index < glyphs.len() {
            let font = glyphs[index].font;
            if current != Some(font) {
                content.push_str(&format!("/P{font} {} Tf\n", number(size)));
                current = Some(font);
            }
            let first = &glyphs[index];
            // A glyph moved off the line, a mark mostly, is placed on its own.
            if first.offset != (0.0, 0.0) {
                let cid = self.register(first);
                content.push_str(&format!(
                    "{} Tm\n<{cid:04X}> Tj\n",
                    text_matrix(at(pen + first.offset.0, first.offset.1))
                ));
                pen += first.advance;
                index += 1;
                continue;
            }
            content.push_str(&format!("{} Tm\n[", text_matrix(at(pen, 0.0))));
            while let Some(glyph) = glyphs.get(index)
                && glyph.font == font
                && glyph.offset == (0.0, 0.0)
            {
                let cid = self.register(glyph);
                let width = self.fonts[font].used[&cid].0;
                content.push_str(&format!("<{cid:04X}>"));
                // What the font's own width would move, corrected to where
                // shaping put the next glyph.
                let correction = width - glyph.advance;
                if correction.abs() > 0.01 {
                    content.push_str(&format!(" {} ", number(correction)));
                }
                pen += glyph.advance;
                index += 1;
            }
            content.push_str("] TJ\n");
        }
        content
    }

    /// The glyph's id in the subset, which is also its code.
    fn register(&mut self, glyph: &Glyph) -> u16 {
        let loaded = &mut self.fonts[glyph.font];
        let cid = loaded.remapper.remap(glyph.id);
        let width = loaded
            .face
            .glyph_hor_advance(GlyphId(glyph.id))
            .map_or(0.0, |advance| advance as f32 * loaded.scale);
        let entry = loaded.used.entry(cid).or_insert((width, String::new()));
        if entry.1.is_empty() {
            entry.1.clone_from(&glyph.text);
        }
        cid
    }

    /// Writes every font a page names.
    pub(crate) fn write(self, document: &mut Document) -> Result<(), String> {
        for font in self.fonts {
            let Some(id) = font.id else { continue };
            write_font(document, id, &font)?;
        }
        Ok(())
    }
}

/// Marks and joiners belong to the character before them.
fn follows(character: char) -> bool {
    character.script() == Script::Inherited
}

fn text_matrix(matrix: [f32; 6]) -> String {
    matrix.map(number).join(" ")
}

fn write_font(document: &mut Document, id: ObjectId, font: &Loaded<'_>) -> Result<(), String> {
    let failed = || format!("The {} font could not be embedded.", font.supplied.group);
    let subset = subsetter::subset(font.supplied.bytes, 0, &font.remapper).map_err(|_| failed())?;
    let cff = RawFace::parse(&subset, 0)
        .ok()
        .and_then(|raw| raw.table(Tag::from_bytes(b"CFF ")))
        .map(<[u8]>::to_vec);
    let face = &font.face;
    let postscript = face
        .names()
        .into_iter()
        .find(|name| name.name_id == name_id::POST_SCRIPT_NAME)
        .and_then(|name| name.to_string())
        .unwrap_or_else(|| "Font".into())
        .chars()
        .filter(|character| character.is_ascii_graphic() && !"[](){}<>/%".contains(*character))
        .collect::<String>();
    // A subset's name starts with six capitals, different for different
    // subsets of one font (ISO 32000-1, 9.6.4).
    let mut digest = Md5::new();
    digest.update(postscript.as_bytes());
    for cid in font.used.keys() {
        digest.update(cid.to_be_bytes());
    }
    let hash = digest.finalize();
    let tag = hash[..6]
        .iter()
        .map(|byte| char::from(b'A' + byte % 26))
        .collect::<String>();
    let base = format!("{tag}+{postscript}");

    let scaled = |value: i16| Object::Real(value as f32 * font.scale);
    let bbox = face.global_bounding_box();
    let mut flags = 4;
    if face.is_monospaced() {
        flags |= 1;
    }
    let (file_key, file) = match cff {
        Some(cff) => (
            "FontFile3",
            Stream::new(dictionary! { "Subtype" => "CIDFontType0C" }, cff),
        ),
        None => {
            let length = subset.len() as i64;
            (
                "FontFile2",
                Stream::new(dictionary! { "Length1" => length }, subset),
            )
        }
    };
    let is_cff = file_key == "FontFile3";
    let file = document.add_object(file);
    let descriptor = document.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(base.clone().into_bytes()),
        "Flags" => flags,
        "FontBBox" => vec![scaled(bbox.x_min), scaled(bbox.y_min), scaled(bbox.x_max), scaled(bbox.y_max)],
        "ItalicAngle" => Object::Real(face.italic_angle()),
        "Ascent" => scaled(face.ascender()),
        "Descent" => scaled(face.descender()),
        "CapHeight" => scaled(face.capital_height().unwrap_or(face.ascender())),
        "StemV" => if font.supplied.bold { 120 } else { 80 },
        file_key => file,
    });

    // Ids in a subset run from 0 with no gaps, so one list covers them all.
    let widths = (0..font.remapper.num_gids())
        .map(|cid| {
            let width = font.used.get(&cid).map_or_else(
                || {
                    face.glyph_hor_advance(GlyphId(0))
                        .map_or(0.0, |advance| advance as f32 * font.scale)
                },
                |(width, _)| *width,
            );
            Object::Real(width.round())
        })
        .collect::<Vec<_>>();
    let mut descendant = dictionary! {
        "Type" => "Font",
        "Subtype" => if is_cff { "CIDFontType0" } else { "CIDFontType2" },
        "BaseFont" => Object::Name(base.clone().into_bytes()),
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => 0,
        },
        "FontDescriptor" => descriptor,
        "W" => vec![Object::Integer(0), Object::Array(widths)],
    };
    if !is_cff {
        descendant.set("CIDToGIDMap", "Identity");
    }
    let descendant = document.add_object(descendant);
    let to_unicode = document.add_object(Stream::new(Dictionary::new(), to_unicode(&font.used)));
    document.objects.insert(
        id,
        Object::Dictionary(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => Object::Name(base.into_bytes()),
            "Encoding" => "Identity-H",
            "DescendantFonts" => vec![Object::Reference(descendant)],
            "ToUnicode" => to_unicode,
        }),
    );
    Ok(())
}

fn to_unicode(used: &BTreeMap<u16, (f32, String)>) -> Vec<u8> {
    let entries = used
        .iter()
        .filter(|(_, (_, text))| !text.is_empty())
        .map(|(cid, (_, text))| {
            let units = text
                .encode_utf16()
                .map(|unit| format!("{unit:04X}"))
                .collect::<String>();
            format!("<{cid:04X}> <{units}>\n")
        })
        .collect::<Vec<_>>();
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    // At most 100 entries to a block (Adobe Technical Note 5411).
    for block in entries.chunks(100) {
        cmap.push_str(&format!("{} beginbfchar\n", block.len()));
        for entry in block {
            cmap.push_str(entry);
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    cmap.into_bytes()
}
