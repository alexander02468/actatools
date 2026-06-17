// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::paths::Directory;
use crate::study::executionplan::ExeStepId;
use crate::study::templatedstring::{ArgString, ExePath};

/// Runner heartbeat interval for the status indicator
pub const HEARTBEAT_INTERVAL_SECONDS: usize = 5;

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("Runner initialization was attempted while the run files seem to already be present")]
    InitializationFilesAlreadyPresent,
}

#[derive(Debug)]
pub enum RunnerStatus {
    Unknown,
    Uninitialized,
    Running,
    Error,
    Completed,
}

/// Execution Step Id
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct RunnerId(ExeStepId);

impl From<ExeStepId> for RunnerId {
    fn from(exestep_id: ExeStepId) -> Self {
        Self(exestep_id)
    }
}

impl std::fmt::Display for RunnerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug)]
pub enum Runner {
    Local(LocalRunner),
}

impl Runner {
    pub fn run(&mut self) -> Result<(), RunnerError> {
        match self {
            Runner::Local(local_runner) => local_runner.run(),
        }
    }

    pub fn status(&self) -> Result<RunnerStatus, RunnerError> {
        match self {
            Runner::Local(local_runner) => local_runner.status(),
        }
    }
}

/// Typical local runner
#[derive(Debug)]
pub struct LocalRunner {
    uid: RunnerId,
    run_exe: ExePath,
    run_args: Vec<ArgString>,
    run_dir: Directory,
    status: RunnerStatus,
}

impl LocalRunner {
    /// Creates a new runner with the arguments supplied and immediately attempts to update its status (as the job may
    /// already exist in some state)
    pub fn new(
        uid: RunnerId,
        run_exe: ExePath,
        run_args: Vec<ArgString>,
        run_dir: Directory,
    ) -> Result<Self, RunnerError> {
        let mut new_runner = Self {
            uid,
            run_exe,
            run_args,
            run_dir,
            status: RunnerStatus::Unknown,
        };
        new_runner.update_status();

        Ok(new_runner)
    }

    pub fn run(&mut self) -> Result<(), RunnerError> {
        todo!()
    }

    /// Initialize the working directory, status and such. If these files already exist, returns an RunnerError
    fn initialize(&mut self) -> Result<(), RunnerError> {
        todo!()
    }

    /// Attempts to update own status based on directories. If no status file is present, or directories are missing,
    /// status will be updated to RunnerStatus::Unknown.
    fn update_status(&mut self) -> Result<(), RunnerError> {
        todo!()
    }

    pub fn status(&self) -> Result<RunnerStatus, RunnerError> {
        todo!()
    }
}
