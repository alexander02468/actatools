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

    #[error(transparent)]
    TomlDeserializationError(#[from] toml::de::Error),
}

#[derive(Debug, serde::Deserialize)]
struct RawStudyConfig {
    name: String,
    design_path: PathBuf,
    run_dir: Option<PathBuf>,
    shared_dir: Option<PathBuf>,
    evidence_dir: Option<PathBuf>,
    steps: Vec<RawConfigStep>,
}

impl RawStudyConfig {
    fn into_study_config(self) -> Result<StudyConfiguration, ConfigFileError> {
        // normalize and check each filepath
        let design_path = FilePath::new(self.design_path, None)?;

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

        let settings = StudySettings {
            name: self.name,
            design_path,
            run_dir,
            shared_dir,
            evidence_dir,
        };

        Ok(StudyConfiguration { settings, steps })
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

    use super::*;
}
