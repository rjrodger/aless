//! Copying text out: the system clipboard when the `clipboard` feature is
//! on and a clipboard exists, else the terminal's own OSC 52 sequence,
//! which terminals such as xterm, kitty, WezTerm, iTerm2, Alacritty,
//! foot and Windows Terminal turn into a clipboard write (over SSH too).

use std::io::Write;

/// The OSC 52 sequence that asks the terminal to set its clipboard.
pub fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

/// Standard base64 with padding; small enough not to want a crate.
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(feature = "clipboard")]
fn system(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text.to_string()).map_err(|e| e.to_string())
}

#[cfg(not(feature = "clipboard"))]
fn system(_text: &str) -> Result<(), String> {
    Err("built without the clipboard feature".to_string())
}

/// Copy `text`, telling where it went ("the clipboard" or "the terminal
/// clipboard (OSC 52)").
pub fn copy(out: &mut impl Write, text: &str) -> Result<&'static str, String> {
    match system(text) {
        Ok(()) => Ok("the clipboard"),
        Err(system_err) => {
            out.write_all(osc52(text).as_bytes())
                .and_then(|_| out.flush())
                .map_err(|e| format!("{system_err}; OSC 52 failed too: {e}"))?;
            Ok("the terminal clipboard (OSC 52)")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64("héllo".as_bytes()), "aMOpbGxv");
    }

    #[test]
    fn osc52_shape() {
        assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
    }
}
