// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use crate::{
    paths::Directory,
    study::{
        configuration::{ConfigStepName, StudySettings},
        orchestrator::StudyOrchestrator,
        plan::VarStepId,
        runner::RunnerId,
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
    pub execution_step_dependencies: HashMap<ExeStepId, Vec<ExeStepId>>,
}

impl StudyExecutionPlan {
    pub fn into_orchestrator(self) -> Result<StudyOrchestrator, StudyExecutionPlanError> {
        // convert the exe_dependicies to runner_dependencies
        let runner_dependencies = self
            .execution_step_dependencies
            .iter()
            .map(|(exe_uid, dependencies)| {
                let exe_uid = RunnerId::from(*exe_uid);
                let exe_dependencies: Vec<RunnerId> = dependencies
                    .iter()
                    .map(|inner_exe_uid| RunnerId::from(*inner_exe_uid))
                    .collect();
                (exe_uid, exe_dependencies)
            })
            .collect::<HashMap<RunnerId, Vec<RunnerId>>>();
        todo!()
    }
}

/// Execution Step Id
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ExeStepId(VarStepId);

impl From<VarStepId> for ExeStepId {
    fn from(varstep_id: VarStepId) -> Self {
        Self(varstep_id)
    }
}

impl std::fmt::Display for ExeStepId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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
