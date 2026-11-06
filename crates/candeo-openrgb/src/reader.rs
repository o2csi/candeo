//! A cursor over a payload that never reads past its end.
//!
//! Everything a server sends is checked here before anything is allocated:
//! a count is refused when the bytes left could not hold that many elements,
//! so a forged `0xFFFF` does not reserve memory for 65,535 modes
//! (`openrgb-output.md` §6.6, *Reading*).

use crate::controller::ParseError;
use crate::Rgb;

pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Self { buf, at: 0 }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.buf.len() - self.at
    }

    pub(crate) fn position(&self) -> usize {
        self.at
    }

    fn error(&self, what: &'static str) -> ParseError {
        ParseError { what, at: self.at }
    }

    pub(crate) fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], ParseError> {
        if n > self.remaining() {
            return Err(self.error(what));
        }
        let bytes = &self.buf[self.at..self.at + n];
        self.at += n;
        Ok(bytes)
    }

    pub(crate) fn u16(&mut self, what: &'static str) -> Result<u16, ParseError> {
        let b = self.take(2, what)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub(crate) fn u32(&mut self, what: &'static str) -> Result<u32, ParseError> {
        let b = self.take(4, what)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn i32(&mut self, what: &'static str) -> Result<i32, ParseError> {
        let b = self.take(4, what)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// A string behind a `u16` length that counts its terminating zero
    /// (OpenRGBSDK.md, *Device Data*: "including null termination"). Invalid
    /// UTF-8 is replaced rather than refused: a name is shown, never parsed.
    pub(crate) fn string(&mut self, what: &'static str) -> Result<String, ParseError> {
        let len = usize::from(self.u16(what)?);
        let bytes = self.take(len, what)?;
        Ok(text(bytes))
    }

    /// A string behind a `u32` length: the controller's `configuration`
    /// (protocol 6, *Device Data*).
    pub(crate) fn long_string(&mut self, what: &'static str) -> Result<String, ParseError> {
        let len = usize::try_from(self.u32(what)?).map_err(|_| self.error(what))?;
        let bytes = self.take(len, what)?;
        Ok(text(bytes))
    }

    /// A `u16` count of elements at least `min_each` bytes long, refused when
    /// what is left could not hold them.
    pub(crate) fn count(
        &mut self,
        what: &'static str,
        min_each: usize,
    ) -> Result<usize, ParseError> {
        let n = usize::from(self.u16(what)?);
        match n.checked_mul(min_each) {
            Some(bytes) if bytes <= self.remaining() => Ok(n),
            _ => Err(self.error(what)),
        }
    }

    pub(crate) fn colors(&mut self, n: usize, what: &'static str) -> Result<Vec<Rgb>, ParseError> {
        let bytes = self.take(n.checked_mul(4).ok_or_else(|| self.error(what))?, what)?;
        Ok(bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| Rgb::from_wire(*c))
            .collect())
    }
}

/// The bytes of a string, its terminating zero dropped.
fn text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_suffix(&[0]).unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_past_the_end_is_an_error_not_a_panic() {
        let mut r = Reader::new(&[1, 0, 0]);
        assert_eq!(r.u16("a").unwrap(), 1);
        let e = r.u32("b").unwrap_err();
        assert_eq!((e.what, e.at), ("b", 2));
    }

    /// A count the payload cannot hold is refused before anything is reserved.
    #[test]
    fn a_count_larger_than_the_payload_is_refused() {
        let mut r = Reader::new(&[0xff, 0xff, 0, 0]);
        assert!(r.count("modes", 2).is_err());
        let mut r = Reader::new(&[2, 0, 0, 0, 0, 0]);
        assert_eq!(r.count("modes", 2).unwrap(), 2);
    }

    #[test]
    fn a_string_drops_its_terminating_zero_and_survives_bad_utf8() {
        let mut r = Reader::new(&[3, 0, b'a', b'b', 0, 2, 0, 0xff, 0]);
        assert_eq!(r.string("s").unwrap(), "ab");
        assert_eq!(r.string("s").unwrap(), "\u{fffd}");
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn an_empty_string_is_zero_bytes() {
        let mut r = Reader::new(&[0, 0]);
        assert_eq!(r.string("s").unwrap(), "");
    }
}
