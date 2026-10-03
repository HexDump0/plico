//! Form filling: values put into a PDF's own fields.
//!
//! A field's value is only half of it. Readers draw a widget's stored
//! appearance, not the value, unless the form asks them to redraw its fields,
//! so each widget of a changed text or choice field gets a new appearance,
//! drawn in the standard font closest to the one the field asks for. Check
//! boxes and radio buttons carry a look for each state already; only the state
//! shown changes, and one is drawn only for a widget that has none.
//!
//! Pages are not touched and nothing is renumbered: values and appearances
//! live on field and widget objects. With `flatten`, the filled form is then
//! flattened like Flatten's form fields scope, after every text and choice
//! field whose stored look may not show its value has been redrawn.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, decode_text_string, dictionary};

use crate::annotate::{ellipse, wrap};
use crate::documents::{load_document, text_string};
use crate::flatten::{
    FlattenScope, Flattened, field_type, flatten_pdf_bytes, name_of, resolve_dict,
};
use crate::stamps::{
    FontFamily, LEADING, base_font, cap_height, finish, number, rectangle, resolve, text_width,
    win_ansi,
};

pub enum FieldValue<'a> {
    /// A text field's text, or what is typed into a combo box that allows it.
    Text(&'a str),
    /// The export values chosen in a list or combo box; none clears it.
    Choices(Vec<&'a str>),
    /// Whether the widget named turns on, which ticks or clears a check box
    /// and chooses a radio button. Its own appearance states say what its
    /// value is, so pdf.js's export values never need mapping back.
    Button(bool),
}

pub struct FieldFill<'a> {
    /// Any widget of the field, as pdf.js names it: "41R" is (41, 0).
    pub widget: ObjectId,
    pub value: FieldValue<'a>,
}

// Field flags (ISO 32000-1, 12.7.3.1 and 12.7.4).
const READ_ONLY: i64 = 1;
const MULTILINE: i64 = 1 << 12;
const PASSWORD: i64 = 1 << 13;
const RADIO: i64 = 1 << 15;
const PUSH_BUTTON: i64 = 1 << 16;
const COMBO: i64 = 1 << 17;
const EDIT: i64 = 1 << 18;
const MULTI_SELECT: i64 = 1 << 21;
const COMB: i64 = 1 << 24;

const MAX_NESTING: usize = 64;

/// Space between a field's border and its text, in points, as Acrobat leaves.
const PADDING: f32 = 2.0;

/// The smallest size an automatically sized field shrinks text to; past it,
/// text is cut off by the field's edge rather than becoming unreadable.
const SMALLEST: f32 = 4.0;

/// The size an automatically sized multiline or list field starts from.
const LARGEST_BLOCK: f32 = 12.0;

/// Line height over font size for fitting single lines to a field's height,
/// the factor pdf.js uses so its preview and this output agree.
const LINE_FIT: f32 = 1.35;

/// Acrobat's background for chosen options in a list box.
const SELECTED: &str = "0.6 0.75 0.85 rg";

pub fn fill_form_bytes(
    input: &[u8],
    password: &str,
    fills: &[FieldFill<'_>],
    flatten: bool,
) -> Result<Flattened, String> {
    if fills.is_empty() {
        return Err("Fill in at least one field.".into());
    }
    let mut document = load_document(input, 1, password)?;
    let catalog_id = document
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|_| "This PDF has no catalog.".to_string())?;
    let form = document
        .get_dictionary(catalog_id)
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|value| resolve_dict(&document, value))
        .cloned()
        // Widgets with no form around them still work in readers
        // (pdf.js corpus: issue12963.pdf).
        .unwrap_or_default();
    let defaults = Defaults::of(&document, &form);

    // Several widgets of one field arrive as one value for it. Buttons arrive
    // as every widget of a name, the chosen one on and the rest off, since
    // separate fields can share a name (pdf.js corpus: issue15096.pdf, two
    // radio buttons that are each a field); within one field, on wins.
    let mut values = BTreeMap::<ObjectId, (ObjectId, &FieldValue<'_>)>::new();
    for fill in fills {
        let is_widget = document
            .get_dictionary(fill.widget)
            .is_ok_and(|widget| name_of(widget, b"Subtype") == Some(b"Widget"));
        if !is_widget {
            return Err("Some of the fields to fill are no longer in this PDF.".into());
        }
        let field = field_of(&document, fill.widget);
        let chosen = matches!(values.get(&field), Some((_, FieldValue::Button(true))));
        if !(chosen && matches!(fill.value, FieldValue::Button(false))) {
            values.insert(field, (fill.widget, &fill.value));
        }
    }

    let mut fonts = Fonts::default();
    let mut drawn = BTreeSet::new();
    for (&field, &(widget, value)) in &values {
        let redraw = set_value(&mut document, &defaults, &mut fonts, field, widget, value)?;
        clear_kid_values(&mut document, field);
        if redraw {
            draw_field(&mut document, &defaults, &mut fonts, field)?;
            drawn.insert(field);
        }
    }

    // Flattening draws stored appearances only, so every text or choice field
    // whose stored look may not show its value is drawn first: all of them
    // when the form asks readers to redraw its fields.
    let mut stale = false;
    if flatten {
        let mut complete = true;
        for field in terminal_fields(&document, &form) {
            if drawn.contains(&field) || !needs_drawing(&document, &defaults, field) {
                continue;
            }
            if draw_field(&mut document, &defaults, &mut fonts, field).is_err() {
                complete = false;
            }
        }
        stale = defaults.redraws && !complete;
    }

    let form_id = document
        .get_dictionary(catalog_id)
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|form| form.as_reference().ok());
    let form_holder = match form_id {
        Some(id) => document.get_dictionary_mut(id).ok(),
        None => document
            .get_dictionary_mut(catalog_id)
            .ok()
            .and_then(|catalog| catalog.get_mut(b"AcroForm").ok())
            .and_then(|form| form.as_dict_mut().ok()),
    };
    if let Some(form) = form_holder {
        // XFA describes the same form again and readers that know it show
        // its values instead of the fields'.
        form.remove(b"XFA");
        if flatten && !stale {
            form.remove(b"NeedAppearances");
        }
    }

    let bytes = finish(document, 1.0)?;
    if flatten {
        flatten_pdf_bytes(&bytes, "", FlattenScope::FormFields)
    } else {
        Ok(Flattened { bytes, kept: 0 })
    }
}

/// Sets `field`'s value, and says whether its widgets need drawing again.
fn set_value(
    document: &mut Document,
    defaults: &Defaults,
    fonts: &mut Fonts,
    field: ObjectId,
    widget: ObjectId,
    value: &FieldValue<'_>,
) -> Result<bool, String> {
    let node = document
        .get_dictionary(field)
        .map_err(|_| "Some of the fields to fill are no longer in this PDF.".to_string())?
        .clone();
    let name = field_name(document, &node);
    let kind = field_type(document, &node);
    let flags = flags_of(document, &node);
    if flags & READ_ONLY != 0 {
        return Err(format!("“{name}” is read-only."));
    }
    let set = |document: &mut Document, update: &dyn Fn(&mut Dictionary)| {
        document
            .get_dictionary_mut(field)
            .map(update)
            .map_err(|error| format!("A form field could not be read: {error}"))
    };
    match (kind.as_deref(), value) {
        (Some(b"Tx"), FieldValue::Text(text)) => {
            if flags & PASSWORD != 0 {
                return Err(format!(
                    "“{name}” is a password field, which is never saved in a PDF."
                ));
            }
            encode(text, &name)?;
            if let Some(limit) = max_length(document, &node)
                && text.chars().count() > limit
            {
                return Err(format!("“{name}” takes at most {limit} characters."));
            }
            set(document, &|node| {
                node.set("V", text_string(text));
                // A rich text value is shown in place of the plain one.
                node.remove(b"RV");
            })?;
            Ok(true)
        }
        (Some(b"Ch"), FieldValue::Text(text)) if flags & COMBO != 0 && flags & EDIT != 0 => {
            encode(text, &name)?;
            set(document, &|node| {
                node.set("V", text_string(text));
                node.remove(b"I");
            })?;
            Ok(true)
        }
        (Some(b"Ch"), FieldValue::Choices(chosen)) => {
            if chosen.len() > 1 && flags & MULTI_SELECT == 0 {
                return Err(format!("Choose one option in “{name}”."));
            }
            let options = options_of(document, &node);
            // A combo box that takes typed text holds anything, listed or not
            // (pdf.js corpus: issue19083.pdf, whose only choice is its value).
            let typed = flags & COMBO != 0 && flags & EDIT != 0;
            let mut indices = Vec::new();
            for choice in chosen {
                match options.iter().position(|(export, _)| export == choice) {
                    Some(index) => indices.push(index),
                    None if typed => {
                        encode(choice, &name)?;
                    }
                    None => return Err(format!("“{name}” has no option “{choice}”.")),
                }
            }
            indices.sort_unstable();
            indices.dedup();
            let list = flags & COMBO == 0;
            set(document, &|node| {
                match chosen[..] {
                    [] => {
                        node.remove(b"V");
                    }
                    [choice] => node.set("V", text_string(choice)),
                    _ => node.set(
                        "V",
                        chosen
                            .iter()
                            .map(|choice| text_string(choice))
                            .collect::<Vec<_>>(),
                    ),
                }
                // Readers take a list's selection from /I before /V, so a
                // stale one would undo the change.
                if list && !indices.is_empty() {
                    node.set(
                        "I",
                        indices
                            .iter()
                            .map(|&index| Object::Integer(index as i64))
                            .collect::<Vec<_>>(),
                    );
                } else {
                    node.remove(b"I");
                }
            })?;
            Ok(true)
        }
        (Some(b"Btn"), FieldValue::Button(on)) => {
            if flags & PUSH_BUTTON != 0 {
                return Err(format!("“{name}” is a button, not a field to fill."));
            }
            set_state(
                document,
                defaults,
                fonts,
                field,
                widget,
                *on,
                flags & RADIO != 0,
            )?;
            Ok(false)
        }
        (Some(b"Sig"), _) => Err(format!(
            "“{name}” is for a digital signature, which this tool does not add."
        )),
        _ => Err(format!("“{name}” cannot take that kind of value.")),
    }
}

/// Turns `chosen` on, or off, and with it every widget of the field sharing
/// its state while the rest turn off. One value drives a group of radio
/// buttons, or check boxes sharing a name, this way (12.7.4.2.3).
fn set_state(
    document: &mut Document,
    defaults: &Defaults,
    fonts: &mut Fonts,
    field: ObjectId,
    chosen: ObjectId,
    on: bool,
    radio: bool,
) -> Result<(), String> {
    let state = on.then(|| {
        button_states(document, chosen)
            .and_then(|states| states.into_iter().find(|state| state != b"Off"))
            .unwrap_or_else(|| b"Yes".to_vec())
    });
    for widget in widgets_of(document, field) {
        let shows = match &state {
            Some(state) => match button_states(document, widget) {
                Some(states) => states.contains(state),
                // A widget with no look for any state gets one, or it would
                // show nothing either way.
                None => {
                    widget == chosen && draw_button(document, defaults, fonts, widget, state, radio)
                }
            },
            None => false,
        };
        let shown = match (&state, shows) {
            (Some(state), true) => state.clone(),
            _ => b"Off".to_vec(),
        };
        if let Ok(widget) = document.get_dictionary_mut(widget) {
            widget.set("AS", Object::Name(shown));
        }
    }
    let value = Object::Name(state.unwrap_or_else(|| b"Off".to_vec()));
    document
        .get_dictionary_mut(field)
        .map(|node| node.set("V", value))
        .map_err(|error| format!("A form field could not be read: {error}"))
}

/// Draws a new normal appearance for every widget of a text or choice field,
/// showing its value. Down and rollover appearances go, since they show the
/// old one.
fn draw_field(
    document: &mut Document,
    defaults: &Defaults,
    fonts: &mut Fonts,
    field: ObjectId,
) -> Result<(), String> {
    let node = document
        .get_dictionary(field)
        .map_err(|error| format!("A form field could not be read: {error}"))?
        .clone();
    let name = field_name(document, &node);
    let flags = flags_of(document, &node);
    let shown = match field_type(document, &node).as_deref() {
        Some(b"Tx") => {
            let text = value_of(document, &node)
                .into_iter()
                .next()
                .unwrap_or_default();
            if flags & PASSWORD != 0 {
                Shown::Line("*".repeat(text.chars().count()))
            } else if flags & MULTILINE != 0 {
                Shown::Block(text)
            } else if flags & COMB != 0
                && let Some(cells) = max_length(document, &node).filter(|&cells| cells > 0)
            {
                Shown::Comb(text, cells)
            } else {
                Shown::Line(text)
            }
        }
        Some(b"Ch") => {
            let options = options_of(document, &node);
            let chosen = value_of(document, &node);
            if flags & COMBO != 0 {
                // A combo box shows what was chosen as typed, or by its
                // display name.
                let first = chosen.into_iter().next().unwrap_or_default();
                let display = options
                    .iter()
                    .find(|(export, _)| *export == first)
                    .map_or(first, |(_, display)| display.clone());
                Shown::Line(display)
            } else {
                let selected = options
                    .iter()
                    .enumerate()
                    .filter(|(_, (export, _))| chosen.contains(export))
                    .map(|(index, _)| index)
                    .collect();
                let top = node
                    .get(b"TI")
                    .ok()
                    .and_then(|value| resolve(document, value))
                    .and_then(|value| value.as_i64().ok())
                    .and_then(|top| usize::try_from(top).ok())
                    .unwrap_or(0);
                Shown::List {
                    options: options.into_iter().map(|(_, display)| display).collect(),
                    selected,
                    top,
                }
            }
        }
        _ => return Ok(()),
    };

    for widget in widgets_of(document, field) {
        let widget_node = document
            .get_dictionary(widget)
            .map_err(|error| format!("A form field could not be read: {error}"))?
            .clone();
        let Some(frame) = WidgetBox::of(document, &widget_node) else {
            continue;
        };
        let look = Look::of(document, defaults, &widget_node);
        let alignment = inherited(document, &widget_node, b"Q")
            .and_then(|value| value.as_i64().ok())
            .unwrap_or(defaults.alignment);
        let mut content = frame.decorations(false);
        content.push_str(&shown.draw(&frame, &look, alignment, &name)?);
        let font = fonts.text(document, look.family, look.bold);
        let appearance = frame.stream(
            content,
            dictionary! { "Font" => dictionary! { "F0" => font } },
        );
        let appearance = document.add_object(appearance);
        if let Ok(widget) = document.get_dictionary_mut(widget) {
            widget.set("AP", dictionary! { "N" => appearance });
        }
    }
    Ok(())
}

/// Whether a flattened `field` would draw something other than its value: a
/// text or choice field in a form that asks readers to redraw its fields, or
/// one with a value and a widget with nothing stored to show it.
fn needs_drawing(document: &Document, defaults: &Defaults, field: ObjectId) -> bool {
    let Ok(node) = document.get_dictionary(field) else {
        return false;
    };
    if !matches!(field_type(document, node).as_deref(), Some(b"Tx" | b"Ch")) {
        return false;
    }
    if defaults.redraws {
        return true;
    }
    !value_of(document, node).iter().all(String::is_empty)
        && widgets_of(document, field).iter().any(|&widget| {
            document
                .get_dictionary(widget)
                .ok()
                .and_then(|widget| widget.get(b"AP").ok())
                .and_then(|value| resolve_dict(document, value))
                .is_none_or(|appearance| !appearance.has(b"N"))
        })
}

/// What a text or choice widget shows.
enum Shown {
    Line(String),
    Block(String),
    /// Text spread one character to a cell, across this many cells.
    Comb(String, usize),
    List {
        options: Vec<String>,
        selected: Vec<usize>,
        top: usize,
    },
}

impl Shown {
    fn draw(
        &self,
        frame: &WidgetBox,
        look: &Look,
        alignment: i64,
        name: &str,
    ) -> Result<String, String> {
        let (width, height) = (frame.width, frame.height);
        let edge = frame.border.as_ref().map_or(0.0, |border| border.width);
        let inset = edge + PADDING;
        let room = (width - 2.0 * inset).max(0.0);
        let (family, bold) = (look.family, look.bold);
        let measure =
            |codes: &[u8], size: f32| text_width(family, bold, codes) as f32 / 1000.0 * size;
        let cap = cap_height(family, bold);
        let across = |line_width: f32| match alignment {
            1 => (width - line_width) / 2.0,
            2 => width - inset - line_width,
            _ => inset,
        };
        // Lines of codes, each at its x and baseline.
        let mut runs = Vec::<(f32, f32, Vec<u8>)>::new();
        let mut highlights = Vec::<[f32; 4]>::new();
        let fit_height = ((height - 2.0 * edge - 2.0) / LINE_FIT).max(SMALLEST);
        let size = match self {
            Shown::Line(text) => {
                let codes = encode(&text.replace(['\r', '\n'], " "), name)?;
                let size = if look.size > 0.0 {
                    look.size
                } else {
                    let wide = measure(&codes, 1.0);
                    let fit_width = if wide > 0.0 {
                        room / wide
                    } else {
                        f32::INFINITY
                    };
                    fit_height.min(fit_width).max(SMALLEST)
                };
                let y = (height - cap * size) / 2.0;
                runs.push((across(measure(&codes, size)), y, codes));
                size
            }
            Shown::Comb(text, cells) => {
                let codes = encode(text, name)?;
                let cell = width / *cells as f32;
                let size = if look.size > 0.0 {
                    look.size
                } else {
                    let widest = codes
                        .iter()
                        .map(|&code| measure(&[code], 1.0))
                        .fold(0.0, f32::max);
                    let fit_width = if widest > 0.0 {
                        cell / widest
                    } else {
                        f32::INFINITY
                    };
                    fit_height.min(fit_width).max(SMALLEST)
                };
                let y = (height - cap * size) / 2.0;
                for (index, &code) in codes.iter().take(*cells).enumerate() {
                    let x = index as f32 * cell + (cell - measure(&[code], size)) / 2.0;
                    runs.push((x, y, vec![code]));
                }
                size
            }
            Shown::Block(text) => {
                encode(text, name)?;
                let top = height - edge - PADDING;
                let lines_at = |size: f32| wrap(text, family, bold, size, room);
                let size = if look.size > 0.0 {
                    look.size
                } else {
                    // Shrinks in half points until every line fits.
                    let mut size = LARGEST_BLOCK;
                    while size > SMALLEST {
                        let lines = lines_at(size).len() as f32;
                        if top - edge - size * (cap + LEADING * (lines - 1.0)) >= 0.0 {
                            break;
                        }
                        size -= 0.5;
                    }
                    size
                };
                for (index, line) in lines_at(size).into_iter().enumerate() {
                    let y = top - cap * size - index as f32 * LEADING * size;
                    if y < edge - size {
                        break;
                    }
                    runs.push((across(measure(&line, size)), y, line));
                }
                size
            }
            Shown::List {
                options,
                selected,
                top,
            } => {
                let size = if look.size > 0.0 {
                    look.size
                } else {
                    LARGEST_BLOCK
                };
                let row = size * LEADING;
                let rows = ((height - 2.0 * edge) / row).floor().max(1.0) as usize;
                // The first chosen option stays in view.
                let first = match selected.first() {
                    Some(&chosen) if chosen < *top || chosen >= top + rows => chosen,
                    _ => (*top).min(options.len().saturating_sub(1)),
                };
                for (shown, index) in (first..options.len()).enumerate() {
                    let row_top = height - edge - shown as f32 * row;
                    if row_top <= edge {
                        break;
                    }
                    if selected.contains(&index) {
                        highlights.push([edge, row_top - row, width - 2.0 * edge, row]);
                    }
                    let codes = encode(&options[index], name)?;
                    let y = row_top - row + (row - cap * size) / 2.0;
                    runs.push((across(measure(&codes, size)), y, codes));
                }
                size
            }
        };

        let mut content = String::from("/Tx BMC\nq\n");
        content.push_str(&format!(
            "{} {} {} {} re\nW\nn\n",
            number(edge),
            number(edge),
            number((width - 2.0 * edge).max(0.0)),
            number((height - 2.0 * edge).max(0.0))
        ));
        for [x, y, w, h] in highlights {
            content.push_str(&format!(
                "{SELECTED}\n{} {} {} {} re\nf\n",
                number(x),
                number(y),
                number(w),
                number(h)
            ));
        }
        content.push_str(&format!("BT\n/F0 {} Tf\n{}\n", number(size), look.colour));
        for (x, y, codes) in runs {
            content.push_str(&format!(
                "1 0 0 1 {} {} Tm\n<{}> Tj\n",
                number(x),
                number(y),
                codes
                    .iter()
                    .map(|code| format!("{code:02X}"))
                    .collect::<String>()
            ));
        }
        content.push_str("ET\nQ\nEMC\n");
        Ok(content)
    }
}

/// Draws on and off appearances for a check box or radio button that has
/// none, from its /MK caption: a check, or a dot for a radio button.
fn draw_button(
    document: &mut Document,
    defaults: &Defaults,
    fonts: &mut Fonts,
    widget: ObjectId,
    state: &[u8],
    radio: bool,
) -> bool {
    let Ok(node) = document.get_dictionary(widget).cloned() else {
        return false;
    };
    let Some(frame) = WidgetBox::of(document, &node) else {
        return false;
    };
    let look = Look::of(document, defaults, &node);
    // ZapfDingbats: 4 is a check, l a filled circle.
    let caption = node
        .get(b"MK")
        .ok()
        .and_then(|value| resolve_dict(document, value))
        .and_then(|characteristics| characteristics.get(b"CA").ok())
        .and_then(|value| resolve(document, value))
        .and_then(|value| value.as_str().ok())
        .and_then(|caption| caption.first().copied())
        .unwrap_or(if radio { b'l' } else { b'4' });
    let size = if look.size > 0.0 {
        look.size
    } else {
        frame.width.min(frame.height) * 0.8
    };
    // Dingbats are about 0.8 em wide and 0.7 em tall.
    let (x, y) = (
        (frame.width - 0.8 * size) / 2.0,
        (frame.height - 0.7 * size) / 2.0,
    );
    let decorations = frame.decorations(radio);
    let on = format!(
        "{decorations}q\nBT\n/F0 {} Tf\n{}\n{} {} Td\n<{caption:02X}> Tj\nET\nQ\n",
        number(size),
        look.colour,
        number(x),
        number(y)
    );
    let font = fonts.dingbats(document);
    let on = frame.stream(on, dictionary! { "Font" => dictionary! { "F0" => font } });
    let off = frame.stream(decorations, dictionary! {});
    let (on, off) = (document.add_object(on), document.add_object(off));
    let mut states = Dictionary::new();
    states.set(state.to_vec(), on);
    states.set("Off", off);
    if let Ok(widget) = document.get_dictionary_mut(widget) {
        widget.set("AP", dictionary! { "N" => states });
    }
    true
}

/// A widget's appearance box, upright as its text reads, with the matrix
/// that turns it onto the widget's rectangle and the colours of its frame.
struct WidgetBox {
    width: f32,
    height: f32,
    matrix: Option<[f32; 6]>,
    background: Option<String>,
    border: Option<Border>,
}

struct Border {
    colour: String,
    width: f32,
    dash: Option<String>,
    underline: bool,
}

impl WidgetBox {
    fn of(document: &Document, widget: &Dictionary) -> Option<WidgetBox> {
        let [x0, y0, x1, y1] = rectangle(document, widget.get(b"Rect").ok()?)?;
        let (wide, tall) = (x1 - x0, y1 - y0);
        let characteristics = widget
            .get(b"MK")
            .ok()
            .and_then(|value| resolve_dict(document, value));
        let entry = |key: &[u8]| {
            characteristics
                .and_then(|characteristics| characteristics.get(key).ok())
                .and_then(|value| resolve(document, value))
        };
        let rotation = entry(b"R")
            .and_then(|value| value.as_float().ok())
            .filter(|rotation| rotation.is_finite())
            .map_or(0, |rotation| {
                ((rotation / 90.0).round() as i64 * 90).rem_euclid(360)
            });
        // /R turns the content counterclockwise within the rectangle
        // (12.5.6.19), the way pdf.js lays it out.
        let (width, height, matrix) = match rotation {
            90 => (tall, wide, Some([0.0, 1.0, -1.0, 0.0, wide, 0.0])),
            180 => (wide, tall, Some([-1.0, 0.0, 0.0, -1.0, wide, tall])),
            270 => (tall, wide, Some([0.0, -1.0, 1.0, 0.0, 0.0, tall])),
            _ => (wide, tall, None),
        };
        let style = widget
            .get(b"BS")
            .ok()
            .and_then(|value| resolve_dict(document, value));
        let border_width = match style {
            Some(style) => style
                .get(b"W")
                .ok()
                .and_then(|value| resolve(document, value))
                .and_then(|value| value.as_float().ok()),
            // The older /Border array: corner radii, then width.
            None => widget
                .get(b"Border")
                .ok()
                .and_then(|value| resolve(document, value))
                .and_then(|value| value.as_array().ok())
                .and_then(|border| border.get(2))
                .and_then(|value| resolve(document, value))
                .and_then(|value| value.as_float().ok()),
        }
        .filter(|width| width.is_finite() && *width >= 0.0)
        .unwrap_or(1.0)
        .min(width.min(height) / 4.0);
        let kind = style.and_then(|style| name_of(style, b"S"));
        let dash = (kind == Some(b"D")).then(|| {
            let pattern = style
                .and_then(|style| style.get(b"D").ok())
                .and_then(|value| resolve(document, value))
                .and_then(|value| value.as_array().ok())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| resolve(document, item)?.as_float().ok())
                        .filter(|value| value.is_finite() && *value >= 0.0)
                        .map(number)
                        .collect::<Vec<_>>()
                })
                .filter(|pattern| !pattern.is_empty())
                .unwrap_or_else(|| vec!["3".into()]);
            format!("[{}] 0 d", pattern.join(" "))
        });
        let border = colour(document, entry(b"BC"), false)
            .filter(|_| border_width > 0.0)
            .map(|colour| Border {
                colour,
                width: border_width,
                dash,
                underline: kind == Some(b"U"),
            });
        Some(WidgetBox {
            width,
            height,
            matrix,
            background: colour(document, entry(b"BG"), true),
            border,
        })
    }

    /// The background and border a reader would draw from /MK, round for a
    /// radio button.
    fn decorations(&self, round: bool) -> String {
        let (width, height) = (self.width, self.height);
        let shape = |inset: f32| {
            let [x0, y0, x1, y1] = [inset, inset, width - inset, height - inset];
            if round {
                ellipse(x0, y0, x1, y1)
            } else {
                format!(
                    "{} {} {} {} re\n",
                    number(x0),
                    number(y0),
                    number(x1 - x0),
                    number(y1 - y0)
                )
            }
        };
        let mut content = String::new();
        if let Some(background) = &self.background {
            content.push_str(&format!("q\n{background}\n{}f\nQ\n", shape(0.0)));
        }
        if let Some(border) = &self.border {
            content.push_str(&format!(
                "q\n{}\n{} w\n",
                border.colour,
                number(border.width)
            ));
            if let Some(dash) = &border.dash {
                content.push_str(dash);
                content.push('\n');
            }
            if border.underline {
                let y = number(border.width / 2.0);
                content.push_str(&format!("0 {y} m\n{} {y} l\nS\n", number(width)));
            } else {
                content.push_str(&shape(border.width / 2.0));
                content.push_str("S\n");
            }
            content.push_str("Q\n");
        }
        content
    }

    fn stream(&self, content: String, resources: Dictionary) -> Stream {
        let mut dictionary = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Real(self.width),
                Object::Real(self.height),
            ],
            "Resources" => resources,
        };
        if let Some(matrix) = self.matrix {
            dictionary.set(
                "Matrix",
                matrix
                    .iter()
                    .map(|&value| Object::Real(value))
                    .collect::<Vec<_>>(),
            );
        }
        Stream::new(dictionary, content.into_bytes())
    }
}

/// A colour array from /MK as a fill or stroke operation; an empty one is
/// transparent (12.5.6.19).
fn colour(document: &Document, value: Option<&Object>, fill: bool) -> Option<String> {
    let components = value?
        .as_array()
        .ok()?
        .iter()
        .map(|item| resolve(document, item)?.as_float().ok())
        .collect::<Option<Vec<_>>>()?;
    let operator = match components.len() {
        1 => "g",
        3 => "rg",
        4 => "k",
        _ => return None,
    };
    let operator = if fill {
        operator.to_owned()
    } else {
        operator.to_uppercase()
    };
    let values = components
        .into_iter()
        .map(|value| number(value.clamp(0.0, 1.0)))
        .collect::<Vec<_>>()
        .join(" ");
    Some(format!("{values} {operator}"))
}

/// The text a field's default appearance asks for, mapped onto a standard
/// font. Size 0 means fit the field.
struct Look {
    family: FontFamily,
    bold: bool,
    size: f32,
    colour: String,
}

impl Look {
    fn of(document: &Document, defaults: &Defaults, widget: &Dictionary) -> Look {
        let appearance = inherited(document, widget, b"DA")
            .and_then(|value| value.as_str().ok())
            .map(|value| String::from_utf8_lossy(value).into_owned())
            .or_else(|| defaults.appearance.clone())
            .unwrap_or_default();
        let tokens = appearance.split_whitespace().collect::<Vec<_>>();
        let numbers = |end: usize, count: usize| {
            let start = end.checked_sub(count)?;
            tokens[start..end]
                .iter()
                .map(|token| token.parse::<f32>().ok().filter(|value| value.is_finite()))
                .collect::<Option<Vec<_>>>()
        };
        let mut font = None;
        let mut size = 0.0;
        let mut colour = None;
        for (index, token) in tokens.iter().enumerate() {
            match *token {
                "Tf" if index >= 2 => {
                    font = tokens[index - 2].strip_prefix('/');
                    size = tokens[index - 1]
                        .parse::<f32>()
                        .ok()
                        .filter(|size| size.is_finite() && *size >= 0.0)
                        .unwrap_or(0.0);
                }
                "g" | "rg" | "k" => {
                    let count = match *token {
                        "g" => 1,
                        "rg" => 3,
                        _ => 4,
                    };
                    if let Some(values) = numbers(index, count) {
                        let values = values
                            .into_iter()
                            .map(|value| number(value.clamp(0.0, 1.0)))
                            .collect::<Vec<_>>();
                        colour = Some(format!("{} {token}", values.join(" ")));
                    }
                }
                _ => {}
            }
        }
        let (family, bold) = font.map_or((FontFamily::Helvetica, false), |font| {
            let base = defaults
                .fonts
                .as_ref()
                .and_then(|fonts| fonts.get(font.as_bytes()).ok())
                .and_then(|value| resolve_dict(document, value))
                .and_then(|font| name_of(font, b"BaseFont"));
            standard_family(base.unwrap_or(font.as_bytes()))
        });
        Look {
            family,
            bold,
            size,
            colour: colour.unwrap_or_else(|| "0 g".into()),
        }
    }
}

/// The standard font nearest a form font, by its name: Arial and most sans
/// faces become Helvetica, Times New Roman and other serifs Times, monospaced
/// faces Courier. Acrobat's own resource names count too.
fn standard_family(name: &[u8]) -> (FontFamily, bool) {
    let name = String::from_utf8_lossy(name).to_lowercase();
    // A subset's name starts with six capitals and a plus.
    let name = match name.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 => rest,
        _ => name.as_str(),
    };
    let has = |part: &str| name.contains(part);
    let family = if has("cour") || has("mono") || name == "cobo" {
        FontFamily::Courier
    } else if has("times")
        || name == "tiro"
        || name == "tibo"
        || (has("serif") && !has("sans"))
        || ["roman", "georgia", "garamond", "minion", "cambria", "book"]
            .iter()
            .any(|part| has(part))
    {
        FontFamily::Times
    } else {
        FontFamily::Helvetica
    };
    let bold =
        has("bold") || has("black") || has("heavy") || matches!(name, "hebo" | "tibo" | "cobo");
    (family, bold)
}

/// What the form gives every field that does not say otherwise.
struct Defaults {
    /// /DR's fonts, to learn what a default appearance's font is.
    fonts: Option<Dictionary>,
    appearance: Option<String>,
    alignment: i64,
    /// /NeedAppearances: readers are asked to redraw every field.
    redraws: bool,
}

impl Defaults {
    fn of(document: &Document, form: &Dictionary) -> Defaults {
        let fonts = form
            .get(b"DR")
            .ok()
            .and_then(|value| resolve_dict(document, value))
            .and_then(|resources| resources.get(b"Font").ok())
            .and_then(|value| resolve_dict(document, value))
            .cloned();
        let entry = |key: &[u8]| {
            form.get(key)
                .ok()
                .and_then(|value| resolve(document, value))
        };
        Defaults {
            fonts,
            appearance: entry(b"DA")
                .and_then(|value| value.as_str().ok())
                .map(|value| String::from_utf8_lossy(value).into_owned()),
            alignment: entry(b"Q")
                .and_then(|value| value.as_i64().ok())
                .unwrap_or(0),
            redraws: entry(b"NeedAppearances")
                .and_then(|value| value.as_bool().ok())
                .unwrap_or(false),
        }
    }
}

/// Font objects written so far, shared by every appearance.
#[derive(Default)]
struct Fonts {
    text: HashMap<&'static str, ObjectId>,
    dingbats: Option<ObjectId>,
}

impl Fonts {
    fn text(&mut self, document: &mut Document, family: FontFamily, bold: bool) -> ObjectId {
        let name = base_font(family, bold);
        *self.text.entry(name).or_insert_with(|| {
            document.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => name,
                "Encoding" => "WinAnsiEncoding",
            })
        })
    }

    fn dingbats(&mut self, document: &mut Document) -> ObjectId {
        *self.dingbats.get_or_insert_with(|| {
            document.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => "ZapfDingbats",
            })
        })
    }
}

/// WinAnsi codes for `text`, line breaks left out.
fn encode(text: &str, name: &str) -> Result<Vec<u8>, String> {
    text.chars()
        .filter(|character| !matches!(character, '\n' | '\r'))
        .map(|character| {
            win_ansi(character).ok_or_else(|| {
                format!("“{character}” in “{name}” cannot be drawn with the built-in PDF fonts.")
            })
        })
        .collect()
}

/// An attribute from the node or the nearest parent field that has it
/// (12.7.3.1).
fn inherited<'a>(document: &'a Document, node: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    let mut node = node;
    for _ in 0..MAX_NESTING {
        if let Ok(value) = node.get(key) {
            return resolve(document, value);
        }
        node = resolve_dict(document, node.get(b"Parent").ok()?)?;
    }
    None
}

fn flags_of(document: &Document, node: &Dictionary) -> i64 {
    inherited(document, node, b"Ff")
        .and_then(|value| value.as_i64().ok())
        .unwrap_or(0)
}

fn max_length(document: &Document, node: &Dictionary) -> Option<usize> {
    inherited(document, node, b"MaxLen")
        .and_then(|value| value.as_i64().ok())
        .and_then(|limit| usize::try_from(limit).ok())
        .filter(|&limit| limit > 0)
}

/// The value as text: one entry for a text field or a single choice, one per
/// choice otherwise.
fn value_of(document: &Document, node: &Dictionary) -> Vec<String> {
    match inherited(document, node, b"V") {
        Some(Object::Array(items)) => items
            .iter()
            .filter_map(|item| decode_text_string(resolve(document, item)?).ok())
            .collect(),
        Some(value @ Object::String(..)) => decode_text_string(value).ok().into_iter().collect(),
        Some(Object::Name(name)) => vec![String::from_utf8_lossy(name).into_owned()],
        _ => Vec::new(),
    }
}

/// A choice field's options as export value and display text (12.7.4.4).
fn options_of(document: &Document, node: &Dictionary) -> Vec<(String, String)> {
    let Some(options) = inherited(document, node, b"Opt").and_then(|value| value.as_array().ok())
    else {
        return Vec::new();
    };
    options
        .iter()
        .filter_map(|option| match resolve(document, option)? {
            Object::Array(pair) => {
                let text =
                    |index: usize| decode_text_string(resolve(document, pair.get(index)?)?).ok();
                let export = text(0)?;
                let display = text(1).unwrap_or_else(|| export.clone());
                Some((export, display))
            }
            value => {
                let text = decode_text_string(value).ok()?;
                Some((text.clone(), text))
            }
        })
        .collect()
}

/// Drops values a field's widgets carry of their own. The value belongs to
/// the field, but pdf.js reads a widget's first (pdf.js corpus:
/// issue15092.pdf, where one of two widgets kept showing its old total).
fn clear_kid_values(document: &mut Document, field: ObjectId) {
    for widget in widgets_of(document, field) {
        if widget == field {
            continue;
        }
        if let Ok(widget) = document.get_dictionary_mut(widget) {
            for key in [b"V".as_slice(), b"RV", b"I"] {
                widget.remove(key);
            }
        }
    }
}

/// The field a widget belongs to: itself when it has a name or nothing above
/// it, its parent otherwise (12.7.3.1).
fn field_of(document: &Document, widget: ObjectId) -> ObjectId {
    let Ok(node) = document.get_dictionary(widget) else {
        return widget;
    };
    if node.has(b"T") {
        return widget;
    }
    node.get(b"Parent")
        .and_then(Object::as_reference)
        .ok()
        .filter(|parent| document.get_dictionary(*parent).is_ok())
        .unwrap_or(widget)
}

fn widgets_of(document: &Document, field: ObjectId) -> Vec<ObjectId> {
    let Ok(node) = document.get_dictionary(field) else {
        return Vec::new();
    };
    let Some(kids) = node
        .get(b"Kids")
        .ok()
        .and_then(|value| resolve(document, value))
        .and_then(|value| value.as_array().ok())
    else {
        return vec![field];
    };
    kids.iter()
        .filter_map(|kid| kid.as_reference().ok())
        .filter(|&kid| {
            document.get_dictionary(kid).is_ok_and(|kid| {
                name_of(kid, b"Subtype") == Some(b"Widget") || !(kid.has(b"T") || kid.has(b"Kids"))
            })
        })
        .collect()
}

/// The named states of a button widget's normal appearance, or `None` when
/// it has none to choose from.
fn button_states(document: &Document, widget: ObjectId) -> Option<Vec<Vec<u8>>> {
    let states = document
        .get_dictionary(widget)
        .ok()?
        .get(b"AP")
        .ok()
        .and_then(|value| resolve_dict(document, value))?
        .get(b"N")
        .ok()
        .and_then(|value| resolve_dict(document, value))?;
    Some(states.iter().map(|(name, _)| name.clone()).collect())
}

/// Fields holding a value, found from the form's field tree.
fn terminal_fields(document: &Document, form: &Dictionary) -> Vec<ObjectId> {
    let mut found = Vec::new();
    let mut seen = BTreeSet::new();
    let mut nodes = form
        .get(b"Fields")
        .ok()
        .and_then(|value| resolve(document, value))
        .and_then(|value| value.as_array().ok())
        .map(|fields| {
            fields
                .iter()
                .filter_map(|field| Some((field.as_reference().ok()?, 0)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    while let Some((id, depth)) = nodes.pop() {
        if depth > MAX_NESTING || !seen.insert(id) {
            continue;
        }
        let Ok(node) = document.get_dictionary(id) else {
            continue;
        };
        let children = node
            .get(b"Kids")
            .ok()
            .and_then(|value| resolve(document, value))
            .and_then(|value| value.as_array().ok())
            .map(|kids| {
                kids.iter()
                    .filter_map(|kid| kid.as_reference().ok())
                    .filter(|&kid| {
                        document
                            .get_dictionary(kid)
                            .is_ok_and(|kid| kid.has(b"T") || kid.has(b"Kids"))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if children.is_empty() {
            found.push(id);
        } else {
            nodes.extend(children.into_iter().map(|kid| (kid, depth + 1)));
        }
    }
    found
}

/// What a person would call the field: its tooltip, or its full name.
fn field_name(document: &Document, node: &Dictionary) -> String {
    if let Some(tooltip) = node
        .get(b"TU")
        .ok()
        .and_then(|value| resolve(document, value))
        .and_then(|value| decode_text_string(value).ok())
        .filter(|tooltip| !tooltip.trim().is_empty())
    {
        return tooltip.trim().to_owned();
    }
    let mut parts = Vec::new();
    let mut node = Some(node);
    for _ in 0..MAX_NESTING {
        let Some(current) = node else { break };
        if let Some(part) = current
            .get(b"T")
            .ok()
            .and_then(|value| resolve(document, value))
            .and_then(|value| decode_text_string(value).ok())
        {
            parts.push(part);
        }
        node = current
            .get(b"Parent")
            .ok()
            .and_then(|parent| resolve_dict(document, parent));
    }
    parts.reverse();
    let name = parts.join(".");
    if name.is_empty() {
        "A field".into()
    } else {
        name
    }
}
