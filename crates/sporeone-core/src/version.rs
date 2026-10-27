//! Versions as the Launcher Kit reads them: .NET's `System.Version`, two to four numbers with dots.
//! A part left out counts as zero, so `2.5.20` and `2.5.20.0` are the same build.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, Copy, Debug, Default, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Version {
    parts: [u32; 4],
    /// How many parts were written, to write it back the same way.
    len: u8,
}

impl Version {
    pub const fn new(major: u32, minor: u32, build: u32, revision: u32) -> Version {
        Version { parts: [major, minor, build, revision], len: 4 }
    }

    /// `1.2`, `2.5.20`, `1.0.1.3`, with a `v` in front or not. One number alone is taken too, as
    /// mod authors tag releases `v1`.
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim();
        let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
        let mut parts = [0u32; 4];
        let mut len = 0;
        for p in s.split('.') {
            if len == 4 || p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            parts[len] = p.parse().ok()?;
            len += 1;
        }
        (len > 0).then_some(Version { parts, len: len as u8 })
    }

    /// The first three parts, which is all `dllsBuild` names.
    pub fn build3(self) -> Version {
        Version { parts: [self.parts[0], self.parts[1], self.parts[2], 0], len: 3 }
    }

    pub fn parts(&self) -> [u32; 4] {
        self.parts
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Version) -> bool {
        self.parts == other.parts
    }
}

impl std::hash::Hash for Version {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.parts.hash(h);
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Version) -> Ordering {
        self.parts.cmp(&other.parts)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Version) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = usize::from(self.len.max(1));
        let s = self.parts[..len].iter().map(u32::to_string).collect::<Vec<_>>().join(".");
        f.write_str(&s)
    }
}

impl TryFrom<String> for Version {
    type Error = String;
    fn try_from(s: String) -> Result<Version, String> {
        Version::parse(&s).ok_or_else(|| format!("not a version: {s}"))
    }
}

impl From<Version> for String {
    fn from(v: Version) -> String {
        v.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_parts_are_zero() {
        assert_eq!(Version::parse("2.5.20"), Version::parse("2.5.20.0"));
        assert!(Version::parse("2.5.20").unwrap() < Version::parse("2.5.21").unwrap());
        assert!(Version::parse("1.10").unwrap() > Version::parse("1.9.9").unwrap());
        assert_eq!(Version::parse("v1.0.1.3").unwrap().to_string(), "1.0.1.3");
        assert_eq!(Version::parse("3").unwrap().to_string(), "3");
    }

    #[test]
    fn rejects_what_is_not_a_version() {
        for s in ["", "1..2", "1.2.3.4.5", "1.2-beta", "abc", "1.2 "] {
            if s == "1.2 " {
                assert!(Version::parse(s).is_some());
            } else {
                assert!(Version::parse(s).is_none(), "{s}");
            }
        }
    }
}
