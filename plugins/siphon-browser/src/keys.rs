//! Keys as the DOM names them — `Enter`, `ArrowDown`, `a` — as the browser's input takes them: the code of the key
//! pressed, its Windows key code, and the text it types, where it types any.

/// A key as the browser's input takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    /// Its name, as the DOM's `KeyboardEvent.key` has it.
    pub name: String,
    pub code: String,
    pub windows_code: u32,
    pub text: Option<String>,
}

/// The named keys: the DOM's name, its code, and its Windows key code.
const NAMED: [(&str, &str, u32); 30] = [
    ("Enter", "Enter", 13),
    ("Tab", "Tab", 9),
    ("Escape", "Escape", 27),
    ("Backspace", "Backspace", 8),
    ("Delete", "Delete", 46),
    ("Space", "Space", 32),
    ("ArrowUp", "ArrowUp", 38),
    ("ArrowDown", "ArrowDown", 40),
    ("ArrowLeft", "ArrowLeft", 37),
    ("ArrowRight", "ArrowRight", 39),
    ("Home", "Home", 36),
    ("End", "End", 35),
    ("PageUp", "PageUp", 33),
    ("PageDown", "PageDown", 34),
    ("Insert", "Insert", 45),
    ("F1", "F1", 112),
    ("F2", "F2", 113),
    ("F3", "F3", 114),
    ("F4", "F4", 115),
    ("F5", "F5", 116),
    ("F6", "F6", 117),
    ("F7", "F7", 118),
    ("F8", "F8", 119),
    ("F9", "F9", 120),
    ("F10", "F10", 121),
    ("F11", "F11", 122),
    ("F12", "F12", 123),
    ("Shift", "ShiftLeft", 16),
    ("Control", "ControlLeft", 17),
    ("Alt", "AltLeft", 18),
];

/// The key the DOM names `name`; a single character as itself; none for a name no key has.
#[must_use]
pub fn key(name: &str) -> Option<Key> {
    if let Some((key, code, key_code)) = NAMED.iter().find(|(key, _, _)| key.eq_ignore_ascii_case(name)) {
        let text = match *key {
            "Enter" => Some("\r".to_owned()),
            "Tab" => Some("\t".to_owned()),
            "Space" => Some(" ".to_owned()),
            _ => None,
        };
        let key = if *key == "Space" { " " } else { key };
        return Some(Key {
            name: key.to_owned(),
            code: (*code).to_owned(),
            windows_code: *key_code,
            text,
        });
    }
    if name == "Meta" {
        return Some(Key {
            name: "Meta".into(),
            code: "MetaLeft".into(),
            windows_code: 91,
            text: None,
        });
    }
    let mut chars = name.chars();
    let (Some(char), None) = (chars.next(), chars.next()) else {
        return None;
    };
    let upper = char.to_ascii_uppercase();
    let code = match char {
        'a'..='z' | 'A'..='Z' => format!("Key{upper}"),
        '0'..='9' => format!("Digit{char}"),
        _ => String::new(),
    };
    let key_code = if char.is_ascii_alphanumeric() { u32::from(upper) } else { 0 };
    Some(Key {
        name: char.to_string(),
        code,
        windows_code: key_code,
        text: Some(char.to_string()),
    })
}

/// The modifiers' bits as the browser's input takes them: Alt 1, Control 2, Meta 4, Shift 8.
#[must_use]
pub fn modifiers(names: &[String]) -> u32 {
    names
        .iter()
        .map(|name| match name.as_str() {
            "Alt" => 1,
            "Control" => 2,
            "Meta" => 4,
            "Shift" => 8,
            _ => 0,
        })
        .sum()
}
