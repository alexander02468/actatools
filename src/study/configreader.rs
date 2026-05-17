// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{io::Read, path::PathBuf};

use crate::{
    configparsing::{ParsedString, StringParseError, TemplatedString},
    paths::{Directory, FilePath, PathError},
    study::studyconfig::{ConfigStep, StudyConfiguration, StudySettings}, // studyconfig::{ConfigStep, StudyConfiguration},
};

pub const DEFAULT_RUN_DIR: &str = "run";
pub const DEFAULT_EVIDENCE_DIR: &str = "evidence";
pub const DEFAULT_SHARED_DIR: &str = "shared";

#[derive(Debug, thiserror::Error)]
pub enum ConfigFileError {
    #[error("missing required field `{0}`")]
    MissingRequiredField(&'static str),

    #[error("the value must be a string, if provided for field `{field_name}`")]
    FieldFormatIncorrectString { field_name: &'static str },

    #[error("the value must be a array, if provided for field `{field_name}`")]
    FieldFormatIncorrectArray { field_name: &'static str },

    #[error(transparent)]
    FilePathError(#[from] PathError),

    #[error(transparent)]
    StringParseError(#[from] StringParseError),

    #[error("failed to deserialize with TOML")]
    TomlDeserializationError(#[from] toml::de::Error),
}

#[derive(Debug, serde::Deserialize)]
struct RawStudyConfig {
    study_name: String,
    design_file: PathBuf,
    run_dir: Option<PathBuf>,
    shared_dir: Option<PathBuf>,
    evidence_dir: Option<PathBuf>,
    shared: Option<Vec<String>>,
    steps: Vec<RawConfigStep>,
}

impl RawStudyConfig {
    fn into_study_config(self) -> Result<StudyConfiguration, ConfigFileError> {
        // normalize and check each filepath
        let design_path = FilePath::new(self.design_file, None)?;

        let run_dir = match self.run_dir {
            Some(run_dir_inner) => Directory::new(run_dir_inner)?,
            None => Directory::new(DEFAULT_RUN_DIR)?,
        };

        let shared_dir = match self.shared_dir {
            Some(shared_dir_inner) => Directory::new(shared_dir_inner)?,
            None => Directory::new(DEFAULT_SHARED_DIR)?,
        };

        let evidence_dir = match self.evidence_dir {
            Some(evidence_dir_inner) => Directory::new(evidence_dir_inner)?,
            None => Directory::new(DEFAULT_EVIDENCE_DIR.to_owned())?,
        };

        // convert all the RawConfigSteps in ConfigStep
        let steps: Vec<ConfigStep> = self
            .steps
            .into_iter()
            .map(|x| x.into_config_step())
            .collect::<Result<Vec<ConfigStep>, ConfigFileError>>()?;

        let parsed_shared: Vec<ParsedString> = self
            .shared
            .unwrap_or_default()
            .into_iter()
            .map(|x| ParsedString::from_string(&x))
            .collect::<Result<Vec<_>, _>>()?;

        let shared = parsed_shared
            .into_iter()
            .map(|x| x.into_templated_string_with_context(""))
            .collect();

        let settings = StudySettings {
            name: self.study_name,
            design_path,
            run_dir,
            shared_dir,
            evidence_dir,
        };

        Ok(StudyConfiguration {
            settings,
            shared,
            steps,
        })
    }
}

#[derive(Debug, serde::Deserialize)]
struct RawConfigStep {
    name: String,
    run_exe: String,
    run_args: Option<Vec<String>>,
}

impl RawConfigStep {
    fn into_config_step(self) -> Result<ConfigStep, ConfigFileError> {
        // normalize/check the run_args
        let run_args_parsed = self
            .run_args
            .unwrap_or_default()
            .into_iter()
            .map(|x| ParsedString::from_string(&x))
            .collect::<Result<Vec<ParsedString>, StringParseError>>()?;

        let run_args = run_args_parsed
            .into_iter()
            .map(|x| x.into_templated_string_with_context(&self.name))
            .collect::<Vec<TemplatedString>>();

        // now the exe
        let run_exe_raw = ParsedString::from_string(&self.run_exe)?;
        let run_exe = run_exe_raw.into_templated_string_with_context(&self.name);

        Ok(ConfigStep {
            name: self.name,
            run_args,
            run_exe,
        })
    }
}

pub struct ConfigReader;
impl ConfigReader {
    pub fn build_from_reader<R: Read>(
        reader: &mut R,
    ) -> Result<StudyConfiguration, ConfigFileError> {
        let mut string_buffer: String = String::new();
        reader.read_to_string(&mut string_buffer);
        Self::build_study_config(&string_buffer)
    }

    pub fn build_study_config(toml_str: &str) -> Result<StudyConfiguration, ConfigFileError> {
        let raw_study_config: RawStudyConfig = toml::from_str(toml_str)?;
        raw_study_config.into_study_config()
    }
}

#[cfg(test)]
mod test {

    fn good_root_toml() -> &'static str {
        r#"
    study_name = "Example"
    design_file = "./design.csv"
    run_dir = "./runs"
    evidence_dir = "./evidence"
    shared_dir = "./shared"
    shared = [
    "scripts/run_preprocess.sh",
    "solver_settings.json",
    "scripts/postprocess.sh",
]
"#
    }

    fn good_step_toml() -> &'static str {
        r#"[[steps]]
    name = "preprocess"
    run_exe = "/bin/bash"
    run_args = ["{shared}/scripts/preprocess.sh",
                "{variables.sleep_time}",        
                "{steps.self}/subject.mesh"]
        
        "#
    }

    use super::*;

    #[test]
    fn test_into_study_config() {
        let full_toml = [good_root_toml(), good_step_toml()].concat();
        let study_config = ConfigReader::build_study_config(&full_toml).unwrap();
        assert_eq!(study_config.settings.name, "Example");
        assert_eq!(
            study_config.settings.design_path,
            FilePath::new("./design.csv", None).unwrap()
        );
        assert_eq!(
            study_config.settings.shared_dir,
            Directory::new("./shared").unwrap()
        );
        assert_eq!(
            study_config.settings.evidence_dir,
            Directory::new("./evidence").unwrap()
        );
        assert_eq!(
            study_config.settings.run_dir,
            Directory::new("./runs").unwrap()
        );

        assert_eq!(study_config.steps.len(), 1);
        assert_eq!(study_config.steps[0].name, "preprocess");
    }

    #[test]
    fn test_into_config_step() {
        let raw_config_step = RawConfigStep {
            name: "test".to_string(),
            run_exe: "test.exe".to_string(),
            run_args: Some(vec![
                "test.csv".to_string(),
                "{steps.self}/test.csv".to_string(),
            ]),
        };

        let config_step = raw_config_step.into_config_step().unwrap();

        // some basic accounting
        assert_eq!(config_step.name, "test".to_string());
        assert_eq!(config_step.run_args.len(), 2);
    }

    #[test]
    fn test_defaults() {
        let toml_front_str = r#"
    study_name = "Example"
    design_file = "./design.csv" 
"#;
        let toml_str = [toml_front_str, good_step_toml()].concat();
        let study_config = ConfigReader::build_study_config(&toml_str).unwrap();

        assert_eq!(
            study_config.settings.evidence_dir,
            Directory::new(DEFAULT_EVIDENCE_DIR).unwrap()
        );
        assert_eq!(
            study_config.settings.run_dir,
            Directory::new(DEFAULT_RUN_DIR).unwrap()
        );
        assert_eq!(
            study_config.settings.shared_dir,
            Directory::new(DEFAULT_SHARED_DIR).unwrap()
        );
    }

    #[test]
    fn test_missing_required_study_name() {
        let study_config_res = ConfigReader::build_study_config("");
        assert!(study_config_res.is_err());
    }

    #[test]
    fn test_incorrect_field_type() {
        let toml_front_str = r#"
    study_name = 5
    design_file = "./design.csv" 
"#;

        let toml_str = [toml_front_str, good_step_toml()].concat();
        let study_config_res = ConfigReader::build_study_config(&toml_str);
        assert!(study_config_res.is_err())
    }
}
