// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::study::{configuration::StudyConfiguration, design::StudyDesign, plan::StudyPlan};

#[derive(Debug)]
pub struct StudyPlanBuilder;

impl StudyPlanBuilder {
    pub fn build_study_plan(study_config: StudyConfiguration, design: StudyDesign) -> StudyPlan {
        todo!()
    }
}
