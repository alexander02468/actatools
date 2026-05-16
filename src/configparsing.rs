// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use thiserror;

#[derive(Debug, thiserror::Error)]
pub enum StringParseError {
    #[error("Incorrect format")]
    IncorrectFormatGeneral,

    #[error("Incorrect format with `{0}`")]
    IncorrectFormat(String),

    #[error("Incorrect format with `{0}`, and `{1}`")]
    IncorrectFormat2(String, String),

    #[error("Incorrect format with `{0}`, `{1}`, and `{2}`")]
    IncorrectFormat3(String, String, String),

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
    LocalStep(StepLoc), // still needs Step context
    Step { name: String, loc: StepLoc },
    StudyVariable(String),
    StudyShared,
}

impl ParsedPart {
    /// Creates a ParsedPart from a separated string part based on set key terms. These are not resolved or
    /// interpreted at all -- that is left to the TemplatedStringPart
    pub fn from_string_part(string_part: &str) -> Result<Self, StringParseError> {
        match string_part.split(".").collect::<Vec<_>>().as_slice() {
            [s1] => match s1 {
                &"inputs" => Ok(Self::LocalStep(StepLoc::Inputs)),
                &"outputs" => Ok(Self::LocalStep(StepLoc::Outputs)),
                &"shared" => Ok(Self::StudyShared),
                _ => Err(StringParseError::IncorrectFormat(s1.to_string())),
            },

            [s1, s2] => match (s1, s2) {
                (&"variables", s2) => Ok(Self::StudyVariable(String::from(*s2))),
                _ => Err(StringParseError::IncorrectFormat2(
                    s1.to_string(),
                    s2.to_string(),
                )),
            },

            [s1, s2, s3] => match (s1, s2, s3) {
                (&"steps", s2, &"inputs") => Ok(Self::Step {
                    name: String::from(*s2),
                    loc: StepLoc::Inputs,
                }),
                (&"steps", s2, &"outputs") => Ok(Self::Step {
                    name: String::from(*s2),
                    loc: StepLoc::Outputs,
                }),
                _ => Err(StringParseError::IncorrectFormat3(
                    s1.to_string(),
                    s2.to_string(),
                    s3.to_string(),
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
                    if opened == false {
                        Err(StringParseError::TemplatedStringClosedWithoutOpen)?
                    }

                    // flush everything between the brackets, tag as Variable part
                    let open_idx_clean = open_idx
                        .ok_or_else(|| StringParseError::TemplatedStringClosedWithoutOpen)?;
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
    /// It may be worthwhile to separate out the Step context later so that it is only supplied if needed, but the cost
    /// is relatively cheap to provide, just a little less clean from a code design POV
    pub fn into_templated_string_with_context(self, step_name: &str) -> TemplatedString {
        let mut parts: Vec<TemplatedStringPart> = Vec::with_capacity(self.parts.len());
        for parsed_part in self.parts {
            let template_part = match parsed_part {
                ParsedPart::Literal(s) => TemplatedStringPart::Literal(s.clone()),
                ParsedPart::LocalStep(step_loc) => TemplatedStringPart::Step {
                    name: String::from(step_name),
                    loc: step_loc.clone(),
                },
                ParsedPart::Step { name, loc } => TemplatedStringPart::Step {
                    name: String::from(name),
                    loc: loc.clone(),
                },
                ParsedPart::StudyVariable(v) => TemplatedStringPart::StudyVariable(String::from(v)),
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
    Step { name: String, loc: StepLoc },
    StudyShared,
    StudyVariable(String),
}

impl std::fmt::Display for TemplatedStringPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            TemplatedStringPart::Literal(s) => write!(f, "{s}"),
            TemplatedStringPart::Step {
                name,
                loc: StepLoc::Inputs,
            } => write!(f, "<steps.{name}.inputs>"),
            TemplatedStringPart::Step {
                name,
                loc: StepLoc::Outputs,
            } => write!(f, "<steps.{name}.outputs>"),
            TemplatedStringPart::StudyShared => write!(f, "<shared>"),
            TemplatedStringPart::StudyVariable(s) => write!(f, "<variable.{s}>"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub enum StepLoc {
    Inputs,
    Outputs,
}

#[derive(Debug, thiserror::Error)]
pub enum TemplatedStringError {
    #[error("Key `{0}` not found in context map")]
    MissingContextKey(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplatedString {
    pub parts: Vec<TemplatedStringPart>,
}

impl TemplatedString {
    /// creates a string using the context_map to map templated parts to realizations
    pub fn realize_to_string(
        &self,
        context_map: &HashMap<TemplatedStringPart, String>,
    ) -> Result<String, TemplatedStringError> {
        let mut out_str = String::new();

        for p in &self.parts {
            let str_to_add = match p {
                TemplatedStringPart::Literal(s) => s.as_str(),

                other => context_map
                    .get(p)
                    .ok_or_else(|| TemplatedStringError::MissingContextKey(other.to_string()))?
                    .as_str(),
            };

            out_str.push_str(str_to_add);
        }
        Ok(out_str)
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
    fn local_step_inputs() {
        let result = ParsedString::from_string("{inputs}").unwrap();
        assert_eq!(result.parts, [ParsedPart::LocalStep(StepLoc::Inputs)])
    }

    #[test]
    fn local_step_outputs() {
        let result = ParsedString::from_string("{outputs}").unwrap();
        assert_eq!(result.parts, [ParsedPart::LocalStep(StepLoc::Outputs)])
    }

    #[test]
    fn variable_foo() {
        let result = ParsedString::from_string("{variables.foo}").unwrap();
        assert_eq!(result.parts, [ParsedPart::StudyVariable("foo".to_string())])
    }

    #[test]
    fn step_foo_inputs() {
        let result = ParsedString::from_string("{steps.foo.inputs}").unwrap();
        assert_eq!(
            result.parts,
            [ParsedPart::Step {
                name: "foo".to_string(),
                loc: StepLoc::Inputs
            }]
        )
    }

    #[test]
    fn step_foo_outputs() {
        let result = ParsedString::from_string("{steps.foo.outputs}").unwrap();
        assert_eq!(
            result.parts,
            [ParsedPart::Step {
                name: "foo".to_string(),
                loc: StepLoc::Outputs
            }]
        )
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
