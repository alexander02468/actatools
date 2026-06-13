// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{collections::HashMap, path::PathBuf};
use thiserror;

use crate::{
    paths::{Directory, PathError},
    study::{
        configuration::ConfigStepName,
        design::{VariableName, VariableValue},
        plan::VarStepId,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum StringParseError {
    #[error("Incorrect format")]
    IncorrectFormatGeneral,

    #[error("Incorrect format with `{0}`")]
    IncorrectFormat(String),

    #[error("Incorrect format with `{0}`, and `{1}`")]
    IncorrectFormat2(String, String),

    #[error("brackets closed without being opened")]
    TemplatedStringClosedWithoutOpen,

    #[error("brackets were opened and never closed")]
    TemplatedStringLeftOpen,

    #[error("brackets were opened while already opened")]
    TemplatedStringOpenTwice,
}

#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub enum ParsedPart {
    Literal(String),
    LocalStep, // still needs Step context
    Step(String),
    StudyVariable(String),
    StudyShared,
}

impl ParsedPart {
    /// Creates a ParsedPart from a separated string part based on set key terms. These are not resolved or
    /// interpreted at all -- that is left to the TemplatedStringPart
    pub fn from_string_part(string_part: &str) -> Result<Self, StringParseError> {
        match string_part.split(".").collect::<Vec<_>>().as_slice() {
            [s1] => match s1 {
                &"shared" => Ok(Self::StudyShared),
                s => Err(StringParseError::IncorrectFormat(s.to_string())),
            },

            [s1, s2] => match (s1, s2) {
                (&"variables", s2) => Ok(Self::StudyVariable(String::from(*s2))),
                (&"steps", &"self") => Ok(Self::LocalStep),
                (&"steps", s2) => Ok(Self::Step(s2.to_string())),
                _ => Err(StringParseError::IncorrectFormat2(
                    s1.to_string(),
                    s2.to_string(),
                )),
            },

            _ => Err(StringParseError::IncorrectFormatGeneral),
        }
    }
}

/// Represents a parsed output that separates the string into literals and things that need to be replaced
#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub struct ParsedString {
    parts: Vec<ParsedPart>,
}

impl ParsedString {
    /// Creates a ParsedString from a string. This converts any {...} into a ParsedStringPart and any sections
    /// between as a ParsedStrinPart::Literal
    pub fn from_string(text: &str) -> Result<Self, StringParseError> {
        // loop through the string, when a "{" is found, wait for the next "}" and then extract its contents into a part
        // If the end is never reached, return an error.
        let mut parts: Vec<ParsedPart> = Vec::new();
        let mut closed_idx: usize = 0; // keeps track when it was closed
        let mut open_idx: Option<usize> = None; // keep track of the opening index
        let mut opened = false;

        for (i, c) in text.char_indices() {
            match c {
                '}' => {
                    // catch the case it was not opened
                    if !opened {
                        Err(StringParseError::TemplatedStringClosedWithoutOpen)?
                    }

                    // flush everything between the brackets, tag as Variable part
                    let open_idx_clean =
                        open_idx.ok_or(StringParseError::TemplatedStringClosedWithoutOpen)?;
                    let part_string = String::from(&text[open_idx_clean + 1..i]);

                    parts.push(ParsedPart::from_string_part(&part_string)?);

                    //  reset the open back to None
                    open_idx = None;

                    // remember when it is closed
                    closed_idx = i;

                    // track that it was closed
                    opened = false;
                }

                '{' => {
                    // catch the case that it was already opened
                    if opened {
                        Err(StringParseError::TemplatedStringOpenTwice)?
                    }

                    // flush everything before if this isn't the very start, tag as string literal
                    if i > 0 {
                        let part_string = String::from(&text[closed_idx + 1..i]);
                        parts.push(ParsedPart::Literal(part_string));
                    }
                    open_idx = Some(i);
                    opened = true;
                }

                _ => {} // do nothing
            }
        }

        // catch the case it was left open
        if opened {
            Err(StringParseError::TemplatedStringLeftOpen)?
        }

        // catch the case it doesn't end on a }
        if text.chars().last() != Some('}') {
            let part_string = match closed_idx {
                0 => String::from(&text[closed_idx..text.len()]), // catches case when there were no {
                _ => String::from(&text[closed_idx + 1..text.len()]),
            };

            parts.push(ParsedPart::Literal(part_string));
        }

        let parsed_output = Self { parts };
        Ok(parsed_output)
    }

    /// This function takes in a ParsedString to create the TemplatedString, adding Step context if needed
    pub fn into_templated_string_with_context(self, step_name: &ConfigStepName) -> TemplatedString {
        let mut parts: Vec<TemplatedStringPart> = Vec::with_capacity(self.parts.len());
        for parsed_part in self.parts {
            let template_part = match parsed_part {
                ParsedPart::Literal(s) => TemplatedStringPart::Literal(s.clone()),
                ParsedPart::LocalStep => TemplatedStringPart::Step(step_name.clone()),
                ParsedPart::Step(name) => TemplatedStringPart::Step(ConfigStepName::from(name)),
                ParsedPart::StudyVariable(v) => {
                    TemplatedStringPart::StudyVariable(VariableName::new(v))
                }
                ParsedPart::StudyShared => TemplatedStringPart::StudyShared,
            };

            parts.push(template_part)
        }

        TemplatedString { parts }
    }
}

/// All the possible parts that can be in the { }, already with full context
#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub enum TemplatedStringPart {
    Literal(String),
    Step(ConfigStepName),
    StudyShared,
    StudyVariable(VariableName),
}

impl std::fmt::Display for TemplatedStringPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            TemplatedStringPart::Literal(s) => write!(f, "{s}"),
            TemplatedStringPart::Step(name) => write!(f, "<steps.{name}.files>"),
            TemplatedStringPart::StudyShared => write!(f, "<shared>"),
            TemplatedStringPart::StudyVariable(s) => write!(f, "<variable.{s}>"),
        }
    }
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum TemplatedStringError {
    #[error("Key `{0}` not found in context map")]
    MissingContextKey(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplatedString {
    pub parts: Vec<TemplatedStringPart>,
}

impl TemplatedString {
    pub fn try_into_varstep_templated_string(
        self,
        varsteps: &HashMap<ConfigStepName, VarStepId>,
        variable_values: &HashMap<&VariableName, &VariableValue>,
    ) -> Result<VarStepTemplatedString, TemplatedStringError> {
        let mut parts: Vec<VarStepTemplatedStringPart> = Vec::with_capacity(self.parts.len());

        // move the templated string parts over except for the variable, replace that with context_map
        for part in self.parts {
            let varstep_part = match part {
                TemplatedStringPart::Literal(s) => VarStepTemplatedStringPart::Literal(s),
                TemplatedStringPart::Step(name) => {
                    let varstep_name = varsteps
                        .get(&name)
                        .ok_or_else(|| TemplatedStringError::MissingContextKey(name.to_string()))?;
                    VarStepTemplatedStringPart::Varstep(*varstep_name)
                }
                TemplatedStringPart::StudyVariable(v) => {
                    let varname = v;
                    let varvalue = *variable_values.get(&varname).ok_or_else(|| {
                        TemplatedStringError::MissingContextKey(varname.to_string())
                    })?;

                    VarStepTemplatedStringPart::Branch {
                        varname,
                        varvalue: varvalue.clone(),
                    }
                }
                TemplatedStringPart::StudyShared => VarStepTemplatedStringPart::StudyShared,
            };

            parts.push(varstep_part)
        }

        Ok(VarStepTemplatedString { parts })
    }
}

impl std::fmt::Display for TemplatedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out_str = String::new();
        for tp in &self.parts {
            out_str.push_str(&format!("{tp}"));
        }

        write!(f, "{out_str}")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VarStepTemplatedStringError {
    #[error("VarStep directory not found in lookup error")]
    VarstepDirLookupError,
}

#[derive(Debug, Clone)]
pub enum VarStepTemplatedStringPart {
    Literal(String),
    Varstep(VarStepId),
    StudyShared,
    Branch {
        varname: VariableName,
        varvalue: VariableValue,
    },
}

impl VarStepTemplatedStringPart {
    fn try_into_arg_string_part(
        self,
        varstep_dirs: &HashMap<VarStepId, Directory>,
        shared_dir: &Directory,
    ) -> Result<ArgStringPart, VarStepTemplatedStringError> {
        match self {
            VarStepTemplatedStringPart::Literal(s) => Ok(ArgStringPart::Literal(s)),
            VarStepTemplatedStringPart::Varstep(vsid) => Ok(ArgStringPart::Step {
                vsid,
                dir: varstep_dirs
                    .get(&vsid)
                    .ok_or(VarStepTemplatedStringError::VarstepDirLookupError)?
                    .clone(),
            }),
            VarStepTemplatedStringPart::StudyShared => {
                Ok(ArgStringPart::StudyShared(shared_dir.clone()))
            }
            VarStepTemplatedStringPart::Branch { varname, varvalue } => {
                let string = varvalue.to_string();
                Ok(ArgStringPart::StudyVariable {
                    varname,
                    varvalue,
                    string,
                })
            }
        }
    }
}

impl std::fmt::Display for VarStepTemplatedStringPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            VarStepTemplatedStringPart::Literal(s) => write!(f, "{s}"),
            VarStepTemplatedStringPart::Varstep(name) => write!(f, "<steps.{name}.files>"),
            VarStepTemplatedStringPart::StudyShared => write!(f, "<shared>"),
            VarStepTemplatedStringPart::Branch { varname, varvalue } => {
                write!(f, "<{varname}:{varvalue}>")
            }
        }
    }
}

/// Templated String in the VarStep with all variables realized (so not Variable template exists)
#[derive(Debug, Clone)]
pub struct VarStepTemplatedString {
    pub parts: Vec<VarStepTemplatedStringPart>,
}

/// consumes the VarStepTemplatedString and creates an ArgString, which has all strings realized
impl VarStepTemplatedString {
    pub fn try_into_arg_string(
        self,
        varstep_dirs: &HashMap<VarStepId, Directory>,
        shared_dir: &Directory,
    ) -> Result<ArgString, VarStepTemplatedStringError> {
        // loop through the parts, convert if needed to a ArgString(String) or a directory location
        let parts = self
            .parts
            .into_iter()
            .map(|x| x.try_into_arg_string_part(&varstep_dirs, &shared_dir))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ArgString::new(parts))
    }
}

/// Realized String Part that still holds the history of the string (e.g. was it resolved from something)
#[derive(Debug)]
pub enum ArgStringPart {
    Literal(String),
    Step {
        vsid: VarStepId,
        dir: Directory,
    },
    StudyShared(Directory),
    StudyVariable {
        varname: VariableName,
        varvalue: VariableValue,
        string: String,
    },
}

impl ArgStringPart {
    /// consumes into a string representation
    fn into_string(self) -> String {
        match self {
            ArgStringPart::Literal(s) => s,
            ArgStringPart::Step {
                vsid: _,
                dir: directory,
            } => directory
                .as_path()
                .as_os_str()
                .to_string_lossy()
                .to_string(),
            ArgStringPart::StudyShared(directory) => directory
                .as_path()
                .as_os_str()
                .to_string_lossy()
                .to_string(),
            ArgStringPart::StudyVariable {
                varname: _,
                varvalue: _,
                string,
            } => string,
        }
    }
}

/// Realized String that will be used as an argument. Comes from a TemplatedString
#[derive(Debug)]
pub struct ArgString {
    parts: Vec<ArgStringPart>,
}

impl ArgString {
    pub fn new(parts: Vec<ArgStringPart>) -> Self {
        Self { parts }
    }

    // consumes the ArgString to produce a realized String
    pub fn into_string(self) -> String {
        self.parts
            .into_iter()
            .map(|x| x.into_string())
            .collect::<Vec<String>>()
            .join("")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExePathError {
    #[error(transparent)]
    PathError(#[from] PathError),

    #[error(transparent)]
    StdIoError(#[from] std::io::Error),
}

/// holds a realized path that exists
#[derive(Debug)]
pub struct ExePath(PathBuf);

impl ExePath {
    pub fn try_from_path(f: impl Into<PathBuf>) -> Result<Self, ExePathError> {
        let p = f.into().canonicalize()?;
        Ok(Self(p))
    }
}

/// unit test cases for ParsedString, ParsedPart
#[cfg(test)]
mod tests_parsed_string {
    use super::*;

    #[test]
    fn literal() {
        let result = ParsedString::from_string("foo").unwrap();
        assert_eq!(result.parts, [ParsedPart::Literal("foo".to_string())])
    }

    #[test]
    fn shared() {
        let result = ParsedString::from_string("{shared}").unwrap();
        assert_eq!(result.parts, [ParsedPart::StudyShared])
    }

    #[test]
    fn variable_foo() {
        let result = ParsedString::from_string("{variables.foo}").unwrap();
        assert_eq!(result.parts, [ParsedPart::StudyVariable("foo".to_string())])
    }

    #[test]
    fn incorrect_keyword() {
        let result = ParsedString::from_string("{step}"); // step instead of steps --> should error
        assert!(result.is_err())
    }

    #[test]
    fn open_while_opened() {
        let result = ParsedString::from_string("{fo{}");
        assert!(result.is_err())
    }

    #[test]
    fn no_close() {
        let result = ParsedString::from_string("foo{bar");
        assert!(result.is_err())
    }

    #[test]
    fn no_open() {
        let result = ParsedString::from_string("foo}bar");
        assert!(result.is_err())
    }
}

// Unit tests for TemplatedString, TemplatedStringPart
// #[cfg(test)]
// mod tests_templated_string {
//     use super::*;
// }
