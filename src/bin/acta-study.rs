use actatools::execution::RunController;
use actatools::uid::{UidDigest, VarStepId};
use clap::{Args, Parser, Subcommand};
use std::io;
use std::path::PathBuf;
use std::str::FromStr;

use actatools::studyconfig::StudyConfiguration;
use actatools::studycontrol::StudyController;

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
    /// Inspect files for validity
    Inspect(InspectArgs),

    /// Run a Step/Variation/Study
    Run(RunArgs),

    /// Check the status of a Step/Variation/Study
    Status(StatusArgs),
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
struct StatusArgs {
    #[command(subcommand)]
    command: StatusCommands,
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
struct RunArgs {
    #[command(subcommand)]
    command: RunCommands,
}

#[derive(Debug, Subcommand)]
enum RunCommands {
    /// Runs Next Step
    NextStep,

    /// Run a Step <Step Id>
    Step(StepArgs),

    /// Run Steps continously until a Variation <Variation Id> is complete
    Variation(VariationArgs),

    /// Run Steps continuously until the study is complete
    Study,
}

#[derive(Debug, Subcommand)]
enum StatusCommands {
    /// Prints the status of the next step that will be run
    NextStep,

    /// Prints the status of Step <Step Id>
    Step(StepArgs),

    /// Prints the status of Variation <Variation Id>
    Variation(VariationArgs),

    /// Prints an overview status of the study
    Study,
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
struct StepArgs {
    /// Step Id
    step_id: String,
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
struct VariationArgs {
    /// Variation Id
    variation_id: String,
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
struct InspectArgs {
    // #[arg(default_value_t = String::from("config.toml"))]
    file: String,
}

fn main() {
    let cli = Cli::parse();

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match &cli.command {
        Commands::Inspect(inspect_args) => {
            let config_path = PathBuf::from(&inspect_args.file);
            let study_config = StudyConfiguration::from_config_path(&config_path)
                .expect("Error reading Study Config");

            let mut out = io::stdout();
            study_config.render_inspect_to_screen(&mut out).unwrap();
        }

        Commands::Run(run_args) => {
            // println!("Run Called");
            let run_command = &run_args.command;

            match run_command {
                RunCommands::NextStep => {
                    let config_path = PathBuf::from("config.toml"); // hard code for now, make an overrideable option in future
                    let study_config = StudyConfiguration::from_config_path(&config_path)
                        .expect("Error reading Study Config");
                    let study_controller = StudyController::from_study_config(&study_config)
                        .expect("Error making the Study Controller");
                    let mut run_controller = RunController::new(&study_controller, &study_config)
                        .expect("Error making the Run Controller");

                    let next_vsr_option = run_controller
                        .get_next_vsr()
                        .expect("Unable to retrieve next VarStepRunner");
                    match next_vsr_option {
                        Some(vsr_uid) => run_controller
                            .run_vsr(vsr_uid)
                            .expect(&format!("Problem occured while running {vsr_uid}")),
                        None => println!("No VarSteps available to run"),
                    }
                }

                RunCommands::Step(run_step_args) => {
                    let config_path = PathBuf::from("config.toml"); // hard code for now, make an overrideable option in future
                    let study_config = StudyConfiguration::from_config_path(&config_path)
                        .expect("Error reading Study Config");
                    let study_controller = StudyController::from_study_config(&study_config)
                        .expect("Error making the Study Controller");
                    let mut run_controller = RunController::new(&study_controller, &study_config)
                        .expect("Error making the Run Controller");

                    let vsr_uid = VarStepId::from_str(&run_step_args.step_id).expect("Error converting string to VarStep");

                    run_controller.run_vsr(vsr_uid).expect("An error occurred when running");
                }

                RunCommands::Study => {}

                RunCommands::Variation(VariationArgs) => {}
            }
        }
        Commands::Status(status_args) => {
            let status_command = &status_args.command;
            match status_command {
                StatusCommands::NextStep => todo!(),
                StatusCommands::Step(step_args) => todo!(),
                StatusCommands::Variation(variation_args) => todo!(),
                StatusCommands::Study => todo!(),
            }
        }
    }
}
