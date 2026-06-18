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
    pub runners: HashMap<RunnerId, Box<dyn Runner>>,
    run_order: Vec<RunnerId>,
    runner_dependencies: HashMap<RunnerId, Vec<RunnerId>>,
}

impl StudyOrchestrator {
    pub fn new(
        runners: HashMap<RunnerId, Box<dyn Runner>>,
        run_order: Vec<RunnerId>,
        runner_dependencies: HashMap<RunnerId, Vec<RunnerId>>,
    ) -> Self {
        Self {
            runners,
            run_order,
            runner_dependencies,
        }
    }

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
        for runner_id in &self.run_order {
            let runner = self
                .runners
                .get(runner_id)
                .ok_or_else(|| StudyOrchestratorError::RunnerIdNotFound(*runner_id))?;
            match runner
                .status()
                .map_err(|e| StudyOrchestratorError::RunnerStatusError {
                    id: *runner_id,
                    error: e,
                })? {
                RunnerStatus::Uninitialized => {
                    // check all its dependencies are finished
                    let empty: Vec<RunnerId> = Vec::new();
                    let dep_runners = self
                        .runner_dependencies
                        .get(runner_id)
                        .cloned()
                        .unwrap_or(empty);
                    if self.check_dependency_statuses(&dep_runners)? == DependencyReadyStatus::Ready
                    {
                        // if it's uninitialized and all the dependencies are good, then return this runner as good to run
                        return Ok(Some(*runner_id));
                    }
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
mod test_study_orchestrator {
    use std::{assert_matches, collections::HashMap, path::PathBuf};

    use crate::{
        digest::Digest,
        study::{
            orchestrator::{StudyOrchestrator, StudyOrchestratorError},
            plan::VarStepId,
            runner::{Runner, RunnerError, RunnerId, RunnerStatus},
        },
    };

    // A Mock Runner that succeeds when run
    struct MockSuccessRunner {
        status: RunnerStatus,
    }

    impl MockSuccessRunner {
        fn new_box() -> Box<Self> {
            Box::new(Self {
                status: RunnerStatus::Uninitialized,
            })
        }
    }

    impl Runner for MockSuccessRunner {
        fn run(&mut self) -> Result<(), crate::study::runner::RunnerError> {
            self.status = RunnerStatus::Completed;
            Ok(())
        }

        fn status(
            &self,
        ) -> Result<crate::study::runner::RunnerStatus, crate::study::runner::RunnerError> {
            Ok(self.status.clone())
        }
    }

    /// A MockRunner that fails when run
    struct MockFailRunner {
        status: RunnerStatus,
    }

    impl MockFailRunner {
        fn new_box() -> Box<Self> {
            Box::new(Self {
                status: RunnerStatus::Uninitialized,
            })
        }
    }

    impl Runner for MockFailRunner {
        fn run(&mut self) -> Result<(), crate::study::runner::RunnerError> {
            self.status = RunnerStatus::Error;
            Err(RunnerError::RunFailed {
                err_file: PathBuf::from("FakePath.error"),
            })
        }

        fn status(
            &self,
        ) -> Result<crate::study::runner::RunnerStatus, crate::study::runner::RunnerError> {
            Ok(self.status.clone())
        }
    }

    fn create_runner_id(uid: u8) -> RunnerId {
        let digest: Digest<8> = Digest::<8>([0, 0, 0, 0, 0, 0, 0, uid]);
        let varstep_id = VarStepId::new(digest);
        RunnerId::from(varstep_id)
    }

    fn setup_study_orchestrator() -> Result<StudyOrchestrator, StudyOrchestratorError> {
        // build some runners
        let runner1 = MockSuccessRunner::new_box();
        let runner2 = MockSuccessRunner::new_box();
        let runner3 = MockFailRunner::new_box();

        let uid1 = create_runner_id(1);
        let uid2 = create_runner_id(2);
        let uid3 = create_runner_id(3);

        let mut runners: HashMap<RunnerId, Box<dyn Runner>> = HashMap::new();
        runners.insert(uid1, runner1);
        runners.insert(uid2, runner2);
        runners.insert(uid3, runner3);

        // setup some dependencies
        let mut runner_dependencies: HashMap<RunnerId, Vec<RunnerId>> = HashMap::new();
        // let 2 depend on 3 and 3 and on 1
        runner_dependencies.insert(uid2, vec![uid3]);
        runner_dependencies.insert(uid3, vec![uid1]);

        // the desired run order is in numeric order
        let run_order = vec![uid1, uid2, uid3];

        Ok(StudyOrchestrator {
            runners,
            run_order,
            runner_dependencies,
        })
    }

    #[test]
    fn test_study_orchestrator_creation() {
        let orchestrator = setup_study_orchestrator().unwrap();
        assert!(orchestrator.runners.len() == 3)
    }

    #[test]
    fn test_study_orchestrator_get_status() {
        let orchestrator = setup_study_orchestrator().unwrap();
        assert_matches!(
            orchestrator.get_status(create_runner_id(1)).unwrap(),
            RunnerStatus::Uninitialized
        )
    }

    #[test]
    fn test_get_next_ready_runner() {
        let orchestrator = setup_study_orchestrator().unwrap();
        //only uid1 should be ready
        assert_eq!(
            orchestrator.get_next_ready_runner().unwrap(),
            Some(create_runner_id(1))
        )
    }

    #[test]
    fn test_run_runner() {
        let mut orchestrator = setup_study_orchestrator().unwrap();
        let uid1 = create_runner_id(1);

        orchestrator.run_runner(uid1).unwrap();
        let status = orchestrator.get_status(uid1).unwrap();

        assert_matches!(status, RunnerStatus::Completed);
    }

    #[test]
    fn test_run_next() {
        let mut orchestrator = setup_study_orchestrator().unwrap();
        let uid1 = create_runner_id(1);

        orchestrator.run_next_runner().unwrap();
        let status = orchestrator.get_status(uid1).unwrap();

        assert_matches!(status, RunnerStatus::Completed);
    }

    #[test]
    fn test_runner_not_found() {
        // try and use a runner_id that doesn't exist
        let orchestrator = setup_study_orchestrator().unwrap();
        let uid4 = create_runner_id(4);
        let status = orchestrator.get_status(uid4).unwrap_err();

        assert_matches!(status, StudyOrchestratorError::RunnerIdNotFound(_));
    }

    #[test]
    fn test_run_error() {
        let mut orchestrator = setup_study_orchestrator().unwrap();
        let uid3 = create_runner_id(3); //uid3 has a run_error
        let e = orchestrator.run_runner(uid3).unwrap_err();
        assert_matches!(
            e,
            StudyOrchestratorError::RunnerRunError { id: _, error: _ }
        )
    }

    #[test]
    fn test_no_runner_ready() {
        let mut orchestrator = setup_study_orchestrator().unwrap();

        // there are three runs, so just call it 4 times and save the 4th attempt
        orchestrator.run_next_runner().unwrap();
        let _ = orchestrator.run_next_runner();
        let _ = orchestrator.run_next_runner(); // this one has a run error
        let e = orchestrator.run_next_runner().unwrap_err();

        assert_matches!(e, StudyOrchestratorError::NoRunnersReady)
    }
}
