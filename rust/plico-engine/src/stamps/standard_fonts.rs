//! Metrics for the standard Type 1 fonts every PDF reader carries, so text
//! can be placed without embedding a font. Widths are in thousandths of the
//! font size, indexed by WinAnsiEncoding code minus 32, from Adobe's Core 14
//! AFM files. Codes WinAnsiEncoding leaves undefined are 0 and never drawn.

use super::FontFamily;

/// Where the tops of capital letters sit above the baseline, which is how
/// text is centred visually rather than on its full ascent.
pub(super) fn cap_height(family: FontFamily, bold: bool) -> f32 {
    match (family, bold) {
        (FontFamily::Helvetica, _) => 0.718,
        (FontFamily::Times, false) => 0.662,
        (FontFamily::Times, true) => 0.676,
        (FontFamily::Courier, _) => 0.562,
    }
}

pub(super) fn base_font(family: FontFamily, bold: bool) -> &'static str {
    match (family, bold) {
        (FontFamily::Helvetica, false) => "Helvetica",
        (FontFamily::Helvetica, true) => "Helvetica-Bold",
        (FontFamily::Times, false) => "Times-Roman",
        (FontFamily::Times, true) => "Times-Bold",
        (FontFamily::Courier, false) => "Courier",
        (FontFamily::Courier, true) => "Courier-Bold",
    }
}

/// Width of already encoded text, in thousandths of the font size.
pub(super) fn text_width(family: FontFamily, bold: bool, encoded: &[u8]) -> u32 {
    let widths = match (family, bold) {
        (FontFamily::Helvetica, false) => &HELVETICA,
        (FontFamily::Helvetica, true) => &HELVETICA_BOLD,
        (FontFamily::Times, false) => &TIMES_ROMAN,
        (FontFamily::Times, true) => &TIMES_BOLD,
        // Monospaced: every defined glyph is 600 wide.
        (FontFamily::Courier, _) => return 600 * encoded.len() as u32,
    };
    encoded
        .iter()
        .map(|&code| u32::from(widths[usize::from(code) - 32]))
        .sum()
}

/// The width of WinAnsiEncoding `code` in one of the standard 14 fonts, by
/// its name, for a font a PDF uses without giving widths. `None` for the
/// fonts with no table here (italics of Times, Symbol, ZapfDingbats) and for
/// codes the encoding leaves undefined.
pub(crate) fn standard_width(name: &str, code: u8) -> Option<u16> {
    if name.starts_with("Courier") {
        return (code >= 32 && win_ansi_char(code).is_some()).then_some(600);
    }
    let widths = match name {
        "Helvetica" | "Helvetica-Oblique" => &HELVETICA,
        "Helvetica-Bold" | "Helvetica-BoldOblique" => &HELVETICA_BOLD,
        "Times-Roman" => &TIMES_ROMAN,
        "Times-Bold" => &TIMES_BOLD,
        _ => return None,
    };
    let width = *widths.get(usize::from(code).checked_sub(32)?)?;
    (width > 0).then_some(width)
}

/// The character WinAnsiEncoding gives `code`, the reverse of [`win_ansi`].
pub(crate) fn win_ansi_char(code: u8) -> Option<char> {
    const HIGH: [char; 32] = [
        '€', '\0', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\0', 'Ž', '\0', '\0',
        '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\0', 'ž', 'Ÿ',
    ];
    match code {
        0x20..=0x7E | 0xA0..=0xFF => Some(char::from(code)),
        0x80..=0x9F => Some(HIGH[usize::from(code - 0x80)]).filter(|&found| found != '\0'),
        _ => None,
    }
}

/// WinAnsiEncoding is Latin-1 plus typographic punctuation in 0x80 to 0x9F.
/// `None` for anything it cannot represent, including control characters.
pub(super) fn win_ansi(character: char) -> Option<u8> {
    let code = match character {
        ' '..='~' | '\u{A0}'..='\u{FF}' => character as u8,
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8A,
        '‹' => 0x8B,
        'Œ' => 0x8C,
        'Ž' => 0x8E,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9A,
        '›' => 0x9B,
        'œ' => 0x9C,
        'ž' => 0x9E,
        'Ÿ' => 0x9F,
        _ => return None,
    };
    Some(code)
}

const HELVETICA: [u16; 224] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
    0, 556, 0, 222, 556, 333, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0, 0, 222, 222,
    333, 333, 350, 556, 1000, 333, 1000, 500, 333, 944, 0, 500, 667, 278, 333, 556, 556, 556, 556,
    260, 556, 333, 737, 370, 556, 584, 333, 737, 333, 400, 584, 333, 333, 333, 556, 537, 278, 333,
    333, 365, 556, 834, 834, 834, 611, 667, 667, 667, 667, 667, 667, 1000, 722, 667, 667, 667, 667,
    278, 278, 278, 278, 722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667,
    611, 556, 556, 556, 556, 556, 556, 889, 500, 556, 556, 556, 556, 278, 278, 278, 278, 556, 556,
    556, 556, 556, 556, 556, 584, 611, 556, 556, 556, 556, 500, 556, 500,
];

const HELVETICA_BOLD: [u16; 224] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667,
    611, 778, 722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 333, 278, 333, 584, 556, 333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556,
    278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
    0, 556, 0, 278, 556, 500, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0, 0, 278, 278,
    500, 500, 350, 556, 1000, 333, 1000, 556, 333, 944, 0, 500, 667, 278, 333, 556, 556, 556, 556,
    280, 556, 333, 737, 370, 556, 584, 333, 737, 333, 400, 584, 333, 333, 333, 611, 556, 278, 333,
    333, 365, 556, 834, 834, 834, 611, 722, 722, 722, 722, 722, 722, 1000, 722, 667, 667, 667, 667,
    278, 278, 278, 278, 722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667,
    611, 556, 556, 556, 556, 556, 556, 889, 556, 556, 556, 556, 556, 278, 278, 278, 278, 611, 611,
    611, 611, 611, 611, 611, 584, 611, 611, 611, 611, 611, 556, 611, 556,
];

const TIMES_ROMAN: [u16; 224] = [
    250, 333, 408, 500, 500, 833, 778, 180, 333, 333, 500, 564, 250, 333, 250, 278, 500, 500, 500,
    500, 500, 500, 500, 500, 500, 500, 278, 278, 564, 564, 564, 444, 921, 722, 667, 667, 722, 611,
    556, 722, 722, 333, 389, 722, 611, 889, 722, 722, 556, 722, 667, 556, 611, 722, 722, 944, 722,
    722, 611, 333, 278, 333, 469, 500, 333, 444, 500, 444, 500, 444, 333, 500, 500, 278, 278, 500,
    278, 778, 500, 500, 500, 500, 333, 389, 278, 500, 500, 722, 500, 500, 444, 480, 200, 480, 541,
    0, 500, 0, 333, 500, 444, 1000, 500, 500, 333, 1000, 556, 333, 889, 0, 611, 0, 0, 333, 333,
    444, 444, 350, 500, 1000, 333, 980, 389, 333, 722, 0, 444, 722, 250, 333, 500, 500, 500, 500,
    200, 500, 333, 760, 276, 500, 564, 333, 760, 333, 400, 564, 300, 300, 333, 500, 453, 250, 333,
    300, 310, 500, 750, 750, 750, 444, 722, 722, 722, 722, 722, 722, 889, 667, 611, 611, 611, 611,
    333, 333, 333, 333, 722, 722, 722, 722, 722, 722, 722, 564, 722, 722, 722, 722, 722, 722, 556,
    500, 444, 444, 444, 444, 444, 444, 667, 444, 444, 444, 444, 444, 278, 278, 278, 278, 500, 500,
    500, 500, 500, 500, 500, 564, 500, 500, 500, 500, 500, 500, 500, 500,
];

const TIMES_BOLD: [u16; 224] = [
    250, 333, 555, 500, 500, 1000, 833, 278, 333, 333, 500, 570, 250, 333, 250, 278, 500, 500, 500,
    500, 500, 500, 500, 500, 500, 500, 333, 333, 570, 570, 570, 500, 930, 722, 667, 722, 722, 667,
    611, 778, 778, 389, 500, 778, 667, 944, 722, 778, 611, 778, 722, 556, 667, 722, 722, 1000, 722,
    722, 667, 333, 278, 333, 581, 500, 333, 500, 556, 444, 556, 444, 333, 500, 556, 278, 333, 556,
    278, 833, 556, 500, 556, 556, 444, 389, 333, 556, 500, 722, 500, 500, 444, 394, 220, 394, 520,
    0, 500, 0, 333, 500, 500, 1000, 500, 500, 333, 1000, 556, 333, 1000, 0, 667, 0, 0, 333, 333,
    500, 500, 350, 500, 1000, 333, 1000, 389, 333, 722, 0, 444, 722, 250, 333, 500, 500, 500, 500,
    220, 500, 333, 747, 300, 500, 570, 333, 747, 333, 400, 570, 300, 300, 333, 556, 540, 250, 333,
    300, 330, 500, 750, 750, 750, 500, 722, 722, 722, 722, 722, 722, 1000, 722, 667, 667, 667, 667,
    389, 389, 389, 389, 722, 722, 778, 778, 778, 778, 778, 570, 778, 722, 722, 722, 722, 722, 611,
    556, 500, 500, 500, 500, 500, 500, 722, 444, 444, 444, 444, 444, 278, 278, 278, 278, 500, 556,
    500, 500, 500, 500, 500, 570, 500, 556, 556, 556, 556, 500, 556, 500,
];
