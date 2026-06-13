// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use crate::{
    paths::Directory,
    study::{
        configuration::{ConfigStepName, StudySettings},
        plan::VarStepId,
        templatedstring::{ArgString, ExePath},
    },
};

#[derive(Debug, thiserror::Error)]
pub enum StudyExecutionPlanError {}

/// Holds the ExecutionSteps that are fully realized steps with all paths realized.
#[derive(Debug)]
pub struct StudyExecutionPlan {
    pub settings: StudySettings,
    pub run_order: Vec<ExeStepId>,
    pub execution_steps: HashMap<ExeStepId, ExeStep>,
}

/// Execution Step Id
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ExeStepId(VarStepId);

impl From<VarStepId> for ExeStepId {
    fn from(varstep_id: VarStepId) -> Self {
        Self(varstep_id)
    }
}

/// Execution Step, holds all realized paths
#[derive(Debug)]
pub struct ExeStep {
    pub uid: ExeStepId,
    pub name: ConfigStepName,
    pub run_args: Vec<ArgString>,
    pub run_exe: ExePath,
    pub run_dir: Directory,
}
