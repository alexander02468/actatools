// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file contains code related to paths, usually filepaths in both relative (complete/incomplete) and absolute

use serde::{Deserialize, Deserializer, Serialize};
use std::{
    error::Error,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    InvalidParentOfRoot,
    FilePathNeedsBaseDir,
    NewFilePathInvalidArguments,
    NotADirectory(PathBuf),
}

impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathError::InvalidParentOfRoot => write!(f, "Tried to access parent directory of root"),
            PathError::FilePathNeedsBaseDir => {
                write!(f, "FilePath is not complete; needs a base directory")
            }
            PathError::NewFilePathInvalidArguments => {
                write!(f, "FilePath::new had imcompatible arguments provided")
            }
            PathError::NotADirectory(path) => {
                write!(f, "FilePath {} is not a directory", path.to_string_lossy())
            }
        }
    }
}
impl Error for PathError {}

/// Holds only a directory; checked at construction
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directory(PathBuf);

impl Directory {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, PathError> {
        let path = path.into();

        if !path.is_dir() {
            return Err(PathError::NotADirectory(path));
        }

        Ok(Self(path))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Returns a directory for the local directory, "./"
    pub fn here() -> Directory {
        Directory(PathBuf::from("./"))
    }
}

/// Stores a filepath, explicitly differentiates between a relative path that is incomplete
/// and a relative path that has a relative_base_path defined or is absolute (complete).
#[derive(Debug, Clone)]
pub enum FilePath {
    Absolute(PathBuf),
    Relative {
        base_dir: Directory,
        relative: PathBuf,
    },
    RelativeIncomplete(PathBuf),
}
impl FilePath {
    /// Creates a new FilePath
    /// Uses the base_dir to complete, if provided
    pub fn new(path: &Path, base_dir: Option<Directory>) -> Result<Self, PathError> {
        match (path.is_absolute(), base_dir) {
            // absolute && base_directory provided --> for now return Error (trying to path parse would be brittle
            // and it's better to let the caller do the split explicitly)
            (true, Some(_)) => Err(PathError::NewFilePathInvalidArguments),

            // absolute, no base_dir
            (true, None) => Ok(Self::Absolute(path.to_path_buf())),

            // relative, no base dir
            (false, None) => Ok(Self::RelativeIncomplete(path.to_path_buf())),

            // relative, base dir
            (false, Some(base_dir)) => Ok(Self::Relative {
                base_dir,
                relative: path.to_path_buf(),
            }),
        }
    }

    /// Gets the likely relative path, either it forwards the relative path or uses the parent as the base
    pub fn get_base_dir_path(&self) -> Result<&Path, PathError> {
        match self {
            FilePath::Absolute(path_buf) => match path_buf.parent() {
                Some(parent_path) => Ok(parent_path),
                None => Err(PathError::InvalidParentOfRoot),
            },
            FilePath::Relative {
                base_dir,
                relative: _,
            } => Ok(base_dir.as_path()),
            FilePath::RelativeIncomplete(_) => Err(PathError::FilePathNeedsBaseDir),
        }
    }

    /// Returns a copy of the full path
    pub fn get_path(&self) -> Result<PathBuf, PathError> {
        match self {
            FilePath::Absolute(path_buf) => Ok(path_buf.to_path_buf()),
            FilePath::Relative { base_dir, relative } => Ok(base_dir.0.join(relative)),
            FilePath::RelativeIncomplete(_) => Err(PathError::FilePathNeedsBaseDir),
        }
    }

    /// If absolute, gets the full path. Otherwise, returns the relative. More useful as a Record Artifact
    pub fn get_path_compact(&self) -> Result<&Path, PathError> {
        match self {
            FilePath::Absolute(path_buf) => Ok(path_buf.as_path()),
            FilePath::Relative {
                base_dir: _,
                relative,
            } => Ok(relative.as_path()),
            FilePath::RelativeIncomplete(_) => Err(PathError::FilePathNeedsBaseDir),
        }
    }

    /// Convenience function that fills in the base_dir if needed, otherwise ignores
    /// The return here is always an owned complete path
    pub fn into_complete(self, base_dir: Directory) -> Self {
        match self {
            // just return itself
            Self::Absolute(_)
            | Self::Relative {
                base_dir: _,
                relative: _,
            } => self,

            // fill in and convert
            Self::RelativeIncomplete(relative) => Self::Relative { base_dir, relative },
        }
    }
}

impl Serialize for FilePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let path = self.get_path_compact().map_err(serde::ser::Error::custom)?;

        serializer.serialize_str(&path.to_string_lossy())
    }
}

impl<'de> Deserialize<'de> for FilePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        let path = PathBuf::from(raw);
        if path.is_absolute() {
            Ok(FilePath::Absolute(path))
        } else {
            Ok(FilePath::RelativeIncomplete(path))
        }
    }
}
