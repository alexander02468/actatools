// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use crate::{
    paths::{Directory, FilePath},
    study::{
        configuration::{ConfigStepName, StudySettings},
        design::{BrId, VId, VariableBranch, Variation},
        plan::{StudyPlan, VarStep, VarStepId},
        templatedstring::{ArgString, ExePath},
    },
};

enum StudyExecutionPlanError {}

/// Holds the ExecutionSteps that are fully realized steps with all paths realized.
#[derive(Debug)]
pub struct StudyExecutionPlan {
    settings: StudySettings,
    run_order: Vec<ExeStepId>,
    execution_steps: HashMap<ExeStepId, ExeStep>,
    variations: HashMap<VId, Variation>,
    branches: HashMap<BrId, VariableBranch>,
}

impl StudyExecutionPlan {
    pub fn try_from_study_plane(study_plan: StudyPlan) -> Result<Self, StudyExecutionPlanError> {
        todo!()
    }

    fn get_run_dir(exe_id: ExeStepId) -> Result<Directory, StudyExecutionPlanError> {
        todo!()
    }
}

/// Execution Step Id
#[derive(Debug)]
pub struct ExeStepId(VarStepId);

impl From<VarStepId> for ExeStepId {
    fn from(varstep_id: VarStepId) -> Self {
        Self(varstep_id)
    }
}

enum ExeStepError {}

/// Execution Step, holds all realized paths
#[derive(Debug)]
pub struct ExeStep {
    uid: ExeStepId,
    name: ConfigStepName,
    run_args: Vec<ArgString>,
    run_exe: ExePath,
    run_dir: Directory,
}

impl ExeStep {
    pub fn try_from_varstep(varstep: VarStep) -> Result<Self, ExeStepError> {
        todo!()
    }
}
