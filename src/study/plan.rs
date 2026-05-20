// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

pub const VARSTEPID_DIGEST_LEN: usize = 8;

use std::collections::{HashMap};

use crate::{
    digest::{Digest, hash_digests_stable},
    study::{
        configuration::{ConfigStep, ConfigStepName, StudySettings},
        dag::Dag,
        design::{BrId, Variation},
        executionplan::{ExecutionStep, StudyExecutionPlan},
        templatedstring::VarStepTemplatedString,
    },
    uid::{Uid, UidError, UidPrefix},
};

#[derive(Debug)]
pub struct StudyPlan {
    pub settings: StudySettings,
    pub steps: Vec<ConfigStep>,
    pub varsteps: HashMap<VarStepId, VarStep>,
    pub variations: Vec<Variation>,
    pub dag: Dag<VarStepId>,
}

impl StudyPlan {
    pub fn into_execution_plan(self) -> StudyExecutionPlan {
        todo!()
    }
}

#[derive(Debug)]
pub struct VarStep {
    pub name: ConfigStepName,
    pub uid: VarStepId,
    pub run_exe: VarStepTemplatedString,
    pub run_args: Vec<VarStepTemplatedString>,
    pub branch_dependencies: Vec<BrId>,
}

impl VarStep {
    pub fn into_execution_step(self) -> ExecutionStep {
        todo!()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VarStepIdType;
impl UidPrefix for VarStepIdType {
    const PREFIX: &'static str = "vs";
}
pub type VarStepId = Uid<VarStepIdType, VARSTEPID_DIGEST_LEN>;

impl VarStepId {
    pub fn from_step_branches(
        name: &ConfigStepName,
        branch_uids: Vec<BrId>,
    ) -> Result<Self, UidError> {
        let mut digests: Vec<Digest<VARSTEPID_DIGEST_LEN>> =
            Vec::with_capacity(branch_uids.len() + 1);

        let name_digest = Digest::<VARSTEPID_DIGEST_LEN>::from_str_slice(name.as_str())?;
        digests.push(name_digest);

        // use the hex digests in the case that digest lengths are different
        let branch_hex_digests = branch_uids
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>();
        for hex_digest_string in branch_hex_digests {
            let digest =
                Digest::<VARSTEPID_DIGEST_LEN>::from_str_slice(hex_digest_string.as_str())?;
            digests.push(digest);
        }

        let varstep_digest = hash_digests_stable(digests)?;

        Ok(Self::new(varstep_digest))
    }
}
