// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

pub const VARSTEPID_DIGEST_LEN: usize = 8;

use std::collections::{HashMap, HashSet};

use crate::{
    digest::{Digest, hash_digests_stable},
    paths::{Directory, PathError},
    study::{
        configuration::{ConfigStep, ConfigStepName, StudySettings},
        dag::{ActaDag, DagError},
        design::{BrId, VId, Variation},
        executionplan::{ExeStep, ExeStepId, StudyExecutionPlan},
        templatedstring::{
            ArgString, ExePath, ExePathError,
            TemplatedStringError::{self, MissingContextKey},
            VarStepTemplatedString, VarStepTemplatedStringError,
        },
    },
    uid::{Uid, UidError, UidPrefix},
};

#[derive(Debug, thiserror::Error)]
pub enum StudyPlanError {
    #[error(transparent)]
    ExeStepBuildError(#[from] ExeStepBuildError),

    #[error(transparent)]
    PathError(#[from] PathError),

    #[error(transparent)]
    DagError(#[from] DagError),
}

#[derive(Debug)]
pub struct StudyPlan {
    pub settings: StudySettings,
    pub steps: Vec<ConfigStep>,
    pub varsteps: HashMap<VarStepId, VarStep>,
    pub variations: Vec<Variation>,
    pub variation_varsteps: HashMap<VId, Vec<VarStepId>>,
    pub dag: ActaDag<VarStepId>,
}

impl StudyPlan {
    pub fn try_into_execution_plan(self) -> Result<StudyExecutionPlan, StudyPlanError> {
        // Execution plan needs:
        // settings: StudySettings, (from StudyPlan)
        // run_order: Vec<ExeStepId>, (generated from DAG)
        // execution_steps: HashMap<ExeStepId, ExeStep>, (generated from VarSteps)
        // variations: HashMap<VId, Variation>, (from StudyPlan)
        // branches: HashMap<BrId, VariableBranch>, (from StudyPlan)

        // generate the execution order
        // first go through each of the variations and find the starting varsteps
        // these are the ones without any parents
        let mut starting_nodes: Vec<VarStepId> = Vec::new();
        for variation in &self.variations {
            let variation_varsteps = &self.variation_varsteps[&variation.uid];
            for varstep_uid in variation_varsteps {
                let varstep = &self.varsteps[varstep_uid];

                if varstep.get_dependent_steps().len() == 0 {
                    starting_nodes.push(varstep_uid.clone())
                }
            }
        }

        // from each starting node, do a depth first search collecting all the nodes
        let vs_order: Vec<VarStepId> = self.dag.get_all_nodes_dfs(&starting_nodes)?;
        let exe_order: Vec<ExeStepId> = vs_order.into_iter().map(|x| ExeStepId::from(x)).collect();

        // Convert the varsteps into ExeSteps
        let shared_directory = &self.settings.shared_dir;
        let run_dir = &self.settings.run_dir;

        // create a map of the VarstepId run directories
        let mut varstep_dir_map: HashMap<VarStepId, Directory> =
            HashMap::with_capacity(self.varsteps.len());
        for vsid in self.varsteps.keys() {
            let vsid_run_dir = Directory::new(run_dir.as_path().join(vsid.to_string()))?;
            varstep_dir_map.insert(vsid.clone(), vsid_run_dir);
        }

        let exe_steps = self
            .varsteps
            .into_iter()
            .map(|(k, varstep)| {
                let exe_uid = ExeStepId::from(k);
                varstep
                    .try_into_execution_step(&varstep_dir_map, &shared_directory)
                    .map(|r| (exe_uid, r))
            })
            .collect::<Result<HashMap<ExeStepId, ExeStep>, ExeStepBuildError>>()?;

        Ok(StudyExecutionPlan {
            settings: self.settings,
            run_order: exe_order,
            execution_steps: exe_steps,
        })
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

#[derive(Debug, thiserror::Error)]
pub enum ExeStepBuildError {
    #[error(transparent)]
    VarstepTemplatedStringError(#[from] VarStepTemplatedStringError),

    #[error(transparent)]
    TemplatedStringError(#[from] TemplatedStringError),

    #[error(transparent)]
    ExePathError(#[from] ExePathError),
}

impl VarStep {
    pub fn try_into_execution_step(
        self,
        varstep_dirs: &HashMap<Uid<VarStepIdType, 8>, crate::paths::Directory>,
        shared_dir: &Directory,
    ) -> Result<ExeStep, ExeStepBuildError> {
        let run_exe = ExePath::try_from_path(
            self.run_exe
                .try_into_arg_string(varstep_dirs, shared_dir)?
                .into_string(),
        )?;

        // turn all the VarStepTemplatedString into ArgStrings
        let run_args = self
            .run_args
            .into_iter()
            .map(|x| x.try_into_arg_string(varstep_dirs, shared_dir))
            .collect::<Result<Vec<ArgString>, VarStepTemplatedStringError>>()?;

        let run_dir = varstep_dirs
            .get(&self.uid)
            .ok_or_else(|| MissingContextKey(self.uid.to_string()))?
            .clone();

        Ok(ExeStep {
            uid: ExeStepId::from(self.uid),
            name: self.name,
            run_args,
            run_exe,
            run_dir,
        })
    }

    /// Gets all the referenced steps as strings in this Step
    pub fn get_referenced_steps(&self) -> HashSet<VarStepId> {
        let mut referenced_steps: HashSet<VarStepId> = HashSet::new();

        // look for the TemplatedStringPart through all the run_args
        for arg in &self.run_args {
            for p in &arg.parts {
                match p {
                    crate::study::templatedstring::VarStepTemplatedStringPart::Varstep(s) => {
                        referenced_steps.insert(s.clone());
                    }
                    _ => {}
                }
            }
        }
        referenced_steps
    }

    /// Gets the referenced steps minus the own step (as it is not dependent on it)
    pub fn get_dependent_steps(&self) -> HashSet<VarStepId> {
        let mut referenced_steps = self.get_referenced_steps();
        referenced_steps.remove(&self.uid);

        referenced_steps
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

#[cfg(test)]
mod test_plan {
    use crate::study::{
        configuration::ConfigStepName,
        design::{VariableBranch, VariableName, VariableValue},
    };

    use super::*;

    #[test]
    fn test_varstepid() {
        let branch1 =
            VariableBranch::new(VariableName::new("foo"), VariableValue::new("value")).unwrap();
        let config_name = ConfigStepName::from("name");

        let varstep_id = VarStepId::from_step_branches(&config_name, vec![branch1.uid]).unwrap();

        let digest_expected: Digest<8> = Digest([202, 4, 167, 137, 136, 206, 120, 68]);
        let varstep_id_expected = VarStepId::new(digest_expected);

        assert_eq!(varstep_id, varstep_id_expected)
    }

    #[test]
    fn test_varstepid_no_branches() {
        let config_name = ConfigStepName::from("name");

        let varstep_id = VarStepId::from_step_branches(&config_name, vec![]).unwrap();

        let digest_expected: Digest<8> = Digest([178, 24, 98, 207, 94, 229, 102, 185]);
        let varstep_id_expected = VarStepId::new(digest_expected);

        assert_eq!(varstep_id, varstep_id_expected)
    }
}
