// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
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
pub struct UidDigest<const N: usize> {
    pub id: [u8; N],
}
impl<const N: usize> UidDigest<N> {
    /// Function that returns a Hex representation of compacted size width
    /// If the full hex digest fits within `width`, the full digest is returned.
    /// Otherwise, the digest is shortened with `...` in the middle.
    ///

    /// # Examples
    ///
    /// ```
    /// # use actatools::digest::UidDigest;
    /// let digest = UidDigest::<8> {
    ///     id: [0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90],
    /// };
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

    /// creates a UidDigest from a string slice, hashing the string bytes
    pub fn from_str_slice(string: &str) -> Result<Self, Error> {
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(string.as_bytes());
        let digest = blake3::hash(&buf); // 32 bytes
        let out: [u8; N] = digest.as_bytes()[..N].try_into()?;
        Ok(Self { id: out })
    }

}

//     /// Creates a Uid12 from a Hashmap, usually inputs, linking inputs to scalars
//     /// Sorts the names so that it should be order independent, as long as each branch name is unique
//     pub fn from_branches_with_prefix<'a, B>(prefix: &str, branches: B) -> Result<Self, Error>
//     where
//         B: IntoIterator<Item = &'a Branch>,
//     {
//         let mut buf: Vec<u8> = Vec::new();

//         let mut branch_vec: Vec<&Branch> = branches.into_iter().collect();

//         // add a check for branch name uniqueness, this is an edge case
//         let unique_count = branch_vec
//             .iter()
//             .map(|x| &x.variable_name)
//             .collect::<HashSet<_>>()
//             .len();

//         // error out if they are not the same, indicates they are not all unique
//         match unique_count == branch_vec.len() {
//             false => bail!("Multiple copies of variable names in input branches"),
//             _ => {}
//         }

//         // sort the branches for repeatability
//         branch_vec.sort_by_key(|k| &k.variable_name);

//         buf.extend_from_slice(prefix.as_bytes());

//         for b in branch_vec {
//             buf.extend_from_slice(b.variable_name.as_bytes());
//             buf.extend(b.value.as_any_value().to_string().as_bytes());
//         }

//         let digest = blake3::hash(&buf); // 32 bytes
//         let out: [u8; N] = digest.as_bytes()[..N].try_into().unwrap();
//         return Ok(Self { id: out });
//     }
// }

impl<const N: usize> std::fmt::Display for UidDigest<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in &self.id {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

impl<const N: usize> Serialize for UidDigest<N> {
    /// Serializes as a Hex String
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = String::with_capacity(N * 2);

        for b in &self.id {
            use std::fmt::Write;
            write!(&mut s, "{:02x}", b).unwrap();
        }

        serializer.serialize_str(&s)
    }
}

impl<'de, const N: usize> Deserialize<'de> for UidDigest<N> {
    /// This is duplicated beloe in the FromStr --> need to refactor
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UidDigestVisitor<const N: usize>;

        impl<'de, const N: usize> Visitor<'de> for UidDigestVisitor<N> {
            type Value = UidDigest<N>;

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

                Ok(UidDigest { id: bytes })
            }
        }

        deserializer.deserialize_str(UidDigestVisitor::<N>)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UidDigestParseError {
    InvalidLength { expected: usize, actual: usize },
    InvalidHex,
}

impl std::fmt::Display for UidDigestParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UidDigestParseError::InvalidLength { expected, actual } => {
                writeln!(f, "Invalid Length; expected: {expected}, actual {actual}")
            }
            UidDigestParseError::InvalidHex => writeln!(f, "Invalid Hex received"),
        }
    }
}

impl<const N: usize> std::str::FromStr for UidDigest<N> {
    type Err = UidDigestParseError;
    /// Createes a UidDigest from hex String of detected size
    fn from_str(hex: &str) -> Result<Self, UidDigestParseError> {
        if hex.len() != N * 2 {
            return Err(UidDigestParseError::InvalidLength {
                expected: N * 2,
                actual: hex.len(),
            });
        }
        let mut bytes = [0u8; N];
        for i in 0..N {
            let start = i * 2;
            let end = start + 2;
            bytes[i] = u8::from_str_radix(&hex[start..end], 16)
                .map_err(|_| UidDigestParseError::InvalidHex)?;
        }
        Ok(UidDigest { id: bytes })
    }
}

/// Helper function to hash a file
pub fn hash_file<const N: usize>(file: &FilePath) -> Result<UidDigest<N>, Error> {
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
    Ok(UidDigest::<N> { id: digest }) // 32 bytes
}

/// helper function to hash a set of digests stably
pub fn hash_digests_stable<const N: usize>(
    digests: Vec<&UidDigest<N>>,
) -> Result<UidDigest<N>, Error> {
    let mut hasher = blake3::Hasher::new();

    for digest in digests {
        hasher.update(&digest.id);
    }

    let digest: [u8; N] = hasher.finalize().as_bytes()[..N].try_into()?;

    Ok(UidDigest { id: digest })
}

#[cfg(test)]
mod test_util_functions {

    use std::{path::PathBuf, str::FromStr};

    use crate::paths::{Directory, FilePath};

    use super::*;

    #[test]
    fn test_hash_file() {
        let expected_foo_bar_digest: UidDigest<32> =
            UidDigest::from_str("9b61116853b99ee97b0ed5d499da7e486d77db52fbc60a2357e5cbf6183d418c")
                .unwrap();

        let foo_bar_filepath = FilePath::new(
            &PathBuf::from("tests/fixtures/foo.bar"),
            Some(Directory::here()),
        )
        .unwrap();
        let foo_bar_digest: UidDigest<32> = hash_file(&foo_bar_filepath).unwrap();

        assert_eq!(foo_bar_digest, expected_foo_bar_digest);
    }

    #[test]
    fn test_hash_vec() {
        let digest1 = UidDigest::<8>::from_str("a3f91c7e4b08d2aa").unwrap();
        let digest2 = UidDigest::<8>::from_str("09ce44f8a1b7d305").unwrap();

        let expected_digest = UidDigest {
            id: [199, 197, 113, 128, 184, 61, 83, 212],
        };

        let digests = vec![&digest1, &digest2];

        let vec_digest = hash_digests_stable(digests).unwrap();

        assert_eq!(vec_digest, expected_digest)
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
        let uid = UidDigest::<8> { id: c_id };
        assert_eq!(uid.id, [1, 2, 3, 4, 5, 6, 7, 9]);
    }

    #[test]
    fn test_hexstr_representation() {
        let u8_arr: [u8; 8] = [173, 42, 219, 8, 96, 254, 131, 67];
        let hex_str = "ad2adb0860fe8343";
        let uid = UidDigest::<8>::from_str(hex_str).unwrap();
        assert_eq!(uid.id, u8_arr);
    }

    #[test]
    fn test_to_hexstr_representation() {
        let u8_arr: [u8; 8] = [173, 42, 219, 8, 96, 254, 131, 67];
        let uid = UidDigest::<8> { id: u8_arr };

        let hex_string = format!("{uid}");

        assert_eq!(hex_string, "ad2adb0860fe8343".to_string())
    }

    // Error cases

    #[test]
    fn test_too_long_hex() {
        let hex_str = "ad2adb0860fe8343ad2adb0860fe8343".to_string();
        let uid = UidDigest::<8>::from_str(&hex_str);
        assert_eq!(
            uid.unwrap_err(),
            UidDigestParseError::InvalidLength {
                expected: 16,
                actual: 32
            }
        );
    }

    #[test]
    fn test_non_hex_str() {
        // throw some zz in there
        let hex_str = "ad2adb0860fe83zz".to_string();
        let uid = UidDigest::<8>::from_str(&hex_str);
        assert_eq!(uid.unwrap_err(), UidDigestParseError::InvalidHex);
    }
}
