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
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::paths::{Directory, FilePath, PathError};
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
    pub fn new() -> Self {
        let record_entries: Vec<UnhashedRecordEntry> = Vec::new();
        Self { record_entries }
    }

    /// Adds an include by filepath to the exsting RecordIncludes.
    /// An optional relative base path allows for filepaths relative to different folders
    pub fn add_include(&mut self, file: FilePath) -> () {
        let new_record_entry = UnhashedRecordEntry { file };
        self.record_entries.push(new_record_entry);
    }

    /// Extends the current includes with the files in an entire IncludesFile
    pub fn extend_includes_file(&mut self, includes_file: &FilePath) -> Result<(), Error> {
        let f = File::open(includes_file.get_path()?)?;
        let reader = BufReader::new(f);

        let base_dir = Directory::new(includes_file.get_base_dir_path()?)?;

        // Loop through each line, extract an incomplete string (via parse_line) and then complete with the base_dir
        // before added it into record_entries
        for line in reader.lines() {
            let line = line?; // String
            Self::parse_line(&line)?.map(|x| {
                self.record_entries.push(UnhashedRecordEntry {
                    file: x.into_complete(base_dir.clone()),
                })
            });
        }
        Ok(())
    }

    /// Parses a single line to get the path
    fn parse_line(line_string: &str) -> Result<Option<FilePath>, Error> {
        let strsplit = line_string.trim().split_once('#');

        let clean = match strsplit {
            Some((before, _)) => before.trim(),
            None => line_string,
        };

        if clean.is_empty() {
            return Ok(None);
        } else {
            let path_buf = PathBuf::from_str(clean)?;
            return Ok(Some(FilePath::RelativeIncomplete(path_buf)));
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
            let digest = hashed_record.data_digest;
            record_hasher.update(&digest.id);
            hashed_record_entries.push(hashed_record);
        }

        let digest = record_hasher.finalize();
        let digest: [u8; RECORD_ENTRY_LEN] =
            digest.as_bytes()[..RECORD_ENTRY_LEN].try_into().unwrap();

        Ok(Record {
            metadata: Some(RecordMetadata::current()?),
            record_entries: hashed_record_entries,
            digest: UidDigest { id: digest },
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnhashedRecordEntry {
    file: FilePath,
}
impl UnhashedRecordEntry {
    /// Use the hasher to hash the file it is pointing at. Necessary for making a HashedRecordEntry
    fn hash(&self) -> Result<UidDigest<RECORD_ENTRY_LEN>, Error> {
        let f = File::open(&self.file.get_path()?)?;
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
            data_digest: digest,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashedRecordEntry {
    pub file: FilePath,
    pub data_digest: UidDigest<RECORD_ENTRY_LEN>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    metadata: Option<RecordMetadata>,
    pub record_entries: Vec<HashedRecordEntry>,
    digest: UidDigest<RECORD_ENTRY_LEN>,
}

impl Record {
    pub fn render_to<W: Write>(&self, out: &mut W) -> Result<(), Error> {
        let json_self_string = self.as_json_string()?;
        write!(out, "{json_self_string}")?;
        writeln!(out, "")?;
        out.flush()?;

        Ok(())
    }

    /// Writes a JSON by converting to String then writing out
    pub fn write_json(&self, f_path: &Path) -> Result<(), Error> {
        let mut f = File::create(f_path)?;

        self.render_to(&mut f)?;
        Ok(())
    }

    /// Converts to JSON using Serialize
    pub fn as_json_string(&self) -> Result<String, Error> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Loads a JSON using Deserialize
    pub fn load_json(path: &Path) -> Result<Self, Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut record_file: Record = serde_json::from_reader(reader)?;

        // now fix the Filepaths if needed, to use the base directory of the file
        let base_dir = Directory::new(path.parent().ok_or(PathError::InvalidParentOfRoot)?)?;

        let fixed_record_entries: Vec<HashedRecordEntry> = record_file
            .record_entries
            .into_iter()
            .map(|x| HashedRecordEntry {
                file: x.file.into_complete(base_dir.clone()),
                data_digest: x.data_digest,
            })
            .collect();

        record_file.record_entries = fixed_record_entries;

        Ok(record_file)
    }

    /// Uses the Record to pull out filepaths and recalculate a new Record. Used in verification
    /// of an existing record
    pub fn recalculate_record(&self, base_dir: Directory) -> Result<Record, Error> {
        let mut new_record_entries: Vec<UnhashedRecordEntry> =
            Vec::with_capacity(self.record_entries.len());

        // loop through each existing record, complete the FilePath if needed and added a new unhashed version to
        // to the record_includes
        for old_record_entry in &self.record_entries {
            let file = old_record_entry
                .file
                .clone()
                .into_complete(base_dir.clone());
            let new_record_entry = UnhashedRecordEntry { file };
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
    pub meta_digest: UidDigest<32>,
}

impl RecordMetadata {
    pub fn current() -> Result<Self, Error> {
        let record_format = JSON_RECORD_FORMAT_VERSION;
        let generated_by = env!("CARGO_PKG_NAME").to_string();
        let library_version = env!("CARGO_PKG_VERSION").to_string();
        let digest_algorithm = "BLAKE3".to_string(); // Hardcoded, but will change to dynamic if SHA-256 is added
        let generated_at_utc = OffsetDateTime::now_utc().to_string();

        let mut meta_string: String = String::new();
        meta_string.push_str(&JSON_RECORD_FORMAT_VERSION.to_string());
        meta_string.push_str(" ");
        meta_string.push_str(&generated_by);
        meta_string.push_str(" ");
        meta_string.push_str(&library_version);
        meta_string.push_str(" ");
        meta_string.push_str(&digest_algorithm);
        meta_string.push_str(" ");
        meta_string.push_str(&generated_at_utc);
        let meta_digest = UidDigest::<32>::from_str_slice(&meta_string)?;

        Ok(Self {
            record_format,
            generated_by,
            library_version,
            digest_algorithm, // Hardcoded, but will change to dynamic if SHA-256 is added
            generated_at_utc,
            meta_digest,
        })
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
            .get_path()?
            .file_name()
            .ok_or_else(|| {
                anyhow!(
                    "Unable to get filename from {}",
                    unhashed_record_entry
                        .file
                        .get_path()
                        .unwrap()
                        .to_string_lossy() //file.get_path is already checked, unwrap is ok
                )
            })?
            .to_string_lossy()
            .to_string();
        writeln!(f_includes, "{file_from_name}")?;
        let file_from = &unhashed_record_entry.file.get_path()?;
        let file_to = &output_dir.join(file_from_name);

        std::fs::copy(file_from, file_to)?;
    }

    let includes_path = FilePath::Relative {
        base_dir: Directory::here(),
        relative: output_dir.join("manifest.includes"),
    };

    // use the new record_includes to make a manifest.json
    let mut record_includes = RecordIncludes::new();
    record_includes.extend_includes_file(&includes_path)?;
    let record = record_includes.into_record()?;

    record.write_json(&output_dir.join("manifest.json"))?;

    Ok(())
}

#[cfg(test)]
mod test_record_includes {

    use super::*;

    #[test]
    fn initialize() {
        let record = RecordIncludes::new();
        assert!(record.record_entries.is_empty()); // just make sure it's initalized and emtpy
    }

    #[test]
    fn add_include() {
        let mut record = RecordIncludes::new();
        let file = FilePath::RelativeIncomplete(PathBuf::from("foobar"));

        let _result = record.add_include(file);

        assert_eq!(
            record.record_entries[0].file,
            FilePath::RelativeIncomplete(PathBuf::from("foobar"))
        );
    }

    #[test]
    fn parse_line_normal() {
        let filepath = "foobar";
        let result = RecordIncludes::parse_line(filepath).unwrap().unwrap();

        assert_eq!(
            result,
            FilePath::RelativeIncomplete(PathBuf::from(filepath))
        );
    }

    #[test]
    fn parse_line_empty() {
        let filepath = "";
        let result = RecordIncludes::parse_line(filepath).unwrap();
        assert!(result.is_none())
    }

    #[test]
    fn parse_line_only_comment() {
        let filepath = "# foobar comment";
        let result = RecordIncludes::parse_line(filepath).unwrap();
        assert!(result.is_none())
    }

    #[test]
    fn parse_line_with_comment() {
        let filepath = "foobar # commented later";
        let result = RecordIncludes::parse_line(filepath).unwrap().unwrap();

        assert_eq!(
            result,
            FilePath::RelativeIncomplete(PathBuf::from("foobar"))
        );
    }
}

#[cfg(test)]
mod test_unhashed_record_entry {
    use std::path::absolute;

    use crate::{paths::FilePath, records::UnhashedRecordEntry};

    #[test]
    fn test_into_hash_record() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/foo.bar");
        let abs_path = absolute(path).unwrap();
        let file = FilePath::new(&abs_path, None).unwrap();
        let record_entry = UnhashedRecordEntry { file };
        let record_res = record_entry.into_hashed_record();

        assert!(record_res.is_ok());
    }
}

// Test record
#[cfg(test)]
mod test_record {
    use std::path::{PathBuf, absolute};

    use crate::{
        paths::{Directory, FilePath},
        records::RecordIncludes,
    };

    #[test]
    fn construction() {
        let mut record_includes = RecordIncludes::new();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/foo.bar");
        let abs_path = absolute(path).unwrap();
        let file = FilePath::new(&abs_path, None).unwrap();
        record_includes.add_include(file);
        let record_res = record_includes.into_record();
        let digest = record_res.unwrap().digest;
        let gold_digest_hex =
            "2ecb34d99efafac8531a93575adf640155b5c4650d7f530e669a59f146b252c0".to_string();
        let digest_str = digest.to_string();
        assert_eq!(gold_digest_hex, digest_str);
    }

    #[test]
    fn render_to() {
        let mut record_includes = RecordIncludes::new();
        let directory = Directory::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        let file =
            FilePath::new(&PathBuf::from("tests/fixtures/foo.bar"), Some(directory)).unwrap();
        record_includes.add_include(file);
        let mut record_res = record_includes.into_record().unwrap();
        record_res.metadata = None; // so we don't deal with timestamp differences

        let mut buffer: Vec<u8> = Vec::new();
        record_res.render_to(&mut buffer).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        let gold_string = "{\n  \"metadata\": null,\n  \"record_entries\": [\n    {\n      \"file\": \"tests/fixtures/foo.bar\",\n      \"data_digest\": \"9b61116853b99ee97b0ed5d499da7e486d77db52fbc60a2357e5cbf6183d418c\"\n    }\n  ],\n  \"digest\": \"2ecb34d99efafac8531a93575adf640155b5c4650d7f530e669a59f146b252c0\"\n}\n".to_string();

        assert_eq!(output, gold_string);
    }
}

#[cfg(test)]
mod test_record_metadata {

    use crate::records::{JSON_RECORD_FORMAT_VERSION, RecordMetadata};

    /// Does not currently test the time creation
    #[test]
    fn current() {
        let generated_by = env!("CARGO_PKG_NAME").to_string();
        let library_version = env!("CARGO_PKG_VERSION").to_string();
        let digest_algorithm = "BLAKE3".to_string(); // Hardcoded, but will change to dynamic if SHA-256 is added

        let current = RecordMetadata::current().unwrap();
        assert_eq!(current.record_format, JSON_RECORD_FORMAT_VERSION);
        assert_eq!(current.generated_by, generated_by);
        assert_eq!(current.library_version, library_version);
        // let now = OffsetDateTime::now_utc();
        assert_eq!(current.digest_algorithm, digest_algorithm)
    }
}
