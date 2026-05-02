// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;

use anyhow::{Error, bail};
use polars::prelude::Scalar;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, Visitor},
};

use crate::studycontrol::Branch;

// # of bytes to use in the digest length for the IDs. This affects naming
pub const BRID_DIGEST_LEN: usize = 8;
pub const VARSTEPID_DIGEST_LEN: usize = 8;
pub const VID_DIGEST_LEN: usize = 8;

/// VariationStep ID (VarStepID)
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct VarStepId {
    pub id: UidDigest<VARSTEPID_DIGEST_LEN>,
}

/// Constructor to create a stable VarStepId using a config step name + all dependent branches
impl VarStepId {
    pub fn from_uid_branches<'a, B>(
        config_step_uid: &str,
        upstream_branches: B,
    ) -> Result<Self, Error>
    where
        B: IntoIterator<Item = &'a Branch>,
    {
        let id = UidDigest::from_branches_with_prefix(config_step_uid, upstream_branches)?;
        Ok(VarStepId { id })
    }
}

impl std::fmt::Display for VarStepId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "vs")?;
        for b in &self.id.id {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarStepIdParseError {
    MissingPrefix,
    InvalidDigest(UidDigestParseError),
}

impl std::str::FromStr for VarStepId {
    type Err = VarStepIdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s
            .strip_prefix("vs")
            .ok_or(VarStepIdParseError::MissingPrefix)?;
        let id = hex
            .parse::<UidDigest<VARSTEPID_DIGEST_LEN>>()
            .map_err(VarStepIdParseError::InvalidDigest)?;
        Ok(VarStepId { id })
    }
}

impl std::fmt::Display for VarStepIdParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VarStepIdParseError::MissingPrefix => {
                write!(f, "VarStepId must start with `vs`")
            }
            VarStepIdParseError::InvalidDigest(err) => {
                write!(f, "invalid VarStepId digest: {err}")
            }
        }
    }
}

/// Variation ID
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct VId {
    pub id: UidDigest<VID_DIGEST_LEN>,
}
impl std::fmt::Display for VId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "V")?;
        for b in &self.id.id {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VIdParseError {
    MissingPrefix,
    InvalidDigest(UidDigestParseError),
}

impl std::str::FromStr for VId {
    type Err = VIdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s.strip_prefix("V").ok_or(VIdParseError::MissingPrefix)?;
        let id = hex
            .parse::<UidDigest<BRID_DIGEST_LEN>>()
            .map_err(VIdParseError::InvalidDigest)?;
        Ok(VId { id })
    }
}

impl std::fmt::Display for VIdParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VIdParseError::MissingPrefix => {
                write!(f, "VId must start with `V`")
            }
            VIdParseError::InvalidDigest(err) => {
                write!(f, "invalid VId digest: {err}")
            }
        }
    }
}

/// Branch ID
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BrId {
    pub id: UidDigest<BRID_DIGEST_LEN>,
}
impl std::fmt::Display for BrId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Br")?;
        for b in &self.id.id {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrIdParseError {
    MissingPrefix,
    InvalidDigest(UidDigestParseError),
}

impl std::str::FromStr for BrId {
    type Err = BrIdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s.strip_prefix("Br").ok_or(BrIdParseError::MissingPrefix)?;
        let id = hex
            .parse::<UidDigest<BRID_DIGEST_LEN>>()
            .map_err(BrIdParseError::InvalidDigest)?;
        Ok(BrId { id })
    }
}

impl std::fmt::Display for BrIdParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BrIdParseError::MissingPrefix => {
                write!(f, "BrId must start with `Br`")
            }
            BrIdParseError::InvalidDigest(err) => {
                write!(f, "invalid BrId digest: {err}")
            }
        }
    }
}

/// Digest container of <N> bytes
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct UidDigest<const N: usize> {
    pub id: [u8; N],
}
impl<const N: usize> UidDigest<N> {
    pub fn from_str_value(string: &str, val: &Scalar) -> Result<Self, Error> {
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(string.as_bytes());
        buf.extend(
            crate::conversion::convert_scalar_to_bytes_array(val)
                .expect("Unable to convert to bytes array"),
        );
        let digest = blake3::hash(&buf); // 32 bytes
        let out: [u8; N] = digest.as_bytes()[..N].try_into()?;

        Ok(Self { id: out })
    }

    /// Creates a Uid12 from a Hashmap, usually inputs, linking inputs to scalars
    /// Sorts the names so that it should be order independent, as long as each branch name is unique
    pub fn from_branches_with_prefix<'a, B>(prefix: &str, branches: B) -> Result<Self, Error>
    where
        B: IntoIterator<Item = &'a Branch>,
    {
        let mut buf: Vec<u8> = Vec::new();

        let mut branch_vec: Vec<&Branch> = branches.into_iter().collect();

        // add a check for branch name uniqueness, this is an edge case
        let unique_count = branch_vec
            .iter()
            .map(|x| &x.variable_name)
            .collect::<HashSet<_>>()
            .len();

        // error out if they are not the same, indicates they are not all unique
        match unique_count == branch_vec.len() {
            false => bail!("Multiple copies of variable names in input branches"),
            _ => {}
        }

        // sort the branches for repeatability
        branch_vec.sort_by_key(|k| &k.variable_name);

        buf.extend_from_slice(prefix.as_bytes());

        for b in branch_vec {
            buf.extend_from_slice(b.variable_name.as_bytes());
            buf.extend(
                crate::conversion::convert_scalar_to_bytes_array(&b.value)
                    .expect("Unable to convert to bytes array"),
            );
        }

        let digest = blake3::hash(&buf); // 32 bytes
        let out: [u8; N] = digest.as_bytes()[..N].try_into().unwrap();
        return Ok(Self { id: out });
    }
}

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

/// Tests for VarStepId, BrId, VId, UidDigest
#[cfg(test)]
mod test_uid_digest {

    use std::str::FromStr;

    use polars::prelude::{AnyValue, DataType};

    use super::*;

    // Normal usage
    #[test]
    fn test_direct_construct() {
        let c_id: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 9];
        let uid = UidDigest::<8> { id: c_id };
        assert_eq!(uid.id, [1, 2, 3, 4, 5, 6, 7, 9]);
    }

    #[test]
    fn test_str_value_construction() {
        let str = "sleep_time".to_string();
        let dtype = DataType::String;
        let value = Scalar::new(dtype.clone(), AnyValue::String("1.2"));

        // just make sure it doesn't error
        let uid_res = UidDigest::<8>::from_str_value(&str, &value);
        assert!(uid_res.is_ok());
    }

    #[test]
    fn test_branch_construction_and_invariance() {
        // define branches directly
        let br1_id = BrId::from_str("Br8ab76063a44fe25f").unwrap(); // actually the BrId doesn't matter
        let variable1_name = "sleep_time".to_string();
        let dtype = DataType::String;
        let variable1_value = Scalar::new(dtype.clone(), AnyValue::String("1.2"));
        let branch1 = Branch {
            uid: br1_id,
            variable_name: variable1_name,
            value: variable1_value,
        };

        let br2_id = BrId::from_str("Br03f41d5e9f2c560b").unwrap(); // actually the BrId doesn't matter
        let variable2_name = "wait_time".to_string();
        let variable2_value = Scalar::new(dtype.clone(), AnyValue::String("1.5"));
        let branch2 = Branch {
            uid: br2_id,
            variable_name: variable2_name,
            value: variable2_value,
        };

        let branches = vec![&branch1, &branch2];
        let branches_inverted = vec![&branch2, &branch1];

        // just check that it is invariant to order, but not invariant if the branch name is the same
        let uid = UidDigest::<8>::from_branches_with_prefix("prefix", branches).unwrap();
        let uid_inverted =
            UidDigest::<8>::from_branches_with_prefix("prefix", branches_inverted).unwrap();

        assert_eq!(uid, uid_inverted);
    }

    /// similar to test_branch_construction_and_invariance, except this is an edge case where branch names are the same
    /// should through an error (otherwise it can be dependent on order, if values evaluate the same)
    #[test]
    fn test_nonunique_branch_vars() {
        // define branches directly
        let br1_id = BrId::from_str("Br8ab76063a44fe25f").unwrap(); // actually the BrId doesn't matter
        let variable1_name = "sleep_time".to_string();
        let dtype = DataType::String;
        let variable1_value = Scalar::new(dtype.clone(), AnyValue::String("1.2"));
        let branch1 = Branch {
            uid: br1_id,
            variable_name: variable1_name,
            value: variable1_value,
        };

        let br2_id = BrId::from_str("Br03f41d5e9f2c560b").unwrap(); // actually the BrId doesn't matter
        let variable2_name = "sleep_time".to_string();
        let variable2_value = Scalar::new(dtype.clone(), AnyValue::String("1.5"));
        let branch2 = Branch {
            uid: br2_id,
            variable_name: variable2_name,
            value: variable2_value,
        };

        let branches = vec![&branch1, &branch2];

        // just check that it is invariant to order, but not invariant if the branch name is the same
        let uid = UidDigest::<8>::from_branches_with_prefix("prefix", branches);
        assert!(uid.is_err())
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

#[cfg(test)]
mod test_br_uid {

    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_construction() {
        let hex_str = "ad2adb0860fe8343";
        let uid = UidDigest::<8>::from_str(hex_str).unwrap();
        let br_id = BrId { id: uid };
        let u8_arr: [u8; 8] = [173, 42, 219, 8, 96, 254, 131, 67];
        assert_eq!(br_id.id, UidDigest { id: u8_arr });
    }

    #[test]
    fn test_construction_from_str() {
        let br_id = BrId::from_str("Brad2adb0860fe8343").unwrap();
        let u8_arr: [u8; 8] = [173, 42, 219, 8, 96, 254, 131, 67];
        assert_eq!(br_id.id, UidDigest { id: u8_arr });
    }

    #[test]
    fn test_construction_from_str_bad_prefix() {
        let br_id = BrId::from_str("Vrad2adb0860fe8343");

        assert_eq!(br_id.unwrap_err(), BrIdParseError::MissingPrefix);
    }

    #[test]
    fn test_construction_from_str_wrong_size() {
        let size: usize = (BRID_DIGEST_LEN - 1) * 2;
        // need size + 2 b/c of the extra Br prefix
        let str = "Brad2adb0860fe8343ad2adb0860fe8343"[..size + 2].to_string();
        let br_id = BrId::from_str(&str);
        assert_eq!(
            br_id.unwrap_err(),
            BrIdParseError::InvalidDigest(UidDigestParseError::InvalidLength {
                expected: BRID_DIGEST_LEN * 2,
                actual: (BRID_DIGEST_LEN - 1) * 2
            })
        );
    }

    #[test]
    fn test_construction_from_str_bad_hex() {
        let br_id = BrId::from_str("Brad2adb0860fe83zz");
        assert_eq!(
            br_id.unwrap_err(),
            BrIdParseError::InvalidDigest(UidDigestParseError::InvalidHex)
        );
    }
}

#[cfg(test)]
mod test_vid_uid {

    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_construction() {
        let hex_str = "ad2adb0860fe8343";
        let uid = UidDigest::<8>::from_str(hex_str).unwrap();
        let vid_id = VId { id: uid };
        let u8_arr: [u8; 8] = [173, 42, 219, 8, 96, 254, 131, 67];
        assert_eq!(vid_id.id, UidDigest { id: u8_arr });
    }

    #[test]
    fn test_construction_from_str() {
        let vid_id = VId::from_str("Vad2adb0860fe8343").unwrap();
        let u8_arr: [u8; 8] = [173, 42, 219, 8, 96, 254, 131, 67];
        assert_eq!(vid_id.id, UidDigest { id: u8_arr });
    }

    #[test]
    fn test_construction_from_str_bad_prefix() {
        let vid_id = VId::from_str("Brad2adb0860fe8343");

        assert_eq!(vid_id.unwrap_err(), VIdParseError::MissingPrefix);
    }

    #[test]
    fn test_construction_from_str_wrong_size() {
        let size: usize = (VID_DIGEST_LEN - 1) * 2;
        // need size + 1 b/c of the extra Vid prefix
        let str = "Vad2adb0860fe8343ad2adb0860fe8343"[..size + 1].to_string();
        let vid = VId::from_str(&str);
        assert_eq!(
            vid.unwrap_err(),
            VIdParseError::InvalidDigest(UidDigestParseError::InvalidLength {
                expected: VID_DIGEST_LEN * 2,
                actual: (VID_DIGEST_LEN - 1) * 2
            })
        );
    }

    #[test]
    fn test_construction_from_str_bad_hex() {
        let vid = VId::from_str("Vad2adb0860fe83zz");
        assert_eq!(
            vid.unwrap_err(),
            VIdParseError::InvalidDigest(UidDigestParseError::InvalidHex)
        );
    }
}

/// Tests for the VarStep Uid
#[cfg(test)]
mod test_vs_uid {

    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_construction() {
        let hex_str = "ad2adb0860fe8343";
        let uid = UidDigest::<8>::from_str(hex_str).unwrap();
        let vsid_id = VarStepId { id: uid };
        let u8_arr: [u8; 8] = [173, 42, 219, 8, 96, 254, 131, 67];
        assert_eq!(vsid_id.id, UidDigest { id: u8_arr });
    }

    #[test]
    fn test_construction_from_str() {
        let vs_id = VarStepId::from_str("vsad2adb0860fe8343").unwrap();
        let u8_arr: [u8; 8] = [173, 42, 219, 8, 96, 254, 131, 67];
        assert_eq!(vs_id.id, UidDigest { id: u8_arr });
    }

    #[test]
    fn test_construction_from_str_bad_prefix() {
        let vs_id = VarStepId::from_str("Brad2adb0860fe8343");

        assert_eq!(vs_id.unwrap_err(), VarStepIdParseError::MissingPrefix);
    }

    #[test]
    fn test_construction_from_str_wrong_size() {
        let size: usize = (VARSTEPID_DIGEST_LEN - 1) * 2;
        // need size + 2 b/c of the extra vs prefix
        let str = "vsad2adb0860fe8343ad2adb0860fe8343"[..size + 2].to_string();
        let vs_id = VarStepId::from_str(&str);
        assert_eq!(
            vs_id.unwrap_err(),
            VarStepIdParseError::InvalidDigest(UidDigestParseError::InvalidLength {
                expected: VARSTEPID_DIGEST_LEN * 2,
                actual: (VARSTEPID_DIGEST_LEN - 1) * 2
            })
        );
    }

    #[test]
    fn test_construction_from_str_bad_hex() {
        let vs_id = VarStepId::from_str("vsad2adb0860fe83zz");
        assert_eq!(
            vs_id.unwrap_err(),
            VarStepIdParseError::InvalidDigest(UidDigestParseError::InvalidHex)
        );
    }
}
