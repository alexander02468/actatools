// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file contains code related to retrieving the status of the Study items

use std::{io::Write, iter::zip};

use anyhow::{Error, anyhow};

use crate::{
    execution::{RunController, VarStepRunner, VarStepStatus},
    studycontrol::StudyController,
    uid::VarStepId,
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
