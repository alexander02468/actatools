// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    fs::{File, create_dir_all, rename},
    iter::zip,
    path::PathBuf,
    thread::sleep,
    time::{self, Duration, Instant},
};

use std::io::Write;

use anyhow::{Context, Error, anyhow, bail};

use crate::{
    // manifest::{ManifestSpec, RecordSpec, RecordType},
    configparsing::{StepLoc, TemplatedStringPart},
    records::RecordIncludes,
    studyconfig::StudyConfiguration,
    studycontrol::{Branch, StudyController, VarStepRunStyle},
    uid::VarStepId,
};

/// Heartbeat interval in seconds
const HEARTBEAT_INTERVAL: u64 = 5;

/// The RunController organizes the actual VarStepRunners -- keeping track of run order, run directories, and
/// run arguments. It is the main controller of the execution layer, and coordinates the running of actual jobs
#[derive(Debug)]
pub struct RunController {
    run_order: Vec<VarStepId>,
    runners: HashMap<VarStepId, VarStepRunner>, // All of the VarStepRunners
    evidence_dir: PathBuf,                      // where the evidence should be bundled and stored
    run_top_dir: PathBuf,                       // top directory of where the varstep runs should be
    vsr_dependencies: HashMap<VarStepId, Vec<VarStepId>>, // tracks all the dependencies of a VarStep
}

impl RunController {
    pub fn new(
        study_controller: &StudyController, // varstep container from the StudyController
        study_config: &StudyConfiguration,
    ) -> Result<Self, Error> {
        let evidence_dir = study_config.evidence_dir.clone();
        let run_top_dir = study_config.run_dir.clone();

        let mut varstep_runners: HashMap<VarStepId, VarStepRunner> =
            HashMap::with_capacity(study_controller.varsteps.len());

        // first determine run order by extracting the root nodes in the variation order (most natural, as it is
        // derived from the Design File and is likely the order expected by the user)
        // This implemention is not efficient (O(n^2)), but probably fine for now.
        let mut root_varsteps: Vec<VarStepId> = Vec::new();
        for variation_uid in &study_controller.variations_ordered {
            for varstep_uid in &study_controller.varsteps_by_vid[variation_uid] {
                match &study_controller.varstep_direct_dependencies[varstep_uid].len() {
                    0 => root_varsteps.push(varstep_uid.clone()),
                    _ => {}
                }
            }
        }

        // create a children map that is required for depth first searches
        let mut children: HashMap<VarStepId, HashSet<VarStepId>> = HashMap::new();

        // loop through each varstep, for each depends on varstep, add itself to the varstep children
        for (vs_uid, vs_dendencies_uids) in &study_controller.varstep_direct_dependencies {
            // first add the child
            children.entry(vs_uid.clone()).or_insert(HashSet::new());

            for parent_vs_uid in vs_dendencies_uids {
                let childset = children
                    .entry(parent_vs_uid.clone())
                    .or_insert(HashSet::new());

                childset.insert(vs_uid.clone());
            }
        }

        let run_order = ascii_dag::layout::generic::traversal::collect_all_nodes_dfs_fn(
            root_varsteps.as_slice(),
            |x| {
                children[x]
                    .iter()
                    .map(|x| x.clone())
                    .collect::<Vec<VarStepId>>()
            },
        );

        for variation_uid in &study_controller.variations_ordered {
            // We need to resolve the <template> strings for fields within the VarSteps. In theory, the mapping
            // between these strings is specific to each varstep, but is shared along the branch. Our choices are
            // to build a VarStep specific template map dynamically, or try and build a big one that is reused.
            //
            // Perhaps a cleaner way would be to inject the Upstream Branches associated with each Template request
            // and then keep a global version of that, so that each <templatestringpart> is one to one mapped with
            // the correct upstream dependencies (and no more). Then we can build the map once in the beginning
            // by the reverse (going forward through the DAG, resolving each branch dependency set instead of building
            // it "backwards" as we're doing  here -- e.g. we are building as needed at the Variation/VarStep)
            //
            // For now, let's build a template map for each Variation -- we know that each VarStep in the variation
            // will share the same history. The cost is that we may do a bit more extra work as some VarSteps can
            // be shared across Variations. Also, we cannot efficiently/cleanly store a global template map so it
            // will be lost every Variation (we could just dirtily wrap it in HashMap<Variation, _>) if caching is
            // needed

            // only valid through this Variation loop instance
            let mut context_map: HashMap<TemplatedStringPart, String> = HashMap::new();

            // first add the more global context paths
            context_map.insert(
                TemplatedStringPart::StudyShared,
                study_config
                    .shared_dir
                    .canonicalize()?
                    .as_os_str()
                    .to_str()
                    .map(String::from)
                    .ok_or(anyhow!("Error converting shared path to string"))?,
            );

            // with the context map, resolve the VarStep TemplatedStrings to Strings/PathBufs
            for varstep_uid in &study_controller.varsteps_by_vid[variation_uid] {
                // add each varsteps run location into the template
                let varstep = &study_controller.varsteps[varstep_uid];

                let key_inputs = TemplatedStringPart::Step {
                    name: varstep.configstep_uid.clone(),
                    loc: StepLoc::Inputs,
                };
                let key_outputs = TemplatedStringPart::Step {
                    name: varstep.configstep_uid.clone(),
                    loc: StepLoc::Outputs,
                };

                // now build the directories
                let local_inputs = run_top_dir.join(format!("{variation_uid}")).join("inputs");

                let string_inputs = std::path::absolute(local_inputs)?
                    .as_os_str()
                    .to_str()
                    .map(String::from)
                    .ok_or(anyhow!(
                        "Unable to convert {variation_uid}/inputs path to string"
                    ))?;

                let local_outputs = run_top_dir.join(format!("{variation_uid}")).join("outputs");

                let string_outputs = std::path::absolute(local_outputs)?
                    .as_os_str()
                    .to_str()
                    .map(String::from)
                    .ok_or(anyhow!(
                        "Unable to convert {variation_uid}/outputs path to string"
                    ))?;

                // now insert them into the context map
                context_map.insert(key_inputs, string_inputs);
                context_map.insert(key_outputs, string_outputs);
            }

            // with the context map, resolve the variables
            for br in study_controller.variations[variation_uid]
                .get_branches_into_iter(&study_controller.branches)
            {
                let variable_name = &br.variable_name;
                let branch_name = &br.uid;
                let value_string = br.value.value().get_str().map(String::from).ok_or(anyhow!(
                    "Unable to convert variable value {variable_name} from Branch {branch_name}"
                ))?;
                context_map.insert(
                    TemplatedStringPart::StudyVariable(br.variable_name.clone()),
                    value_string,
                );
            }

            // now we builld the varstep runners by resolving all templated strings in varsteps
            for varstep_uid in &study_controller.varsteps_by_vid[variation_uid] {
                if !varstep_runners.contains_key(varstep_uid) {
                    let varstep = &study_controller.varsteps[varstep_uid];
                    let config_step_uid = varstep.configstep_uid.clone();
                    let config_step = study_config
                        .get_step_by_uid(&varstep.configstep_uid)
                        .ok_or(anyhow!("Error getting {config_step_uid}"))?;

                    let run_dir = run_top_dir.join(format!("{varstep_uid}"));
                    let evidence_dir = evidence_dir.join(format!("{varstep_uid}"));

                    // populate the templated paths
                    let run_args = config_step
                        .run_args
                        .iter()
                        .map(|x| x.realize_to_string(&context_map))
                        .collect::<Result<Vec<String>, Error>>()?;

                    // collect the actual branches
                    let branches = varstep
                        .branches
                        .iter()
                        .map(|x| study_controller.branches[x].clone())
                        .collect::<Vec<Branch>>();

                    let mut vs_runner = VarStepRunner {
                        status: VarStepStatus::Uninitialized,
                        run_dir,
                        evidence_dir: evidence_dir,
                        step_uid: config_step_uid.clone(),
                        uid: varstep_uid.clone(),
                        run_exe_path: PathBuf::from(
                            config_step.run_exe.realize_to_string(&context_map)?,
                        ),
                        run_args: run_args,
                        kind: varstep.kind,
                        branches,
                    };
                    vs_runner.update_status()?; // Update the status to check itself (for existing jobs)
                    varstep_runners.insert(varstep_uid.clone(), vs_runner); // insert the runner
                }
            }
        }

        let controller = RunController {
            run_order: run_order,
            runners: varstep_runners,
            evidence_dir: evidence_dir,
            run_top_dir: run_top_dir,
            vsr_dependencies: study_controller.varstep_direct_dependencies.clone(),
        };

        Ok(controller)
    }

    /// Returns a reference to the next VarStepUid that is not initialized
    pub fn get_next_vsr(&self) -> Result<Option<VarStepId>, Error> {
        let out = Option::None;
        for vsr_uid in &self.run_order {
            let vsr = &self.runners[vsr_uid];
            match vsr.check_status()? {
                VarStepStatus::Uninitialized | VarStepStatus::NotRunning => {
                    // check if dependencies are satisfied
                    if self.check_dependencies(vsr_uid)? == VSRDependencyStatus::Ready {
                        return Ok(Some(vsr_uid.clone()));
                    }
                }
                _ => {}
            }
        }
        return Ok(out);
    }

    pub fn get_runner(&self, varstep_uid: &VarStepId) -> Result<&VarStepRunner, Error> {
        self.runners
            .get(varstep_uid)
            .ok_or_else(|| anyhow!("Unable to find varstep in runners"))
    }

    fn check_dependencies(&self, vsr_uid: &VarStepId) -> Result<VSRDependencyStatus, Error> {
        for dependent_vsr_uid in &self.vsr_dependencies[&vsr_uid] {
            let dependent_vsr = &self
                .runners
                .get(dependent_vsr_uid)
                .ok_or_else(|| anyhow!("unable to find {dependent_vsr_uid}"))?;

            match dependent_vsr.check_status()? {
                VarStepStatus::Uninitialized
                | VarStepStatus::NotRunning
                | VarStepStatus::Running
                | VarStepStatus::Error => return Ok(VSRDependencyStatus::NotReady),

                VarStepStatus::Finished => {}
            };
        }

        Ok(VSRDependencyStatus::Ready)
    }

    /// Runs a VSR by uid. This function makes sure all dependencies are satisfied and then also running
    /// the correct run kind (blocking versus detached)
    pub fn run_vsr(&mut self, vsr_uid: VarStepId) -> Result<(), Error> {
        // first check run_dependencies to make sure they're complete
        match self.check_dependencies(&vsr_uid)? {
            VSRDependencyStatus::Ready => {} // continue
            VSRDependencyStatus::NotReady => bail!("Dependencies not satisfied"),
        }

        // now get the vsr and run it
        let vsr = self.runners.get_mut(&vsr_uid).unwrap();

        vsr.initialize()?;

        let _ = match vsr.kind {
            VarStepRunStyle::Blocking => vsr.run_blocking()?,
            VarStepRunStyle::Detached => todo!(),
        };

        Ok(())
    }

    /// Runs next VarStep
    pub fn run_next_vsr(&mut self) -> Result<(), Error> {
        let vsr_uid = self
            .get_next_vsr()?
            .ok_or_else(|| anyhow!("Nothing to run"))?;

        self.runners
            .get_mut(&vsr_uid)
            .ok_or_else(|| anyhow!("Problem getting runner"))?
            .initialize()?;

        Ok(())
    }
}

impl Display for RunController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let evidence_dir = &self.evidence_dir.to_string_lossy();
        writeln!(f, "Evidence Dir: {evidence_dir}")?;
        let run_dir = &self.run_top_dir.to_string_lossy();
        writeln!(f, "Run Top Directory: {run_dir}")?;

        writeln!(f, "")?;
        writeln!(
            f,
            "{:4} {:18} {:20} {:10}",
            "Row", "UId", "Step Name", "Status"
        )?;

        for (i, vsr_uid) in self.run_order.iter().enumerate() {
            let vsr = &self.runners[vsr_uid];
            writeln!(f, "{:4} {}", i, vsr)?; // width 10, left-aligned by default for strings
        }

        Ok(())
    }
}

/// Simple status for VarSteps
#[derive(Debug, Clone, Copy)]
pub enum VarStepStatus {
    Uninitialized,
    NotRunning,
    Running,
    Finished,
    Error,
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum VSRDependencyStatus {
    Ready,
    NotReady,
}

impl Display for VarStepStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            VarStepStatus::Uninitialized => write!(f, "Not-Initialized"),
            VarStepStatus::NotRunning => write!(f, "Not-Running"),
            VarStepStatus::Running => write!(f, "Running"),
            VarStepStatus::Finished => write!(f, "Finished"),
            VarStepStatus::Error => write!(f, "Error"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VarStepRunner {
    status: VarStepStatus,
    run_dir: PathBuf,
    evidence_dir: PathBuf,
    pub step_uid: String,
    pub uid: VarStepId,
    run_exe_path: PathBuf,
    branches: Vec<Branch>,
    run_args: Vec<String>,
    kind: VarStepRunStyle,
}

impl VarStepRunner {
    /// Helper function that converts a VarStepStatus to the associate fiename
    fn get_status_filename(status: &VarStepStatus) -> Option<String> {
        match status {
            VarStepStatus::Error => Some(String::from("status.error")),
            VarStepStatus::Finished => Some(String::from("status.finished")),
            VarStepStatus::NotRunning => Some(String::from("status.notrunning")),
            VarStepStatus::Running => Some(String::from("status.running")),
            VarStepStatus::Uninitialized => None,
        }
    }

    /// Returns the status of the VarStepRunner. Fails if it cannot determine the status
    pub fn check_status(&self) -> Result<VarStepStatus, Error> {
        // Look for the file in the working directory under status.VARSTEPSTATUS
        // only look for the initialized states
        let statuses = vec![
            VarStepStatus::Error,
            VarStepStatus::Finished,
            VarStepStatus::NotRunning,
            VarStepStatus::Running,
        ];

        let status_paths = statuses
            .iter()
            .map(|x| {
                self.run_dir
                    .join(VarStepRunner::get_status_filename(x).unwrap())
            })
            .collect::<Vec<PathBuf>>();

        // check for each path, if multiple exist, give an error
        let mut cur_status: Option<VarStepStatus> = Option::None; // initialize to missing
        let mut num_status: usize = 0; // for error tracking
        for (_status, _path) in zip(statuses, status_paths) {
            if _path.exists() {
                // error out if a status has already been found
                if num_status >= 1 {
                    Err::<VarStepStatus, Error>(Error::msg(
                        "Multiple statuses found in this directory",
                    ))?; // immediately bail
                } else {
                    // get the current status
                    cur_status = Some(_status);
                    num_status += 1;
                }
            }
        }

        // convert the option to be VarStepStatus::Uninitialized if None
        match cur_status {
            Some(_status) => Ok(_status),
            None => Ok(VarStepStatus::Uninitialized),
        }
    }

    /// Checks and updates its own status using check_status
    pub fn update_status(&mut self) -> Result<(), Error> {
        self.status = self.check_status()?;
        Ok(())
    }

    /// Changes the status by first detecting what the status is, and then renaming to the new file state status
    fn change_status(&mut self, new_status: VarStepStatus) -> Result<(), Error> {
        let cur_status = self.check_status()?;

        // where the new status path should be
        let new_status_path = match new_status {
            // catch the condition of trying to change to uninitialized which probably will cause issues to maintain
            // this as a allowed state, as it functions as a unknown state
            VarStepStatus::Uninitialized => {
                bail!("Cannot change a status to unitialized (this is an undefined state)")
            }

            _other => self
                .run_dir
                .join(VarStepRunner::get_status_filename(&new_status).unwrap()),
        };

        // if nothing is there, assume unitialized and then create a new file state. Otherwise, rename
        match cur_status {
            VarStepStatus::Uninitialized => {
                File::create_new(&new_status_path).context("Error when writing first state")?;
            }

            _other => {
                let old_status_path = self
                    .run_dir
                    .join(VarStepRunner::get_status_filename(&cur_status).unwrap());

                rename(old_status_path, new_status_path)
                    .context("Error when renaming status file")?;
            }
        }
        Ok(self.update_status()?)
    }

    /// Sets up the working directory and updates status
    pub fn initialize(&mut self) -> Result<(), Error> {
        // make the folder and initialize to NotRunning status
        create_dir_all(&self.run_dir).context("Error creating a working directory")?;

        // hardcoded inputs/outputs dir for now
        create_dir_all(&self.run_dir.join("inputs"))
            .context("Error creating the inputs directory")?;
        create_dir_all(&self.run_dir.join("outputs"))
            .context("Error creating the outputs directory")?;

        // change the status there
        self.change_status(VarStepStatus::NotRunning)?;

        Ok(())
    }

    /// Runs the blocking step using the realized values, must already be initialized
    pub fn run_blocking(&mut self) -> Result<(), Error> {
        let varstep_uid = self.uid;
        println!("Running step {varstep_uid}");

        // harcoded paths for now, possibly make them optional or tagged with the varstepid
        let std_out_file = File::create(self.run_dir.join("output"))?;
        let err_out_file = File::create(self.run_dir.join("error"))?;

        // Make sure its initialized
        match self.check_status()? {
            VarStepStatus::Uninitialized => bail!("Step is not yet initialized"),
            VarStepStatus::NotRunning => {} // this is clean at this point,
            VarStepStatus::Running => bail!("Step is already running"),
            VarStepStatus::Finished => bail!("Step is already run"),
            VarStepStatus::Error => bail!("Step is already run (with error"),
        }

        // run detached so that we can still keep an updated heartbeat
        let heartbeat_interval = Duration::from_secs(HEARTBEAT_INTERVAL); // 2 second intervals

        // build the command line step command and spawn
        let run_exe_path = self
            .run_exe_path
            .canonicalize()
            .context(format!("run exe path not found"))?; // make sure it exists
        let run_dir = self
            .run_dir
            .canonicalize()
            .context("run directory is not found")?; // make sure it exists

        let mut child_handle = std::process::Command::new(run_exe_path)
            .args(self.run_args.clone())
            .stdout(std_out_file)
            .stderr(err_out_file)
            .current_dir(run_dir.clone())
            .spawn()?;

        // keep the running non-stale to show it's being worked on
        let last_time = Instant::now();
        self.change_status(VarStepStatus::Running)?;
        let status_path = self
            .run_dir
            .join(VarStepRunner::get_status_filename(&self.status).unwrap());
        let heartbeat_file = File::create(status_path).context("Unable to open file")?;
        loop {
            match child_handle.try_wait() {
                // Ok means it has exited
                Ok(Some(_status)) => {
                    self.change_status(VarStepStatus::Finished)?;
                    break;
                }

                // Err means it is not exited
                Ok(None) => {
                    // check the current time
                    let cur_time = Instant::now();
                    if cur_time - last_time > heartbeat_interval {
                        heartbeat_file.set_modified(time::SystemTime::now())?;
                    }
                    sleep(heartbeat_interval / 100); // target to be within 1/100th of the interval
                }
                Err(_) => {
                    self.change_status(VarStepStatus::Error)?;
                    bail!("Error occured during run")
                }
            }
        }

        let mut f = File::create(run_dir.join("record.includes"))?;

        // write out the variables and step information with comments
        let varstep_uid = self.uid.clone();
        writeln!(f, "# Varstep : {varstep_uid}")?;
        writeln!(f, "#")?;
        for br in &self.branches {
            let variable_name = &br.variable_name;
            let variable_value = &br.value.value().to_string();
            writeln!(f, "# {variable_name} : {variable_value}")?;
        }

        for arg in &self.run_args {
            if PathBuf::from(arg).exists() {
                writeln!(f, "{arg}")?;
            }
        }

        // read it back in
        let record_includes =
            RecordIncludes::parse_includes_file(&run_dir.join("record.includes"))?;

        let records = record_includes.into_record()?;
        records.write_json(&run_dir.join("record.json"))?;
        Ok(())
    }

    /// Runs the detached step using the realized values, must already be initialized
    pub fn run_detached(&mut self) -> Result<(), Error> {
        todo!()
    }

    pub fn render_varstep_runner_status(&self, out: &mut impl Write) -> Result<(), Error> {
        writeln!(out, "  UId : {}", self.uid)?;
        writeln!(out, "  Step UId :{}", self.step_uid)?;
        writeln!(out, "  Status :  {}", self.status)?;
        writeln!(out, "  Run Dir : {}", self.run_dir.to_string_lossy())?;
        writeln!(out, "  Run EXE : {}", self.run_exe_path.to_string_lossy())?;
        writeln!(out, "  Run Args : [{}]", self.run_args.join(", "))?;

        writeln!(
            out,
            "  Evidence Dir : {}",
            self.evidence_dir.to_string_lossy()
        )?;
        writeln!(out, "  Inputs : ")?;

        Ok(())
    }
}

impl Display for VarStepRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let vsr_uid_short = &self.uid.to_string();
        let vsr_uid = format!("{vsr_uid_short}");
        write!(f, "{:18} {:20} {:10}", vsr_uid, self.step_uid, self.status)?;
        Ok(()) // width 10, left-aligned by default for strings
    }
}

/// The runs the vsrs subset continuously, making sure to check that
pub fn run_continuous(
    vsr_uids: Vec<VarStepId>,
    run_control: &mut RunController,
) -> Result<(), Error> {
    // sort the vsrs according to the run order, assume they come in unordered
    // convert to hashset, then simply retrieve them when going through the run order
    let vsr_uids_to_run = vsr_uids.into_iter().collect::<HashSet<VarStepId>>();

    // figure out how any need to be run
    let mut num_to_run: usize = 0;
    for varstep_uid in &vsr_uids_to_run {
        match run_control.get_runner(&varstep_uid)?.check_status()? {
            VarStepStatus::Uninitialized | VarStepStatus::NotRunning => num_to_run += 1,
            _ => {}
        }
    }
    println!("{num_to_run} steps need to be run");

    // just run over the run_order, and if the VSR is in the vsr_uids and it's ready to run, run it
    let mut finished = false;

    // while they aren't finished, keep running
    while !finished {
        finished = true;
        // loop through all the runs, if any are unfinished, switch the flag
        for ord_vsr in run_control.run_order.clone() {
            // basically use a lot of matches to have nested if statements
            // Run if:
            //  - inside the vsr_uids_to_run, otherwise skip, and
            //  - it is not yet run, and
            //  - dependencies are satisifed
            // Not elegant but should perform reasonably
            match vsr_uids_to_run.contains(&ord_vsr) {
                true => {
                    let vsr = run_control.get_runner(&ord_vsr)?;
                    match vsr.check_status()? {
                        VarStepStatus::Uninitialized | VarStepStatus::NotRunning => {
                            // attempt to run it
                            finished = false;
                            match run_control.check_dependencies(&ord_vsr)? {
                                VSRDependencyStatus::Ready => run_control.run_vsr(ord_vsr)?,
                                VSRDependencyStatus::NotReady => {}
                            }
                        }
                        _ => {}
                    }
                }
                false => {}
            }
        }
    }
    Ok(())
}
