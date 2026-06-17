// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use crate::study::runner::{Runner, RunnerError, RunnerId, RunnerStatus};

#[derive(Debug, thiserror::Error)]
pub enum StudyOrchestratorError {
    #[error("{0} not found in orchestrator runners")]
    RunnerIdNotFound(RunnerId),

    #[error("Error occured while running {id} : {error}")]
    RunnerRunError { id: RunnerId, error: RunnerError },

    #[error("Error occured while getting status {id} : {error}")]
    RunnerStatusError { id: RunnerId, error: RunnerError },


    #[error("No runners are ready to run")]
    NoRunnersReady,
}
/// Basically a typed "good to go" bool. Any issues should be notready
#[derive(Debug, PartialEq, Eq)]
enum DependencyReadyStatus {
    Ready,
    NotReady,
}

/// Orchestrators job is to Manage the runners, querying their results, finding which ones can run next
pub struct StudyOrchestrator {
    runners: HashMap<RunnerId, Runner>,
    runner_dependencies: HashMap<RunnerId, Vec<RunnerId>>,
}

impl StudyOrchestrator {
    // query_status()
    pub fn get_status(&self, runner_id: RunnerId) -> Result<RunnerStatus, StudyOrchestratorError> {
        self.runners
            .get(&runner_id)
            .ok_or_else(|| StudyOrchestratorError::RunnerIdNotFound(runner_id))?
            .status()
            .map_err(|e| StudyOrchestratorError::RunnerStatusError {
                id: runner_id,
                error: e,
            })
    }

    pub fn run_next_runner(&mut self) -> Result<RunnerStatus, StudyOrchestratorError> {
        let runner_id = self
            .get_next_ready_runner()?
            .ok_or(StudyOrchestratorError::NoRunnersReady)?;

        self.run_runner(runner_id)
    }

    pub fn get_next_ready_runner(&self) -> Result<Option<RunnerId>, StudyOrchestratorError> {
        // loop through the runners getting the status. If it is Uninitialized, that means it is ready to run (initialize
        // and then run). Also make sure all its dependencies (if any) are cleanly finished
        for (runner_id, runner) in &self.runners {
            match runner
                .status()
                .map_err(|e| StudyOrchestratorError::RunnerStatusError {
                    id: *runner_id,
                    error: e,
                })? {
                RunnerStatus::Uninitialized => {
                    // check all its dependencies are finished
                    let dep_runners = self
                        .runner_dependencies
                        .get(runner_id)
                        .ok_or_else(|| StudyOrchestratorError::RunnerIdNotFound(*runner_id))?;
                    if self.check_dependency_statuses(&dep_runners)? == DependencyReadyStatus::Ready
                    {
                        return Ok(Some(*runner_id));
                    }
                    // if it's uninitialized and all the dependencies are good, then return this runner as good to run
                }
                _ => continue,
            }
        }
        // no errors, but nothing ready to run
        Ok(None)
    }

    fn check_dependency_statuses(
        &self,
        dep_runner_ids: &Vec<RunnerId>,
    ) -> Result<DependencyReadyStatus, StudyOrchestratorError> {
        for runner_id in dep_runner_ids {
            match self
                .runners
                .get(runner_id)
                .ok_or_else(|| StudyOrchestratorError::RunnerIdNotFound(*runner_id))?
                .status()
                .map_err(|e| StudyOrchestratorError::RunnerStatusError {
                    id: *runner_id,
                    error: e,
                })? {
                RunnerStatus::Completed => {}
                _ => return Ok(DependencyReadyStatus::NotReady),
            }
        }

        Ok(DependencyReadyStatus::Ready)
    }

    pub fn run_runner(
        &mut self,
        runner_id: RunnerId,
    ) -> Result<RunnerStatus, StudyOrchestratorError> {
        let runner = self
            .runners
            .get_mut(&runner_id)
            .ok_or(StudyOrchestratorError::RunnerIdNotFound(runner_id))?;

        // tag and raise a potential Err
        match runner.run() {
            Ok(_) => Ok(()),
            Err(e) => Err(StudyOrchestratorError::RunnerRunError {
                id: runner_id,
                error: e,
            }),
        }?;

        self.get_status(runner_id)
    }
}


#[cfg(test)]
mod test_study_orchestrator{
    

}
