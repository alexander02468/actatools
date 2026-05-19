// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::marker::PhantomData;

use crate::digest::{Digest, DigestError};

#[derive(Debug, thiserror::Error)]
pub enum UidError {
    #[error("Missing Prefix [{}] in digest", .0)]
    MissingPrefix(String),

    #[error("Desired compact size is too small")]
    FormatTooSmall,

    #[error(transparent)]
    DigestParseError(#[from] DigestError),
}

pub trait UidPrefix {
    const PREFIX: &'static str;
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct Uid<K, const N: usize> {
    digest: Digest<N>,
    _kind: PhantomData<K>,
}

// Functions that don't need a prefix
impl<K, const N: usize> Uid<K, N> {
    pub fn new(digest: Digest<N>) -> Self {
        Self {
            digest,
            _kind: PhantomData,
        }
    }

    pub fn as_digest(&self) -> Digest<N> {
        self.digest
    }
}

// Functions that need a Prefix
impl<K: UidPrefix, const N: usize> Uid<K, N> {
    pub fn get_compact(&self, width: usize) -> Result<String, UidError> {
        let prefix_length = K::PREFIX.len();

        // check that the desired width is wider than the prefix length
        if prefix_length > width {
            return Err(UidError::FormatTooSmall);
        }

        Ok(format!(
            "{}{}",
            K::PREFIX,
            self.digest.compact_hex(width - prefix_length)
        ))
    }
}

impl<K: UidPrefix, const N: usize> std::fmt::Display for Uid<K, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", K::PREFIX, self.digest)
    }
}

// FromStr with prefix
impl<K: UidPrefix, const N: usize> std::str::FromStr for Uid<K, N> {
    fn from_str(s: &str) -> Result<Self, UidError> {
        let hex = s
            .strip_prefix(K::PREFIX)
            .ok_or_else(|| UidError::MissingPrefix(K::PREFIX.to_string()))?;

        let digest = hex.parse::<Digest<N>>()?;

        Ok(Self {
            digest,
            _kind: PhantomData,
        })
    }

    type Err = UidError;
}

#[cfg(test)]
mod test_Uid {

    use std::str::FromStr;

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct TestIdKind;
    impl UidPrefix for TestIdKind {
        const PREFIX: &'static str = "Test";
    }

    pub type TestUid = Uid<TestIdKind, 8>;

    #[test]
    fn test_uid_creation() {
        let u8_arr: [u8; 8] = [173, 42, 219, 8, 96, 254, 131, 67];
        let test_uid = TestUid::new(Digest::<8>(u8_arr));

        let hex_string = format!("{test_uid}");

        assert_eq!(hex_string, "Testad2adb0860fe8343".to_string())
    }

    #[test]
    fn test_uid_compact_hex() {
        let u8_arr: [u8; 8] = [173, 42, 219, 8, 96, 254, 131, 67];
        let test_uid = TestUid::new(Digest::<8>(u8_arr));

        let hex_string = format!("{}", test_uid.get_compact(12).unwrap());

        assert_eq!(hex_string, "Testad...343".to_string())
    }

    #[test]
    fn test_uid_from_str() {
        let test_uid = TestUid::from_str("Testad2adb0860fe8343").unwrap();
        let hex_string = format!("{test_uid}");

        assert_eq!(hex_string, "Testad2adb0860fe8343".to_string())
    }

    #[test]
    fn test_uid_from_str_missing_prefix() {
        let test_uid = TestUid::from_str("ad2adb0860fe8343");
        assert!(test_uid.is_err())
    }
}
