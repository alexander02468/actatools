// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::digest::UidDigest;
use crate::{paths::FilePath, study::studyplan::Branch};

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

mod test {

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
