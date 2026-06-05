// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::study::plan::VarStepId;

pub enum RunnerError {}

pub enum RunnerStatus {}

#[derive(Debug)]
pub enum Runner {
    Local(LocalRunner),
}

#[derive(Debug, Clone, Copy)]
pub struct RunnerId(VarStepId);

impl Runner {
    pub fn run(&mut self) -> Result<(), RunnerError> {
        match self {
            Runner::Local(local_runner) => local_runner.run(),
        }
    }

    pub fn status(&self) -> Result<RunnerStatus, RunnerError> {
        todo!()
    }
}

/// Typical local runner
#[derive(Debug)]
struct LocalRunner {
    uid: RunnerId,
}

impl LocalRunner {
    fn run(&mut self) -> Result<(), RunnerError> {
        todo!()
    }

    fn status(&self) -> Result<RunnerStatus, RunnerError> {
        todo!()
    }
}
