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
pub enum RecordComparisonError {
    MissortedHashedRecord(HashedRecordEntry),
    MissingMatchAttempt,
}

impl std::fmt::Display for RecordComparisonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Hashed record was in the wrong PartitionedRecord attribute"
        )
    }
}

impl std::error::Error for RecordComparisonError {}

#[derive(Debug)]
pub struct MatchEngine {
    pub match_strategies: Vec<Box<dyn MatchStrategy>>,
}
impl MatchEngine {
    /// empty MatchEngine
    pub fn new() -> Self {
        let match_strategies: Vec<Box<dyn MatchStrategy>> = Vec::new();
        MatchEngine { match_strategies }
    }

    /// adds a hash hex match strategy
    pub fn with_hash_match_strategy(self) -> Self {
        let mut match_strategies = self.match_strategies;
        match_strategies.push(Box::new(HashHexMatchStrategy));
        Self { match_strategies }
    }

    pub fn with_filename_match_strategy(self) -> Self {
        let mut match_strategies = self.match_strategies;
        match_strategies.push(Box::new(FileNameMatchStrategy));
        Self { match_strategies }
    }

    /// Apply the match strategy
    pub fn apply_strategies<'a>(
        &self,
        group_a: Vec<&'a HashedRecordEntry>,
        group_b: Vec<&'a HashedRecordEntry>,
    ) -> PartitionedMatches<'a> {
        // apply each strategy sequentially
        let mut partitioned_matches = PartitionedMatches::new(group_a, group_b);
        for ms in &self.match_strategies {
            partitioned_matches = ms.match_and_partition(partitioned_matches.clone());
        }
        partitioned_matches
    }
}

pub trait MatchStrategy: Debug {
    fn name(&self) -> &'static str;

    fn match_and_partition<'r>(
        &self,
        partitioned_matches: PartitionedMatches<'r>,
    ) -> PartitionedMatches<'r>;
}

#[derive(Debug, Hash, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ComparisonKey {
    HashedHex(String),
    Filename(String),
}

impl std::fmt::Display for ComparisonKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComparisonKey::HashedHex(s) => write!(f, "(Hash)  {}", s),
            ComparisonKey::Filename(s) => write!(f, "(FileName)  {}", s),
        }
    }
}

#[derive(Debug)]
struct FileNameMatchStrategy;
impl FileNameMatchStrategy {
    /// Helper function to extract the HashHex
    fn extract_key_filename(record: &HashedRecordEntry) -> Option<ComparisonKey> {
        record.file.get_filename().map(ComparisonKey::Filename).ok()
    }
}
impl MatchStrategy for FileNameMatchStrategy {
    fn name(&self) -> &'static str {
        "FileNameStrategy"
    }

    fn match_and_partition<'r>(
        &self,
        partitioned_matches: PartitionedMatches<'r>,
    ) -> PartitionedMatches<'r> {
        let group1_records: Vec<UnmatchedRecordEntry<'r>> = partitioned_matches.unmatched_a;
        let group2_records: Vec<UnmatchedRecordEntry<'r>> = partitioned_matches.unmatched_b;

        let group1_groups: GroupedRecordEntries<'r> =
            group_by_key(group1_records, &Self::extract_key_filename);
        let group2_groups: GroupedRecordEntries<'r> =
            group_by_key(group2_records, &Self::extract_key_filename);

        let partitioned = matched_results_from_groups(group1_groups, group2_groups);

        // create a new partition, extending the old matches, and keeping the new_unmatched
        PartitionedMatches {
            matched: partitioned_matches
                .matched
                .into_iter()
                .chain(partitioned.matched)
                .collect(),
            unmatched_a: partitioned.unmatched_a,
            unmatched_b: partitioned.unmatched_b,
        }
    }
}

#[derive(Debug)]
struct HashHexMatchStrategy;
impl HashHexMatchStrategy {
    /// Helper function to extract the HashHex
    fn extract_key_hashhex(record: &HashedRecordEntry) -> Option<ComparisonKey> {
        let k = ComparisonKey::HashedHex(format!("{}", record.data_digest.compact_hex(8)));
        Some(k)
    }
}

impl MatchStrategy for HashHexMatchStrategy {
    fn name(&self) -> &'static str {
        "HashHexStrategy"
    }

    fn match_and_partition<'r>(
        &self,
        partitioned_matches: PartitionedMatches<'r>,
    ) -> PartitionedMatches<'r> {
        let group1_records: Vec<UnmatchedRecordEntry<'r>> = partitioned_matches.unmatched_a;
        let group2_records: Vec<UnmatchedRecordEntry<'r>> = partitioned_matches.unmatched_b;

        let group1_groups: GroupedRecordEntries<'r> =
            group_by_key(group1_records, &Self::extract_key_hashhex);
        let group2_groups: GroupedRecordEntries<'r> =
            group_by_key(group2_records, &Self::extract_key_hashhex);

        let partitioned = matched_results_from_groups(group1_groups, group2_groups);

        // create a new partition, extending the old matches, and keeping the new_unmatched
        PartitionedMatches {
            matched: partitioned_matches
                .matched
                .into_iter()
                .chain(partitioned.matched)
                .collect(),
            unmatched_a: partitioned.unmatched_a,
            unmatched_b: partitioned.unmatched_b,
        }
    }
}

struct GroupedRecordEntries<'r> {
    grouped: BTreeMap<ComparisonKey, Vec<UnmatchedRecordEntry<'r>>>,
    ungrouped: Vec<UnmatchedRecordEntry<'r>>,
}

// helper function to get results from groups
fn matched_results_from_groups<'r>(
    group1_grouped: GroupedRecordEntries<'r>,
    group2_grouped: GroupedRecordEntries<'r>,
) -> PartitionedMatches<'r> {
    // collect the keys from each group
    let group1_grouped_keys = group1_grouped
        .grouped
        .keys()
        .collect::<BTreeSet<&ComparisonKey>>();
    let group2_grouped_keys = group2_grouped
        .grouped
        .keys()
        .collect::<BTreeSet<&ComparisonKey>>();

    let all_keys: BTreeSet<_> = group1_grouped_keys
        .into_iter()
        .chain(group2_grouped_keys)
        .collect();

    let mut matched: Vec<MatchedRecordEntry> = Vec::new();
    let mut unmatches_a: Vec<UnmatchedRecordEntry> = Vec::new();
    let mut unmatches_b: Vec<UnmatchedRecordEntry> = Vec::new();

    // loop through all the keys, push the match results
    for k in all_keys {
        let empty_default: Vec<UnmatchedRecordEntry> = Vec::new();

        let group1_matches = group1_grouped.grouped.get(&k).unwrap_or(&empty_default);

        let group2_matches = group2_grouped.grouped.get(&k).unwrap_or(&empty_default);

        // Only add a "good" match, otherwise they get added to the undetermined
        let match_attempt = MatchAttempt { key: k.clone() };

        match (group1_matches.len(), group2_matches.len()) {
            (1, 1) => {
                let mut attempts = group1_matches[0].attempts.clone();
                attempts.push(match_attempt.clone());
                let m = MatchedRecordEntry {
                    r1: group1_matches[0].r,
                    r2: group2_matches[0].r,
                    attempts,
                };
                matched.push(m);
            }

            _ => {
                if !group1_matches.is_empty() {
                    let mut attempts = group1_matches[0].attempts.clone();
                    attempts.push(match_attempt.clone());

                    // add the group1 undetermined matches
                    for re in group1_matches {
                        let m = UnmatchedRecordEntry {
                            attempts: attempts.clone(),
                            r: re.r,
                        };
                        unmatches_a.push(m);
                    }
                }

                if !group2_matches.is_empty() {
                    let mut attempts = group2_matches[0].attempts.clone();
                    attempts.push(match_attempt.clone());
                    // add the group2 undetermined matches
                    for re in group2_matches {
                        let m = UnmatchedRecordEntry {
                            attempts: attempts.clone(),
                            r: re.r,
                        };
                        unmatches_b.push(m);
                    }
                }
            }
        }
    }

    // add the ungrouped ones to the unmatches
    for re in group1_grouped.ungrouped {
        unmatches_a.push(re)
    }
    for re in group2_grouped.ungrouped {
        unmatches_b.push(re)
    }

    PartitionedMatches {
        matched,
        unmatched_a: unmatches_a,
        unmatched_b: unmatches_b,
    }
}

/// This function uses a group of entries and groups them by a key as dictated by the input
/// extractor function
fn group_by_key<'r, E>(
    items: Vec<UnmatchedRecordEntry<'r>>,
    extractor: E,
) -> GroupedRecordEntries<'r>
where
    E: Fn(&HashedRecordEntry) -> Option<ComparisonKey>,
{
    let mut grouped: BTreeMap<ComparisonKey, Vec<UnmatchedRecordEntry<'r>>> = BTreeMap::new();
    let mut ungrouped: Vec<UnmatchedRecordEntry<'r>> = Vec::new();

    for item in items {
        match extractor(item.r) {
            Some(key) => {
                grouped
                    .entry(key)
                    .or_insert_with(Vec::new)
                    .push(item.into());
            }
            None => {
                ungrouped.push(item.into());
            }
        }
    }
    GroupedRecordEntries { grouped, ungrouped }
}

/// Owner of the Matched/UnmatchedRecords (holding a primary reference)
#[derive(Debug, Clone)]
pub struct PartitionedMatches<'r> {
    matched: Vec<MatchedRecordEntry<'r>>,
    unmatched_a: Vec<UnmatchedRecordEntry<'r>>,
    unmatched_b: Vec<UnmatchedRecordEntry<'r>>,
}
impl<'r> PartitionedMatches<'r> {
    fn new(records_a: Vec<&'r HashedRecordEntry>, records_b: Vec<&'r HashedRecordEntry>) -> Self {
        let matched: Vec<MatchedRecordEntry<'r>> = Vec::new();
        let attempts_a: Vec<UnmatchedRecordEntry> = records_a
            .into_iter()
            .map(|r| UnmatchedRecordEntry {
                r,
                attempts: Vec::<MatchAttempt>::new(),
            })
            .collect();
        let attempts_b: Vec<UnmatchedRecordEntry> = records_b
            .into_iter()
            .map(|r| UnmatchedRecordEntry {
                r,
                attempts: Vec::<MatchAttempt>::new(),
            })
            .collect();
        Self {
            matched,
            unmatched_a: attempts_a,
            unmatched_b: attempts_b,
        }
    }

    /// Consumes the PartitionedMatches to create a PartitionedDiffs
    pub fn into_partitioned_diffs(self) -> Result<PartitionedDiffs<'r>, Error> {
        let mut no_change: Vec<RecordEntryDiff<'r>> = Vec::new();
        let mut hash_change: Vec<RecordEntryDiff<'r>> = Vec::new();
        let mut unmatched_a: Vec<RecordEntryDiff<'r>> = Vec::new();
        let mut unmatched_b: Vec<RecordEntryDiff<'r>> = Vec::new();

        for m in self.matched {
            let d = m.into_diff();
            match d {
                RecordEntryDiff::NoChange {
                    record1: _,
                    record2: _,
                    attempts: _,
                } => no_change.push(d),
                RecordEntryDiff::HashChange {
                    record1: _,
                    record2: _,
                    attempts: _,
                } => hash_change.push(d),
                RecordEntryDiff::Unmatched {
                    record,
                    attempts: _,
                } => Err(RecordComparisonError::MissortedHashedRecord(record.clone()))?,
            }
        }

        for m in self.unmatched_a {
            let d = m.into_diff();
            match d {
                RecordEntryDiff::NoChange {
                    record1,
                    record2: _,
                    attempts: _,
                } => Err(RecordComparisonError::MissortedHashedRecord(
                    record1.clone(),
                ))?,
                RecordEntryDiff::HashChange {
                    record1,
                    record2: _,
                    attempts: _,
                } => Err(RecordComparisonError::MissortedHashedRecord(
                    record1.clone(),
                ))?,
                RecordEntryDiff::Unmatched {
                    record: _,
                    attempts: _,
                } => unmatched_a.push(d),
            }
        }
        for m in self.unmatched_b {
            let d = m.into_diff();
            match d {
                RecordEntryDiff::NoChange {
                    record1,
                    record2: _,
                    attempts: _,
                } => Err(RecordComparisonError::MissortedHashedRecord(
                    record1.clone(),
                ))?,
                RecordEntryDiff::HashChange {
                    record1,
                    record2: _,
                    attempts: _,
                } => Err(RecordComparisonError::MissortedHashedRecord(
                    record1.clone(),
                ))?,
                RecordEntryDiff::Unmatched {
                    record: _,
                    attempts: _,
                } => unmatched_b.push(d),
            }
        }

        Ok(PartitionedDiffs {
            no_change,
            hash_change,
            unmatched_1: unmatched_a,
            unmatched_2: unmatched_b,
        })
    }
}

pub struct PartitionedDiffs<'r> {
    no_change: Vec<RecordEntryDiff<'r>>,
    hash_change: Vec<RecordEntryDiff<'r>>,
    unmatched_1: Vec<RecordEntryDiff<'r>>,
    unmatched_2: Vec<RecordEntryDiff<'r>>,
}

#[derive(Debug, Clone)]
struct MatchedRecordEntry<'r> {
    r1: &'r HashedRecordEntry,
    r2: &'r HashedRecordEntry,
    attempts: Vec<MatchAttempt>,
}

impl<'r> MatchedRecordEntry<'r> {
    fn into_diff(self) -> RecordEntryDiff<'r> {
        match self.r1.data_digest == self.r2.data_digest {
            true => RecordEntryDiff::NoChange {
                record1: self.r1,
                record2: self.r2,
                attempts: self.attempts,
            },
            false => RecordEntryDiff::HashChange {
                record1: self.r1,
                record2: self.r2,
                attempts: self.attempts,
            },
        }
    }
}

#[derive(Debug, Clone)]
struct UnmatchedRecordEntry<'r> {
    r: &'r HashedRecordEntry,
    attempts: Vec<MatchAttempt>,
}

impl<'r> UnmatchedRecordEntry<'r> {
    fn into_diff(self) -> RecordEntryDiff<'r> {
        RecordEntryDiff::Unmatched {
            record: self.r,
            attempts: (self.attempts),
        }
    }
}

#[derive(Debug, Clone)]
struct MatchAttempt {
    key: ComparisonKey,
}

enum RecordEntryDiff<'a> {
    NoChange {
        record1: &'a HashedRecordEntry,
        record2: &'a HashedRecordEntry,
        attempts: Vec<MatchAttempt>,
    },
    HashChange {
        record1: &'a HashedRecordEntry,
        record2: &'a HashedRecordEntry,
        attempts: Vec<MatchAttempt>,
    },
    Unmatched {
        record: &'a HashedRecordEntry,
        attempts: Vec<MatchAttempt>,
    },
}

struct DifferenceSummary {
    num_same: usize,
    num_changed: usize,
    num_undetermined_a: usize,
    num_undetermined_b: usize,
}

impl DifferenceSummary {
    fn from_partioned_record_diffs<'a>(partitioned_diffs: &PartitionedDiffs<'a>) -> Self {
        let num_same: usize = partitioned_diffs.no_change.len();
        let num_changed: usize = partitioned_diffs.hash_change.len();
        let num_undetermined_a: usize = partitioned_diffs.unmatched_1.len();
        let num_undetermined_b: usize = partitioned_diffs.unmatched_2.len();

        Self {
            num_same,
            num_changed,
            num_undetermined_a,
            num_undetermined_b,
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
        partitioned_diffs: &PartitionedDiffs<'a>,
        out: &mut W,
    ) -> Result<(), Error> {
        let difference_summary = DifferenceSummary::from_partioned_record_diffs(partitioned_diffs);
        self.render_header(out)?;
        self.render_summary(out, &difference_summary)?;
        self.render_legend(out)?;
        self.render_results(out, partitioned_diffs)?;
        Ok(())
    }

    fn render_header<W: Write>(&self, out: &mut W) -> Result<(), Error> {
        writeln!(out, "Record comparison")?;
        writeln!(out, "=================")?;
        writeln!(out)?;

        writeln!(out, "Inputs")?;
        writeln!(out, "------")?;
        writeln!(out)?;

        writeln!(out, "Record 1: {}", &self.input1_label)?;
        writeln!(out, "Record 2: {}", &self.input2_label)?;
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
            + counts.num_undetermined_a
            + counts.num_undetermined_b;

        writeln!(out, "Summary")?;
        writeln!(out, "-------")?;
        writeln!(out)?;

        writeln!(out, "  =  Same               {:>5}", counts.num_same)?;
        writeln!(out, "  ~  Changed            {:>5}", counts.num_changed)?;
        writeln!(
            out,
            "  !  Undetermined (1)   {:>5}",
            counts.num_undetermined_a
        )?;
        writeln!(
            out,
            "  !  Undetermined (2)   {:>5}",
            counts.num_undetermined_b
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
        writeln!(out, "  !  Undetermined   matcher could not match record")?;

        writeln!(out)?;

        Ok(())
    }

    fn render_results<W: Write>(
        &self,
        out: &mut W,
        partitioned_diffs: &PartitionedDiffs,
    ) -> Result<(), Error> {
        writeln!(out, "No Change")?;
        writeln!(out, "---------")?;
        if partitioned_diffs.no_change.is_empty() {
            writeln!(out, "(None)")?
        } else {
            writeln!(out, "")?;
        }
        for (number, diff) in partitioned_diffs.no_change.iter().enumerate() {
            Self::render_diff_record(out, number, diff)?
        }

        writeln!(out, "")?;
        writeln!(out, "Changed")?;
        writeln!(out, "-------")?;
        if partitioned_diffs.hash_change.is_empty() {
            writeln!(out, "(None)")?
        } else {
            writeln!(out, "")?;
        }
        for (number, diff) in partitioned_diffs.hash_change.iter().enumerate() {
            Self::render_diff_record(out, number, diff)?
        }

        writeln!(out, "")?;
        writeln!(out, "Undetermined Record 1")?;
        writeln!(out, "---------------------")?;
        if partitioned_diffs.unmatched_1.is_empty() {
            writeln!(out, "(None)")?
        } else {
            writeln!(out, "")?;
        }
        for (number, diff) in partitioned_diffs.unmatched_1.iter().enumerate() {
            Self::render_diff_record(out, number, diff)?
        }

        writeln!(out, "")?;
        writeln!(out, "Undetermined Record 2")?;
        writeln!(out, "---------------------")?;
        if partitioned_diffs.unmatched_2.is_empty() {
            writeln!(out, "(None)")?
        } else {
            writeln!(out, "")?;
        }
        for (number, diff) in partitioned_diffs.unmatched_2.iter().enumerate() {
            Self::render_diff_record(out, number, diff)?
        }
        Ok(())
    }

    fn render_diff_record<W: Write>(
        out: &mut W,
        number: usize,
        diff_record: &RecordEntryDiff,
    ) -> Result<(), Error> {
        match diff_record {
            RecordEntryDiff::NoChange {
                record1,
                record2,
                attempts,
            } => {
                let key = &attempts
                    .last()
                    .ok_or_else(|| RecordComparisonError::MissingMatchAttempt)?
                    .key;
                Self::render_no_change(out, number, record1, record2, &key)?;
            }
            RecordEntryDiff::HashChange {
                record1,
                record2,
                attempts,
            } => {
                let key = &attempts
                    .last()
                    .ok_or_else(|| RecordComparisonError::MissingMatchAttempt)?
                    .key;
                Self::render_hash_change(out, number, record1, record2, &key)?;
            }
            RecordEntryDiff::Unmatched { record, attempts } => {
                let key = &attempts
                    .last()
                    .ok_or_else(|| RecordComparisonError::MissingMatchAttempt)?
                    .key;
                Self::render_undetermined(out, number, record, &key)?;
            }
        }

        Ok(())
    }

    fn render_no_change<W: Write>(
        out: &mut W,
        number: usize,
        before: &HashedRecordEntry,
        after: &HashedRecordEntry,
        key: &ComparisonKey,
    ) -> Result<(), Error> {
        writeln!(out, "[{:04}] = SAME", number)?;
        writeln!(out, "  key:        {}", key)?;
        writeln!(
            out,
            "  Record 1:     {}",
            before.file.get_path_compact()?.display()
        )?;
        writeln!(
            out,
            "  Record 2:     {}",
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
        key: &ComparisonKey,
    ) -> Result<(), Error> {
        writeln!(out, "[{:04}] ~ CHANGED", number)?;
        writeln!(out, "  key:        {}", key)?;
        writeln!(
            out,
            "  Record 1:     {}",
            before.file.get_path_compact()?.display()
        )?;
        writeln!(
            out,
            "  Record 2:     {}",
            after.file.get_path_compact()?.display()
        )?;
        writeln!(
            out,
            "  digest:     {} -> {}",
            before.data_digest, after.data_digest
        )?;
        Ok(())
    }

    fn render_undetermined<W: Write>(
        out: &mut W,
        number: usize,
        record: &HashedRecordEntry,
        key: &ComparisonKey,
    ) -> Result<(), Error> {
        writeln!(out, "[{:04}] ! UNDETERMINED", number)?;
        writeln!(out, "  key:        {}", key)?;
        writeln!(
            out,
            "  Record:     {}",
            record.file.get_path_compact()?.display()
        )?;
        writeln!(out, "  digest:     {} ", record.data_digest)?;
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
        let match_engine = MatchEngine::new().with_filename_match_strategy();

        assert_eq!(match_engine.match_strategies.len(), 1);
    }

    /// Tests when multiple filenames are grouped separately
    #[test]
    fn test_group_by_filename() {
        // different filenames
        let record1 = make_fake_hashed_record("foo", "bar");
        let record2 = make_fake_hashed_record("bar", "foobar");

        let unmatched_record1 = UnmatchedRecordEntry {
            r: &record1,
            attempts: Vec::<MatchAttempt>::new(),
        };
        let unmatched_record2 = UnmatchedRecordEntry {
            r: &record2,
            attempts: Vec::<MatchAttempt>::new(),
        };

        let items = vec![unmatched_record1, unmatched_record2];

        let grouped_hash_records =
            group_by_key(items, &FileNameMatchStrategy::extract_key_filename);

        let expected_key_record1 = ComparisonKey::Filename("foo".to_string());
        let expected_key_record2 = ComparisonKey::Filename("bar".to_string());
        assert_eq!(grouped_hash_records.grouped.len(), 2);
        assert_eq!(grouped_hash_records.grouped[&expected_key_record1].len(), 1);
        assert_eq!(grouped_hash_records.grouped[&expected_key_record2].len(), 1);
        assert_eq!(grouped_hash_records.ungrouped.len(), 0);
    }

    /// Tests when multiple filenames are grouped together
    #[test]
    fn test_group_by_filename_multi() {
        // same filenames
        let record1 = make_fake_hashed_record("foo", "bar");
        let record2 = make_fake_hashed_record("foo", "foobar");
        let unmatched_record1 = UnmatchedRecordEntry {
            r: &record1,
            attempts: Vec::<MatchAttempt>::new(),
        };
        let unmatched_record2 = UnmatchedRecordEntry {
            r: &record2,
            attempts: Vec::<MatchAttempt>::new(),
        };

        let items = vec![unmatched_record1, unmatched_record2];

        let grouped_hash_records =
            group_by_key(items, &FileNameMatchStrategy::extract_key_filename);

        let expected_key = ComparisonKey::Filename("foo".to_string());
        assert_eq!(grouped_hash_records.grouped.len(), 1);
        assert_eq!(grouped_hash_records.grouped[&expected_key].len(), 2);
        assert_eq!(grouped_hash_records.ungrouped.len(), 0);
    }

    #[test]
    fn test_match_filename() {
        // different data but same filename
        let record1 = make_fake_hashed_record("foo", "bar");
        let record2 = make_fake_hashed_record("foo", "foobar");

        let record1_vec = vec![&record1];
        let record2_vec = vec![&record2];

        let match_engine = MatchEngine::new().with_filename_match_strategy();

        let partitioned = match_engine.apply_strategies(record1_vec, record2_vec);
        assert_eq!(partitioned.matched.len(), 1); // should only be 1 match
    }
    #[test]
    fn test_nonmatch_filename() {
        // same data but different filename
        let record1 = make_fake_hashed_record("foo", "bar");
        let record2 = make_fake_hashed_record("bar", "bar");
        let record1_vec = vec![&record1];
        let record2_vec = vec![&record2];

        let match_engine = MatchEngine::new().with_filename_match_strategy();

        let partitioned = match_engine.apply_strategies(record1_vec, record2_vec);

        assert_eq!(partitioned.unmatched_a.len(), 1); // should be (unmatched)
        assert_eq!(partitioned.unmatched_b.len(), 1); // should be (unmatched)
    }

    #[test]
    fn test_match_hash() {
        // same data but different filename
        let record1 = make_fake_hashed_record("foo", "bar");
        let record2 = make_fake_hashed_record("bar", "bar");
        let record1_vec = vec![&record1];
        let record2_vec = vec![&record2];

        let match_engine = MatchEngine::new().with_hash_match_strategy();

        let partitioned = match_engine.apply_strategies(record1_vec, record2_vec);

        assert_eq!(partitioned.matched.len(), 1);
    }
}

#[cfg(test)]
mod test_diff_engine {
    use crate::recordcomparison::test_match_engine::make_fake_hashed_record;
    use crate::recordcomparison::{DifferenceSummary, MatchEngine};

    #[test]
    fn test_diff_result_same() {
        let record1 = make_fake_hashed_record("foo", "bar");
        let record2 = make_fake_hashed_record("foo", "bar");

        let record1_vec = vec![&record1];
        let record2_vec = vec![&record2];

        let match_engine = MatchEngine::new().with_filename_match_strategy();
        let partitioned = match_engine.apply_strategies(record1_vec, record2_vec);

        let partitioned_diffs = partitioned.into_partitioned_diffs().unwrap();

        assert_eq!(partitioned_diffs.no_change.len(), 1);
    }

    #[test]
    fn test_diff_result_changed() {
        let record1 = make_fake_hashed_record("foo", "bar");
        let record2 = make_fake_hashed_record("foo", "foobar");

        let record1_vec = vec![&record1];
        let record2_vec = vec![&record2];

        let match_engine = MatchEngine::new().with_filename_match_strategy();
        let partitioned = match_engine.apply_strategies(record1_vec, record2_vec);

        let partitioned_diffs = partitioned.into_partitioned_diffs().unwrap();

        assert_eq!(partitioned_diffs.hash_change.len(), 1);
    }

    #[test]
    fn test_diff_result_unmatched() {
        let record1 = make_fake_hashed_record("foo", "bar");
        let record2 = make_fake_hashed_record("bar", "newbar");

        let record1_vec = vec![&record1];
        let record2_vec = vec![&record2];

        let match_engine = MatchEngine::new().with_filename_match_strategy();
        let partitioned = match_engine.apply_strategies(record1_vec, record2_vec);

        let partitioned_diffs = partitioned.into_partitioned_diffs().unwrap();

        assert_eq!(partitioned_diffs.unmatched_1.len(), 1);
        assert_eq!(partitioned_diffs.unmatched_2.len(), 1)
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

        let match_engine = MatchEngine::new().with_filename_match_strategy();

        let partitioned_matches = match_engine.apply_strategies(records1, records2);

        // 1 same, 1 change, 1 added, 1 remove = 4
        let partitioned_diffs = partitioned_matches.into_partitioned_diffs().unwrap();

        let diff_summary = DifferenceSummary::from_partioned_record_diffs(&partitioned_diffs);

        assert_eq!(diff_summary.num_same, 1);
        assert_eq!(diff_summary.num_changed, 1);
        assert_eq!(diff_summary.num_undetermined_a, 1);
        assert_eq!(diff_summary.num_undetermined_b, 1);
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

        let match_engine = MatchEngine::new().with_filename_match_strategy();

        let partitioned_matches = match_engine.apply_strategies(records1, records2);

        // 1 same, 1 change, 1 added, 1 remove = 4
        let partitioned_diffs = partitioned_matches.into_partitioned_diffs().unwrap();

        let diff_summary = DifferenceSummary::from_partioned_record_diffs(&partitioned_diffs);

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

        let expected = "Summary\n-------\n\n  =  Same                   1\n  ~  Changed                1\n  !  Undetermined (1)       1\n  !  Undetermined (2)       1\n  -----------------------\n     Total              4\n\n".to_owned();

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

        let match_engine = MatchEngine::new().with_filename_match_strategy();

        let partitioned_matches = match_engine.apply_strategies(records1, records2);

        // 1 same, 1 change, 1 added, 1 remove = 4
        let partitioned_diffs = partitioned_matches.into_partitioned_diffs().unwrap();

        let renderer = Render {
            input1_label: "record1".to_owned(),
            input2_label: "record2".to_owned(),
        };
        let mut buffer = Vec::new();
        renderer
            .render_to_screen(&partitioned_diffs, &mut buffer)
            .unwrap();
        let actual = String::from_utf8(buffer).unwrap();
        let expected = "Record comparison\n=================\n\nInputs\n------\n\nRecord 1: record1\nRecord 2: record2\n\nSummary\n-------\n\n  =  Same                   1\n  ~  Changed                1\n  !  Undetermined (1)       1\n  !  Undetermined (2)       1\n  -----------------------\n     Total              4\n\nLegend\n------\n\n  =  Same           record matched and digest is unchanged\n  ~  Changed        record matched but digest changed\n  !  Undetermined   matcher could not match record\n\nNo Change\n---------\n\n[0000] = SAME\n  key:        (FileName)  foo\n  Record 1:     foo\n  Record 2:     foo\n  digest:     f2e897eed7d206cd855d441598fa521abc75aa96953e97c030c9612c30c1293d\n\nChanged\n-------\n\n[0000] ~ CHANGED\n  key:        (FileName)  bar\n  Record 1:     bar\n  Record 2:     bar\n  digest:     f2e897eed7d206cd855d441598fa521abc75aa96953e97c030c9612c30c1293d -> aa51dcd43d5c6c5203ee16906fd6b35db298b9b2e1de3fce81811d4806b76b7d\n\nUndetermined Record 1\n---------------------\n\n[0000] ! UNDETERMINED\n  key:        (FileName)  foobar\n  Record:     foobar\n  digest:     aa51dcd43d5c6c5203ee16906fd6b35db298b9b2e1de3fce81811d4806b76b7d \n\nUndetermined Record 2\n---------------------\n\n[0000] ! UNDETERMINED\n  key:        (FileName)  barfoo\n  Record:     barfoo\n  digest:     d51127a308538a4f33d1c8d5b691d887740a5eceec2345d657981d30c7883e3a \n".to_string();

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
  !  Undetermined   matcher could not match record

";

        assert_eq!(actual, expected);
    }
}
