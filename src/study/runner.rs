// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs::File;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::paths::{Directory, FilePath, PathError};
use crate::study::executionplan::ExeStepId;
use crate::study::plan::VarStepId;
use crate::study::templatedstring::{ArgString, ExePath};

/// Runner heartbeat interval for the status indicator
pub const HEARTBEAT_INTERVAL_SECONDS: u64 = 5;

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error(transparent)]
    LocalRunnerError(#[from] LocalRunnerError),
}

#[derive(Debug, thiserror::Error)]
pub enum LocalRunnerError {
    #[error("Runner initialization was attempted while the run files seem to already be present")]
    InitializationFilesAlreadyPresent,

    #[error("Unable to retrieve the filename for RunnerStatus : {0}")]
    UnableGetStatusFilename(RunnerStatus),

    #[error("Run {id} did not complete successfully, see {err_file}")]
    RunFailed { id: RunnerId, err_file: PathBuf },

    #[error("Not allowed to change status from {0} to {1}")]
    ChangeStatusAdvanceError(RunnerStatus, RunnerStatus),

    #[error(transparent)]
    StatusFileError(#[from] StatusFileError),

    #[error(transparent)]
    PathError(#[from] PathError),

    #[error("An IO Error occurred when trying to run {id} : {e}")]
    RunIoError { id: RunnerId, e: std::io::Error },
}

/// Status of a Runner
/// A runner operates in 5 different states
///     1. Unknown - Only on first construction -- immediately turned into another state unless something goes wrong
///     2. Uninitialized - No status file is present. Runners that have not been run should resolve to this state
///     3. Running - The Runner is currently running and actively managing the process that is performing the Step
///     4. Error - The Runner has been already be run and completed -- and error occurred during the Run, either in the
///         management or what was being run
///     5. Completed - The Runner has been run and the run was determined to have completed successfully.
#[derive(Debug, Clone)]
pub enum RunnerStatus {
    Unknown,
    Running,
    Error,
    Completed,
}

impl RunnerStatus {
    pub fn as_filename(&self) -> Option<&'static str> {
        match self {
            RunnerStatus::Unknown => None,
            RunnerStatus::Running => Some("status.running"),
            RunnerStatus::Error => Some("status.error"),
            RunnerStatus::Completed => Some("status.completed"),
        }
    }
}

impl std::fmt::Display for RunnerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunnerStatus::Unknown => write!(f, "Unknown"),
            RunnerStatus::Running => write!(f, "Running"),
            RunnerStatus::Error => write!(f, "Error"),
            RunnerStatus::Completed => write!(f, "Completed"),
        }
    }
}

/// Execution Step Id
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct RunnerId(ExeStepId);

impl From<ExeStepId> for RunnerId {
    fn from(exestep_id: ExeStepId) -> Self {
        Self(exestep_id)
    }
}

impl From<VarStepId> for RunnerId {
    fn from(varstep_id: VarStepId) -> Self {
        Self(ExeStepId::from(varstep_id))
    }
}

impl std::fmt::Display for RunnerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StatusFileError {
    #[error("Unable to get FilePath {0}")]
    UnableToGetFilePath(FilePath),

    #[error(transparent)]
    PathError(#[from] PathError),

    #[error("Multiple status files found : {}", .0.iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(", "))]
    MultipleStatusFilesFound(Vec<StatusFile>),

    #[error("unable to determine runner status from StatusFile : {0}")]
    UnableToDetermineRunnerStatus(StatusFile),

    #[error("unable to create file for Unknown status")]
    CannotTouchUnknown,

    #[error("unable to remove file for Unknown status")]
    CannotRemoveUnknown,

    #[error("unable to rename file for Unknwon status")]
    CannotRenameUnknown,

    #[error("unable to determine run directory from existing StatusFile")]
    UnableToGetRunDir,

    #[error("file must exist before it can be touched")]
    CannotTouchNonExistingFile,
}

#[derive(Debug, Clone)]
pub struct StatusFile(Option<FilePath>);

impl StatusFile {
    pub fn new(runner_status: &RunnerStatus, run_dir: &Directory) -> Result<Self, StatusFileError> {
        match runner_status {
            RunnerStatus::Unknown => Ok(Self(None)),
            RunnerStatus::Running | RunnerStatus::Error | RunnerStatus::Completed => {
                let filename = runner_status.as_filename().unwrap();
                let filepath = FilePath::new(PathBuf::from(filename), Some(run_dir.clone()))?;
                Ok(Self(Some(filepath)))
            }
        }
    }

    pub fn as_runner_status(&self) -> Result<RunnerStatus, StatusFileError> {
        match &self.0 {
            Some(fp) => {
                for runner_status in vec![
                    RunnerStatus::Running,
                    RunnerStatus::Completed,
                    RunnerStatus::Error,
                ] {
                    if fp.get_filename()?
                        == runner_status.as_filename().ok_or_else(|| {
                            StatusFileError::UnableToDetermineRunnerStatus(self.clone())
                        })?
                    {
                        return Ok(runner_status);
                    }
                }
                Err(StatusFileError::UnableToDetermineRunnerStatus(self.clone()))
            }
            None => Ok(RunnerStatus::Unknown),
        }
    }

    pub fn exists(&self) -> Result<bool, StatusFileError> {
        match &self.0 {
            Some(inner) => Ok(inner
                .get_path()
                .map_err(|_e| StatusFileError::UnableToGetFilePath(inner.clone()))?
                .exists()),
            None => Ok(false),
        }
    }

    /// Searches for a StatusFile, if more are detected, returns and error
    pub fn find_active(dir: &Directory) -> Result<StatusFile, StatusFileError> {
        let status_files = Self::find(dir)?;
        match status_files.len() {
            0 => Ok(StatusFile::new(&RunnerStatus::Unknown, dir)?),
            1 => Ok(status_files[0].clone()),
            _ => Err(StatusFileError::MultipleStatusFilesFound(status_files)),
        }
    }

    /// Attempts to find any status files in the directory
    fn find(dir: &Directory) -> Result<Vec<StatusFile>, StatusFileError> {
        let mut status_files: Vec<StatusFile> = Vec::new();

        for runner_status in vec![
            RunnerStatus::Running,
            RunnerStatus::Error,
            RunnerStatus::Completed,
        ] {
            match runner_status.as_filename() {
                Some(_) => {
                    let status_file = StatusFile::new(&runner_status, dir)?;
                    if status_file.exists()? {
                        status_files.push(status_file)
                    }
                }
                None => {}
            }
        }

        Ok(status_files)
    }

    /// Equivalent to using Linux touch() to create or update a file
    pub fn create(&self) -> Result<(), StatusFileError> {
        Ok(self
            .0
            .as_ref()
            .ok_or(StatusFileError::CannotTouchUnknown)?
            .touch()?)
    }

    /// Touch (update modified time) of an existing file. An error is returned if file does not already exist
    pub fn touch(&self) -> Result<(), StatusFileError>{
        if !self.exists()? {return Err(StatusFileError::CannotTouchNonExistingFile);}

        Ok(self
            .0
            .as_ref()
            .ok_or(StatusFileError::CannotTouchUnknown)?
            .touch()?)
    }

    fn remove(&self) -> Result<(), StatusFileError> {
        Ok(self
            .0
            .as_ref()
            .ok_or(StatusFileError::CannotRemoveUnknown)?
            .remove()?)
    }

    /// Consume the StatusFile to create a new StatusFile around a new RunnerStatus
    pub fn into_runner_status(self, new_status: &RunnerStatus) -> Result<Self, StatusFileError> {
        // rename the file and then create a new StatusFile with the renamed file
        let cur_file_path = self
            .0
            .ok_or_else(|| StatusFileError::CannotRenameUnknown)?
            .get_path()?;
        let run_dir = Directory::new(
            cur_file_path
                .parent()
                .ok_or(StatusFileError::UnableToGetRunDir)?,
        )?;
        let new_path = run_dir.as_path().join(
            new_status
                .as_filename()
                .ok_or(StatusFileError::CannotRenameUnknown)?,
        );

        std::fs::rename(cur_file_path, new_path);

        Self::new(new_status, &run_dir)
    }
}

impl std::fmt::Display for StatusFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(path) => write!(f, "{path}"),
            None => write!(f, "<Empty>"),
        }
    }
}

pub trait Runner {
    fn run(&self) -> Result<(), RunnerError>;
    fn status(&self) -> Result<RunnerStatus, RunnerError>;
}

/// Typical local runner
#[derive(Debug)]
pub struct LocalRunner {
    uid: RunnerId,
    run_exe: ExePath,
    run_args: Vec<ArgString>,
    run_dir: Directory,
}

impl LocalRunner {
    /// Creates a new runner with the arguments supplied and immediately attempts to update its status (as the job may
    /// already exist in some state)
    pub fn new(
        uid: RunnerId,
        run_exe: ExePath,
        run_args: Vec<ArgString>,
        run_dir: Directory,
    ) -> Result<Self, LocalRunnerError> {
        let new_runner = Self {
            uid,
            run_exe,
            run_args,
            run_dir,
        };

        Ok(new_runner)
    }

    /// Attempts to change status file.
    /// Note that the RunnerStatus should progress from Unknown->Uninitialized->Running->(Completed OR Error)
    fn change_status(&self, new_status: RunnerStatus) -> Result<StatusFile, LocalRunnerError> {
        let cur_status_file = self.get_active_statusfile()?;
        let cur_status = cur_status_file.as_runner_status()?;

        // if nothing is there, assume unitialized and then create a new file state. Otherwise, rename
        match (&cur_status, &new_status) {
            (RunnerStatus::Unknown, RunnerStatus::Running)
            | (RunnerStatus::Running, RunnerStatus::Error)
            | (RunnerStatus::Running, RunnerStatus::Completed) => {
                Ok(cur_status_file.into_runner_status(&new_status)?)
            }

            _ => {
                return Err(LocalRunnerError::ChangeStatusAdvanceError(
                    cur_status, new_status,
                ));
            }
        }
    }

    /// Function which returns the active StatusFile, if any
    fn get_active_statusfile(&self) -> Result<StatusFile, LocalRunnerError> {
        Ok(StatusFile::find_active(&self.run_dir)?)
    }
}

impl Runner for LocalRunner {
    fn run(&self) -> Result<(), RunnerError> {

        // grab the status file and change it to running. hold and handle to it
        let status_file = self.change_status(RunnerStatus::Running)?;

        let runner_id = self.uid;

        // convert to path for clarity
        let run_dir = self.run_dir.as_path();

        // harcoded paths for now, possibly make them optional or tagged with the varstepid
        let std_out_path = run_dir.join("output");
        let std_out_file = File::create(std_out_path)
            .map_err(|e| LocalRunnerError::RunIoError { id: runner_id, e })?;

        let err_out_path = run_dir.join("error");
        let err_out_file = File::create(&err_out_path)
            .map_err(|e| LocalRunnerError::RunIoError { id: runner_id, e })?;

        // run detached so that we can still keep an updated heartbeat
        let heartbeat_interval = Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS); // 2 second intervals

        // build the command line step command and spawn
        let run_exe_path = self
            .run_exe
            .0
            .get_path()
            .map_err(LocalRunnerError::from)?
            .canonicalize()
            .map_err(|e| LocalRunnerError::RunIoError { id: runner_id, e })?;

        let run_args = self.run_args.iter().map(|x| x.clone().into_string());
        let mut child_handle = std::process::Command::new(run_exe_path)
            .args(run_args)
            .stdout(std_out_file)
            .stderr(err_out_file)
            .current_dir(run_dir.clone())
            .spawn()
            .map_err(|e| LocalRunnerError::RunIoError { id: runner_id, e })?;

        // keep the running non-stale to show it's being worked on
        let last_time = Instant::now();
        self.change_status(RunnerStatus::Running)?;
        loop {
            match child_handle.try_wait() {
                // Ok means it has exited
                Ok(Some(_status)) => {
                    self.change_status(RunnerStatus::Completed)?;
                    break;
                }

                // Err means it is not exited
                Ok(None) => {
                    // check the current time
                    let cur_time = Instant::now();
                    if cur_time - last_time > heartbeat_interval {
                        status_file.touch();
                    }
                    sleep(heartbeat_interval / 100); // target to be within 1/100th of the interval
                }
                Err(_) => {
                    self.change_status(RunnerStatus::Error)?;
                    Err(LocalRunnerError::RunFailed {
                        id: runner_id,
                        err_file: err_out_path.clone(),
                    })?
                }
            }
        }

        Ok(())
    }

    fn status(&self) -> Result<RunnerStatus, RunnerError> {
        // look for the appropriate status filenames in order
        Ok(self
            .get_active_statusfile()?
            .as_runner_status()
            .map_err(|e| LocalRunnerError::StatusFileError(e))?)
    }
}
