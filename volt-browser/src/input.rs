use std::fmt::Display;

use keyboard_types::{Key, Modifiers, NamedKey};
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::Point;

/// One key, with the modifier keys held while it is pressed. A [`NamedKey`], a [`Key`] and a
/// `char` each turn into the key alone: `press(NamedKey::Enter)`, `press('a')`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keystroke {
    /// The key, by its W3C name or its character.
    pub key: Key,
    /// The modifier keys held down.
    pub modifiers: Modifiers,
}

impl Keystroke {
    /// Returns `keystroke` pressed while `modifiers` are held:
    /// `Keystroke::with(Modifiers::CONTROL, 'a')`.
    #[must_use]
    pub fn with(modifiers: Modifiers, keystroke: impl Into<Self>) -> Self {
        Self {
            modifiers,
            ..keystroke.into()
        }
    }
}

impl From<Key> for Keystroke {
    fn from(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers::empty(),
        }
    }
}

impl From<NamedKey> for Keystroke {
    fn from(key: NamedKey) -> Self {
        Self::from(Key::Named(key))
    }
}

impl From<char> for Keystroke {
    fn from(key: char) -> Self {
        Self::from(Key::Character(key.to_string()))
    }
}

impl Serialize for Keystroke {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut keystroke = serializer.serialize_struct("Keystroke", 2)?;
        keystroke.serialize_field("key", &Shown(&self.key))?;
        keystroke.serialize_field("held", &Held::from(self.modifiers))?;
        keystroke.end()
    }
}

/// A value written into JSON as the text it displays as.
struct Shown<'a, T>(&'a T);

impl<T: Display> Serialize for Shown<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self.0)
    }
}

/// The modifier keys held, written for the helper script as the four a page's events tell
/// apart: Control, Shift, Alt and Meta.
struct Held(Modifiers);

impl From<Modifiers> for Held {
    fn from(modifiers: Modifiers) -> Self {
        Self(modifiers)
    }
}

impl Serialize for Held {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut held = serializer.serialize_struct("Held", 4)?;
        held.serialize_field("ctrl", &self.0.contains(Modifiers::CONTROL))?;
        held.serialize_field("shift", &self.0.contains(Modifiers::SHIFT))?;
        held.serialize_field("alt", &self.0.contains(Modifiers::ALT))?;
        held.serialize_field("meta", &self.0.contains(Modifiers::META))?;
        held.end()
    }
}

/// A mouse button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    /// The main button.
    #[default]
    Left,
    /// The wheel button.
    Middle,
    /// The button that opens the context menu.
    Right,
}

/// How to click; the default is one click of the left button in the middle of the element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Click {
    /// The button.
    pub button: Button,
    /// How many clicks in a row; two make a double click.
    pub count: u8,
    /// The modifier keys held during the click.
    pub modifiers: Modifiers,
    /// Where to click, measured from the element's top left corner; `None` for its middle.
    pub offset: Option<Point>,
}

impl Default for Click {
    fn default() -> Self {
        Self {
            button: Button::Left,
            count: 1,
            modifiers: Modifiers::empty(),
            offset: None,
        }
    }
}

impl Serialize for Click {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut click = serializer.serialize_struct("Click", 4)?;
        click.serialize_field("button", &self.button)?;
        click.serialize_field("count", &self.count)?;
        click.serialize_field("held", &Held::from(self.modifiers))?;
        click.serialize_field("offset", &self.offset)?;
        click.end()
    }
}

/// One option of a `<select>` to choose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice<'a> {
    /// The option with this `value`.
    Value(&'a str),
    /// The option shown with this text.
    Label(&'a str),
    /// The option at this place, counting from zero.
    Index(usize),
}

#[cfg(test)]
mod tests {
    use keyboard_types::{Key, Modifiers, NamedKey};
    use serde_json::json;

    use super::{Button, Choice, Click, Keystroke};
    use crate::Point;

    #[test]
    fn a_key_alone_holds_no_modifiers() {
        assert_eq!(
            serde_json::to_value(Keystroke::from(NamedKey::Enter)).unwrap(),
            json!({"key": "Enter", "held": {"ctrl": false, "shift": false, "alt": false, "meta": false}})
        );
        assert_eq!(Keystroke::from('a').key, Key::Character("a".to_owned()));
    }

    #[test]
    fn a_chord_carries_the_held_modifiers() {
        let chord = Keystroke::with(Modifiers::CONTROL | Modifiers::SHIFT, 'a');
        assert_eq!(
            serde_json::to_value(chord).unwrap(),
            json!({"key": "a", "held": {"ctrl": true, "shift": true, "alt": false, "meta": false}})
        );
    }

    #[test]
    fn the_default_click_is_one_left_click_in_the_middle() {
        assert_eq!(
            serde_json::to_value(Click::default()).unwrap(),
            json!({
                "button": "left",
                "count": 1,
                "held": {"ctrl": false, "shift": false, "alt": false, "meta": false},
                "offset": null
            })
        );
        let aimed = Click {
            button: Button::Right,
            offset: Some(Point { x: 2.0, y: 3.0 }),
            ..Click::default()
        };
        assert_eq!(
            serde_json::to_value(aimed).unwrap()["offset"],
            json!({"x": 2.0, "y": 3.0})
        );
    }

    #[test]
    fn a_choice_reaches_the_page_under_its_kind() {
        assert_eq!(
            serde_json::to_value([
                Choice::Value("de"),
                Choice::Label("German"),
                Choice::Index(2)
            ])
            .unwrap(),
            json!([{"value": "de"}, {"label": "German"}, {"index": 2}])
        );
    }
}
