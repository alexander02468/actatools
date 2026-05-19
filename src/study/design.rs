// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    io::Read,
    iter::zip,
};

use crate::{
    digest::{Digest, hash_digests_stable, hash_parts},
    uid::{Uid, UidError, UidPrefix},
};

pub const VID_DIGEST_LEN: usize = 8;
pub const BRID_DIGEST_LEN: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum StudyDesignBuildError {
    #[error("Missing variable [{}] in Design File", .0)]
    MissingVariable(String),

    #[error("Missing value for variable [{}] in Design File on row [{}]", .1, .0)]
    MissingFieldValue(usize, String),

    #[error(transparent)]
    FileReadError(#[from] std::io::Error),

    #[error(transparent)]
    CSVReadError(#[from] csv::Error),

    #[error(transparent)]
    BranchError(#[from] BranchError),

    #[error(transparent)]
    UidError(#[from] UidError),
}

/// wraps the Design File to allow for construction of branches and Variations and such
#[derive(Debug)]
pub struct StudyDesign {
    pub branches: HashMap<BrId, VariableBranch>,
    pub variations: Vec<Variation>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct VIdType;
impl UidPrefix for VIdType {
    const PREFIX: &'static str = "V";
}

pub type VId = Uid<VIdType, VID_DIGEST_LEN>;
impl VId {
    pub fn from_brids<B: IntoIterator<Item = BrId>>(brids: B) -> Result<Self, UidError> {
        let brid_digests: Vec<Digest<VID_DIGEST_LEN>> =
            brids.into_iter().map(|x| x.as_digest()).collect::<Vec<_>>();
        let digest = hash_digests_stable(brid_digests)?;
        Ok(VId::new(digest))
    }
}

#[derive(Debug)]
pub struct Variation {
    pub uid: VId,
    pub branch_ids: HashSet<BrId>,
}

impl Variation {
    pub fn new<B>(brids: B) -> Result<Self, UidError>
    where
        B: IntoIterator<Item = BrId>,
    {
        let brids: Vec<BrId> = brids.into_iter().collect();

        let uid = VId::from_brids(brids.iter().copied())?;

        Ok(Self {
            uid,
            branch_ids: HashSet::from_iter(brids),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BranchIdType;
impl UidPrefix for BranchIdType {
    const PREFIX: &'static str = "Br";
}

pub type BrId = Uid<BranchIdType, BRID_DIGEST_LEN>;
impl BrId {
    fn from_branch_parts(
        variable_name: &VariableName,
        variable_value: &VariableValue,
    ) -> Result<Self, UidError> {
        let digest = hash_parts([variable_name.0.as_bytes(), variable_value.0.as_bytes()])?;
        Ok(Self::new(digest))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BranchError {
    #[error("Variable name is empty during construction for value [{}]", .0)]
    EmptyVariableName(String),

    #[error("Variable value is empty during constructionfor value [{}]", .0)]
    EmptyVariableValue(String),

    #[error(transparent)]
    UidError(#[from] UidError),
}

#[derive(Debug)]
pub struct VariableBranch {
    pub uid: BrId,
    pub name: VariableName,
    pub value: VariableValue,
}

impl VariableBranch {
    pub fn new(
        variable_name: VariableName,
        variable_value: VariableValue,
    ) -> Result<Self, BranchError> {
        // check if either are empty
        if variable_name.0.is_empty() {
            return Err(BranchError::EmptyVariableName(variable_value.0.to_string()));
        }

        if variable_value.0.is_empty() {
            return Err(BranchError::EmptyVariableValue(variable_name.0.to_string()));
        }

        Ok(Self {
            uid: BrId::from_branch_parts(&variable_name, &variable_value)?,
            name: variable_name,
            value: variable_value,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariableName(String);
impl VariableName {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into().trim().to_string())
    }
}

/// Represents a value of a variable. Only Strings are represented at the moment, but already abstracting in case we need
/// to have type safety or want to support variable value types
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariableValue(String);
impl VariableValue {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into().trim().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for VariableValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Helper struct to build the StudyDesign and check against StudyConfiguration for correctness
pub struct StudyDesignBuilder<'a> {
    variables: Vec<&'a str>,
}
impl<'a> StudyDesignBuilder<'a> {

    /// Builds a StudyDesign using a Reader
    pub fn build_from_reader<R: Read>(
        &self,
        reader: R,
    ) -> Result<StudyDesign, StudyDesignBuildError> {
        let mut reader: csv::Reader<R> = csv::Reader::from_reader(reader);

        let header = reader.headers()?;

        let mut indices: Vec<usize> = Vec::with_capacity(self.variables.len());

        // iterate over the variables in order so that the indices are in order of the variables
        for var in &self.variables {
            let idx = header
                .iter()
                .position(|val| val.trim() == *var)
                .ok_or_else(|| StudyDesignBuildError::MissingVariable(var.to_string()))?;
            indices.push(idx)
        }

        // now loop through and build the Branches and Variations
        let mut variations: Vec<Variation> = Vec::new();
        let mut branches: HashMap<BrId, VariableBranch> = HashMap::new();

        for (row_idx, r) in reader.records().enumerate() {
            let csv_record = r?;
            let mut variation_brids: Vec<BrId> = Vec::with_capacity(self.variables.len());

            for (idx, var) in zip(&indices, &self.variables) {
                // build each VariableBranch
                let val = csv_record.get(*idx).ok_or_else(|| {
                    StudyDesignBuildError::MissingFieldValue(row_idx, var.to_string())
                })?;

                let b = VariableBranch::new(VariableName::new(*var), VariableValue::new(val))?;
                variation_brids.push(b.uid);
                branches.insert(b.uid, b);
            }

            variations.push(Variation::new(variation_brids)?);
        }

        Ok(StudyDesign {
            branches,
            variations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builder<'a>(variables: Vec<&'a str>) -> StudyDesignBuilder<'a> {
        StudyDesignBuilder { variables }
    }

    #[test]
    fn variable_name_trims_input() {
        let name = VariableName::new("  alpha  ");

        assert_eq!(name.0, "alpha");
    }

    #[test]
    fn variable_value_trims_input() {
        let value = VariableValue::new("  fast  ");

        assert_eq!(value.as_str(), "fast");
    }

    #[test]
    fn branch_ids_are_stable_for_same_name_and_value() {
        let b1 = VariableBranch::new(
            VariableName::new("alpha"),
            VariableValue::new("1"),
        )
        .unwrap();

        let b2 = VariableBranch::new(
            VariableName::new("alpha"),
            VariableValue::new("1"),
        )
        .unwrap();

        assert_eq!(b1.uid, b2.uid);
    }

    #[test]
    fn branch_ids_differ_for_different_values() {
        let b1 = VariableBranch::new(
            VariableName::new("alpha"),
            VariableValue::new("1"),
        )
        .unwrap();

        let b2 = VariableBranch::new(
            VariableName::new("alpha"),
            VariableValue::new("2"),
        )
        .unwrap();

        assert_ne!(b1.uid, b2.uid);
    }

    #[test]
    fn variable_branch_rejects_empty_variable_name() {
        let err = VariableBranch::new(
            VariableName::new("   "),
            VariableValue::new("1"),
        )
        .unwrap_err();

        assert!(matches!(err, BranchError::EmptyVariableName(_)));
    }

    #[test]
    fn variable_branch_rejects_empty_variable_value() {
        let err = VariableBranch::new(
            VariableName::new("alpha"),
            VariableValue::new("   "),
        )
        .unwrap_err();

        assert!(matches!(err, BranchError::EmptyVariableValue(_)));
    }

    #[test]
    fn variation_ids_are_order_invariant() {
        let b1 = VariableBranch::new(
            VariableName::new("alpha"),
            VariableValue::new("1"),
        )
        .unwrap();

        let b2 = VariableBranch::new(
            VariableName::new("beta"),
            VariableValue::new("fast"),
        )
        .unwrap();

        let v1 = Variation::new([b1.uid, b2.uid]).unwrap();
        let v2 = Variation::new([b2.uid, b1.uid]).unwrap();

        assert_eq!(v1.uid, v2.uid);
    }

    #[test]
    fn study_design_builder_builds_branches_and_variations_from_csv() {
        let csv = "\
alpha,beta
1,fast
2,slow
";

        let builder = builder(vec!["alpha", "beta"]);

        let design = builder
            .build_from_reader(csv.as_bytes())
            .unwrap();

        assert_eq!(design.variations.len(), 2);

        // alpha=1, beta=fast, alpha=2, beta=slow
        assert_eq!(design.branches.len(), 4);

        for variation in &design.variations {
            assert_eq!(variation.branch_ids.len(), 2);
        }
    }

    #[test]
    fn study_design_builder_trims_headers() {
        let csv = "\
 alpha , beta 
1,fast
";

        let builder = builder(vec!["alpha", "beta"]);

        let design = builder
            .build_from_reader(csv.as_bytes())
            .unwrap();

        assert_eq!(design.variations.len(), 1);
        assert_eq!(design.branches.len(), 2);
    }

    #[test]
    fn study_design_builder_reuses_duplicate_branches_across_rows() {
        let csv = "\
alpha,beta
1,fast
1,fast
";

        let builder = builder(vec!["alpha", "beta"]);

        let design = builder
            .build_from_reader(csv.as_bytes())
            .unwrap();

        assert_eq!(design.variations.len(), 2);

        // Same alpha=1 and beta=fast branches should be reused by ID.
        assert_eq!(design.branches.len(), 2);

        assert_eq!(
            design.variations[0].branch_ids,
            design.variations[1].branch_ids,
        );

        assert_eq!(
            design.variations[0].uid,
            design.variations[1].uid,
        );
    }

    #[test]
    fn study_design_builder_errors_when_required_variable_missing() {
        let csv = "\
alpha,beta
1,fast
";

        let builder = builder(vec!["alpha", "gamma"]);

        let err = builder
            .build_from_reader(csv.as_bytes())
            .unwrap_err();

        assert!(matches!(
            err,
            StudyDesignBuildError::MissingVariable(var) if var == "gamma"
        ));
    }

}