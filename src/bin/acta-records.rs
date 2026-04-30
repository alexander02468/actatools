use std::io;
use std::path::PathBuf;
use std::str::FromStr;

use actatools::recordcomparison::{self, KeyExtractFilename, MatchEngine, Render};
use actatools::records::{self, Record, RecordIncludes};
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
    Record(CreateArgs),

    /// Bundle files listed a Includes File to a directory
    Bundle(BundleArgs),

    /// Verify the digest of a Record
    Verify(VerifyArgs),

    /// Compare two Records
    Compare(CompareArgs),
}

#[derive(Debug, Args)]
struct CreateArgs {
    /// Includes File that lists what should be in the Record
    includes_file: String,

    /// Where the Record is written to
    output_record_file: String,
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
    record: String,
}

#[derive(Debug, Args)]
struct CompareArgs {
    /// Specificiation File
    record1: String,

    ///
    record2: String,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Record(create_args) => {
            let record_includes_file_string = create_args.includes_file;
            let record_includes_file = PathBuf::from(&record_includes_file_string);
            let record_includes = RecordIncludes::parse_includes_file(&record_includes_file)
            .expect(&format!("An error occurred while reading in the includes file from {record_includes_file_string}"));

            dbg!(&record_includes);
            let record = record_includes
                .into_record()
                .expect("An error occurred while creating the record");

            let output_file = PathBuf::from(create_args.output_record_file);
            record
                .write_json(&output_file)
                .expect("An Error was encountered when writing JSON");
        }
        Commands::Bundle(bundle_args) => {
            let record_includes_file_string = &bundle_args.includes_file;
            let record_includes_file = PathBuf::from(&bundle_args.includes_file);
            let record_includes = RecordIncludes::parse_includes_file(&record_includes_file)
                .expect(&format!("An error occurred while reading in the includes file from {record_includes_file_string}"));
            let output_dir = PathBuf::from(bundle_args.output_directory);

            records::bundle(record_includes, &output_dir)
                .expect("An error occured while bundling the files");
        }
        Commands::Verify(verify_args) => {
            let record_path_str = verify_args.record;
            let record = Record::load_json(&record_path_str).expect(&format!(
                "There was a problem loading Record from {record_path_str}"
            ));

            let record_base_path =
                PathBuf::from_str(&record_path_str).expect("Unable to create path");
            let record_base_path = record_base_path.parent().expect("Unable to find parent");

            let new_record = record
                .recalculate_record(record_base_path)
                .expect("There was a problem recalculating the Record Entries");

            // now just use the same compare code
            let matcher = MatchEngine {
                extractor: Box::new(KeyExtractFilename),
            };
            let matches = matcher.match_record_entries(&record, &new_record);

            let record_diffs = recordcomparison::DiffEngine::diff_matches(matches);

            let renderer = Render {
                input1_label: record_path_str,
                input2_label: "Input 1 Copy".to_string(),
            };

            let mut out = io::stdout();
            renderer.render_to_screen(&record_diffs, &mut out);
        }
        Commands::Compare(compare_args) => {
            let record_file_1 = compare_args.record1;
            let record_file_2 = compare_args.record2;

            let record1 = Record::load_json(&record_file_1)
                .expect(&format!("Unable to create Record from {record_file_1}"));
            let record2 = Record::load_json(&record_file_2)
                .expect(&format!("Unable to create Record from {record_file_2}"));

            let matcher = MatchEngine {
                extractor: Box::new(KeyExtractFilename),
            };
            let matches = matcher.match_record_entries(&record1, &record2);

            let record_diffs = recordcomparison::DiffEngine::diff_matches(matches);

            let renderer = Render {
                input1_label: record_file_1,
                input2_label: record_file_2,
            };

            let mut out = io::stdout();
            renderer.render_to_screen(&record_diffs, &mut out);
        }
    }
}
