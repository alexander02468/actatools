use std::fs::File;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};

use actatools::paths::{Directory, FilePath};
use actatools::recordcomparison::{self, MatchEngine, Render};
use actatools::records::{self, Record, RecordIncludes, render_record};
use anyhow::{Error, anyhow, bail};
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Create a Record using an Includes File
    Record(RecordArgs),

    /// Bundle files listed a Includes File to a directory
    Bundle(BundleArgs),

    /// Verify the digest of a Record
    Verify(VerifyArgs),

    /// Compare two Records
    Compare(CompareArgs),
}

#[derive(Debug, Args)]
struct RecordArgs {
    /// Files to include in the Record
    files: Option<Vec<PathBuf>>,

    /// Output to <FILE> instead of stdout
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Read NUL-separated paths from stdin.
    ///
    /// Intended for: find . -type f -print0 | acta-records record --stdin0
    #[arg(long)]
    stdin0: bool,
}

#[derive(Debug, Args)]
struct BundleArgs {
    /// Includes File that lists what should be in the Record
    includes_file: String,

    /// Output Directory
    output_directory: String,
}

#[derive(Debug, Args)]
struct VerifyArgs {
    /// Specificiation File
    record: PathBuf,
}

#[derive(Debug, Args)]
struct CompareArgs {
    /// Specificiation File
    record1: PathBuf,

    ///
    record2: PathBuf,
}

fn read_paths_from_stdin_lines() -> Result<Vec<PathBuf>, Error> {
    let mut input = Vec::new();
    if io::stdin().is_terminal() {
        bail!("Values needed in stdin")
    }
    io::stdin().read_to_end(&mut input)?;

    if input.contains(&0) {
        bail!("stdin contains NUL bytes; use --stdin0 for NUL-separated path input");
    }

    let input = String::from_utf8(input)?;

    Ok(input
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect())
}

fn read_paths_from_stdin_nul() -> Result<Vec<PathBuf>, Error> {
    let mut input = Vec::new();
    if io::stdin().is_terminal() {
        bail!("Values needed in stdin")
    }
    io::stdin().read_to_end(&mut input)?;

    // dbg!(&input);

    let paths = input
        .split(|byte| *byte == 0)
        .filter(|chunk| !chunk.is_empty())
        .map(|chunk| PathBuf::from(String::from_utf8_lossy(chunk).to_string()))
        .collect();

    Ok(paths)
}

enum PathInputMode {
    ExplicitPaths,
    StdinNewlines,
    StdinNul,
}

fn determine_path_input_mode(paths: &Vec<PathBuf>, stdin0: bool) -> anyhow::Result<PathInputMode> {
    let has_dash = paths.iter().any(|path| path == Path::new("-"));

    if stdin0 {
        if !paths.is_empty() && !has_dash {
            anyhow::bail!("--stdin0 cannot be combined with explicit path arguments");
        }
        return Ok(PathInputMode::StdinNul);
    }

    if has_dash {
        if paths.len() > 1 {
            anyhow::bail!("`-` cannot be combined with explicit path arguments");
        }
        return Ok(PathInputMode::StdinNewlines);
    }

    if paths.is_empty() {
        return Ok(PathInputMode::StdinNewlines);
    }

    Ok(PathInputMode::ExplicitPaths)
}

fn main() -> Result<(), Error> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Record(record_args) => {
            // error out if there are no args + no stdin
            let record_files = record_args.files.unwrap_or_default();
            if record_files.is_empty() && !record_args.stdin0 && io::stdin().is_terminal() {
                let mut cmd = RecordArgs::augment_args(clap::Command::new("record"));
                cmd.error(
                    clap::error::ErrorKind::MissingRequiredArgument,
                    "no input paths provided; pass paths, use `-` for stdin, or use `--stdin0`",
                )
                .exit();
            }

            // convert record Paths into filename
            let output = record_args.output;
            let mut record_includes = RecordIncludes::new();

            // we have 3 scenarios here,
            let file_paths = match determine_path_input_mode(&record_files, record_args.stdin0)? {
                PathInputMode::ExplicitPaths => record_files,
                PathInputMode::StdinNewlines => read_paths_from_stdin_lines()?,
                PathInputMode::StdinNul => read_paths_from_stdin_nul()?,
            };

            // add the files in the arguments
            for file in file_paths {
                let filepath = FilePath::new(&file, Some(Directory::here()))?;
                record_includes.add_include(filepath);
            }

            if record_includes.record_entries.is_empty() {
                bail!("No files provided")
            }

            let record = record_includes.into_record()?;

            let mut out: Box<dyn std::io::Write> = match output.clone() {
                Some(file) => Box::new(File::create(file)?),
                None => Box::new(io::stdout()),
            };

            let parent_dir = match output {
                Some(d) => match d.clone().parent() {
                    Some(d) => Some(Directory::new(d)?),
                    None => Some(Directory::here()),
                },
                None => None,
            };

            render_record(&record, &mut out, parent_dir)?;

            Ok(())
        }
        Commands::Bundle(bundle_args) => {
            let record_includes_file = FilePath::new(
                &PathBuf::from(&bundle_args.includes_file),
                Some(Directory::here()),
            )?;

            let mut record_includes = RecordIncludes::new();
            record_includes.extend_includes_file(&record_includes_file)?;

            let output_dir = PathBuf::from(bundle_args.output_directory);

            records::bundle(record_includes, &output_dir)
                .expect("An error occured while bundling the files");

            Ok(())
        }
        Commands::Verify(verify_args) => {
            // let record_path_str = verify_args.record;
            let record = Record::load_json(&verify_args.record)?;

            let record_path = match verify_args.record.is_absolute() {
                false => PathBuf::from("./").join(verify_args.record),
                true => verify_args.record,
            };

            // let record_base_path = Directory::here();
            let record_base_path = Directory::new(
                record_path
                    .parent()
                    .ok_or_else(|| anyhow!("unable to get parent"))?,
            )?;

            let new_record = record.recalculate_record(record_base_path)?;

            // now just use the same compare code
            let matcher = MatchEngine::new().with_filename_extractor();

            // extract the records out of each
            let record_orig_entries: Vec<&records::HashedRecordEntry> =
                record.record_entries.iter().collect();
            let record_new_entries: Vec<&records::HashedRecordEntry> =
                new_record.record_entries.iter().collect();

            let matches = matcher.match_record_entries(&record_orig_entries, &record_new_entries);

            let record_diffs = recordcomparison::DiffEngine::diff_matches(matches);

            let renderer = Render {
                input1_label: record_path.to_string_lossy().to_string(),
                input2_label: "Copied Version".to_string(),
            };

            let mut out = io::stdout();
            renderer.render_to_screen(&record_diffs, &mut out)?;
            Ok(())
        }
        Commands::Compare(compare_args) => {
            let record_file_1 = compare_args.record1;
            let record_file_2 = compare_args.record2;

            let record1 = Record::load_json(&record_file_1)
                .expect(&format!("Unable to create Record from record file 1"));
            let record2 = Record::load_json(&record_file_2)
                .expect(&format!("Unable to create Record from record file 2"));

            // extract the records out of each
            let record1_entries: Vec<&records::HashedRecordEntry> =
                record1.record_entries.iter().collect();
            let record2_entries: Vec<&records::HashedRecordEntry> =
                record2.record_entries.iter().collect();

            let matcher = MatchEngine::new().with_filename_extractor();
            let matches = matcher.match_record_entries(&record1_entries, &record2_entries);

            let record_diffs = recordcomparison::DiffEngine::diff_matches(matches);

            let renderer = Render {
                input1_label: record_file_1.to_string_lossy().to_string(),
                input2_label: record_file_2.to_string_lossy().to_string(),
            };

            let mut out = io::stdout();
            renderer.render_to_screen(&record_diffs, &mut out)?;
            Ok(())
        }
    }
}
