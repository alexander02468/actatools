// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use crate::study::{executionplan::StudyExecutionPlan, plan::VarStepId, runner::Runner};

pub struct StudyOrchestrator {
    runners: HashMap<VarStepId, Runner>,
}

impl StudyOrchestrator {

}
