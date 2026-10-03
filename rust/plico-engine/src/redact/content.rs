//! Walks a content stream the way a reader draws it and writes it back with
//! whatever lies under the boxes taken out. Operations that draw nothing
//! under a box are copied byte for byte.
//!
//! Text goes glyph by glyph, the pen moved past each removed glyph so the
//! rest of the line stays where it was. Paths are cut: fills lose the box's
//! area exactly, strokes lose the segments inside it. Images get their pixels
//! overwritten in a copy. Forms are redacted in a copy for this use only.

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use super::Unremovable;
use super::decode::decode;
use super::fonts::Font;
use super::geometry::{
    IDENTITY, Matrix, Point, Rect, apply, covered, flatten_cubic, invert, multiply,
    polygon_touches, segment_outside, subtract, translate,
};
use super::lexer::{self, Operation, Value, format_number, write_name, write_value};
use super::pixels::{ImageEdit, redact_image, redact_inline};

/// The page cannot be redacted in place: something under a box could not be
/// measured or decoded, so the caller draws the page as a picture instead.
#[derive(Debug)]
pub(super) struct NeedsImage(pub(super) Unremovable);

pub(super) struct FoundGlyph {
    /// Page space.
    pub(super) bounds: Rect,
    pub(super) text: String,
}

pub(super) struct Context<'a> {
    /// Page space.
    pub(super) boxes: &'a [Rect],
    pub(super) fill: [f64; 3],
    /// Set to gather every glyph instead of redacting, for finding text.
    pub(super) found: Option<Vec<FoundGlyph>>,
    fonts: HashMap<ObjectId, Rc<Font>>,
    names: usize,
}

impl<'a> Context<'a> {
    pub(super) fn new(boxes: &'a [Rect], fill: [f64; 3]) -> Context<'a> {
        Context {
            boxes,
            fill,
            found: None,
            fonts: HashMap::new(),
            names: 0,
        }
    }
}

pub(super) struct Rewritten {
    pub(super) content: Vec<u8>,
    pub(super) changed: bool,
    pub(super) resources: Dictionary,
    /// Graphics states saved and never restored.
    pub(super) open: usize,
    /// Marked content whose content changed, by MCID.
    pub(super) touched: BTreeSet<i64>,
}

/// Forms inside forms deeper than this are not followed.
const MAX_FORM_DEPTH: usize = 12;

/// How closely curves are followed when one has to be cut, in points.
const CURVE_TOLERANCE: f64 = 0.05;

/// Longer than any line of text, in ems: the band an unmeasurable run of
/// text could reach along its line.
const UNKNOWN_REACH: f64 = 1e6;

/// Cut-out pieces of one path beyond this are dropped rather than written.
const MAX_PIECES: usize = 4096;

pub(super) fn rewrite(
    document: &mut Document,
    context: &mut Context<'_>,
    content: &[u8],
    resources: Dictionary,
    ctm: Matrix,
    depth: usize,
) -> Result<Rewritten, NeedsImage> {
    let operations = lexer::operations(content).map_err(|_| NeedsImage(Unremovable::Content))?;
    let mut interpreter = Interpreter {
        document,
        context,
        bytes: content,
        resources,
        out: Vec::with_capacity(content.len() + 64),
        state: State {
            ctm,
            font: None,
            size: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            scale: 1.0,
            leading: 0.0,
            rise: 0.0,
        },
        stack: Vec::new(),
        text_matrix: IDENTITY,
        line_matrix: IDENTITY,
        uncertain: false,
        path: Path::default(),
        marks: Vec::new(),
        changed: false,
        touched: BTreeSet::new(),
        used: BTreeSet::new(),
        replaced: BTreeSet::new(),
        depth,
    };
    for operation in &operations {
        interpreter.run(operation)?;
    }
    // A path left unpainted at the end draws nothing.
    let mut resources = interpreter.resources;
    if !interpreter.replaced.is_empty() {
        let unused = interpreter
            .replaced
            .difference(&interpreter.used)
            .cloned()
            .collect::<Vec<_>>();
        if !unused.is_empty() {
            let xobjects = own_subdictionary(interpreter.document, &mut resources, b"XObject");
            for name in unused {
                xobjects.remove(&name);
            }
        }
    }
    Ok(Rewritten {
        content: interpreter.out,
        changed: interpreter.changed,
        resources,
        open: interpreter.stack.len(),
        touched: interpreter.touched,
    })
}

#[derive(Clone)]
struct State {
    ctm: Matrix,
    font: Option<Rc<Font>>,
    size: f64,
    char_spacing: f64,
    word_spacing: f64,
    /// Horizontal scaling as a fraction.
    scale: f64,
    leading: f64,
    rise: f64,
}

#[derive(Clone, Copy)]
enum Segment {
    Move(Point),
    Line(Point),
    Curve(Point, Point, Point),
    Close,
}

#[derive(Default)]
struct Path {
    /// The construction operations, to copy when the path is left alone.
    spans: Vec<std::ops::Range<usize>>,
    /// In user space.
    segments: Vec<Segment>,
    current: Point,
    start: Point,
    clip: Option<&'static [u8]>,
}

struct Mark {
    /// Where the opening operation was written.
    at: std::ops::Range<usize>,
    tag: Vec<u8>,
    mcid: Option<i64>,
    /// An inline property list carrying text that describes the content.
    describes: Option<Vec<(Vec<u8>, Value)>>,
    touched: bool,
}

/// Property list entries that repeat or describe the content in words.
const DESCRIPTIONS: [&[u8]; 3] = [b"ActualText", b"Alt", b"E"];

enum Element {
    Text(Vec<u8>),
    Adjust(f64),
}

struct Interpreter<'i, 'a> {
    document: &'i mut Document,
    context: &'i mut Context<'a>,
    bytes: &'i [u8],
    resources: Dictionary,
    out: Vec<u8>,
    state: State,
    stack: Vec<State>,
    text_matrix: Matrix,
    line_matrix: Matrix,
    /// The pen's position along the line is unknown: an unmeasurable glyph
    /// moved it.
    uncertain: bool,
    path: Path,
    marks: Vec<Mark>,
    changed: bool,
    touched: BTreeSet<i64>,
    /// XObject names drawn as they were, and names whose drawing changed.
    used: BTreeSet<Vec<u8>>,
    replaced: BTreeSet<Vec<u8>>,
    depth: usize,
}

impl Interpreter<'_, '_> {
    fn collecting(&self) -> bool {
        self.context.found.is_some()
    }

    fn copy(&mut self, operation: &Operation) {
        self.out
            .extend_from_slice(&self.bytes[operation.span.clone()]);
        self.out.push(b'\n');
    }

    fn write(&mut self, operands: &[Value], operator: &[u8]) {
        for operand in operands {
            write_value(&mut self.out, operand);
            self.out.push(b' ');
        }
        self.out.extend_from_slice(operator);
        self.out.push(b'\n');
    }

    /// Records that something under a box was taken out here.
    fn removed(&mut self) {
        self.changed = true;
        for mark in &mut self.marks {
            mark.touched = true;
        }
    }

    fn run(&mut self, operation: &Operation) -> Result<(), NeedsImage> {
        let operator = operation.operator.as_slice();
        match operator {
            b"m" | b"l" | b"c" | b"v" | b"y" | b"h" | b"re" => {
                self.construct(operation);
                return Ok(());
            }
            b"W" | b"W*" => {
                self.path.clip = Some(if operator == b"W" { b"W" } else { b"W*" });
                return Ok(());
            }
            b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" | b"n" => {
                self.paint(operation);
                return Ok(());
            }
            _ => {}
        }
        // Anything else ends a path that was never painted, which readers
        // ignore; it is copied as it was.
        self.flush_path();
        match operator {
            b"q" => {
                self.stack.push(self.state.clone());
                self.copy(operation);
            }
            b"Q" => {
                if let Some(state) = self.stack.pop() {
                    self.state = state;
                    self.copy(operation);
                }
            }
            b"cm" => {
                if let Some(matrix) = operation.numbers::<6>() {
                    self.state.ctm = multiply(matrix, self.state.ctm);
                }
                self.copy(operation);
            }
            b"BT" => {
                self.text_matrix = IDENTITY;
                self.line_matrix = IDENTITY;
                self.uncertain = false;
                self.copy(operation);
            }
            b"Td" | b"TD" => {
                if let Some([x, y]) = operation.numbers::<2>() {
                    if operator == b"TD" {
                        self.state.leading = -y;
                    }
                    self.next_line(x, y);
                }
                self.copy(operation);
            }
            b"Tm" => {
                if let Some(matrix) = operation.numbers::<6>() {
                    self.text_matrix = matrix;
                    self.line_matrix = matrix;
                    self.uncertain = false;
                }
                self.copy(operation);
            }
            b"T*" => {
                self.next_line(0.0, -self.state.leading);
                self.copy(operation);
            }
            b"Tc" | b"Tw" | b"Tz" | b"TL" | b"Ts" => {
                if let Some([value]) = operation.numbers::<1>() {
                    match operator {
                        b"Tc" => self.state.char_spacing = value,
                        b"Tw" => self.state.word_spacing = value,
                        b"Tz" => self.state.scale = value / 100.0,
                        b"TL" => self.state.leading = value,
                        _ => self.state.rise = value,
                    }
                }
                self.copy(operation);
            }
            b"Tf" => {
                if let [Value::Name(name), size] = operation.operands.as_slice()
                    && let Some(size) = size.number()
                {
                    self.state.font = self.font(name);
                    self.state.size = size;
                }
                self.copy(operation);
            }
            b"gs" => {
                self.graphics_state(operation);
                self.copy(operation);
            }
            b"Tj" => {
                if let [Value::String(text)] = operation.operands.as_slice() {
                    self.show(operation, vec![Element::Text(text.clone())], b"")?;
                } else {
                    self.copy(operation);
                }
            }
            b"'" => {
                if let [Value::String(text)] = operation.operands.as_slice() {
                    self.next_line(0.0, -self.state.leading);
                    self.show(operation, vec![Element::Text(text.clone())], b"T*\n")?;
                } else {
                    self.copy(operation);
                }
            }
            b"\"" => {
                if let [word, character, Value::String(text)] = operation.operands.as_slice()
                    && let (Some(word), Some(character)) = (word.number(), character.number())
                {
                    self.state.word_spacing = word;
                    self.state.char_spacing = character;
                    self.next_line(0.0, -self.state.leading);
                    let prefix = format!(
                        "{} Tw\n{} Tc\nT*\n",
                        format_number(word),
                        format_number(character)
                    );
                    self.show(
                        operation,
                        vec![Element::Text(text.clone())],
                        prefix.as_bytes(),
                    )?;
                } else {
                    self.copy(operation);
                }
            }
            b"TJ" => {
                if let [Value::Array(items)] = operation.operands.as_slice() {
                    let elements = items
                        .iter()
                        .filter_map(|item| match item {
                            Value::String(text) => Some(Element::Text(text.clone())),
                            item => item.number().map(Element::Adjust),
                        })
                        .collect();
                    self.show(operation, elements, b"")?;
                } else {
                    self.copy(operation);
                }
            }
            b"Do" => self.draw_xobject(operation)?,
            b"BI" => self.inline_image(operation)?,
            b"BMC" | b"BDC" => self.begin_marked(operation),
            b"EMC" => self.end_marked(operation),
            _ => self.copy(operation),
        }
        Ok(())
    }

    fn next_line(&mut self, x: f64, y: f64) {
        self.line_matrix = multiply(translate(x, y), self.line_matrix);
        self.text_matrix = self.line_matrix;
        self.uncertain = false;
    }

    fn font(&mut self, name: &[u8]) -> Option<Rc<Font>> {
        let fonts = self.resources.get(b"Font").ok()?;
        let fonts = match fonts {
            Object::Reference(id) => self.document.get_dictionary(*id).ok()?,
            Object::Dictionary(fonts) => fonts,
            _ => return None,
        };
        match fonts.get(name).ok()? {
            Object::Reference(id) => {
                if let Some(font) = self.context.fonts.get(id) {
                    return Some(font.clone());
                }
                let font = Rc::new(Font::load(
                    self.document,
                    self.document.get_dictionary(*id).ok()?,
                ));
                self.context.fonts.insert(*id, font.clone());
                Some(font)
            }
            Object::Dictionary(font) => Some(Rc::new(Font::load(self.document, font))),
            _ => None,
        }
    }

    /// A graphics state may set the font (8.4.5).
    fn graphics_state(&mut self, operation: &Operation) {
        let [Value::Name(name)] = operation.operands.as_slice() else {
            return;
        };
        let document = &*self.document;
        let resolve = |value: &Object| match value {
            Object::Reference(id) => document.get_object(*id).ok().cloned(),
            value => Some(value.clone()),
        };
        let Some(Object::Dictionary(states)) =
            self.resources.get(b"ExtGState").ok().and_then(resolve)
        else {
            return;
        };
        let Some(Object::Dictionary(state)) = states.get(name).ok().and_then(resolve) else {
            return;
        };
        let Some(Object::Array(font)) = state.get(b"Font").ok().and_then(resolve) else {
            return;
        };
        if let [Object::Reference(id), size] = font.as_slice()
            && let Ok(size) = size.as_float()
        {
            let font = match self.context.fonts.get(id) {
                Some(font) => Some(font.clone()),
                None => self.document.get_dictionary(*id).ok().map(|dictionary| {
                    let font = Rc::new(Font::load(self.document, dictionary));
                    self.context.fonts.insert(*id, font.clone());
                    font
                }),
            };
            if font.is_some() {
                self.state.font = font;
                self.state.size = f64::from(size);
            }
        }
    }

    fn show(
        &mut self,
        operation: &Operation,
        elements: Vec<Element>,
        prefix: &[u8],
    ) -> Result<(), NeedsImage> {
        let state = self.state.clone();
        let font = state.font.clone();
        let vertical = font.as_ref().is_some_and(|font| font.vertical());
        let render = [
            state.size * state.scale,
            0.0,
            0.0,
            state.size,
            0.0,
            state.rise,
        ];
        // How far an adjustment of one unit moves the pen, which a removed
        // glyph's advance has to be converted into.
        let unit = if vertical {
            state.size / 1000.0
        } else {
            state.size * state.scale / 1000.0
        };
        let mut rebuilt = Rebuilt::default();
        let mut any_removed = false;

        for element in &elements {
            match element {
                Element::Adjust(amount) => {
                    let shift = -amount * unit;
                    self.advance(shift, vertical);
                    rebuilt.gap(*amount);
                }
                Element::Text(text) => {
                    let glyphs = match &font {
                        Some(font) => font.glyphs(text),
                        None => Vec::new(),
                    };
                    if font.is_none() {
                        self.unmeasured(&state, render, None)?;
                        self.uncertain = true;
                        rebuilt.text(text);
                        continue;
                    }
                    for glyph in glyphs {
                        let bytes = &text[glyph.start..glyph.end];
                        let Some(advance) = glyph.advance.filter(|_| !self.uncertain) else {
                            self.unmeasured(&state, render, Some(&glyph.bounds))?;
                            self.uncertain = true;
                            rebuilt.text(bytes);
                            continue;
                        };
                        let to_page = multiply(multiply(render, self.text_matrix), state.ctm);
                        let [x0, y0, x1, y1] = glyph.bounds;
                        let bounds = Rect::around(
                            [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
                                .map(|corner| apply(to_page, corner)),
                        );
                        let shift = advance * state.size
                            + state.char_spacing
                            + if glyph.space { state.word_spacing } else { 0.0 };
                        let shift = if vertical { shift } else { shift * state.scale };
                        if let (Some(found), Some(bounds)) = (&mut self.context.found, bounds) {
                            found.push(FoundGlyph {
                                bounds,
                                text: glyph.text.clone(),
                            });
                        }
                        let remove = !self.context.boxes.is_empty()
                            && bounds.is_some_and(|bounds| covered(bounds, self.context.boxes));
                        if remove {
                            any_removed = true;
                            if shift != 0.0 {
                                if unit == 0.0 {
                                    // Nothing can stand in for the advance.
                                    return Err(NeedsImage(Unremovable::Text));
                                }
                                rebuilt.gap(-shift / unit);
                            }
                        } else {
                            rebuilt.text(bytes);
                        }
                        self.advance(shift, vertical);
                    }
                }
            }
        }

        if !any_removed {
            self.copy(operation);
            return Ok(());
        }
        let rebuilt = rebuilt.finish();
        self.removed();
        self.out.extend_from_slice(prefix);
        if !rebuilt.is_empty() {
            self.write(&[Value::Array(rebuilt)], b"TJ");
        }
        Ok(())
    }

    fn advance(&mut self, shift: f64, vertical: bool) {
        let step = if vertical {
            translate(0.0, shift)
        } else {
            translate(shift, 0.0)
        };
        self.text_matrix = multiply(step, self.text_matrix);
    }

    /// Text whose place along its line is unknown could be anywhere on it:
    /// if a box crosses that line, the page cannot be redacted in place.
    fn unmeasured(
        &self,
        state: &State,
        render: Matrix,
        bounds: Option<&[f64; 4]>,
    ) -> Result<(), NeedsImage> {
        if self.context.boxes.is_empty() {
            return Ok(());
        }
        let [_, bottom, _, top] = bounds.copied().unwrap_or([0.0, -0.35, 0.0, 1.1]);
        let to_page = multiply(multiply(render, self.text_matrix), state.ctm);
        let band = Rect::around(
            [
                (-UNKNOWN_REACH, bottom.min(-0.35)),
                (UNKNOWN_REACH, bottom.min(-0.35)),
                (UNKNOWN_REACH, top.max(1.1)),
                (-UNKNOWN_REACH, top.max(1.1)),
            ]
            .map(|corner| apply(to_page, corner)),
        );
        match band {
            Some(band) if !self.context.boxes.iter().any(|area| area.overlaps(band)) => Ok(()),
            _ => Err(NeedsImage(Unremovable::Text)),
        }
    }

    fn construct(&mut self, operation: &Operation) {
        let path = &mut self.path;
        let point = |values: &[f64]| (values[0], values[1]);
        let ok = match operation.operator.as_slice() {
            b"m" => operation.numbers::<2>().map(|values| {
                let to = point(&values);
                path.segments.push(Segment::Move(to));
                path.current = to;
                path.start = to;
            }),
            b"l" => operation.numbers::<2>().map(|values| {
                let to = point(&values);
                path.segments.push(Segment::Line(to));
                path.current = to;
            }),
            b"c" => operation.numbers::<6>().map(|values| {
                let to = point(&values[4..]);
                path.segments
                    .push(Segment::Curve(point(&values), point(&values[2..]), to));
                path.current = to;
            }),
            b"v" => operation.numbers::<4>().map(|values| {
                let to = point(&values[2..]);
                path.segments
                    .push(Segment::Curve(path.current, point(&values), to));
                path.current = to;
            }),
            b"y" => operation.numbers::<4>().map(|values| {
                let to = point(&values[2..]);
                path.segments.push(Segment::Curve(point(&values), to, to));
                path.current = to;
            }),
            b"h" => {
                path.segments.push(Segment::Close);
                path.current = path.start;
                Some(())
            }
            _ => operation.numbers::<4>().map(|[x, y, width, height]| {
                path.segments.extend([
                    Segment::Move((x, y)),
                    Segment::Line((x + width, y)),
                    Segment::Line((x + width, y + height)),
                    Segment::Line((x, y + height)),
                    Segment::Close,
                ]);
                path.current = (x, y);
                path.start = (x, y);
            }),
        };
        // A malformed operation is kept as it was; readers skip it.
        let _ = ok;
        path.spans.push(operation.span.clone());
    }

    fn flush_path(&mut self) {
        if self.path.spans.is_empty() && self.path.clip.is_none() {
            return;
        }
        let path = std::mem::take(&mut self.path);
        for span in path.spans {
            self.out.extend_from_slice(&self.bytes[span]);
            self.out.push(b'\n');
        }
        if let Some(clip) = path.clip {
            self.out.extend_from_slice(clip);
            self.out.push(b'\n');
        }
        self.path.current = path.current;
        self.path.start = path.start;
    }

    fn paint(&mut self, operation: &Operation) {
        let operator = operation.operator.as_slice();
        let ctm = self.state.ctm;
        let points = self
            .path
            .segments
            .iter()
            .flat_map(|segment| match *segment {
                Segment::Move(a) | Segment::Line(a) => vec![a],
                Segment::Curve(a, b, c) => vec![a, b, c],
                Segment::Close => Vec::new(),
            })
            .map(|point| apply(ctm, point));
        let bounds = Rect::around(points);
        let boxes = self.context.boxes;
        let touches = operator != b"n"
            && bounds.is_some_and(|bounds| boxes.iter().any(|area| area.overlaps(bounds)));
        if !touches {
            self.flush_path();
            self.copy(operation);
            return;
        }

        let path = std::mem::take(&mut self.path);
        self.path.current = path.current;
        self.path.start = path.start;
        let fills = matches!(operator, b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*");
        let strokes = matches!(operator, b"S" | b"s" | b"B" | b"B*" | b"b" | b"b*");
        // s, b and b* close the last subpath first, as h would.
        let closes_last = matches!(operator, b"s" | b"b" | b"b*");
        let even_odd = matches!(operator, b"f*" | b"B*" | b"b*");
        let subpaths = split_subpaths(&path.segments, closes_last);
        let inverse = invert(ctm);
        let mut changed = false;
        let mut written = Vec::new();

        if let Some(inverse) = inverse {
            if fills {
                let mut body = Vec::new();
                for subpath in &subpaths {
                    let polygon = flatten(subpath, ctm);
                    if polygon.len() < 3
                        || !boxes.iter().any(|area| polygon_touches(&polygon, *area))
                    {
                        write_subpath(&mut body, subpath, false);
                        continue;
                    }
                    changed = true;
                    let mut pieces = vec![polygon];
                    for area in boxes {
                        pieces = pieces
                            .iter()
                            .flat_map(|piece| subtract(piece, *area))
                            .collect();
                        if pieces.len() > MAX_PIECES {
                            pieces.clear();
                            break;
                        }
                    }
                    for piece in pieces {
                        write_points(&mut body, &piece, inverse, true);
                    }
                }
                if !body.is_empty() {
                    written.extend(body);
                    written.extend_from_slice(if even_odd { b"f*\n" } else { b"f\n" });
                }
            }
            if strokes {
                let mut body = Vec::new();
                for subpath in &subpaths {
                    let line = flatten(subpath, ctm);
                    let closed = subpath.closed && line.len() > 1;
                    let mut segments = line
                        .windows(2)
                        .map(|pair| (pair[0], pair[1]))
                        .collect::<Vec<_>>();
                    if closed {
                        segments.push((line[line.len() - 1], line[0]));
                    }
                    if line.len() == 1 {
                        segments.push((line[0], line[0]));
                    }
                    let mut runs: Vec<Vec<Point>> = Vec::new();
                    let mut cut = false;
                    for (a, b) in segments {
                        let kept = segment_outside(a, b, boxes);
                        if kept.len() != 1 || kept[0] != (a, b) {
                            cut = true;
                        }
                        for (from, to) in kept {
                            match runs.last_mut() {
                                Some(run) if run.last() == Some(&from) => run.push(to),
                                _ => runs.push(vec![from, to]),
                            }
                        }
                    }
                    if !cut {
                        write_subpath(&mut body, subpath, true);
                        continue;
                    }
                    changed = true;
                    // A closed outline cut somewhere else keeps its join at
                    // the start rather than gaining two caps there.
                    if closed
                        && runs.len() > 1
                        && runs.last().and_then(|run| run.last()) == runs[0].first()
                    {
                        let first = runs.remove(0);
                        if let Some(last) = runs.last_mut() {
                            last.extend_from_slice(&first[1..]);
                        }
                    }
                    for run in runs {
                        write_points(&mut body, &run, inverse, false);
                    }
                }
                if !body.is_empty() {
                    written.extend(body);
                    written.extend_from_slice(b"S\n");
                }
            }
        } else {
            // A degenerate matrix draws nothing, so the painting goes.
            changed = true;
        }

        if !changed {
            for span in path.spans {
                self.out.extend_from_slice(&self.bytes[span]);
                self.out.push(b'\n');
            }
            if let Some(clip) = path.clip {
                self.out.extend_from_slice(clip);
                self.out.push(b'\n');
            }
            self.copy(operation);
            return;
        }
        self.removed();
        self.out.extend(written);
        // The clip takes effect after painting, so drawing the cut path first
        // and then setting the clip from the original is the same.
        if let Some(clip) = path.clip {
            for span in path.spans {
                self.out.extend_from_slice(&self.bytes[span]);
                self.out.push(b'\n');
            }
            self.out.extend_from_slice(clip);
            self.out.extend_from_slice(b" n\n");
        }
    }

    fn draw_xobject(&mut self, operation: &Operation) -> Result<(), NeedsImage> {
        let [Value::Name(name)] = operation.operands.as_slice() else {
            self.copy(operation);
            return Ok(());
        };
        let Some(id) = xobject_id(self.document, &self.resources, name) else {
            self.copy(operation);
            return Ok(());
        };
        let Ok(Object::Stream(stream)) = self.document.get_object(id) else {
            self.copy(operation);
            return Ok(());
        };
        let subtype = stream
            .dict
            .get(b"Subtype")
            .and_then(Object::as_name)
            .unwrap_or_default()
            .to_vec();
        match subtype.as_slice() {
            b"Image" if !self.collecting() => {
                let stream = stream.clone();
                match redact_image(
                    self.document,
                    &stream,
                    self.state.ctm,
                    self.context.boxes,
                    self.context.fill,
                ) {
                    ImageEdit::Keep => {
                        self.used.insert(name.clone());
                        self.copy(operation);
                    }
                    ImageEdit::Drop => {
                        self.replaced.insert(name.clone());
                        self.removed();
                    }
                    ImageEdit::Replace(copy) => {
                        self.replaced.insert(name.clone());
                        self.removed();
                        let fresh = self.add_xobject(copy);
                        self.write(&[Value::Name(fresh)], b"Do");
                    }
                    ImageEdit::Unsupported => return Err(NeedsImage(Unremovable::Image)),
                }
            }
            b"Form" => {
                let stream = stream.clone();
                self.draw_form(operation, name, &stream)?;
            }
            _ => {
                self.used.insert(name.clone());
                self.copy(operation);
            }
        }
        Ok(())
    }

    fn draw_form(
        &mut self,
        operation: &Operation,
        name: &[u8],
        form: &Stream,
    ) -> Result<(), NeedsImage> {
        let document = &*self.document;
        let numbers = |key: &[u8]| {
            let value = form.dict.get(key).ok()?;
            let value = match value {
                Object::Reference(id) => document.get_object(*id).ok()?,
                value => value,
            };
            value
                .as_array()
                .ok()?
                .iter()
                .map(|item| item.as_float().ok().map(f64::from))
                .collect::<Option<Vec<_>>>()
        };
        let matrix = numbers(b"Matrix")
            .and_then(|values| <Matrix>::try_from(values).ok())
            .unwrap_or(IDENTITY);
        let ctm = multiply(matrix, self.state.ctm);
        let bbox = numbers(b"BBox")
            .filter(|values| values.len() == 4)
            .and_then(|values| Rect::around([(values[0], values[1]), (values[2], values[3])]));
        let corners = bbox.map(|bbox| bbox.corners().map(|corner| apply(ctm, corner)));
        let bounds = corners.and_then(Rect::around);
        let boxes = self.context.boxes;

        if !self.collecting() {
            let touches = match bounds {
                Some(bounds) => boxes.iter().any(|area| area.overlaps(bounds)),
                None => true,
            };
            if !touches {
                self.used.insert(name.to_vec());
                self.copy(operation);
                return Ok(());
            }
            if let Some(corners) = corners
                && boxes
                    .iter()
                    .any(|area| corners.iter().all(|corner| area.contains(*corner)))
            {
                self.replaced.insert(name.to_vec());
                self.removed();
                return Ok(());
            }
        }
        if self.depth >= MAX_FORM_DEPTH {
            if self.collecting() {
                self.used.insert(name.to_vec());
                self.copy(operation);
                return Ok(());
            }
            return Err(NeedsImage(Unremovable::Content));
        }

        let content = match decode(self.document, form) {
            Some((content, false)) => content,
            _ if self.collecting() => {
                self.copy(operation);
                return Ok(());
            }
            _ => return Err(NeedsImage(Unremovable::Content)),
        };
        let resources = match form.dict.get(b"Resources") {
            Ok(Object::Reference(id)) => self
                .document
                .get_dictionary(*id)
                .cloned()
                .unwrap_or_default(),
            Ok(Object::Dictionary(resources)) => resources.clone(),
            // Older files let a form use the resources of what draws it.
            _ => self.resources.clone(),
        };
        let result = rewrite(
            self.document,
            self.context,
            &content,
            resources,
            ctm,
            self.depth + 1,
        );
        let rewritten = match result {
            Ok(rewritten) => rewritten,
            Err(_) if self.collecting() => {
                self.copy(operation);
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        if !rewritten.changed || self.collecting() {
            self.used.insert(name.to_vec());
            self.copy(operation);
            return Ok(());
        }
        let mut dict = form.dict.clone();
        dict.remove(b"Filter");
        dict.remove(b"DecodeParms");
        dict.remove(b"Length");
        dict.set("Resources", rewritten.resources);
        let copy = self
            .document
            .add_object(Stream::new(dict, rewritten.content));
        self.replaced.insert(name.to_vec());
        self.removed();
        let fresh = self.add_xobject(copy);
        self.write(&[Value::Name(fresh)], b"Do");
        Ok(())
    }

    fn inline_image(&mut self, operation: &Operation) -> Result<(), NeedsImage> {
        if self.collecting() {
            self.copy(operation);
            return Ok(());
        }
        let ctm = self.state.ctm;
        let corners =
            [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].map(|corner| apply(ctm, corner));
        let boxes = self.context.boxes;
        let touches = Rect::around(corners)
            .is_some_and(|bounds| boxes.iter().any(|area| area.overlaps(bounds)));
        if !touches {
            self.copy(operation);
            return Ok(());
        }
        if boxes
            .iter()
            .any(|area| corners.iter().all(|corner| area.contains(*corner)))
        {
            self.removed();
            return Ok(());
        }
        let (Some(dictionary), Some(data)) = (operation.operands.first(), operation.image.clone())
        else {
            return Err(NeedsImage(Unremovable::Image));
        };
        match redact_inline(dictionary, &self.bytes[data], ctm, boxes, self.context.fill) {
            Some(None) => self.copy(operation),
            Some(Some(samples)) => {
                self.removed();
                self.out.extend_from_slice(b"BI");
                if let Value::Dictionary(entries) = dictionary {
                    for (key, value) in entries {
                        self.out.push(b' ');
                        write_name(&mut self.out, key);
                        self.out.push(b' ');
                        write_value(&mut self.out, value);
                    }
                }
                self.out.extend_from_slice(b" ID ");
                self.out.extend_from_slice(&samples);
                self.out.extend_from_slice(b"\nEI\n");
            }
            None => return Err(NeedsImage(Unremovable::Image)),
        }
        Ok(())
    }

    fn begin_marked(&mut self, operation: &Operation) {
        let start = self.out.len();
        self.copy(operation);
        let tag = operation
            .operands
            .first()
            .and_then(Value::name)
            .unwrap_or_default()
            .to_vec();
        let (mcid, describes) = match operation.operands.get(1) {
            Some(Value::Dictionary(entries)) => (
                operation.operands[1].get(b"MCID").and_then(Value::number),
                entries
                    .iter()
                    .any(|(key, _)| DESCRIPTIONS.contains(&key.as_slice()))
                    .then(|| entries.clone()),
            ),
            Some(Value::Name(name)) => {
                let properties = self.named_properties(name);
                let mcid = properties
                    .as_ref()
                    .and_then(|properties| properties.get(b"MCID").ok())
                    .and_then(|value| value.as_i64().ok())
                    .map(|mcid| mcid as f64);
                let describes = properties
                    .filter(|properties| {
                        tag != b"OC" && DESCRIPTIONS.iter().any(|key| properties.has(key))
                    })
                    .map(|_| {
                        mcid.map(|mcid| vec![(b"MCID".to_vec(), Value::Number(mcid))])
                            .unwrap_or_default()
                    });
                (mcid, describes)
            }
            _ => (None, None),
        };
        self.marks.push(Mark {
            at: start..self.out.len(),
            tag,
            mcid: mcid.map(|mcid| mcid as i64),
            describes,
            touched: false,
        });
    }

    fn named_properties(&self, name: &[u8]) -> Option<Dictionary> {
        let properties = match self.resources.get(b"Properties").ok()? {
            Object::Reference(id) => self.document.get_dictionary(*id).ok()?,
            Object::Dictionary(properties) => properties,
            _ => return None,
        };
        match properties.get(name).ok()? {
            Object::Reference(id) => self.document.get_dictionary(*id).ok().cloned(),
            Object::Dictionary(found) => Some(found.clone()),
            _ => None,
        }
    }

    /// Text describing content that lost something under a box would repeat
    /// what was removed, so it goes from the opening operation.
    fn end_marked(&mut self, operation: &Operation) {
        self.copy(operation);
        let Some(mark) = self.marks.pop() else {
            return;
        };
        if !mark.touched {
            return;
        }
        if let Some(mcid) = mark.mcid {
            self.touched.insert(mcid);
        }
        if let Some(entries) = mark.describes {
            let kept = entries
                .into_iter()
                .filter(|(key, _)| !DESCRIPTIONS.contains(&key.as_slice()))
                .collect::<Vec<_>>();
            let mut opening = Vec::new();
            write_name(&mut opening, &mark.tag);
            if kept.is_empty() {
                opening.extend_from_slice(b" BMC\n");
            } else {
                opening.push(b' ');
                write_value(&mut opening, &Value::Dictionary(kept));
                opening.extend_from_slice(b" BDC\n");
            }
            self.out.splice(mark.at, opening);
        }
    }

    fn add_xobject(&mut self, id: ObjectId) -> Vec<u8> {
        let xobjects = own_subdictionary(self.document, &mut self.resources, b"XObject");
        let name = loop {
            self.context.names += 1;
            let name = format!("Redacted{}", self.context.names).into_bytes();
            if !xobjects.has(&name) {
                break name;
            }
        };
        xobjects.set(name.clone(), id);
        self.used.insert(name.clone());
        name
    }
}

/// A TJ array being written: runs of kept bytes, with the pen moved past
/// removed glyphs in one adjustment per gap.
#[derive(Default)]
struct Rebuilt {
    items: Vec<Value>,
    pending: Vec<u8>,
    gap: f64,
}

impl Rebuilt {
    fn text(&mut self, bytes: &[u8]) {
        if self.gap != 0.0 {
            self.items.push(Value::Number(self.gap));
            self.gap = 0.0;
        }
        self.pending.extend_from_slice(bytes);
    }

    fn gap(&mut self, amount: f64) {
        if !self.pending.is_empty() {
            self.items
                .push(Value::String(std::mem::take(&mut self.pending)));
        }
        self.gap += amount;
    }

    fn finish(mut self) -> Vec<Value> {
        self.gap(0.0);
        if self.gap != 0.0 {
            self.items.push(Value::Number(self.gap));
        }
        self.items
    }
}

struct Subpath {
    segments: Vec<Segment>,
    closed: bool,
}

fn split_subpaths(segments: &[Segment], close_last: bool) -> Vec<Subpath> {
    let mut subpaths: Vec<Subpath> = Vec::new();
    let mut current: Option<Subpath> = None;
    let mut last = (0.0, 0.0);
    for segment in segments {
        match segment {
            Segment::Move(point) => {
                if let Some(subpath) = current.take() {
                    subpaths.push(subpath);
                }
                current = Some(Subpath {
                    segments: vec![*segment],
                    closed: false,
                });
                last = *point;
            }
            Segment::Close => {
                if let Some(subpath) = &mut current {
                    subpath.closed = true;
                    subpath.segments.push(*segment);
                }
                // Drawing after h starts from the subpath's start point.
                if let Some(subpath) = current.take() {
                    let start = match subpath.segments.first() {
                        Some(Segment::Move(point)) => *point,
                        _ => last,
                    };
                    subpaths.push(subpath);
                    current = Some(Subpath {
                        segments: vec![Segment::Move(start)],
                        closed: false,
                    });
                    last = start;
                }
            }
            Segment::Line(point) | Segment::Curve(_, _, point) => {
                let subpath = current.get_or_insert_with(|| Subpath {
                    segments: vec![Segment::Move(last)],
                    closed: false,
                });
                subpath.segments.push(*segment);
                last = *point;
            }
        }
    }
    if let Some(subpath) = current {
        subpaths.push(subpath);
    }
    subpaths.retain(|subpath| subpath.segments.len() > 1);
    if close_last && let Some(subpath) = subpaths.last_mut() {
        subpath.closed = true;
    }
    subpaths
}

/// The subpath's points in page space, curves followed closely.
fn flatten(subpath: &Subpath, ctm: Matrix) -> Vec<Point> {
    let mut points: Vec<Point> = Vec::new();
    for segment in &subpath.segments {
        match *segment {
            Segment::Move(point) | Segment::Line(point) => points.push(apply(ctm, point)),
            Segment::Curve(first, second, end) => {
                let start = points.last().copied().unwrap_or_else(|| apply(ctm, first));
                points.extend(flatten_cubic(
                    start,
                    apply(ctm, first),
                    apply(ctm, second),
                    apply(ctm, end),
                    CURVE_TOLERANCE,
                ));
            }
            Segment::Close => {}
        }
    }
    points.dedup();
    points
}

/// The subpath as it was, in its own user space numbers.
fn write_subpath(out: &mut Vec<u8>, subpath: &Subpath, stroke: bool) {
    let number = |value: f64| format_number(value);
    for segment in &subpath.segments {
        match *segment {
            Segment::Move((x, y)) => {
                out.extend_from_slice(format!("{} {} m\n", number(x), number(y)).as_bytes())
            }
            Segment::Line((x, y)) => {
                out.extend_from_slice(format!("{} {} l\n", number(x), number(y)).as_bytes())
            }
            Segment::Curve((x1, y1), (x2, y2), (x3, y3)) => out.extend_from_slice(
                format!(
                    "{} {} {} {} {} {} c\n",
                    number(x1),
                    number(y1),
                    number(x2),
                    number(y2),
                    number(x3),
                    number(y3)
                )
                .as_bytes(),
            ),
            Segment::Close => out.extend_from_slice(b"h\n"),
        }
    }
    if stroke && subpath.closed && !matches!(subpath.segments.last(), Some(Segment::Close)) {
        out.extend_from_slice(b"h\n");
    }
}

/// Page space points written back in user space.
fn write_points(out: &mut Vec<u8>, points: &[Point], inverse: Matrix, close: bool) {
    for (index, point) in points.iter().enumerate() {
        let (x, y) = apply(inverse, *point);
        let operator = if index == 0 { "m" } else { "l" };
        out.extend_from_slice(
            format!("{} {} {operator}\n", format_number(x), format_number(y)).as_bytes(),
        );
    }
    if close {
        out.extend_from_slice(b"h\n");
    }
}

fn xobject_id(document: &Document, resources: &Dictionary, name: &[u8]) -> Option<ObjectId> {
    let xobjects = match resources.get(b"XObject").ok()? {
        Object::Reference(id) => document.get_dictionary(*id).ok()?,
        Object::Dictionary(xobjects) => xobjects,
        _ => return None,
    };
    xobjects.get(name).ok()?.as_reference().ok()
}

/// One of the resources' dictionaries, such as /XObject, as one of their
/// own, copied out of a shared object if need be.
pub(super) fn own_subdictionary<'r>(
    document: &Document,
    resources: &'r mut Dictionary,
    key: &[u8],
) -> &'r mut Dictionary {
    let own = match resources.get(key) {
        Ok(Object::Reference(id)) => document.get_dictionary(*id).cloned().unwrap_or_default(),
        Ok(Object::Dictionary(found)) => found.clone(),
        _ => Dictionary::new(),
    };
    resources.set(key.to_vec(), own);
    resources
        .get_mut(key)
        .and_then(Object::as_dict_mut)
        .expect("just set")
}

/// How many graphics states `content` saves and never restores.
pub(super) fn open_states(content: &[u8]) -> usize {
    let Ok(operations) = lexer::operations(content) else {
        return 0;
    };
    operations.iter().fold(0, |depth: usize, operation| {
        match operation.operator.as_slice() {
            b"q" => depth + 1,
            b"Q" => depth.saturating_sub(1),
            _ => depth,
        }
    })
}
