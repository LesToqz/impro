//! Xterm keyboard encoding. Text composition (including AltGr) precedes this layer.
#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub control: bool,
}
impl Modifiers {
    fn parameter(self) -> u8 {
        1 + u8::from(self.shift) + 2 * u8::from(self.alt) + 4 * u8::from(self.control)
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Key {
    Text(char),
    Enter,
    Backspace,
    Tab,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    Function(u8),
    Keypad(char),
}
pub fn encode(key: Key, mods: Modifiers, app_cursor: bool, app_keypad: bool) -> Vec<u8> {
    let modifier = mods.parameter();
    let final_char = match key {
        Key::Up => Some('A'),
        Key::Down => Some('B'),
        Key::Right => Some('C'),
        Key::Left => Some('D'),
        Key::Home => Some('H'),
        Key::End => Some('F'),
        _ => None,
    };
    if let Some(c) = final_char {
        return if modifier > 1 {
            format!("\x1b[1;{modifier}{c}")
        } else {
            format!("\x1b{}{c}", if app_cursor { 'O' } else { '[' })
        }
        .into_bytes();
    }
    let function = match key {
        Key::Function(n @ 1..=4) => Some((0, (b'P' + n - 1) as char)),
        Key::Function(n @ 5..=12) => {
            Some(([15, 17, 18, 19, 20, 21, 23, 24][(n - 5) as usize], '~'))
        }
        Key::Insert => Some((2, '~')),
        Key::Delete => Some((3, '~')),
        Key::PageUp => Some((5, '~')),
        Key::PageDown => Some((6, '~')),
        _ => None,
    };
    if let Some((n, c)) = function {
        return if n == 0 {
            if modifier > 1 {
                format!("\x1b[1;{modifier}{c}")
            } else {
                format!("\x1bO{c}")
            }
        } else if modifier > 1 {
            format!("\x1b[{n};{modifier}{c}")
        } else {
            format!("\x1b[{n}{c}")
        }
        .into_bytes();
    }
    if let Key::Keypad(c) = key {
        if app_keypad {
            let end = match c {
                '0'..='9' => (b'p' + c as u8 - b'0') as char,
                '.' => 'n',
                '+' => 'k',
                '-' => 'm',
                '*' => 'j',
                '/' => 'o',
                '\r' => 'M',
                '=' => 'X',
                _ => c,
            };
            return format!("\x1bO{end}").into_bytes();
        }
    }
    let mut text = match key {
        Key::Text(c) if mods.control => {
            let c = c.to_ascii_uppercase();
            match c {
                '@'..='_' => vec![c as u8 & 31],
                ' ' | '2' => vec![0],
                '3' => vec![27],
                '4' => vec![28],
                '5' => vec![29],
                '6' => vec![30],
                '7' | '/' => vec![31],
                '8' | '?' => vec![127],
                _ => c.to_string().into_bytes(),
            }
        }
        Key::Text(c) | Key::Keypad(c) => c.to_string().into_bytes(),
        Key::Enter => vec![13],
        Key::Backspace => vec![127],
        Key::Tab if mods.shift => b"\x1b[Z".to_vec(),
        Key::Tab => vec![9],
        Key::Escape => vec![27],
        _ => Vec::new(),
    };
    if mods.alt && !text.is_empty() {
        text.insert(0, 27);
    }
    text
}
