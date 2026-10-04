//! Recognized text written into pages as an invisible layer, so a scan can be
//! searched, selected and copied while it looks exactly as it did.
//!
//! Pages are edited in place like stamping: each page gets a form drawn over
//! its content, in the page's visible frame. The text is drawn in render mode
//! 3 (neither filled nor stroked) in a glyphless font, the way Tesseract's own
//! PDF renderer and OCRmyPDF write theirs. Every character is its own CID, all
//! of them pointing at the font's one empty glyph, and ToUnicode says which
//! character each CID is, so one tiny font carries every script.

use std::collections::{BTreeMap, HashMap};

use lopdf::{Document, Object, Stream, dictionary};

use crate::documents::load_document;
use crate::stamps::{Stamper, finish, matrix, number, page_frame, selected_pages};

/// One empty glyph half an em wide, built by `scripts/build-glyphless-font.py`.
const GLYPHLESS_FONT: &[u8] = include_bytes!("../assets/glyphless.ttf");
/// The glyph's advance, in thousandths of an em, as the font program has it.
/// PDF/A wants /DW to agree with it (ISO 19005-2, 6.2.11.5).
const GLYPH_WIDTH: f32 = 500.0;
/// Readers place the selection box from the descriptor's ascent and descent,
/// so these make it cover a word from its ascenders to its descenders.
const ASCENT: f32 = 800.0;
const DESCENT: f32 = -200.0;
/// Codes are two bytes, and CID 0 is .notdef, which PDF/A forbids text from
/// using, so this many distinct characters fit.
const MAX_CHARACTERS: usize = 0xFFFF;
/// Horizontal scaling a word may be drawn at, in percent. Past this the word
/// box is nonsense, and a reader would select it as such.
const SCALE_RANGE: (f32, f32) = (1.0, 2_000.0);

/// A word as recognition found it, measured on the page as a reader sees it,
/// from its top left.
pub struct OcrWord<'a> {
    pub text: &'a str,
    /// Left edge and width, as fractions of the visible page's width.
    pub left: f32,
    pub width: f32,
    /// Where the baseline meets the left edge, and the line's height from its
    /// ascenders to its descenders, as fractions of the visible page's height.
    pub baseline: f32,
    pub size: f32,
    /// How far the baseline turns, counterclockwise, in radians.
    pub angle: f32,
    /// Whether a space follows on the same line. It is written out, so text
    /// copies with its spaces whatever gap a reader looks for.
    pub space: bool,
}

pub struct OcrPage<'a> {
    /// From 1.
    pub page: u32,
    pub words: Vec<OcrWord<'a>>,
}

/// Adds each page's words as invisible text over what the page shows. Pages
/// not listed are left as they were.
pub fn add_text_layer_bytes(
    input: &[u8],
    password: &str,
    pages: &[OcrPage<'_>],
) -> Result<Vec<u8>, String> {
    let mut by_page = BTreeMap::<u32, Vec<&OcrWord<'_>>>::new();
    for page in pages {
        let words = by_page.entry(page.page).or_default();
        words.extend(page.words.iter().filter(|word| drawable(word)));
    }
    by_page.retain(|_, words| !words.is_empty());
    if by_page.is_empty() {
        return Err("No text was found to add.".into());
    }

    let mut codes = Codes::default();
    let mut drawn = BTreeMap::new();
    for (&page, words) in &by_page {
        let encoded = words
            .iter()
            .map(|word| codes.encode(word))
            .collect::<Result<Vec<_>, _>>()?;
        drawn.insert(page, encoded);
    }

    let mut document = load_document(input, 1, password)?;
    let numbers = by_page.keys().copied().collect::<Vec<_>>();
    let targets = selected_pages(&document, &numbers)?;
    let font = codes.font(&mut document);
    let mut stamper = Stamper::new(&mut document, 1.0);
    for (page, page_id) in targets {
        let frame = page_frame(&document, page_id)?;
        let mut content = format!("q\n{} cm\nBT\n3 Tr\n", matrix(frame.matrix));
        let mut font_size = None;
        let mut scaling = None;
        for (word, encoded) in by_page[&page].iter().zip(&drawn[&page]) {
            let size = word.size * frame.height;
            let width = word.width * frame.width / word.angle.cos().max(0.1);
            let letters = word.text.trim().chars().count() as f32;
            let scale = (width / (letters * GLYPH_WIDTH / 1000.0 * size) * 100.0)
                .clamp(SCALE_RANGE.0, SCALE_RANGE.1);
            let size = number(size);
            if font_size.as_ref() != Some(&size) {
                content.push_str(&format!("/F0 {size} Tf\n"));
                font_size = Some(size);
            }
            let scale = number(scale);
            if scaling.as_ref() != Some(&scale) {
                content.push_str(&format!("{scale} Tz\n"));
                scaling = Some(scale);
            }
            let (sin, cos) = word.angle.sin_cos();
            let x = word.left * frame.width;
            let y = frame.height - word.baseline * frame.height;
            content.push_str(&format!(
                "{} Tm\n<{}> Tj\n",
                matrix([cos, sin, -sin, cos, x, y]),
                encoded
            ));
        }
        content.push_str("ET\nQ\n");
        let form = stamper.form(&mut document, &frame, content, Some(font), None);
        stamper.place(&mut document, page_id, form, false)?;
    }
    finish(document, 1.0)
}

/// Words with something to say and a place to say it. Recognition reports
/// specks as words now and then; they are left out rather than refused.
fn drawable(word: &OcrWord<'_>) -> bool {
    let finite = [word.left, word.width, word.baseline, word.size, word.angle]
        .iter()
        .all(|value| value.is_finite());
    finite
        && word
            .text
            .trim()
            .chars()
            .any(|character| !character.is_control())
        && word.width > 0.0
        && word.size > 0.0
        && word.size <= 1.0
        && word.angle.abs() < std::f32::consts::FRAC_PI_4
        && (-0.5..1.5).contains(&word.left)
        && (-0.5..1.5).contains(&word.baseline)
}

/// Each character's CID, in the order they first appear.
#[derive(Default)]
struct Codes {
    characters: Vec<char>,
    known: HashMap<char, u16>,
}

impl Codes {
    /// The word as a hex string of two-byte codes, with its space.
    fn encode(&mut self, word: &OcrWord<'_>) -> Result<String, String> {
        let mut hex = String::new();
        let characters = word.text.trim().chars().filter(|c| !c.is_control());
        for character in characters.chain(word.space.then_some(' ')) {
            let code = match self.known.get(&character) {
                Some(&code) => code,
                None => {
                    if self.characters.len() >= MAX_CHARACTERS {
                        return Err("This text uses too many different characters.".into());
                    }
                    self.characters.push(character);
                    let code = self.characters.len() as u16;
                    self.known.insert(character, code);
                    code
                }
            };
            hex.push_str(&format!("{code:04X}"));
        }
        Ok(hex)
    }

    /// The Type 0 font every page's layer draws with.
    fn font(&self, document: &mut Document) -> lopdf::ObjectId {
        let program = document.add_object(Stream::new(
            dictionary! { "Length1" => GLYPHLESS_FONT.len() as i64 },
            GLYPHLESS_FONT.to_vec(),
        ));
        let descriptor = document.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "GlyphLessFont",
            // Fixed pitch and symbolic: its glyphs are not a standard set.
            "Flags" => 5,
            "FontBBox" => vec![0.into(), DESCENT.into(), GLYPH_WIDTH.into(), ASCENT.into()],
            "ItalicAngle" => 0,
            "Ascent" => ASCENT,
            "Descent" => DESCENT,
            "CapHeight" => 700,
            "StemV" => 80,
            "FontFile2" => program,
        });
        // CID 0 stays on .notdef; every character goes to glyph 1.
        let mut glyphs = vec![0u8, 0];
        for _ in &self.characters {
            glyphs.extend_from_slice(&[0, 1]);
        }
        let glyph_map = document.add_object(Stream::new(dictionary! {}, glyphs));
        let descendant = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType2",
            "BaseFont" => "GlyphLessFont",
            "CIDSystemInfo" => dictionary! {
                "Registry" => Object::string_literal("Adobe"),
                "Ordering" => Object::string_literal("Identity"),
                "Supplement" => 0,
            },
            "FontDescriptor" => descriptor,
            "DW" => GLYPH_WIDTH,
            "CIDToGIDMap" => glyph_map,
        });
        let to_unicode = document.add_object(Stream::new(dictionary! {}, self.cmap().into_bytes()));
        document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => "GlyphLessFont",
            "Encoding" => "Identity-H",
            "DescendantFonts" => vec![descendant.into()],
            "ToUnicode" => to_unicode,
        })
    }

    /// A ToUnicode CMap naming each code's character, a hundred to a block as
    /// the CMap format allows.
    fn cmap(&self) -> String {
        let mut cmap = String::from(
            "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
             /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
             /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
             1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
        );
        for (block, characters) in self.characters.chunks(100).enumerate() {
            cmap.push_str(&format!("{} beginbfchar\n", characters.len()));
            for (index, character) in characters.iter().enumerate() {
                let code = block * 100 + index + 1;
                let mut units = [0u16; 2];
                let text = character
                    .encode_utf16(&mut units)
                    .iter()
                    .map(|unit| format!("{unit:04X}"))
                    .collect::<String>();
                cmap.push_str(&format!("<{code:04X}> <{text}>\n"));
            }
            cmap.push_str("endbfchar\n");
        }
        cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
        cmap
    }
}
