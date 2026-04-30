// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file contains code related to Comparing Records which is for evidence packaging

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use crate::records::{HashedRecordEntry, Record};

pub struct MatchEngine {
    pub extractor: Box<dyn KeyExtractStrategy>,
}
impl MatchEngine {
    pub fn match_record_entries<'a>(
        &self,
        record_before: &'a Record,
        record_after: &'a Record,
    ) -> Vec<MatchResult<'a>> {
        let record_entries_before: Vec<&HashedRecordEntry> =
            record_before.record_entries.iter().collect();
        let record_entries_after: Vec<&HashedRecordEntry> =
            record_after.record_entries.iter().collect();

        let grouped_keys_before = group_by_key(record_entries_before, self.extractor.as_ref());
        let grouped_keys_after = group_by_key(record_entries_after, self.extractor.as_ref());

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

pub trait KeyExtractStrategy {
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
#[derive(Debug, Clone)]
pub struct KeyExtractFilename;
impl KeyExtractStrategy for KeyExtractFilename {
    fn extract_key<'r>(&self, record_entry: &'r HashedRecordEntry) -> Option<ExtractedKey> {
        let k = record_entry
            .file
            .get_path()
            .file_name()
            .map(|x| x.to_string_lossy().to_string());
        k.map(|x| ExtractedKey::Filename(x))
    }
}

#[derive(Debug, Clone)]
pub struct GroupedHashedRecordEntries<'a> {
    groups: BTreeMap<ExtractedKey, Vec<&'a HashedRecordEntry>>,
    ungrouped: Vec<&'a HashedRecordEntry>,
}

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

pub struct DiffEngine;

impl DiffEngine {
    /// Thsi function compares the digest of each record
    fn diff_record_matched_comparison<'a>(
        before: &'a HashedRecordEntry,
        after: &'a HashedRecordEntry,
        key: ExtractedKey,
    ) -> RecordDiff<'a> {
        if before.digest == after.digest {
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
    pub fn render_to_screen<'a, W: Write>(&self, record_diffs: &Vec<RecordDiff<'a>>, out: &mut W) {
        let difference_summary = DifferenceSummary::from_record_diffs(record_diffs);
        self.render_header(out);
        self.render_summary(out, &difference_summary);
        self.render_legend(out);
        self.render_results(out, record_diffs);
    }

    fn render_header<W: Write>(&self, out: &mut W) {
        writeln!(out, "Record comparison").unwrap();
        writeln!(out, "=================").unwrap();
        writeln!(out).unwrap();

        writeln!(out, "Inputs").unwrap();
        writeln!(out, "------").unwrap();
        writeln!(out).unwrap();

        writeln!(out, "input1: {}", &self.input1_label).unwrap();
        writeln!(out, "input2: {}", &self.input2_label).unwrap();
        writeln!(out).unwrap();
    }

    fn render_summary<W: Write>(&self, out: &mut W, counts: &DifferenceSummary) {
        let total = counts.num_same
            + counts.num_changed
            + counts.num_added
            + counts.num_removed
            + counts.num_undetermined_before
            + counts.num_undetermined_after;

        writeln!(out, "Summary").unwrap();
        writeln!(out, "-------").unwrap();
        writeln!(out).unwrap();

        writeln!(out, "  =  Same           {:>5}", counts.num_same).unwrap();
        writeln!(out, "  ~  Changed        {:>5}", counts.num_changed).unwrap();
        writeln!(out, "  +  Added          {:>5}", counts.num_added).unwrap();
        writeln!(out, "  -  Removed        {:>5}", counts.num_removed).unwrap();
        writeln!(
            out,
            "  !  Undetermined   {:>5}",
            counts.num_undetermined_before
        )
        .unwrap();
        writeln!(
            out,
            "  !  Undetermined   {:>5}",
            counts.num_undetermined_after
        )
        .unwrap();

        writeln!(out, "  -----------------------").unwrap();
        writeln!(out, "     Total          {:>5}", total).unwrap();
        writeln!(out).unwrap();
    }

    fn render_legend<W: Write>(&self, out: &mut W) {
        writeln!(out, "Legend").unwrap();
        writeln!(out, "------").unwrap();
        writeln!(out).unwrap();
        writeln!(
            out,
            "  =  Same           record matched and digest is unchanged"
        )
        .unwrap();
        writeln!(out, "  ~  Changed        record matched but digest changed").unwrap();
        writeln!(out, "  +  Added          record exists only in input2").unwrap();
        writeln!(out, "  -  Removed        record exists only in input1").unwrap();
        writeln!(
            out,
            "  !  Undetermined   matcher could not safely pair records"
        )
        .unwrap();

        writeln!(out).unwrap();
    }

    fn render_results<W: Write>(&self, out: &mut W, diffs: &[RecordDiff<'_>]) {
        writeln!(out, "Results").unwrap();
        writeln!(out, "-------").unwrap();
        writeln!(out).unwrap();

        for (index, diff) in diffs.iter().enumerate() {
            let number = index + 1;

            match diff {
                RecordDiff::NoChange { before, after, key } => {
                    Self::render_no_change(out, number, before, after, key);
                }
                RecordDiff::HashChange { before, after, key } => {
                    Self::render_hash_change(out, number, before, after, key);
                }
                RecordDiff::Added { after, key } => {
                    Self::render_added(out, number, after, key);
                }
                RecordDiff::Removed { before, key } => {
                    Self::render_removed(out, number, before, key);
                }
                RecordDiff::Undetermined { before, after, key } => {
                    Self::render_undetermined(out, number, before, after, key);
                }
            }
            writeln!(out).unwrap();
        }
    }

    fn render_no_change<W: Write>(
        out: &mut W,
        number: usize,
        before: &HashedRecordEntry,
        after: &HashedRecordEntry,
        key: &ExtractedKey,
    ) {
        writeln!(out, "[{:04}] = SAME", number).unwrap();
        writeln!(out, "  key:        {}", key).unwrap();
        writeln!(
            out,
            "  input1:     {}",
            before.file.get_path_compact().display()
        )
        .unwrap();
        writeln!(
            out,
            "  input2:     {}",
            after.file.get_path_compact().display()
        )
        .unwrap();
        writeln!(out, "  digest:     {}", before.digest).unwrap();
    }

    fn render_hash_change<W: Write>(
        out: &mut W,
        number: usize,
        before: &HashedRecordEntry,
        after: &HashedRecordEntry,
        key: &ExtractedKey,
    ) {
        writeln!(out, "[{:04}] ~ CHANGED", number).unwrap();
        writeln!(out, "  key:        {}", key).unwrap();
        writeln!(
            out,
            "  input1:     {}",
            before.file.get_path_compact().display()
        )
        .unwrap();
        writeln!(
            out,
            "  input2:     {}",
            after.file.get_path_compact().display()
        )
        .unwrap();
        writeln!(out, "  digest:     {} -> {}", before.digest, after.digest).unwrap();
    }

    fn render_added<W: Write>(
        out: &mut W,
        number: usize,
        after: &HashedRecordEntry,
        key: &ExtractedKey,
    ) {
        writeln!(out, "[{:04}] + ADDED", number).unwrap();
        writeln!(out, "  key:        {}", key).unwrap();
        writeln!(
            out,
            "  input2:     {}",
            after.file.get_path_compact().display()
        )
        .unwrap();
        writeln!(out, "  digest:     {}", after.digest).unwrap();
    }

    fn render_removed<W: Write>(
        out: &mut W,
        number: usize,
        before: &HashedRecordEntry,
        key: &ExtractedKey,
    ) {
        writeln!(out, "[{:04}] - REMOVED", number).unwrap();
        writeln!(out, "  key:        {}", key).unwrap();
        writeln!(
            out,
            "  input1:     {}",
            before.file.get_path_compact().display()
        )
        .unwrap();
        writeln!(out, "  digest:     {}", before.digest).unwrap();
    }

    fn render_undetermined<W: Write>(
        out: &mut W,
        number: usize,
        before: &[&HashedRecordEntry],
        after: &[&HashedRecordEntry],
        key: &ExtractedKey,
    ) {
        writeln!(out, "[{:04}] ! UNDETERMINED", number).unwrap();
        writeln!(out, "  key:        {}", key).unwrap();
        writeln!(
            out,
            "  reason:     multiple input1 and input2 records share this key"
        )
        .unwrap();

        writeln!(out, "  input1 candidates:").unwrap();
        for candidate in before {
            writeln!(
                out,
                "    - {}    digest: {}",
                candidate.file.get_path().display(),
                candidate.digest
            )
            .unwrap();
        }

        writeln!(out, "  input2 candidates:").unwrap();
        for candidate in after {
            writeln!(
                out,
                "    - {}    digest: {}",
                candidate.file.get_path().display(),
                candidate.digest
            )
            .unwrap();
        }
    }
}
