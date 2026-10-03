//! A tokenizer for content streams and CMaps that remembers where each
//! operation came from, so whatever redaction leaves alone is copied byte for
//! byte. lopdf's parser re-encodes everything it reads and drops inline images
//! it cannot decode, which would quietly change pages outside the boxes.

use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Value {
    Number(f64),
    Bool(bool),
    Null,
    Name(Vec<u8>),
    String(Vec<u8>),
    Array(Vec<Value>),
    Dictionary(Vec<(Vec<u8>, Value)>),
}

impl Value {
    pub(super) fn number(&self) -> Option<f64> {
        match self {
            Value::Number(number) if number.is_finite() => Some(*number),
            _ => None,
        }
    }

    pub(super) fn name(&self) -> Option<&[u8]> {
        match self {
            Value::Name(name) => Some(name),
            _ => None,
        }
    }

    pub(super) fn get(&self, key: &[u8]) -> Option<&Value> {
        match self {
            Value::Dictionary(entries) => entries
                .iter()
                .find(|(found, _)| found == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }
}

pub(super) struct Operation {
    pub(super) operator: Vec<u8>,
    pub(super) operands: Vec<Value>,
    /// The operands and operator as they appear in the stream.
    pub(super) span: Range<usize>,
    /// For an inline image, its data between `ID` and `EI`.
    pub(super) image: Option<Range<usize>>,
}

impl Operation {
    pub(super) fn numbers<const N: usize>(&self) -> Option<[f64; N]> {
        if self.operands.len() != N {
            return None;
        }
        let mut numbers = [0.0; N];
        for (slot, operand) in numbers.iter_mut().zip(&self.operands) {
            *slot = operand.number()?;
        }
        Some(numbers)
    }
}

const MAX_NESTING: usize = 64;

fn is_space(byte: u8) -> bool {
    matches!(byte, 0 | 9 | 10 | 12 | 13 | 32)
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

pub(super) struct Lexer<'a> {
    bytes: &'a [u8],
    at: usize,
}

enum Token {
    Value(Value),
    Keyword(Vec<u8>),
    ArrayOpen,
    ArrayClose,
    DictionaryOpen,
    DictionaryClose,
}

impl<'a> Lexer<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Lexer<'a> {
        Lexer { bytes, at: 0 }
    }

    fn skip_space(&mut self) {
        while let Some(&byte) = self.bytes.get(self.at) {
            if is_space(byte) {
                self.at += 1;
            } else if byte == b'%' {
                while let Some(&byte) = self.bytes.get(self.at) {
                    if byte == b'\n' || byte == b'\r' {
                        break;
                    }
                    self.at += 1;
                }
            } else {
                break;
            }
        }
    }

    fn token(&mut self) -> Result<Option<Token>, ()> {
        self.skip_space();
        let Some(&byte) = self.bytes.get(self.at) else {
            return Ok(None);
        };
        let token = match byte {
            b'[' => {
                self.at += 1;
                Token::ArrayOpen
            }
            b']' => {
                self.at += 1;
                Token::ArrayClose
            }
            b'<' if self.bytes.get(self.at + 1) == Some(&b'<') => {
                self.at += 2;
                Token::DictionaryOpen
            }
            b'>' if self.bytes.get(self.at + 1) == Some(&b'>') => {
                self.at += 2;
                Token::DictionaryClose
            }
            b'<' => Token::Value(Value::String(self.hex_string()?)),
            b'(' => Token::Value(Value::String(self.literal_string()?)),
            b'/' => {
                self.at += 1;
                Token::Value(Value::Name(self.name()))
            }
            // PostScript procedures in CMaps; nothing in a content stream.
            b'{' | b'}' => {
                self.at += 1;
                return self.token();
            }
            b')' | b'>' => return Err(()),
            _ => {
                let start = self.at;
                while let Some(&byte) = self.bytes.get(self.at) {
                    if is_space(byte) || is_delimiter(byte) {
                        break;
                    }
                    self.at += 1;
                }
                let word = &self.bytes[start..self.at];
                match word {
                    b"true" => Token::Value(Value::Bool(true)),
                    b"false" => Token::Value(Value::Bool(false)),
                    b"null" => Token::Value(Value::Null),
                    _ => match number(word) {
                        Some(number) => Token::Value(Value::Number(number)),
                        None => Token::Keyword(word.to_vec()),
                    },
                }
            }
        };
        Ok(Some(token))
    }

    fn name(&mut self) -> Vec<u8> {
        let mut name = Vec::new();
        while let Some(&byte) = self.bytes.get(self.at) {
            if is_space(byte) || is_delimiter(byte) {
                break;
            }
            self.at += 1;
            if byte == b'#'
                && let Some(code) = self
                    .bytes
                    .get(self.at..self.at + 2)
                    .and_then(|hex| std::str::from_utf8(hex).ok())
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                name.push(code);
                self.at += 2;
            } else {
                name.push(byte);
            }
        }
        name
    }

    fn hex_string(&mut self) -> Result<Vec<u8>, ()> {
        self.at += 1;
        let mut digits = Vec::new();
        loop {
            let &byte = self.bytes.get(self.at).ok_or(())?;
            self.at += 1;
            match byte {
                b'>' => break,
                byte if byte.is_ascii_hexdigit() => digits.push(byte),
                // Readers skip anything else, as pdf.js does.
                _ => {}
            }
        }
        // An odd final digit is followed by an implied 0.
        if digits.len() % 2 == 1 {
            digits.push(b'0');
        }
        Ok(digits
            .chunks(2)
            .map(|pair| {
                let value = |digit: u8| (digit as char).to_digit(16).unwrap_or(0) as u8;
                value(pair[0]) << 4 | value(pair[1])
            })
            .collect())
    }

    fn literal_string(&mut self) -> Result<Vec<u8>, ()> {
        self.at += 1;
        let mut text = Vec::new();
        let mut depth = 0;
        loop {
            let &byte = self.bytes.get(self.at).ok_or(())?;
            self.at += 1;
            match byte {
                b'(' => {
                    depth += 1;
                    text.push(byte);
                }
                b')' if depth == 0 => break,
                b')' => {
                    depth -= 1;
                    text.push(byte);
                }
                b'\\' => {
                    let &escaped = self.bytes.get(self.at).ok_or(())?;
                    self.at += 1;
                    match escaped {
                        b'n' => text.push(b'\n'),
                        b'r' => text.push(b'\r'),
                        b't' => text.push(b'\t'),
                        b'b' => text.push(8),
                        b'f' => text.push(12),
                        b'\r' => {
                            if self.bytes.get(self.at) == Some(&b'\n') {
                                self.at += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut code = u32::from(escaped - b'0');
                            for _ in 0..2 {
                                match self.bytes.get(self.at) {
                                    Some(&digit @ b'0'..=b'7') => {
                                        code = code * 8 + u32::from(digit - b'0');
                                        self.at += 1;
                                    }
                                    _ => break,
                                }
                            }
                            text.push(code as u8);
                        }
                        other => text.push(other),
                    }
                }
                // An end of line in a string reads as a line feed.
                b'\r' => {
                    if self.bytes.get(self.at) == Some(&b'\n') {
                        self.at += 1;
                    }
                    text.push(b'\n');
                }
                _ => text.push(byte),
            }
        }
        Ok(text)
    }

    /// A value, given its first token.
    fn value(&mut self, token: Token, depth: usize) -> Result<Value, ()> {
        if depth > MAX_NESTING {
            return Err(());
        }
        match token {
            Token::Value(value) => Ok(value),
            Token::ArrayOpen => {
                let mut items = Vec::new();
                loop {
                    match self.token()?.ok_or(())? {
                        Token::ArrayClose => return Ok(Value::Array(items)),
                        // Malformed, but readers take the keyword as ending
                        // nothing; it cannot be an operand, so give up.
                        Token::Keyword(_) => return Err(()),
                        token => items.push(self.value(token, depth + 1)?),
                    }
                }
            }
            Token::DictionaryOpen => {
                let mut entries = Vec::new();
                loop {
                    match self.token()?.ok_or(())? {
                        Token::DictionaryClose => return Ok(Value::Dictionary(entries)),
                        Token::Value(Value::Name(key)) => {
                            let token = self.token()?.ok_or(())?;
                            if matches!(token, Token::DictionaryClose) {
                                return Ok(Value::Dictionary(entries));
                            }
                            entries.push((key, self.value(token, depth + 1)?));
                        }
                        _ => return Err(()),
                    }
                }
            }
            Token::ArrayClose | Token::DictionaryClose | Token::Keyword(_) => Err(()),
        }
    }

    /// The next operation, or `None` at the end. Stray closing brackets are
    /// skipped, as readers do.
    pub(super) fn operation(&mut self) -> Result<Option<Operation>, ()> {
        let mut operands = Vec::new();
        let mut start = None;
        loop {
            self.skip_space();
            let here = self.at;
            let Some(token) = self.token()? else {
                // Operands with no operator at the end are ignored by readers.
                return Ok(None);
            };
            let start = *start.get_or_insert(here);
            match token {
                Token::Keyword(operator) => {
                    if operator == b"BI" {
                        return self.inline_image(start).map(Some);
                    }
                    return Ok(Some(Operation {
                        operator,
                        operands,
                        span: start..self.at,
                        image: None,
                    }));
                }
                Token::ArrayClose | Token::DictionaryClose => {}
                token => operands.push(self.value(token, 0)?),
            }
        }
    }

    /// `BI` key value pairs `ID` data `EI`. The data's length is known only
    /// for an unfiltered image, so otherwise its end is the first `EI` that
    /// stands alone and is followed by something that reads as content.
    fn inline_image(&mut self, start: usize) -> Result<Operation, ()> {
        let mut entries = Vec::new();
        loop {
            match self.token()?.ok_or(())? {
                Token::Keyword(keyword) if keyword == b"ID" => break,
                Token::Value(Value::Name(key)) => {
                    let token = self.token()?.ok_or(())?;
                    entries.push((key, self.value(token, 0)?));
                }
                _ => return Err(()),
            }
        }
        // A single white-space character separates ID from the data.
        self.at += 1;
        let data_start = self.at;
        let dictionary = Value::Dictionary(entries);
        let data_end = match unfiltered_length(&dictionary) {
            Some(length) if self.ends_image_at(data_start + length, true) => data_start + length,
            _ => (data_start..self.bytes.len())
                .find(|&at| self.ends_image_at(at, false))
                .ok_or(())?,
        };
        self.at = data_end;
        while self.bytes.get(self.at).is_some_and(|&byte| is_space(byte)) {
            self.at += 1;
        }
        self.at += 2;
        Ok(Operation {
            operator: b"BI".to_vec(),
            operands: vec![dictionary],
            span: start..self.at,
            image: Some(data_start..data_end),
        })
    }

    /// Whether the data could end at `at`: `EI`, after white space unless the
    /// length is `exact`, then the end of the stream or something that reads
    /// as content rather than binary.
    fn ends_image_at(&self, at: usize, exact: bool) -> bool {
        let mut cursor = at;
        while self.bytes.get(cursor).is_some_and(|&byte| is_space(byte)) {
            cursor += 1;
        }
        if (!exact && cursor == at) || self.bytes.get(cursor..cursor + 2) != Some(b"EI") {
            return false;
        }
        let after = cursor + 2;
        match self.bytes.get(after) {
            None => true,
            Some(&byte) if is_space(byte) || is_delimiter(byte) => self.bytes
                [after..(after + 16).min(self.bytes.len())]
                .iter()
                .all(|&byte| is_space(byte) || (0x20..0x7f).contains(&byte)),
            Some(_) => false,
        }
    }
}

fn number(word: &[u8]) -> Option<f64> {
    let text = std::str::from_utf8(word).ok()?;
    if text.is_empty()
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.'))
        || !text.bytes().any(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    // Readers accept doubled signs and stray signs inside a number, such as
    // `--5` or `5-3`, as pdf.js does: the sign up front, then the digits.
    let negative = text.starts_with('-');
    let digits = text.trim_start_matches(['+', '-']).replace(['+', '-'], "");
    let value = if digits.starts_with('.') {
        format!("0{digits}").parse::<f64>().ok()?
    } else {
        let digits = digits.trim_end_matches('.');
        let mut parts = digits.splitn(2, '.');
        let whole = parts.next().unwrap_or("0");
        match parts.next() {
            Some(fraction) => {
                let fraction = fraction.replace('.', "");
                format!("{whole}.{fraction}").parse::<f64>().ok()?
            }
            None => whole.parse::<f64>().ok()?,
        }
    };
    Some(if negative { -value } else { value })
}

/// How many bytes an inline image without a filter holds.
fn unfiltered_length(dictionary: &Value) -> Option<usize> {
    let get = |short: &[u8], long: &[u8]| dictionary.get(short).or_else(|| dictionary.get(long));
    if get(b"F", b"Filter")
        .is_some_and(|filter| !matches!(filter, Value::Array(items) if items.is_empty()))
    {
        return None;
    }
    let width = get(b"W", b"Width")?.number()? as usize;
    let height = get(b"H", b"Height")?.number()? as usize;
    let mask = matches!(get(b"IM", b"ImageMask"), Some(Value::Bool(true)));
    let bits = if mask {
        1
    } else {
        get(b"BPC", b"BitsPerComponent")?.number()? as usize
    };
    let components = if mask {
        1
    } else {
        inline_components(get(b"CS", b"ColorSpace")?)?
    };
    Some((width * components * bits).div_ceil(8) * height)
}

/// Components of an inline image's colour space, abbreviated or not. Named
/// resources and arrays other than indexed ones are not worked out.
pub(super) fn inline_components(space: &Value) -> Option<usize> {
    match space {
        Value::Name(name) => match name.as_slice() {
            b"G" | b"DeviceGray" | b"CalGray" => Some(1),
            b"RGB" | b"DeviceRGB" | b"CalRGB" => Some(3),
            b"CMYK" | b"DeviceCMYK" => Some(4),
            b"I" | b"Indexed" => Some(1),
            _ => None,
        },
        Value::Array(items) => match items.first()?.name()? {
            b"I" | b"Indexed" => Some(1),
            _ => None,
        },
        _ => None,
    }
}

/// Every operation in `bytes`, or `Err` where the stream stops making sense.
pub(super) fn operations(bytes: &[u8]) -> Result<Vec<Operation>, ()> {
    let mut lexer = Lexer::new(bytes);
    let mut operations = Vec::new();
    while let Some(operation) = lexer.operation()? {
        operations.push(operation);
    }
    Ok(operations)
}

/// Writes values back the way the lexer reads them. Strings are always hex,
/// which needs no escaping.
pub(super) fn write_value(out: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Number(number) => out.extend_from_slice(format_number(*number).as_bytes()),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::Null => out.extend_from_slice(b"null"),
        Value::Name(name) => write_name(out, name),
        Value::String(bytes) => {
            out.push(b'<');
            for byte in bytes {
                out.extend_from_slice(format!("{byte:02X}").as_bytes());
            }
            out.push(b'>');
        }
        Value::Array(items) => {
            out.push(b'[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(b' ');
                }
                write_value(out, item);
            }
            out.push(b']');
        }
        Value::Dictionary(entries) => {
            out.extend_from_slice(b"<<");
            for (key, item) in entries {
                write_name(out, key);
                out.push(b' ');
                write_value(out, item);
                out.push(b' ');
            }
            out.extend_from_slice(b">>");
        }
    }
}

pub(super) fn write_name(out: &mut Vec<u8>, name: &[u8]) {
    out.push(b'/');
    for &byte in name {
        if (0x21..0x7f).contains(&byte) && !is_delimiter(byte) && byte != b'#' {
            out.push(byte);
        } else {
            out.extend_from_slice(format!("#{byte:02X}").as_bytes());
        }
    }
}

/// Plain decimal, never an exponent, which PDF does not allow.
pub(super) fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    let text = format!("{value:.6}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    match text {
        "-0" | "" => "0".into(),
        text => text.into(),
    }
}
