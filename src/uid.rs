// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::Error;
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
    /// Sorts the names so that it should be order independent
    pub fn from_branches_with_prefix<'a, B>(prefix: &str, branches: B) -> Result<Self, Error>
    where
        B: IntoIterator<Item = &'a Branch>,
    {
        let mut buf: Vec<u8> = Vec::new();

        let mut branch_vec: Vec<&Branch> = branches.into_iter().collect();

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
