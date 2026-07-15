//! Shared helpers for safely slicing UTF-8 strings on byte-index boundaries.
//!
//! Rust `&str` slicing panics if the given byte index doesn't fall on a char
//! boundary. Several modules in this crate need to truncate or split content
//! at an approximate byte offset (e.g. "roughly 80 chars" or "roughly N
//! tokens' worth of chars"), and multi-byte UTF-8 content (emoji, CJK, accented
//! characters, etc.) can land exactly on one of those offsets and panic.
//!
//! These helpers round the requested index *down* to the nearest valid char
//! boundary, so callers get a slightly shorter (never longer, never panicking)
//! result instead of a crash.

/// Returns the largest byte index `<= index` that lies on a UTF-8 char
/// boundary of `s` (including `0` and `s.len()`).
///
/// Equivalent to the yet-unstable `str::floor_char_boundary`, reimplemented
/// here so this crate can run on stable Rust.
pub fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    // `is_char_boundary` is O(1); walking back at most 3 bytes is enough
    // since UTF-8 code points are at most 4 bytes long.
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Truncates `s` to at most `max_bytes` bytes, rounding down to the nearest
/// char boundary so the result is always valid UTF-8. Never panics.
pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    let boundary = floor_char_boundary(s, max_bytes);
    &s[..boundary]
}

/// Splits `s` into `(prefix, suffix)` at approximately byte offset `mid`,
/// rounding `mid` down to the nearest char boundary so the split never
/// panics. Equivalent to `s.split_at(mid)` but UTF-8-safe.
pub fn safe_split_at(s: &str, mid: usize) -> (&str, &str) {
    let boundary = floor_char_boundary(s, mid);
    s.split_at(boundary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_char_boundary_ascii_is_identity() {
        let s = "hello world";
        assert_eq!(floor_char_boundary(s, 5), 5);
        assert_eq!(floor_char_boundary(s, 0), 0);
        assert_eq!(floor_char_boundary(s, s.len()), s.len());
    }

    #[test]
    fn floor_char_boundary_clamps_past_end() {
        let s = "hi";
        assert_eq!(floor_char_boundary(s, 100), s.len());
    }

    #[test]
    fn floor_char_boundary_rounds_down_multibyte() {
        // "é" is 2 bytes (0xC3 0xA9). "a é" -> a(1) space(1) é(2) = len 4.
        // Index 3 lands inside "é"; should round down to 2 (right after the space).
        let s = "a é";
        assert_eq!(s.len(), 4);
        assert!(!s.is_char_boundary(3));
        assert_eq!(floor_char_boundary(s, 3), 2);
    }

    #[test]
    fn safe_truncate_never_panics_on_emoji_boundary() {
        // 🎉 is 4 bytes. Truncating right in the middle of it must not panic.
        let s = "abc🎉def";
        for n in 0..=s.len() + 5 {
            let out = safe_truncate(s, n);
            assert!(s.starts_with(out));
        }
    }

    #[test]
    fn safe_truncate_exact_boundary_unchanged() {
        let s = "hello";
        assert_eq!(safe_truncate(s, 5), "hello");
        assert_eq!(safe_truncate(s, 3), "hel");
        assert_eq!(safe_truncate(s, 0), "");
    }

    #[test]
    fn safe_split_at_never_panics_on_cjk_boundary() {
        // Each CJK char below is 3 bytes in UTF-8.
        let s = "abc中文字def";
        for n in 0..=s.len() + 5 {
            let (a, b) = safe_split_at(s, n);
            assert_eq!(format!("{a}{b}"), s);
        }
    }

    #[test]
    fn safe_split_at_matches_split_at_on_ascii() {
        let s = "hello world";
        assert_eq!(safe_split_at(s, 5), s.split_at(5));
    }
}
