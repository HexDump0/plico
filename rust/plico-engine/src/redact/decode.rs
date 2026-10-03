//! Decoding streams through every filter readers handle short of image
//! codecs. lopdf alone stops at a filter chain, at parameters held by
//! reference, and at ASCIIHex and RunLength, and each of those would send a
//! page to a picture for no reason.

use lopdf::{Dictionary, Document, Object, Stream};

use crate::documents::MAX_DECOMPRESSED_STREAM;

/// A stream's bytes after every filter up to, not including, a final
/// DCTDecode, and whether one remains. `None` when a filter cannot be undone.
pub(super) fn decode(document: &Document, stream: &Stream) -> Option<(Vec<u8>, bool)> {
    let resolve = |value: &Object| match value {
        Object::Reference(id) => document.get_object(*id).ok().cloned(),
        value => Some(value.clone()),
    };
    let filters = match stream.dict.get(b"Filter").ok().and_then(resolve) {
        None | Some(Object::Null) => Vec::new(),
        Some(Object::Name(name)) => vec![name],
        Some(Object::Array(items)) => items
            .iter()
            .map(|item| resolve(item)?.as_name().ok().map(<[u8]>::to_vec))
            .collect::<Option<Vec<_>>>()?,
        Some(_) => return None,
    };
    let parameters = match stream.dict.get(b"DecodeParms").ok().and_then(resolve) {
        Some(Object::Array(items)) => items.iter().map(resolve).collect(),
        Some(single) => vec![Some(single)],
        None => Vec::new(),
    };
    let mut bytes = stream.content.clone();
    for (index, filter) in filters.iter().enumerate() {
        let parameters = parameters
            .get(index)
            .cloned()
            .flatten()
            .and_then(|value| value.as_dict().ok().cloned());
        bytes = match filter.as_slice() {
            b"DCTDecode" | b"DCT" if index == filters.len() - 1 => return Some((bytes, true)),
            b"ASCIIHexDecode" | b"AHx" => ascii_hex(&bytes)?,
            b"RunLengthDecode" | b"RL" => run_length(&bytes)?,
            b"FlateDecode" | b"Fl" | b"LZWDecode" | b"LZW" | b"ASCII85Decode" | b"A85" => {
                lopdf_filter(filter, parameters, bytes)?
            }
            _ => return None,
        };
        if bytes.len() > MAX_DECOMPRESSED_STREAM {
            return None;
        }
    }
    Some((bytes, false))
}

/// One filter through lopdf, then its predictor. lopdf reads parameters only
/// when given directly, and its PNG Average predictor adds half the pixel
/// above to the whole pixel to the left rather than averaging them
/// (lopdf 0.44, `filters/png.rs`), which garbles every row that uses it
/// (pdf.js corpus: issue14814.pdf). So it only inflates here.
fn lopdf_filter(filter: &[u8], parameters: Option<Dictionary>, bytes: Vec<u8>) -> Option<Vec<u8>> {
    let name = match filter {
        b"Fl" => b"FlateDecode".as_slice(),
        b"LZW" => b"LZWDecode",
        b"A85" => b"ASCII85Decode",
        name => name,
    };
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(name.to_vec()));
    if let Some(early) = parameters
        .as_ref()
        .and_then(|parameters| parameters.get(b"EarlyChange").ok())
    {
        dict.set(
            "DecodeParms",
            lopdf::dictionary! { "EarlyChange" => early.clone() },
        );
    }
    let decoded = Stream::new(dict, bytes)
        .decompressed_content_with_limit(MAX_DECOMPRESSED_STREAM)
        .ok()?;
    match parameters {
        Some(parameters) if name != b"ASCII85Decode" => unpredict(decoded, &parameters),
        _ => Some(decoded),
    }
}

/// Undoes a TIFF or PNG predictor (ISO 32000-1, 7.4.4.4).
fn unpredict(data: Vec<u8>, parameters: &Dictionary) -> Option<Vec<u8>> {
    let read = |key: &[u8], default: i64| {
        parameters
            .get(key)
            .and_then(Object::as_i64)
            .unwrap_or(default)
    };
    let predictor = read(b"Predictor", 1);
    if predictor == 1 {
        return Some(data);
    }
    let colors = usize::try_from(read(b"Colors", 1))
        .ok()
        .filter(|&value| (1..=32).contains(&value))?;
    let bits = usize::try_from(read(b"BitsPerComponent", 8)).ok()?;
    let columns = usize::try_from(read(b"Columns", 1))
        .ok()
        .filter(|&value| value >= 1)?;
    if !matches!(bits, 1 | 2 | 4 | 8 | 16) {
        return None;
    }
    let pixel = (colors * bits).div_ceil(8);
    let row = (colors * bits * columns).div_ceil(8);
    if predictor == 2 {
        // TIFF: each sample adds the one before it. Only whole bytes here.
        if bits != 8 {
            return None;
        }
        let mut data = data;
        for line in data.chunks_mut(row) {
            for at in pixel..line.len() {
                line[at] = line[at].wrapping_add(line[at - pixel]);
            }
        }
        return Some(data);
    }
    if !(10..=15).contains(&predictor) {
        return None;
    }
    // PNG: each row starts with its own filter type.
    let mut decoded = Vec::with_capacity(data.len() / (row + 1) * row);
    let mut previous = vec![0u8; row];
    for chunk in data.chunks(row + 1) {
        let (&filter, bytes) = chunk.split_first()?;
        let mut current = bytes.to_vec();
        current.resize(row, 0);
        for at in 0..row {
            let left = if at >= pixel { current[at - pixel] } else { 0 };
            let above = previous[at];
            let corner = if at >= pixel { previous[at - pixel] } else { 0 };
            let predicted = match filter {
                0 => 0,
                1 => left,
                2 => above,
                3 => ((u16::from(left) + u16::from(above)) / 2) as u8,
                4 => {
                    let estimate = i16::from(left) + i16::from(above) - i16::from(corner);
                    let (to_left, to_above, to_corner) = (
                        (estimate - i16::from(left)).abs(),
                        (estimate - i16::from(above)).abs(),
                        (estimate - i16::from(corner)).abs(),
                    );
                    if to_left <= to_above && to_left <= to_corner {
                        left
                    } else if to_above <= to_corner {
                        above
                    } else {
                        corner
                    }
                }
                _ => return None,
            };
            current[at] = current[at].wrapping_add(predicted);
        }
        decoded.extend_from_slice(&current[..bytes.len().min(row)]);
        previous = current;
    }
    Some(decoded)
}

fn ascii_hex(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut digits = Vec::new();
    for &byte in bytes {
        match byte {
            b'>' => break,
            byte if byte.is_ascii_hexdigit() => digits.push((byte as char).to_digit(16)? as u8),
            byte if byte.is_ascii_whitespace() || byte == 0 => {}
            _ => return None,
        }
    }
    if digits.len() % 2 == 1 {
        digits.push(0);
    }
    Some(
        digits
            .chunks(2)
            .map(|pair| pair[0] << 4 | pair[1])
            .collect(),
    )
}

fn run_length(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut decoded = Vec::new();
    let mut at = 0;
    while let Some(&length) = bytes.get(at) {
        at += 1;
        match length {
            128 => break,
            0..=127 => {
                let count = usize::from(length) + 1;
                decoded.extend_from_slice(bytes.get(at..at + count)?);
                at += count;
            }
            _ => {
                let &byte = bytes.get(at)?;
                at += 1;
                decoded.extend(std::iter::repeat_n(byte, 257 - usize::from(length)));
            }
        }
        if decoded.len() > MAX_DECOMPRESSED_STREAM {
            return None;
        }
    }
    Some(decoded)
}
