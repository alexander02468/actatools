// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    array::TryFromSliceError,
    fs::File,
    io::{BufReader, Read},
};

use anyhow::Error;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, Visitor},
};

use crate::paths::FilePath;

/// Digest container of <N> bytes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Digest<const N: usize>(pub [u8; N]);
impl<const N: usize> Digest<N> {
    /// Function that returns a Hex representation of compacted size width
    /// If the full hex digest fits within `width`, the full digest is returned.
    /// Otherwise, the digest is shortened with `...` in the middle.
    ///

    /// # Examples
    ///
    /// ```
    /// # use actatools::digest::Digest;
    /// let digest = Digest::<8>([0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90]);
    ///
    /// assert_eq!(
    ///     digest.compact_hex(12),
    ///     "abcd...67890"
    /// );
    ///
    /// assert_eq!(
    ///     digest.compact_hex(16),
    ///     "abcdef1234567890"
    /// );
    /// ```
    pub fn compact_hex(&self, width: usize) -> String {
        let full = format!("{}", &self);
        if full.len() <= width {
            return full;
        }

        // Need room for at least "a...b"
        if width < 5 {
            return full[..width.min(full.len())].to_string();
        }
        let ellipsis = "...";
        let remaining = width - ellipsis.len();
        let front_len = remaining / 2;
        let back_len = remaining - front_len;
        format!(
            "{}{}{}",
            &full[..front_len],
            ellipsis,
            &full[full.len() - back_len..]
        )
    }

    /// creates a Digest from a string slice, hashing the string bytes
    pub fn from_str_slice(string: &str) -> Result<Self, Error> {
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(string.as_bytes());
        let digest = blake3::hash(&buf); // 32 bytes
        let out: [u8; N] = digest.as_bytes()[..N].try_into()?;
        Ok(Self(out))
    }
}

impl<const N: usize> std::fmt::Display for Digest<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in &self.0 {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

impl<const N: usize> Serialize for Digest<N> {
    /// Serializes as a Hex String
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = String::with_capacity(N * 2);

        for b in &self.0 {
            use std::fmt::Write;
            write!(&mut s, "{:02x}", b).unwrap();
        }

        serializer.serialize_str(&s)
    }
}

impl<'de, const N: usize> Deserialize<'de> for Digest<N> {
    /// This is duplicated beloe in the FromStr --> need to refactor
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UidDigestVisitor<const N: usize>;

        impl<'de, const N: usize> Visitor<'de> for UidDigestVisitor<N> {
            type Value = Digest<N>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(
                    formatter,
                    "a {}-byte digest encoded as {} hex characters",
                    N,
                    N * 2
                )
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value.len() != N * 2 {
                    return Err(E::custom(format!(
                        "expected hex digest length {}, got {}",
                        N * 2,
                        value.len()
                    )));
                }

                let mut bytes = [0u8; N];

                for i in 0..N {
                    let start = i * 2;
                    let end = start + 2;

                    bytes[i] = u8::from_str_radix(&value[start..end], 16).map_err(E::custom)?;
                }

                Ok(Digest(bytes))
            }
        }

        deserializer.deserialize_str(UidDigestVisitor::<N>)
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum DigestError {
    #[error("Invalid length, expected [{}], actual [{}]", expected, actual)]
    InvalidLength { expected: usize, actual: usize },

    #[error("Invalid Hex String")]
    InvalidHex,

    #[error("Unable to slice Digest u8")]
    TryFromSliceError(#[from] TryFromSliceError),
}

impl<const N: usize> std::str::FromStr for Digest<N> {
    type Err = DigestError;
    /// Createes a UidDigest from hex String of detected size
    fn from_str(hex: &str) -> Result<Self, DigestError> {
        if hex.len() != N * 2 {
            return Err(DigestError::InvalidLength {
                expected: N * 2,
                actual: hex.len(),
            });
        }
        let mut bytes = [0u8; N];
        for i in 0..N {
            let start = i * 2;
            let end = start + 2;
            bytes[i] =
                u8::from_str_radix(&hex[start..end], 16).map_err(|_| DigestError::InvalidHex)?;
        }
        Ok(Digest(bytes))
    }
}

pub fn hash_parts<'a, I, const N: usize>(parts: I) -> Result<Digest<N>, DigestError>
where
    I: IntoIterator<Item = &'a [u8]>,
{
    let mut hasher = blake3::Hasher::new();

    for part in parts {
        hasher.update(part);
    }

    let full = hasher.finalize();

    let bytes: [u8; N] = full.as_bytes()[..N].try_into()?; // direct map 

    Ok(Digest(bytes))
}

/// Helper function to hash a file
pub fn hash_file<const N: usize>(file: &FilePath) -> Result<Digest<N>, Error> {
    let f = File::open(file.get_path()?)?;
    let mut hasher = blake3::Hasher::new();
    let mut reader = BufReader::new(f);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let bytes_read = reader.read(&mut buffer)?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }
    let digest: [u8; N] = hasher.finalize().as_bytes()[..N].try_into()?;
    Ok(Digest::<N>(digest)) // 32 bytes
}

/// helper function to hash a set of digests stably
pub fn hash_digests_stable<const N: usize>(
    digests: Vec<Digest<N>>,
) -> Result<Digest<N>, DigestError> {
    let mut hasher = blake3::Hasher::new();

    let mut digests_sorted = digests.clone();
    digests_sorted.sort();

    for digest in digests_sorted {
        hasher.update(&digest.0);
    }

    let digest: [u8; N] = hasher.finalize().as_bytes()[..N].try_into()?;

    Ok(Digest(digest))
}

#[cfg(test)]
mod test_util_functions {

    use std::{path::PathBuf, str::FromStr};

    use crate::paths::{Directory, FilePath};

    use super::*;

    #[test]
    fn test_hash_file() {
        let expected_foo_bar_digest: Digest<32> =
            Digest::from_str("9b61116853b99ee97b0ed5d499da7e486d77db52fbc60a2357e5cbf6183d418c")
                .unwrap();

        let foo_bar_filepath = FilePath::new(
            &PathBuf::from("tests/fixtures/foo.bar"),
            Some(Directory::here()),
        )
        .unwrap();
        let foo_bar_digest: Digest<32> = hash_file(&foo_bar_filepath).unwrap();

        assert_eq!(foo_bar_digest, expected_foo_bar_digest);
    }

    #[test]
    fn test_hash_vec() {
        let digest1 = Digest::<8>::from_str("a3f91c7e4b08d2aa").unwrap();
        let digest2 = Digest::<8>::from_str("09ce44f8a1b7d305").unwrap();

        let expected_digest = Digest([216, 82, 110, 144, 124, 18, 99, 217]);

        let digests = vec![digest1, digest2];
        let digests_reversed = vec![digest2, digest1];

        let vec_digest = hash_digests_stable(digests).unwrap();
        let vec_digest_reversed = hash_digests_stable(digests_reversed).unwrap();

        assert_eq!(vec_digest, expected_digest);
        assert_eq!(vec_digest_reversed, expected_digest)
    }
}

/// UidDigest
#[cfg(test)]
mod test_uid_digest {

    use std::str::FromStr;

    use super::*;

    // Normal usage
    #[test]
    fn test_direct_construct() {
        let c_id: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 9];
        let uid = Digest::<8>(c_id);
        assert_eq!(uid.0, [1, 2, 3, 4, 5, 6, 7, 9]);
    }

    #[test]
    fn test_hexstr_representation() {
        let u8_arr: [u8; 8] = [173, 42, 219, 8, 96, 254, 131, 67];
        let hex_str = "ad2adb0860fe8343";
        let uid = Digest::<8>::from_str(hex_str).unwrap();
        assert_eq!(uid.0, u8_arr);
    }

    #[test]
    fn test_to_hexstr_representation() {
        let u8_arr: [u8; 8] = [173, 42, 219, 8, 96, 254, 131, 67];
        let uid = Digest::<8>(u8_arr);

        let hex_string = format!("{uid}");

        assert_eq!(hex_string, "ad2adb0860fe8343".to_string())
    }

    // Error cases

    #[test]
    fn test_too_long_hex() {
        let hex_str = "ad2adb0860fe8343ad2adb0860fe8343".to_string();
        let uid_res = Digest::<8>::from_str(&hex_str);

        match uid_res.unwrap_err() {
            DigestError::InvalidLength {
                expected: 16,
                actual: 32,
            } => assert!(true),
            _ => assert!(false),
        }
    }

    #[test]
    fn test_non_hex_str() {
        // throw some zz in there
        let hex_str = "ad2adb0860fe83zz".to_string();
        let uid_res = Digest::<8>::from_str(&hex_str);

        match uid_res.unwrap_err() {
            DigestError::InvalidHex => assert!(true),
            _ => assert!(false),
        }
    }
}
