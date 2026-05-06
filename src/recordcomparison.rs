// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file contains code related to Comparing Records which is for evidence packaging

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::io::Write;

use anyhow::Error;

use crate::records::HashedRecordEntry;
#[derive(Debug)]
pub struct MatchEngine {
    pub extractors: Vec<Box<dyn KeyExtractStrategy>>,
}
impl MatchEngine {
    /// empty MatchEngine
    pub fn new() -> Self {
        let extractors: Vec<Box<dyn KeyExtractStrategy>> = Vec::new();
        MatchEngine { extractors }
    }

    /// adds a filename extractor
    pub fn with_filename_extractor(self) -> Self {
        let mut extractors = self.extractors;
        extractors.push(Box::new(KeyExtractFilename));

        Self { extractors }
    }

    /// Matches records according to the extractor method
    pub fn match_record_entries<'a>(
        &self,
        record_entries_before: &'a Vec<&'a HashedRecordEntry>,
        record_entries_after: &'a Vec<&'a HashedRecordEntry>,
    ) -> Vec<MatchResult<'a>> {
        // create an owned copy of the vector,
        let record_entries_before: Vec<&HashedRecordEntry> = record_entries_before.to_vec();
        let record_entries_after: Vec<&HashedRecordEntry> = record_entries_after.to_vec();

        let grouped_keys_before = group_by_key(record_entries_before, self.extractors[0].as_ref());
        let grouped_keys_after = group_by_key(record_entries_after, self.extractors[0].as_ref());

        let all_keys: BTreeSet<ExtractedKey> = grouped_keys_before
            .groups
            .keys()
            .chain(grouped_keys_after.groups.keys())
            .cloned()
            .collect();

        // loop through all the keys, push the match results
        let mut match_results: Vec<MatchResult> = Vec::with_capacity(all_keys.len());
        for k in all_keys {
            let before_matches = grouped_keys_before
                .groups
                .get(&k)
                .cloned()
                .unwrap_or_default();

            let after_matches = grouped_keys_after
                .groups
                .get(&k)
                .cloned()
                .unwrap_or_default();

            match (before_matches.len(), after_matches.len()) {
                (1, 1) => match_results.push(MatchResult::Matched {
                    before: before_matches[0],
                    after: after_matches[0],
                    key: k,
                }),
                (_, 0) => {
                    for i in before_matches {
                        match_results.push(MatchResult::Removed {
                            before: i,
                            key: k.clone(),
                        })
                    }
                }
                (0, _) => {
                    for i in after_matches {
                        match_results.push(MatchResult::Added {
                            after: i,
                            key: k.clone(),
                        })
                    }
                }
                _ => {
                    // add them both

                    match_results.push(MatchResult::Ambiguous {
                        before_candidates: before_matches,
                        after_candidates: after_matches,
                        key: k.clone(),
                    });

                    //also the ungrouped ones
                    match_results.push(MatchResult::Ambiguous {
                        before_candidates: grouped_keys_before.ungrouped.iter().cloned().collect(),
                        after_candidates: grouped_keys_after.ungrouped.iter().cloned().collect(),
                        key: k.clone(),
                    })
                }
            }
        }

        match_results
    }
}

pub trait KeyExtractStrategy: Debug {
    fn extract_key<'r>(&self, record_entry: &'r HashedRecordEntry) -> Option<ExtractedKey>;
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum ExtractedKey {
    Filename(String),
}
impl std::fmt::Display for ExtractedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractedKey::Filename(value) => {
                write!(f, "filename = {}", value)
            }
        }
    }
}

/// Use the Filename as the extraction
#[derive(Debug, Clone)]
pub struct KeyExtractFilename;
impl KeyExtractStrategy for KeyExtractFilename {
    fn extract_key<'r>(&self, record_entry: &'r HashedRecordEntry) -> Option<ExtractedKey> {
        let k = record_entry.file.get_filename().ok();
        k.map(|x| ExtractedKey::Filename(x))
    }
}

#[derive(Debug, Clone)]
pub struct GroupedHashedRecordEntries<'a> {
    groups: BTreeMap<ExtractedKey, Vec<&'a HashedRecordEntry>>,
    ungrouped: Vec<&'a HashedRecordEntry>,
}

/// This function uses a group of entries and groups them by a key as dictated by the input
/// KeyExtractStrategy
fn group_by_key<'a>(
    items: Vec<&'a HashedRecordEntry>,
    extractor: &dyn KeyExtractStrategy,
) -> GroupedHashedRecordEntries<'a> {
    let mut groups: BTreeMap<ExtractedKey, Vec<&HashedRecordEntry>> = BTreeMap::new();
    let mut ungrouped: Vec<&HashedRecordEntry> = Vec::new();

    for item in items {
        match extractor.extract_key(item) {
            Some(key) => {
                groups.entry(key).or_insert_with(Vec::new).push(item);
            }
            None => {
                ungrouped.push(item);
            }
        }
    }
    GroupedHashedRecordEntries { groups, ungrouped }
}

#[derive(Debug, Clone)]
pub enum MatchResult<'a> {
    Matched {
        before: &'a HashedRecordEntry,
        after: &'a HashedRecordEntry,
        key: ExtractedKey,
    },

    Added {
        after: &'a HashedRecordEntry,
        key: ExtractedKey,
    },

    Removed {
        before: &'a HashedRecordEntry,
        key: ExtractedKey,
    },

    Ambiguous {
        before_candidates: Vec<&'a HashedRecordEntry>,
        after_candidates: Vec<&'a HashedRecordEntry>,
        key: ExtractedKey,
    },
}

impl<'a> MatchResult<'a> {
    /// simple function that returns whether or not the Match is a good match
    pub fn is_matched(&self) -> bool {
        match self {
            MatchResult::Matched {
                before: _,
                after: _,
                key: _,
            } => true,
            MatchResult::Added { after: _, key: _ } => false,
            MatchResult::Removed { before: _, key: _ } => false,
            MatchResult::Ambiguous {
                before_candidates: _,
                after_candidates: _,
                key: _,
            } => false,
        }
    }
}

pub struct DiffEngine;

impl DiffEngine {
    /// Thsi function compares the digest of each record
    fn diff_record_matched_comparison<'a>(
        before: &'a HashedRecordEntry,
        after: &'a HashedRecordEntry,
        key: ExtractedKey,
    ) -> RecordDiff<'a> {
        if before.data_digest == after.data_digest {
            RecordDiff::NoChange { before, after, key }
        } else {
            RecordDiff::HashChange { before, after, key }
        }
    }

    /// This function consumes a MatchResult, returning a RecordDiff
    pub fn diff_record<'a>(m: MatchResult<'a>) -> RecordDiff<'a> {
        match m {
            MatchResult::Matched { before, after, key } => {
                Self::diff_record_matched_comparison(before, after, key)
            }
            MatchResult::Added { after, key } => RecordDiff::Added { after, key },
            MatchResult::Removed { before, key } => RecordDiff::Removed { before, key },
            MatchResult::Ambiguous {
                before_candidates,
                after_candidates,
                key,
            } => RecordDiff::Undetermined {
                before: before_candidates,
                after: after_candidates,
                key,
            },
        }
    }

    pub fn diff_matches<'a>(matches: Vec<MatchResult<'a>>) -> Vec<RecordDiff<'a>> {
        matches.into_iter().map(|x| Self::diff_record(x)).collect()
    }
}

pub enum RecordDiff<'a> {
    NoChange {
        before: &'a HashedRecordEntry,
        after: &'a HashedRecordEntry,
        key: ExtractedKey,
    },
    HashChange {
        before: &'a HashedRecordEntry,
        after: &'a HashedRecordEntry,
        key: ExtractedKey,
    },
    Added {
        after: &'a HashedRecordEntry,
        key: ExtractedKey,
    },
    Removed {
        before: &'a HashedRecordEntry,
        key: ExtractedKey,
    },
    Undetermined {
        before: Vec<&'a HashedRecordEntry>,
        after: Vec<&'a HashedRecordEntry>,
        key: ExtractedKey,
    },
}

struct DifferenceSummary {
    num_same: usize,
    num_changed: usize,
    num_added: usize,
    num_removed: usize,
    num_undetermined_before: usize,
    num_undetermined_after: usize,
}

impl DifferenceSummary {
    fn from_record_diffs<'a>(record_diffs: &Vec<RecordDiff<'a>>) -> Self {
        let mut num_same: usize = 0;
        let mut num_changed: usize = 0;
        let mut num_added: usize = 0;
        let mut num_removed: usize = 0;
        let mut num_undetermined_before: usize = 0;
        let mut num_undetermined_after: usize = 0;

        for record_diff in record_diffs {
            match record_diff {
                RecordDiff::NoChange {
                    before: _,
                    after: _,
                    key: _,
                } => num_same += 1,
                RecordDiff::HashChange {
                    before: _,
                    after: _,
                    key: _,
                } => num_changed += 1,
                RecordDiff::Added { after: _, key: _ } => num_added += 1,
                RecordDiff::Removed { before: _, key: _ } => num_removed += 1,
                RecordDiff::Undetermined {
                    before,
                    after,
                    key: _,
                } => {
                    num_undetermined_before += before.len();
                    num_undetermined_after += after.len();
                }
            }
        }

        Self {
            num_same,
            num_changed,
            num_added,
            num_removed,
            num_undetermined_before,
            num_undetermined_after,
        }
    }
}

pub struct Render {
    pub input1_label: String,
    pub input2_label: String,
}

impl Render {
    pub fn render_to_screen<'a, W: Write>(
        &self,
        record_diffs: &Vec<RecordDiff<'a>>,
        out: &mut W,
    ) -> Result<(), Error> {
        let difference_summary = DifferenceSummary::from_record_diffs(record_diffs);
        self.render_header(out)?;
        self.render_summary(out, &difference_summary)?;
        self.render_legend(out)?;
        self.render_results(out, record_diffs)?;
        Ok(())
    }

    fn render_header<W: Write>(&self, out: &mut W) -> Result<(), Error> {
        writeln!(out, "Record comparison")?;
        writeln!(out, "=================")?;
        writeln!(out)?;

        writeln!(out, "Inputs")?;
        writeln!(out, "------")?;
        writeln!(out)?;

        writeln!(out, "input1: {}", &self.input1_label)?;
        writeln!(out, "input2: {}", &self.input2_label)?;
        writeln!(out)?;
        Ok(())
    }

    fn render_summary<W: Write>(
        &self,
        out: &mut W,
        counts: &DifferenceSummary,
    ) -> Result<(), Error> {
        let total = counts.num_same
            + counts.num_changed
            + counts.num_added
            + counts.num_removed
            + counts.num_undetermined_before
            + counts.num_undetermined_after;

        writeln!(out, "Summary")?;
        writeln!(out, "-------")?;
        writeln!(out)?;

        writeln!(out, "  =  Same           {:>5}", counts.num_same)?;
        writeln!(out, "  ~  Changed        {:>5}", counts.num_changed)?;
        writeln!(out, "  +  Added          {:>5}", counts.num_added)?;
        writeln!(out, "  -  Removed        {:>5}", counts.num_removed)?;
        writeln!(
            out,
            "  !  Undetermined   {:>5}",
            counts.num_undetermined_before
        )?;
        writeln!(
            out,
            "  !  Undetermined   {:>5}",
            counts.num_undetermined_after
        )?;

        writeln!(out, "  -----------------------")?;
        writeln!(out, "     Total          {:>5}", total)?;
        writeln!(out)?;

        Ok(())
    }

    fn render_legend<W: Write>(&self, out: &mut W) -> Result<(), Error> {
        writeln!(out, "Legend")?;
        writeln!(out, "------")?;
        writeln!(out)?;
        writeln!(
            out,
            "  =  Same           record matched and digest is unchanged"
        )?;
        writeln!(out, "  ~  Changed        record matched but digest changed")?;
        writeln!(out, "  +  Added          record exists only in input2")?;
        writeln!(out, "  -  Removed        record exists only in input1")?;
        writeln!(
            out,
            "  !  Undetermined   matcher could not safely pair records"
        )?;

        writeln!(out)?;

        Ok(())
    }

    fn render_results<W: Write>(&self, out: &mut W, diffs: &[RecordDiff<'_>]) -> Result<(), Error> {
        writeln!(out, "Results")?;
        writeln!(out, "-------")?;
        writeln!(out)?;

        for (index, diff) in diffs.iter().enumerate() {
            let number = index + 1;

            match diff {
                RecordDiff::NoChange { before, after, key } => {
                    Self::render_no_change(out, number, before, after, key)?;
                }
                RecordDiff::HashChange { before, after, key } => {
                    Self::render_hash_change(out, number, before, after, key)?;
                }
                RecordDiff::Added { after, key } => {
                    Self::render_added(out, number, after, key)?;
                }
                RecordDiff::Removed { before, key } => {
                    Self::render_removed(out, number, before, key)?;
                }
                RecordDiff::Undetermined { before, after, key } => {
                    Self::render_undetermined(out, number, before, after, key)?;
                }
            }
            writeln!(out)?;
        }
        Ok(())
    }

    fn render_no_change<W: Write>(
        out: &mut W,
        number: usize,
        before: &HashedRecordEntry,
        after: &HashedRecordEntry,
        key: &ExtractedKey,
    ) -> Result<(), Error> {
        writeln!(out, "[{:04}] = SAME", number)?;
        writeln!(out, "  key:        {}", key)?;
        writeln!(
            out,
            "  input1:     {}",
            before.file.get_path_compact()?.display()
        )?;
        writeln!(
            out,
            "  input2:     {}",
            after.file.get_path_compact()?.display()
        )?;
        writeln!(out, "  digest:     {}", before.data_digest)?;
        Ok(())
    }

    fn render_hash_change<W: Write>(
        out: &mut W,
        number: usize,
        before: &HashedRecordEntry,
        after: &HashedRecordEntry,
        key: &ExtractedKey,
    ) -> Result<(), Error> {
        writeln!(out, "[{:04}] ~ CHANGED", number)?;
        writeln!(out, "  key:        {}", key)?;
        writeln!(
            out,
            "  input1:     {}",
            before.file.get_path_compact()?.display()
        )?;
        writeln!(
            out,
            "  input2:     {}",
            after.file.get_path_compact()?.display()
        )?;
        writeln!(
            out,
            "  digest:     {} -> {}",
            before.data_digest, after.data_digest
        )?;
        Ok(())
    }

    fn render_added<W: Write>(
        out: &mut W,
        number: usize,
        after: &HashedRecordEntry,
        key: &ExtractedKey,
    ) -> Result<(), Error> {
        writeln!(out, "[{:04}] + ADDED", number)?;
        writeln!(out, "  key:        {}", key)?;
        writeln!(
            out,
            "  input2:     {}",
            after.file.get_path_compact()?.display()
        )?;
        writeln!(out, "  digest:     {}", after.data_digest)?;
        Ok(())
    }

    fn render_removed<W: Write>(
        out: &mut W,
        number: usize,
        before: &HashedRecordEntry,
        key: &ExtractedKey,
    ) -> Result<(), Error> {
        writeln!(out, "[{:04}] - REMOVED", number)?;
        writeln!(out, "  key:        {}", key)?;
        writeln!(
            out,
            "  input1:     {}",
            before.file.get_path_compact()?.display()
        )?;
        writeln!(out, "  digest:     {}", before.data_digest)?;
        Ok(())
    }

    fn render_undetermined<W: Write>(
        out: &mut W,
        number: usize,
        before: &[&HashedRecordEntry],
        after: &[&HashedRecordEntry],
        key: &ExtractedKey,
    ) -> Result<(), Error> {
        writeln!(out, "[{:04}] ! UNDETERMINED", number)?;
        writeln!(out, "  key:        {}", key)?;
        writeln!(
            out,
            "  reason:     multiple input1 and input2 records share this key"
        )?;
        writeln!(out, "  input1 candidates:")?;
        for candidate in before {
            writeln!(
                out,
                "    - {}    digest: {}",
                candidate.file.get_path()?.display(),
                candidate.data_digest
            )?;
        }
        writeln!(out, "  input2 candidates:")?;
        for candidate in after {
            writeln!(
                out,
                "    - {}    digest: {}",
                candidate.file.get_path()?.display(),
                candidate.data_digest
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_match_engine {

    use std::path::PathBuf;

    use crate::{
        paths::{Directory, FilePath},
        uid::UidDigest,
    };

    use super::*;

    /// convenience function that makes a Hashed record using a filename and a string that represents the data
    /// If the string_data is the same, then it will produce the same data_digest
    /// If the filename is the same, then it will have the same file path
    /// If both are the same, then the hashed record will be exactly the same
    pub fn make_fake_hashed_record(filename: &str, string_data: &str) -> HashedRecordEntry {
        let file = FilePath::new(
            &PathBuf::from(filename),
            Some(Directory::new("./").unwrap()),
        )
        .unwrap();
        let data_digest = UidDigest::<32>::from_str_slice(string_data).unwrap();
        HashedRecordEntry { file, data_digest }
    }

    #[test]
    fn make_match_engine_with_filename() {
        let match_engine = MatchEngine::new().with_filename_extractor();

        assert_eq!(match_engine.extractors.len(), 1);
    }

    /// Tests when multiple filenames are grouped separately
    #[test]
    fn test_group_by_filename() {
        // different filenames
        let record1 = make_fake_hashed_record("foo", "bar");
        let record2 = make_fake_hashed_record("bar", "foobar");
        let items = vec![&record1, &record2];

        let grouped_hash_records = group_by_key(items, &KeyExtractFilename);

        let expected_key_record1 = ExtractedKey::Filename("foo".to_string());
        let expected_key_record2 = ExtractedKey::Filename("bar".to_string());
        assert_eq!(grouped_hash_records.groups.len(), 2);
        assert_eq!(grouped_hash_records.groups[&expected_key_record1].len(), 1);
        assert_eq!(grouped_hash_records.groups[&expected_key_record2].len(), 1);
        assert_eq!(grouped_hash_records.ungrouped.len(), 0);
    }

    /// Tests when multiple filenames are grouped together
    #[test]
    fn test_group_by_filename_multi() {
        // same filenames
        let record1 = make_fake_hashed_record("foo", "bar");
        let record2 = make_fake_hashed_record("foo", "foobar");
        let items = vec![&record1, &record2];

        let grouped_hash_records = group_by_key(items, &KeyExtractFilename);

        let expected_key = ExtractedKey::Filename("foo".to_string());
        assert_eq!(grouped_hash_records.groups.len(), 1);
        assert_eq!(grouped_hash_records.groups[&expected_key].len(), 2);
        assert_eq!(grouped_hash_records.ungrouped.len(), 0);
    }

    #[test]
    fn test_match_filename() {
        // different data but same filename
        let record1 = make_fake_hashed_record("foo", "bar");
        let record2 = make_fake_hashed_record("foo", "foobar");

        let record1_vec = vec![&record1];
        let record2_vec = vec![&record2];

        let match_engine = MatchEngine::new().with_filename_extractor();

        let matches = match_engine.match_record_entries(&record1_vec, &record2_vec);
        assert_eq!(matches.len(), 1); // should only be 1 match
        assert!(matches[0].is_matched()); // should be a match
    }
    #[test]
    fn test_nonmatch_filename() {
        // same data but different filename
        let record1 = make_fake_hashed_record("foo", "bar");
        let record2 = make_fake_hashed_record("bar", "bar");
        let record1_vec = vec![&record1];
        let record2_vec = vec![&record2];
        let match_engine = MatchEngine::new().with_filename_extractor();

        let matches = match_engine.match_record_entries(&record1_vec, &record2_vec);

        assert_eq!(matches.len(), 2); // should be matches (unmatched)
        assert!(!matches[0].is_matched()); // should be a match
    }
}

#[cfg(test)]
mod test_diff_engine {
    use crate::recordcomparison::test_match_engine::make_fake_hashed_record;
    use crate::recordcomparison::{DiffEngine, DifferenceSummary, MatchEngine};

    #[test]
    fn test_diff_result_same() {
        let record1 = make_fake_hashed_record("foo", "bar");
        let record2 = make_fake_hashed_record("foo", "bar");

        let record1_vec = vec![&record1];
        let record2_vec = vec![&record2];

        let match_engine = MatchEngine::new().with_filename_extractor();
        let matches = match_engine.match_record_entries(&record1_vec, &record2_vec);

        let diff_records = DiffEngine::diff_matches(matches);

        assert_eq!(diff_records.len(), 1);

        let diff_record_same = match diff_records[0] {
            crate::recordcomparison::RecordDiff::NoChange {
                before: _,
                after: _,
                key: _,
            } => true,
            _ => false,
        };

        assert!(diff_record_same);
    }

    #[test]
    fn test_diff_result_changed() {
        let record1 = make_fake_hashed_record("foo", "bar");
        let record2 = make_fake_hashed_record("foo", "foobar");

        let record1_vec = vec![&record1];
        let record2_vec = vec![&record2];

        let match_engine = MatchEngine::new().with_filename_extractor();
        let matches = match_engine.match_record_entries(&record1_vec, &record2_vec);

        let diff_records = DiffEngine::diff_matches(matches);

        assert_eq!(diff_records.len(), 1);

        let diff_record_same = match diff_records[0] {
            crate::recordcomparison::RecordDiff::HashChange {
                before: _,
                after: _,
                key: _,
            } => true,
            _ => false,
        };

        assert!(diff_record_same);
    }

    #[test]
    fn test_diff_result_added() {
        let record1 = make_fake_hashed_record("foo", "bar");
        let record2 = make_fake_hashed_record("bar", "newbar");

        let record1_vec = vec![&record1];
        let record2_vec = vec![&record2];

        let match_engine = MatchEngine::new().with_filename_extractor();
        let matches = match_engine.match_record_entries(&record1_vec, &record2_vec);

        let diff_records = DiffEngine::diff_matches(matches);

        assert_eq!(diff_records.len(), 2);

        let diff_record_same = match diff_records[0] {
            crate::recordcomparison::RecordDiff::Added { after: _, key: _ } => true,
            _ => false,
        };

        assert!(diff_record_same);
    }

    #[test]
    fn test_diff_result_removed() {
        let record1 = make_fake_hashed_record("foo", "bar");

        let record1_vec = vec![&record1];
        let record2_vec: Vec<&crate::records::HashedRecordEntry> = Vec::new();

        let match_engine = MatchEngine::new().with_filename_extractor();
        let matches = match_engine.match_record_entries(&record1_vec, &record2_vec);

        let diff_records = DiffEngine::diff_matches(matches);

        assert_eq!(diff_records.len(), 1);

        let diff_record_same = match diff_records[0] {
            crate::recordcomparison::RecordDiff::Removed { before: _, key: _ } => true,
            _ => false,
        };

        assert!(diff_record_same);
    }

    #[test]
    fn test_diff_summary() {
        // record set 1
        let record1_1 = make_fake_hashed_record("foo", "bar"); //same
        let record1_2 = make_fake_hashed_record("bar", "bar"); //change
        let record1_3 = make_fake_hashed_record("foobar", "foobar"); //remove

        // record set 2
        let record2_1 = make_fake_hashed_record("foo", "bar"); //same
        let record2_2 = make_fake_hashed_record("bar", "foobar"); //changed
        let record2_3 = make_fake_hashed_record("barfoo", "barfoo"); //added

        let records1 = vec![&record1_1, &record1_2, &record1_3];
        let records2 = vec![&record2_1, &record2_2, &record2_3];

        let match_engine = MatchEngine::new().with_filename_extractor();

        let matches = match_engine.match_record_entries(&records1, &records2);

        // 1 same, 1 change, 1 added, 1 remove = 4
        let diff_records = DiffEngine::diff_matches(matches);
        assert_eq!(diff_records.len(), 4);

        let diff_summary = DifferenceSummary::from_record_diffs(&diff_records);

        assert_eq!(diff_summary.num_same, 1);
        assert_eq!(diff_summary.num_changed, 1);
        assert_eq!(diff_summary.num_added, 1);
        assert_eq!(diff_summary.num_removed, 1);
    }
}

#[cfg(test)]
mod test_rendering {
    use super::*;
    use crate::recordcomparison::{
        DifferenceSummary, Render, test_match_engine::make_fake_hashed_record,
    };

    fn get_fake_diff_summary() -> DifferenceSummary {
        // record set 1
        let record1_1 = make_fake_hashed_record("foo", "bar"); //same
        let record1_2 = make_fake_hashed_record("bar", "bar"); //change
        let record1_3 = make_fake_hashed_record("foobar", "foobar"); //remove

        // record set 2
        let record2_1 = make_fake_hashed_record("foo", "bar"); //same
        let record2_2 = make_fake_hashed_record("bar", "foobar"); //changed
        let record2_3 = make_fake_hashed_record("barfoo", "barfoo"); //added

        let records1 = vec![&record1_1, &record1_2, &record1_3];
        let records2 = vec![&record2_1, &record2_2, &record2_3];

        let match_engine = MatchEngine::new().with_filename_extractor();

        let matches = match_engine.match_record_entries(&records1, &records2);

        // 1 same, 1 change, 1 added, 1 remove = 4
        let diff_records = DiffEngine::diff_matches(matches);

        let diff_summary = DifferenceSummary::from_record_diffs(&diff_records);

        diff_summary
    }

    #[test]
    fn test_render_summary() {
        let diff_summary = get_fake_diff_summary();
        let renderer = Render {
            input1_label: "record1".to_owned(),
            input2_label: "record2".to_owned(),
        };
        let mut buffer = Vec::new();
        renderer.render_summary(&mut buffer, &diff_summary).unwrap();
        let actual = String::from_utf8(buffer).unwrap();

        let expected = "Summary\n-------\n\n  =  Same               1\n  ~  Changed            1\n  +  Added              1\n  -  Removed            1\n  !  Undetermined       0\n  !  Undetermined       0\n  -----------------------\n     Total              4\n\n".to_owned();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_all() {
        // record set 1
        let record1_1 = make_fake_hashed_record("foo", "bar"); //same
        let record1_2 = make_fake_hashed_record("bar", "bar"); //change
        let record1_3 = make_fake_hashed_record("foobar", "foobar"); //remove

        // record set 2
        let record2_1 = make_fake_hashed_record("foo", "bar"); //same
        let record2_2 = make_fake_hashed_record("bar", "foobar"); //changed
        let record2_3 = make_fake_hashed_record("barfoo", "barfoo"); //added

        let records1 = vec![&record1_1, &record1_2, &record1_3];
        let records2 = vec![&record2_1, &record2_2, &record2_3];

        let match_engine = MatchEngine::new().with_filename_extractor();

        let matches = match_engine.match_record_entries(&records1, &records2);

        // 1 same, 1 change, 1 added, 1 remove = 4
        let diff_records = DiffEngine::diff_matches(matches);

        let renderer = Render {
            input1_label: "record1".to_owned(),
            input2_label: "record2".to_owned(),
        };
        let mut buffer = Vec::new();
        renderer
            .render_to_screen(&diff_records, &mut buffer)
            .unwrap();
        let actual = String::from_utf8(buffer).unwrap();

        let expected = "Record comparison\n=================\n\nInputs\n------\n\ninput1: record1\ninput2: record2\n\nSummary\n-------\n\n  =  Same               1\n  ~  Changed            1\n  +  Added              1\n  -  Removed            1\n  !  Undetermined       0\n  !  Undetermined       0\n  -----------------------\n     Total              4\n\nLegend\n------\n\n  =  Same           record matched and digest is unchanged\n  ~  Changed        record matched but digest changed\n  +  Added          record exists only in input2\n  -  Removed        record exists only in input1\n  !  Undetermined   matcher could not safely pair records\n\nResults\n-------\n\n[0001] ~ CHANGED\n  key:        filename = bar\n  input1:     bar\n  input2:     bar\n  digest:     f2e897eed7d206cd855d441598fa521abc75aa96953e97c030c9612c30c1293d -> aa51dcd43d5c6c5203ee16906fd6b35db298b9b2e1de3fce81811d4806b76b7d\n\n[0002] + ADDED\n  key:        filename = barfoo\n  input2:     barfoo\n  digest:     d51127a308538a4f33d1c8d5b691d887740a5eceec2345d657981d30c7883e3a\n\n[0003] = SAME\n  key:        filename = foo\n  input1:     foo\n  input2:     foo\n  digest:     f2e897eed7d206cd855d441598fa521abc75aa96953e97c030c9612c30c1293d\n\n[0004] - REMOVED\n  key:        filename = foobar\n  input1:     foobar\n  digest:     aa51dcd43d5c6c5203ee16906fd6b35db298b9b2e1de3fce81811d4806b76b7d\n\n".to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn render_legend_matches_expected_output() {
        let renderer = Render {
            input1_label: "".to_string(),
            input2_label: "".to_string(),
        };
        let mut buffer = Vec::new();
        renderer.render_legend(&mut buffer).unwrap();
        let actual = String::from_utf8(buffer).unwrap();
        let expected = "\
Legend
------

  =  Same           record matched and digest is unchanged
  ~  Changed        record matched but digest changed
  +  Added          record exists only in input2
  -  Removed        record exists only in input1
  !  Undetermined   matcher could not safely pair records

";

        assert_eq!(actual, expected);
    }
}
