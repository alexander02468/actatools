use actatools::execution::{RunController, run_continuous};
use actatools::status::{render_status_step, render_status_variation, render_study};
use actatools::uid::{VId, VarStepId};
use anyhow::{Error, bail};
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
    file: PathBuf,
}

fn main() -> Result<(), Error> {


    Ok(())
}