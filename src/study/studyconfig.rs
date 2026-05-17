// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{
    configparsing::TemplatedString,
    paths::{Directory, FilePath},
    study,
};

/// Study configuration is the represention of the Study Configuration file
#[derive(Debug)]
pub struct StudyConfiguration {
    pub settings: StudySettings,
    pub shared: Vec<TemplatedString>,
    pub steps: Vec<ConfigStep>,
}

/// Struct that holds just the global settings for clarity when parsing to hold them temporarily until it's passed into the StudyConfiguration
#[derive(Debug)]
pub struct StudySettings {
    pub name: String,
    pub design_path: FilePath,
    pub run_dir: Directory,
    pub shared_dir: Directory,
    pub evidence_dir: Directory,
}

impl StudyConfiguration {
    pub fn into_study_planner(self) -> study::studyplan::StudyPlan {
        todo!()
    }
}

/// struct that holds the step information
#[derive(Debug)]
pub struct ConfigStep {
    pub name: String,
    pub run_args: Vec<TemplatedString>,
    pub run_exe: TemplatedString,
}
