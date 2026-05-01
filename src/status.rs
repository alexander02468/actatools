// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file contains code related to retrieving the status of the Study items

use std::{io::Write, iter::zip};

use anyhow::{Error, anyhow};

use crate::{
    execution::{RunController, VarStepRunner, VarStepStatus},
    studycontrol::{Branch, StudyController},
    uid::{VId, VarStepId},
};

/// This function prints a detailed status of a VarStep
pub fn render_status_step<W: Write>(
    out: &mut W,
    study_controller: &StudyController,
    run_controller: &RunController,
    vsr_uid: &VarStepId,
) -> Result<(), Error> {
    let vsr = run_controller.get_runner(vsr_uid)?;
    let varstep = study_controller
        .varsteps
        .get(vsr_uid)
        .ok_or_else(|| anyhow!("Unable to retrieve Varstep"))?;
    let config_step_name = &varstep.configstep_uid;

    let dependent_vsrs = study_controller.varstep_direct_dependencies[vsr_uid]
        .clone()
        .iter()
        .map(|x| run_controller.get_runner(x))
        .collect::<Result<Vec<&VarStepRunner>, Error>>()?;

    let dependent_vsr_statuses: Vec<VarStepStatus> = dependent_vsrs
        .iter()
        .map(|x| x.check_status())
        .collect::<Result<Vec<VarStepStatus>, Error>>()?;

    let vsr_status = vsr.check_status()?;

    // Now print out everything
    writeln!(out, "{vsr_uid} - {vsr_status}")?;
    writeln!(out, "  Step     : {config_step_name}")?;
    writeln!(out, "  Status   : {vsr_status}")?;
    writeln!(out, "  Branches : ")?;
    for brid in &study_controller.varstep_upstream_branches_map[vsr_uid] {
        let branch = &study_controller.branches[brid];
        let variable = &branch.variable_name;
        let value = branch.value.value().to_string();
        writeln!(out, "    {variable:<12} : {value} ")?;
    }
    writeln!(out, "  Dependent Steps : ")?;
    for (d_vsr, d_status) in zip(dependent_vsrs, dependent_vsr_statuses) {
        let d_step_name = &d_vsr.step_uid;
        let d_uid = &d_vsr.uid;
        writeln!(out, "    {d_step_name:<10.10} {d_uid} - {d_status}")?;
    }

    Ok(())
}

/// Prints the VSR + status compactly (1 line per VSR) --> useful for giving SLURM like overviews
pub fn render_vsrs_compact<W: Write>(out: &mut W, vsrs: &Vec<&VarStepRunner>) -> Result<(), Error> {

    // write the header
    writeln!(out, "  {:<18} {:<14.14} {}", "VarStepId", "Step Name", "Run Status")?;
    writeln!(out, "----------------------------------------------------")?;

    for vsr in vsrs {
        let config_step = &vsr.step_uid;
        let vsr_uid = vsr.uid;
        let status = vsr.check_status()?;
        writeln!(out, "  {vsr_uid} {config_step:<14.14} {status}")?
    }

    Ok(())
}

/// Prints the info about a Variation, basically the VId and then the branches associated
pub fn render_variation_header<W: Write>(
    out: &mut W,
    variation_uid: &VId,
    study_controller: &StudyController,
) -> Result<(), Error> {
    writeln!(out, "{variation_uid}")?;
    let variation = &study_controller.variations[variation_uid];
    let branches_uids = &variation.branch_uids;
    let branches = branches_uids
        .iter()
        .map(|x| &study_controller.branches[x])
        .collect::<Vec<&Branch>>();
    for branch in branches {
        let branch_name = &branch.variable_name;
        let branch_value = branch.value.value().to_string();
        writeln!(out, "  {branch_name:<10} {branch_value}")?;
    }

    Ok(())
}

/// Prints a nice looking summary + step info for each vsr in the Variation
pub fn render_status_variation<W: Write>(
    out: &mut W,
    variation_uid: &VId,
    study_controller: &StudyController,
    run_controller: &RunController,
) -> Result<(), Error> {
    // get the vsrs associated with the Variation
    let variation_vsr_uids = study_controller.varsteps_by_vid[variation_uid]
        .iter()
        .collect::<Vec<&VarStepId>>();

    let variation_vsrs = variation_vsr_uids
        .iter()
        .map(|x| run_controller.get_runner(x))
        .collect::<Result<Vec<&VarStepRunner>, Error>>()?;

    // First render a summary of the Variation
    render_variation_header(out, &variation_uid, &study_controller)?;

    // Next render a summary of, then a step by step
    writeln!(out, "")?;
    render_vsrs_compact(out, &variation_vsrs)?;
    writeln!(out, "")?;
    Ok(())
}

/// Renders an entire study. List the Variations and their branches only, then a compact list of all the VSRs
pub fn render_study<W: Write>(
    out: &mut W,
    study_controller: &StudyController,
    run_controller: &RunController,
) -> Result<(), Error> {

    writeln!(out, "Variations")?;
    writeln!(out, "----------")?;

    // Variations and Branches first
    for variation_uid in study_controller.variations.keys() {
        render_variation_header(out, variation_uid, &study_controller)?;
    }
    writeln!(out, "")?;

    // then render all the vsrs compactly
    let vsrs = study_controller
        .varsteps
        .iter()
        .map(|(x, _)| run_controller.get_runner(x))
        .collect::<Result<Vec<&VarStepRunner>, Error>>()?;

    render_vsrs_compact(out, &vsrs)?;

    Ok(())
}
