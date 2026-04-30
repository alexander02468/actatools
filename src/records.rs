// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file contains code related to Records which is for evidence packaging

use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Error, anyhow};
use serde::{Deserialize, Deserializer, Serialize};
use time::OffsetDateTime;

use crate::uid::UidDigest;

const RECORD_ENTRY_LEN: usize = 32;

const JSON_RECORD_FORMAT_VERSION: usize = 1;

/// Holds the parsed data in the record.includes file. Can be thought of a constructor template for a Record
/// (which has the files hashed as well)
#[derive(Debug, Clone)]
pub struct RecordIncludes {
    record_entries: Vec<UnhashedRecordEntry>,
}

impl RecordIncludes {
    /// Parses an entire IncludesFile to make a RecordIncludes
    pub fn parse_includes_file(includes_file: &Path) -> Result<Self, Error> {
        let f = File::open(includes_file)?;
        let reader = BufReader::new(f);

        let mut record_entries: Vec<UnhashedRecordEntry> = Vec::new();

        for line in reader.lines() {
            let line = line?; // String
            match Self::parse_line(&line)? {
                Some(incomplete_file) => {
                    let complete_file = incomplete_file.complete(
                        &includes_file
                            .parent()
                            .ok_or_else(|| {
                                anyhow!("Unable to access parent directory, is this a file?")
                            })?
                            .to_path_buf(),
                    );
                    record_entries.push(UnhashedRecordEntry {
                        file: complete_file,
                    });
                }

                None => {}
            }
        }
        Ok(Self { record_entries })
    }

    /// Parses a single line to get the path
    fn parse_line(line_string: &str) -> Result<Option<FilePathIncomplete>, Error> {
        let strsplit = line_string.trim().split_once('#');

        let clean = match strsplit {
            Some((before, _)) => before.trim(),
            None => line_string,
        };

        if clean.is_empty() {
            return Ok(None);
        } else {
            let path_buf = PathBuf::from_str(clean)?;
            return Ok(Some(FilePathIncomplete::new(&path_buf)));
        }
    }

    /// Consumes the RecordIncludes to make a Record
    pub fn into_record(self) -> Result<Record, Error> {
        // make sure each record is hashed if it is not already. Keep a running hash and save a
        // the end to Record.digest

        let mut hashed_record_entries: Vec<HashedRecordEntry> =
            Vec::with_capacity(self.record_entries.len());
        let mut record_hasher = blake3::Hasher::new();

        for record_entry in self.record_entries.into_iter() {
            let hashed_record = record_entry.into_hashed_record()?;
            let digest = hashed_record.digest;
            record_hasher.update(&digest.id);
            hashed_record_entries.push(hashed_record);
        }

        let digest = record_hasher.finalize();
        let digest: [u8; RECORD_ENTRY_LEN] =
            digest.as_bytes()[..RECORD_ENTRY_LEN].try_into().unwrap();

        Ok(Record {
            metadata: Some(RecordMetadata::current()),
            record_entries: hashed_record_entries,
            digest: UidDigest { id: digest },
        })
    }
}

/// Stores incomplete file paths, need to add a base path
#[derive(Debug, Clone, Serialize)]
pub enum FilePathIncomplete {
    Absolute(PathBuf),
    RelativeNeedsContext(PathBuf),
}
impl FilePathIncomplete {
    pub fn new(path: &Path) -> Self {
        match path.is_absolute() {
            true => Self::Absolute(path.to_path_buf()),
            false => Self::RelativeNeedsContext(path.to_path_buf()),
        }
    }

    pub fn complete(self, base_path: &Path) -> FilePath {
        match self {
            FilePathIncomplete::Absolute(path_buf) => FilePath::Absolute(path_buf),
            FilePathIncomplete::RelativeNeedsContext(path_buf) => FilePath::Relative {
                base: base_path.to_path_buf(),
                relative: path_buf,
            },
        }
    }
}

/// Stores a complete filepath
#[derive(Debug, Clone)]
pub enum FilePath {
    Absolute(PathBuf),
    Relative { base: PathBuf, relative: PathBuf },
}
impl FilePath {
    /// Gets the full path
    pub fn get_path(&self) -> PathBuf {
        match self {
            FilePath::Absolute(path_buf) => path_buf.to_path_buf(),
            FilePath::Relative { base, relative } => base.join(relative).to_path_buf(),
        }
    }
    /// If absolute, gets the full path. Otherwise, returns the relative. More useful as a Record Artifact
    pub fn get_path_compact(&self) -> PathBuf {
        match self {
            FilePath::Absolute(path_buf) => path_buf.to_path_buf(),
            FilePath::Relative { base: _, relative } => relative.to_path_buf(),
        }
    }
}

impl Serialize for FilePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.get_path_compact().to_string_lossy().to_string())
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
            // this is dirty but leave the base empty. Needs to be filled in immediately
            Ok(FilePath::Relative {
                base: PathBuf::from(""),
                relative: path,
            })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnhashedRecordEntry {
    file: FilePath,
}
impl UnhashedRecordEntry {
    /// Use the hasher to hash the file it is pointing at. Necessary for making a HashedRecordEntry
    fn hash(&self) -> Result<UidDigest<RECORD_ENTRY_LEN>, Error> {
        let f = File::open(&self.file.get_path())?;
        let mut hasher = blake3::Hasher::new();
        let mut reader = BufReader::new(f);
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let bytes_read = reader.read(&mut buffer)?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
        }
        let digest: [u8; RECORD_ENTRY_LEN] =
            hasher.finalize().as_bytes()[..RECORD_ENTRY_LEN].try_into()?;
        Ok(UidDigest::<RECORD_ENTRY_LEN> { id: digest }) // 32 bytes
    }

    /// Converts into a HashRecordEntry, consumes the UnhashedRecordEntry
    fn into_hashed_record(self) -> Result<HashedRecordEntry, Error> {
        let digest = self.hash()?;
        Ok(HashedRecordEntry {
            file: self.file,
            digest,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashedRecordEntry {
    pub file: FilePath,
    pub digest: UidDigest<RECORD_ENTRY_LEN>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    metadata: Option<RecordMetadata>,
    pub record_entries: Vec<HashedRecordEntry>,
    digest: UidDigest<RECORD_ENTRY_LEN>,
}

impl Record {
    /// Writes a JSON by converting to String then writing out
    pub fn write_json(&self, f_path: &Path) -> Result<(), Error> {
        let mut f = File::create(f_path)?;

        let json_self_string = self.as_json_string()?;
        write!(f, "{json_self_string}")?;
        Ok(())
    }

    /// Converts to JSON using Serialize
    pub fn as_json_string(&self) -> Result<String, Error> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Loads a JSON using Deserialize
    pub fn load_json(path: &str) -> Result<Self, Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let record_file = serde_json::from_reader(reader)?;

        Ok(record_file)
    }

    /// Uses the Record to pull out filepaths and recalculate a new Record. Used in verification
    /// of an existing record
    pub fn recalculate_record(&self, record_base_path: &Path) -> Result<Record, Error> {
        let mut new_record_entries: Vec<UnhashedRecordEntry> =
            Vec::with_capacity(self.record_entries.len());

        for existing_record_entry in &self.record_entries {
            // we need to check what the old file was and inject the new location if filepath is not absolute
            let new_filepath = match &existing_record_entry.file {
                FilePath::Absolute(path_buf) => FilePath::Absolute(path_buf.clone()),
                FilePath::Relative { base: _, relative } => FilePath::Relative {
                    base: record_base_path.to_path_buf(),
                    relative: relative.clone(),
                },
            };

            let new_record_entry = UnhashedRecordEntry { file: new_filepath };
            new_record_entries.push(new_record_entry);
        }

        let record_includes = RecordIncludes {
            record_entries: new_record_entries,
        };

        Ok(record_includes.into_record()?)
    }
}

/// Metadata to attach to a Record, optional, but usually generate when constructing the Record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordMetadata {
    pub record_format: usize,
    pub generated_by: String,
    pub library_version: String,
    pub digest_algorithm: String,
    pub generated_at_utc: String,
}

impl RecordMetadata {
    pub fn current() -> Self {
        Self {
            record_format: JSON_RECORD_FORMAT_VERSION,
            generated_by: env!("CARGO_PKG_NAME").to_string(),
            library_version: env!("CARGO_PKG_VERSION").to_string(),
            digest_algorithm: "BLAKE3".to_string(), // Hardcoded, but will change to dynamic if SHA-256 is added
            generated_at_utc: OffsetDateTime::now_utc().to_string(),
        }
    }
}

/// Copies all the files in the RecordIncludes to output_directory, writing a new record_includes + manifest.json
pub fn bundle(record_includes: RecordIncludes, output_directory: &Path) -> Result<(), Error> {
    // create the directory if it does not exist, do not allow for super nested directories to be autocreated
    let output_dir = PathBuf::from(output_directory);
    if output_dir.exists() == false {
        fs::create_dir(&output_dir)?;
    }

    let mut f_includes = File::create(&output_dir.join("manifest.includes"))?;
    writeln!(f_includes, "# Autogenerated manifest includes")?;

    // copy each file to the output directory
    // add to a list of filepaths that will make up a new record_includes
    for unhashed_record_entry in record_includes.record_entries {
        let file_from_name = unhashed_record_entry
            .file
            .get_path()
            .file_name()
            .ok_or_else(|| {
                anyhow!(
                    "Unable to get filename from {}",
                    unhashed_record_entry.file.get_path().to_string_lossy()
                )
            })?
            .to_string_lossy()
            .to_string();
        writeln!(f_includes, "{file_from_name}")?;
        let file_from = &unhashed_record_entry.file.get_path();
        let file_to = &output_dir.join(file_from_name);

        std::fs::copy(file_from, file_to)?;
    }

    // use the new record_includes to make a manifest.json
    let record_includes =
        RecordIncludes::parse_includes_file(&output_dir.join("manifest.includes"))?;
    let record = record_includes.into_record()?;
    record.write_json(&output_dir.join("manifest.json"))?;
    Ok(())
}
