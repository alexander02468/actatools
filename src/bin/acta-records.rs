use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use actatools::paths::{Directory, FilePath};
use actatools::recordcomparison::{MatchEngine, Render};
use actatools::records::{
    self, Record, RecordIncludes, render_record, render_record_verification,
    render_record_verification_compact,
};
use anyhow::{Error, bail};
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

    /// Options to set an Includes File
    #[arg(long)]
    includes_file: Option<PathBuf>,

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
    /// Record Files to analyze
    records: Option<Vec<PathBuf>>,

    /// Output to <FILE> instead of stdout
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Long output form
    #[arg(long)]
    long: bool,

    /// Read NUL-separated paths from stdin.
    ///
    /// Intended for: find . -type f -print0 | acta-records record --stdin0
    #[arg(long)]
    stdin0: bool,

    /// Writes NUL-separated entries to stdin.
    ///
    /// Intended for services that want to parse the --long reports
    #[arg(long)]
    fprint0: bool,
}

#[derive(Debug, Args)]
struct CompareArgs {
    /// Referenc record
    record1: PathBuf,

    /// Record to compare to
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

fn determine_path_input_mode(paths: &[PathBuf], stdin0: bool) -> anyhow::Result<PathInputMode> {
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

            // we have  scenarios here,
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
            // error out if there are no args + no stdin
            let record_files = verify_args.records.unwrap_or_default();
            if record_files.is_empty() && !verify_args.stdin0 && io::stdin().is_terminal() {
                let mut cmd = RecordArgs::augment_args(clap::Command::new("record"));
                cmd.error(
                    clap::error::ErrorKind::MissingRequiredArgument,
                    "no input paths provided; pass paths, use `-` for stdin, or use `--stdin0`",
                )
                .exit();
            }

            // we have 3 scenarios here,
            let file_paths = match determine_path_input_mode(&record_files, verify_args.stdin0)? {
                PathInputMode::ExplicitPaths => record_files,
                PathInputMode::StdinNewlines => read_paths_from_stdin_lines()?,
                PathInputMode::StdinNul => read_paths_from_stdin_nul()?,
            };

            // we can process these in a loop as there is no need to load them all into memory
            for filepath in file_paths {
                let record = Record::load_json(&filepath)?;
                let filepath_str = filepath.to_string_lossy().to_string();

                let record_verification = record.verify()?;

                let mut out = io::stdout();

                match verify_args.long {
                    true => {
                        render_record_verification(&mut out, &filepath_str, &record_verification)?
                    }
                    false => render_record_verification_compact(
                        &mut out,
                        &filepath_str,
                        &record_verification,
                    )?,
                }

                if verify_args.fprint0 {
                    write!(out, "\0")?;
                }
            }
            Ok(())
        }
        Commands::Compare(compare_args) => {
            let record_file_1 = compare_args.record1;
            let record_file_2 = compare_args.record2;

            let record1 = Record::load_json(&record_file_1)?;
            let record2 = Record::load_json(&record_file_2)?;

            // extract the records out of each
            let record1_entries: Vec<&records::HashedRecordEntry> =
                record1.record_entries.iter().collect();
            let record2_entries: Vec<&records::HashedRecordEntry> =
                record2.record_entries.iter().collect();

            let matcher = MatchEngine::new()
                .with_hash_match_strategy()
                .with_filename_match_strategy();
            let partitioned_matches = matcher.apply_strategies(record1_entries, record2_entries);

            let partitioned_diffs = partitioned_matches.into_partitioned_diffs()?;

            let renderer = Render {
                input1_label: record_file_1.to_string_lossy().to_string(),
                input2_label: record_file_2.to_string_lossy().to_string(),
            };

            let mut out = io::stdout();
            renderer.render_to_screen(&partitioned_diffs, &mut out)?;
            Ok(())
        }
    }
}
