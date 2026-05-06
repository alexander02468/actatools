use std::fs::File;
use std::io;
use std::path::PathBuf;

use actatools::paths::{Directory, FilePath};
use actatools::recordcomparison::{self, KeyExtractFilename, MatchEngine, Render};
use actatools::records::{self, Record, RecordIncludes};
use anyhow::{Error, anyhow};
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
    files: Vec<PathBuf>,

    /// Output to <FILE> instead of stdout
    #[arg(short, long, value_name = "FILE")]
    output_file: Option<PathBuf>,
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

fn main() -> Result<(), Error> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Record(record_args) => {
            // convert record Paths into filename
            let record_files = record_args.files;

            let mut record_includes = RecordIncludes::new();
            for file in record_files {
                let filepath = FilePath::new(&file, Some(Directory::here()))?;
                record_includes.add_include(filepath);
            }

            let record = record_includes.into_record()?;

            let mut out: Box<dyn std::io::Write> = match record_args.output_file {
                Some(file) => Box::new(File::create(file)?),
                None => Box::new(io::stdout()),
            };

            record.render_to(&mut out)?;

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
            let record = Record::load_json(&verify_args.record)
                .expect(&format!("There was a problem loading Record"));

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
