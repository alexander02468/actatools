// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;

use crate::{
    paths::{Directory, FilePath},
    study::{self, design::VariableName, templatedstring::TemplatedString},
};

#[derive(Debug, thiserror::Error)]
pub enum StudyConfigurationError {
    #[error("Referenced step definitions missing: {}", .0.join(", "))]
    StepReferenceVerificationFail(Vec<String>),
}

/// Study configuration is the represention of the Study Configuration file with Parse checks.
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
    // Checks that all referenced steps are defined
    pub fn check_step_references(&self) -> StepReferenceCheckResult {
        check_steps(&self.steps)
    }
}

/// checks for internal agreement of the steps (are all of the steps that referenced defined in here)
fn check_steps(steps: &Vec<ConfigStep>) -> StepReferenceCheckResult {
    let step_names: HashSet<ConfigStepName> = steps.iter().map(|x| x.name.clone()).collect();
    let step_references: HashSet<ConfigStepName> = ConfigStep::collect_references(steps);

    check_step_references(&step_references, &step_names)
}

/// checks if step_references_b are in step_references a, and returns any that are missing
fn check_step_references(
    steps_references_a: &HashSet<ConfigStepName>,
    steps_references_b: &HashSet<ConfigStepName>,
) -> StepReferenceCheckResult {
    let missing: Vec<ConfigStepName> = steps_references_a
        .difference(&steps_references_b)
        .cloned()
        .collect();

    match missing.len() {
        0 => StepReferenceCheckResult::Pass,
        _ => StepReferenceCheckResult::Fail(missing.into_iter().map(|x| x.to_string()).collect()),
    }
}

#[derive(Debug)]
pub enum StepReferenceCheckResult {
    Pass,
    Fail(Vec<String>),
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct ConfigStepName(String);
impl ConfigStepName {
    pub fn from(name: impl Into<String>) -> Self {
        Self(name.into())
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}

impl std::fmt::Display for ConfigStepName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_str())
    }
}

/// struct that holds the step information
#[derive(Debug, Clone)]
pub struct ConfigStep {
    pub name: ConfigStepName,
    pub run_args: Vec<TemplatedString>,
    pub run_exe: TemplatedString,
}

impl ConfigStep {
    /// Gets all the referenced steps as strings in this Step
    pub fn get_referenced_steps(&self) -> HashSet<ConfigStepName> {
        let mut referenced_steps: HashSet<ConfigStepName> = HashSet::new();

        // look for the TemplatedStringPart through all the run_args
        for arg in &self.run_args {
            for p in &arg.parts {
                match p {
                    crate::study::templatedstring::TemplatedStringPart::Step(s) => {
                        referenced_steps.insert(s.clone());
                    }
                    _ => {}
                }
            }
        }
        referenced_steps
    }

    /// Gets the referenced steps minus the own step (as it is not dependent on it)
    pub fn get_dependent_steps(&self) -> HashSet<ConfigStepName> {
        let mut referenced_steps = self.get_referenced_steps();
        referenced_steps.remove(&self.name);

        referenced_steps
    }

    pub fn get_referenced_variables(&self) -> HashSet<VariableName> {
        let mut referenced_variables: HashSet<VariableName> = HashSet::new();

        // look for the TemplatedStringPart through all the run_args
        for arg in &self.run_args {
            for p in &arg.parts {
                match p {
                    crate::study::templatedstring::TemplatedStringPart::StudyVariable(v) => {
                        referenced_variables.insert(v.clone());
                    }
                    _ => {}
                }
            }
        }
        referenced_variables
    }

    pub fn collect_references<'a, S>(steps: S) -> HashSet<ConfigStepName>
    where
        S: IntoIterator<Item = &'a ConfigStep>,
    {
        let mut references: HashSet<ConfigStepName> = HashSet::new();
        for step in steps {
            references.extend(step.get_referenced_steps())
        }

        references
    }
}

#[cfg(test)]
mod test_study_config {
    use std::{collections::HashSet, vec};

    use super::*;

    use crate::study::{
        configuration::{ConfigStep, check_steps},
        templatedstring::ParsedString,
    };

    /// Step 1 needs 2
    fn build_step1() -> ConfigStep {
        ConfigStep {
            name: ConfigStepName::from("test1"),
            run_args: vec![
                ParsedString::from_string("{steps.self}/test.csv")
                    .unwrap()
                    .into_templated_string_with_context(&ConfigStepName::from("test1")),
                ParsedString::from_string("{steps.test2}/test.csv")
                    .unwrap()
                    .into_templated_string_with_context(&ConfigStepName::from("test1")),
            ],
            run_exe: ParsedString::from_string("test.exe")
                .unwrap()
                .into_templated_string_with_context(&ConfigStepName::from("step_name")),
        }
    }

    /// Step 2 needs 1
    fn build_step2() -> ConfigStep {
        ConfigStep {
            name: ConfigStepName::from("test2"),
            run_args: vec![
                ParsedString::from_string("{steps.self}/test.csv")
                    .unwrap()
                    .into_templated_string_with_context(&ConfigStepName::from("test2")),
                ParsedString::from_string("{steps.test1}/test.csv")
                    .unwrap()
                    .into_templated_string_with_context(&ConfigStepName::from("test2")),
            ],
            run_exe: ParsedString::from_string("test.exe")
                .unwrap()
                .into_templated_string_with_context(&ConfigStepName::from("step_name")),
        }
    }

    /// Step3 needs 1 + 2
    fn build_step3() -> ConfigStep {
        ConfigStep {
            name: ConfigStepName::from("test3"),
            run_args: vec![
                ParsedString::from_string("{steps.test2}/test.csv")
                    .unwrap()
                    .into_templated_string_with_context(&ConfigStepName::from("test2")),
                ParsedString::from_string("{steps.test1}/test.csv")
                    .unwrap()
                    .into_templated_string_with_context(&ConfigStepName::from("test2")),
            ],
            run_exe: ParsedString::from_string("test.exe")
                .unwrap()
                .into_templated_string_with_context(&ConfigStepName::from("step_name")),
        }
    }

    #[test]
    fn test_check_step_references() {
        let steps = vec![build_step1(), build_step2(), build_step3()];
        match check_steps(&steps) {
            super::StepReferenceCheckResult::Pass => assert!(true),
            super::StepReferenceCheckResult::Fail(items) => {
                dbg!(&items);

                assert!(false)
            }
        }
    }

    #[test]
    fn test_missing_references() {
        let steps = vec![build_step1(), build_step3()];

        match check_steps(&steps) {
            super::StepReferenceCheckResult::Pass => assert!(false),
            super::StepReferenceCheckResult::Fail(_) => assert!(true),
        }
    }

    #[test]
    fn test_get_referenced_steps() {
        let step1 = build_step1();
        let referenced_steps = step1.get_referenced_steps();
        let actual_step_references: HashSet<ConfigStepName> = HashSet::from_iter(
            vec![ConfigStepName::from("test2"), ConfigStepName::from("test1")].into_iter(),
        );

        assert_eq!(referenced_steps, actual_step_references);
    }
}
