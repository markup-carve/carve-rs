//! The text of an owned tree: a range of a shared `Arc<str>`, so the strings
//! of one parse share the copy of its source instead of one heap block each.

use std::borrow::Borrow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::Arc;

/// `start` value of a string that is its whole buffer, so a buffer longer
/// than `u32::MAX` still works.
const WHOLE: u32 = u32::MAX;

/// An immutable string of an owned tree. Reads as `&str`; cloning shares
/// the buffer.
#[derive(Clone, Default)]
pub struct Str {
    buf: Option<Arc<str>>,
    start: u32,
    end: u32,
}

impl Str {
    /// The string as `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match &self.buf {
            None => "",
            Some(b) if self.start == WHOLE => b,
            Some(b) => b.get(self.start as usize..self.end as usize).unwrap_or(""),
        }
    }

    /// `buf[start..end]`, sharing `buf`.
    fn slice(buf: &Arc<str>, start: usize, end: usize) -> Option<Self> {
        let (s, e) = (u32::try_from(start).ok()?, u32::try_from(end).ok()?);
        (s != WHOLE && buf.get(start..end).is_some()).then(|| Self {
            buf: Some(Arc::clone(buf)),
            start: s,
            end: e,
        })
    }

    fn whole(buf: Arc<str>) -> Self {
        if buf.is_empty() {
            return Self::default();
        }
        Self {
            buf: Some(buf),
            start: WHOLE,
            end: WHOLE,
        }
    }
}

/// Copies arena strings into owned ones: a string that lies in `text` becomes
/// a range of one shared copy of it.
pub(crate) struct Texts<'s> {
    text: &'s str,
    buf: Option<Arc<str>>,
}

impl<'s> Texts<'s> {
    /// No shared text: every string gets its own buffer.
    pub(crate) const NONE: Texts<'static> = Texts {
        text: "",
        buf: None,
    };

    /// Strings that borrow `text` share one copy of it.
    pub(crate) fn of(text: &'s str) -> Self {
        Self {
            text,
            buf: (!text.is_empty()).then(|| Arc::from(text)),
        }
    }

    /// The owned copy of `s`.
    pub(crate) fn str(&self, s: &str) -> Str {
        if s.is_empty() {
            return Str::default();
        }
        if let Some(buf) = &self.buf
            && let Some(at) = s.as_ptr().addr().checked_sub(self.text.as_ptr().addr())
            && at + s.len() <= self.text.len()
            && let Some(out) = Str::slice(buf, at, at + s.len())
        {
            return out;
        }
        Str::from(s)
    }
}

impl Deref for Str {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for Str {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<[u8]> for Str {
    fn as_ref(&self) -> &[u8] {
        self.as_str().as_bytes()
    }
}

impl AsRef<std::ffi::OsStr> for Str {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.as_str().as_ref()
    }
}

impl AsRef<std::path::Path> for Str {
    fn as_ref(&self) -> &std::path::Path {
        self.as_str().as_ref()
    }
}

impl Borrow<str> for Str {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Debug for Str {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for Str {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

impl PartialEq for Str {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for Str {}

impl PartialOrd for Str {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Str {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for Str {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

macro_rules! eq_with {
    ($($t:ty),*) => {$(
        impl PartialEq<$t> for Str {
            fn eq(&self, other: &$t) -> bool {
                self.as_str() == AsRef::<str>::as_ref(other)
            }
        }
        impl PartialEq<Str> for $t {
            fn eq(&self, other: &Str) -> bool {
                AsRef::<str>::as_ref(self) == other.as_str()
            }
        }
    )*};
}

eq_with!(str, String, std::borrow::Cow<'_, str>);

impl PartialEq<&str> for Str {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Str> for &str {
    fn eq(&self, other: &Str) -> bool {
        *self == other.as_str()
    }
}

impl From<&str> for Str {
    fn from(s: &str) -> Self {
        Self::whole(Arc::from(s))
    }
}

impl From<&String> for Str {
    fn from(s: &String) -> Self {
        Self::from(s.as_str())
    }
}

impl From<String> for Str {
    fn from(s: String) -> Self {
        Self::whole(Arc::from(s))
    }
}

impl From<Box<str>> for Str {
    fn from(s: Box<str>) -> Self {
        Self::whole(Arc::from(s))
    }
}

impl From<std::borrow::Cow<'_, str>> for Str {
    fn from(s: std::borrow::Cow<'_, str>) -> Self {
        Self::from(s.as_ref())
    }
}

impl From<char> for Str {
    fn from(c: char) -> Self {
        Self::from(c.encode_utf8(&mut [0; 4]) as &str)
    }
}

impl From<&Str> for Str {
    fn from(s: &Str) -> Self {
        s.clone()
    }
}

impl From<Str> for String {
    fn from(s: Str) -> Self {
        s.as_str().to_owned()
    }
}

impl From<&Str> for String {
    fn from(s: &Str) -> Self {
        s.as_str().to_owned()
    }
}

impl From<Str> for serde_json::Value {
    fn from(s: Str) -> Self {
        Self::String(s.into())
    }
}

impl From<&Str> for serde_json::Value {
    fn from(s: &Str) -> Self {
        Self::String(s.into())
    }
}

impl std::str::FromStr for Str {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(s))
    }
}

impl serde::Serialize for Str {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for Str {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d).map(Self::from)
    }
}

#[cfg(test)]
mod tests {
    use super::{Str, Texts};

    #[test]
    fn a_string_inside_the_text_shares_its_copy() {
        let text = String::from("alpha beta");
        let texts = Texts::of(&text);
        let beta = texts.str(text.get(6..).unwrap_or(""));
        assert_eq!(beta, "beta");
        let other = texts.str("beta");
        assert_eq!(other, beta);
        assert_eq!(Texts::NONE.str("x").as_str(), "x");
        assert_eq!(Str::default().as_str(), "");
        assert_eq!(format!("{:?}", Str::from("a\"b")), format!("{:?}", "a\"b"));
    }
}
