// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    collections::{HashMap, HashSet, hash_map},
    env::var,
};

use crate::{
    study::{
        configuration::{ConfigStep, ConfigStepName, StudyConfiguration},
        dag::{Dag, DagBuilder, DagError},
        design::{BrId, StudyDesign, VariableBranch, VariableName, VariableValue},
        plan::{StudyPlan, VarStep, VarStepId},
        templatedstring::TemplatedStringError,
    },
    uid::UidError,
};

#[derive(Debug, thiserror::Error)]
pub enum StudyPlanBuildError {
    #[error(transparent)]
    DagBuilderError(#[from] DagError),

    #[error(transparent)]
    VarStepBuildError(#[from] VarStepBuildError),

    #[error("Unable to find {} in steps map", .0)]
    AncestryBuildKeyError(ConfigStepName),

    #[error("Unable to find variable {} when building dependencies", .0)]
    VarStepDependencyMissingBranch(VariableName),

    #[error(transparent)]
    UidError(#[from] UidError),
}

#[derive(Debug)]
pub struct StudyPlanBuilder;

impl StudyPlanBuilder {
    pub fn build_study_plan(
        study_config: StudyConfiguration,
        design: StudyDesign,
    ) -> Result<StudyPlan, StudyPlanBuildError> {
        // DAG verification

        let mut dag_builder = DagBuilder::<ConfigStepName>::new();
        // build the dependency tree --> each Step needs to know what Branches it depends on (even upstream)
        for step in &study_config.steps {
            dag_builder.add_node(step.name.clone(), step.get_referenced_steps())?;
        }

        let config_step_dag = dag_builder.into_dag()?;

        let mut steps_map: HashMap<ConfigStepName, &ConfigStep> =
            HashMap::with_capacity(study_config.steps.len());
        for s in &study_config.steps {
            steps_map.insert(s.name.clone(), s);
        }

        // generate the ancestry
        let config_step_ancestries: Vec<ConfigStepAncestry> = study_config
            .steps
            .iter()
            .map(|x| Self::create_config_step_ancestry(x, &steps_map, &config_step_dag))
            .collect::<Result<Vec<_>, StudyPlanBuildError>>()?;
        // re-organize the step ancestry into a mp, indexed by the name
        let mut config_step_ancestries_map: HashMap<ConfigStepName, &ConfigStepAncestry> =
            HashMap::with_capacity(config_step_ancestries.len());
        for ancestry in &config_step_ancestries {
            config_step_ancestries_map.insert(ancestry.name.clone(), ancestry);
        }

        // generate all the VarSteps using the ConfigStep + ConfigStepAncestry
        let mut study_varsteps: HashMap<VarStepId, VarStep> = HashMap::new();
        for (step, ancestry) in std::iter::zip(&study_config.steps, &config_step_ancestries) {
            let varstep_builder = VarStepBuilder {
                step,
                step_ancestry: ancestry,
            };

            for variation in &design.variations {
                // filter the BrIds by the ones that are actually in this VarStepAncestry.ancestral_variables
                let varstep_brids: Vec<BrId> = variation
                    .branch_ids
                    .iter()
                    .filter(|x| {
                        ancestry
                            .ancestral_variables
                            .contains(&design.branches[x].name)
                    })
                    .copied()
                    .collect();
                let varstep_branches: Vec<&VariableBranch> =
                    varstep_brids.iter().map(|x| &design.branches[x]).collect();

                let varstep = varstep_builder.build_realized_varstep(varstep_branches)?;

                // get the dependencies as well

                study_varsteps.insert(varstep.uid.clone(), varstep);
            }
        }

        // get the varstep dependencies using the dag + generating the VarStepId
        let mut varstep_dependencies: HashMap<VarStepId, Vec<VarStepId>> = HashMap::new();
        for (vsid, varstep) in &study_varsteps {
            let mut dependent_vsids: Vec<VarStepId> = Vec::new();
            let varstep_branches = varstep
                .branch_dependencies
                .iter()
                .map(|x| &design.branches[x])
                .collect::<Vec<_>>();

            for dependent_step in &config_step_ancestries_map[&varstep.name].parents {
                let dependent_step_ancestry = config_step_ancestries_map[&dependent_step];
                let dependent_step_uid =
                    Self::get_step_uid(dependent_step_ancestry, varstep_branches.clone())?;
                dependent_vsids.push(dependent_step_uid);
            }
            varstep_dependencies.insert(*vsid, dependent_vsids);
        }

        // Using the VarStepsContext create the varstep_dag
        let mut vs_dag_builder: DagBuilder<VarStepId> = DagBuilder::new();
        for (uid, _) in &study_varsteps {
            let dependent_vsids = &varstep_dependencies[uid];
            vs_dag_builder.add_node(uid.clone(), dependent_vsids.clone())?;
        }
        let vs_dag = vs_dag_builder.into_dag()?;

        // Copy/Move everything into the StudyPlan
        Ok(StudyPlan {
            settings: study_config.settings,
            steps: study_config.steps,
            varsteps: study_varsteps,
            variations: design.variations,
            dag: vs_dag,
        })
    }

    /// Function that returns the Uid of the potential VarStep based on the ConfigStep name + the branches associated with the step.
    /// Note that there is *no* check if there are multiple branches that could be part of the Step -- only the first one encountered
    /// is returned.
    fn get_step_uid(
        step_ancestry: &ConfigStepAncestry,
        step_filtered_branches: Vec<&VariableBranch>,
    ) -> Result<VarStepId, StudyPlanBuildError> {
        let name = &step_ancestry.name;

        let mut brids: Vec<BrId> = Vec::with_capacity(step_ancestry.ancestral_variables.len());
        for varname in &step_ancestry.variables {
            let b = step_filtered_branches
                .iter()
                .find(|x| x.name == *varname)
                .ok_or_else(|| {
                    StudyPlanBuildError::VarStepDependencyMissingBranch(varname.clone())
                })?;
            brids.push(b.uid);
        }

        Ok(VarStepId::from_step_branches(name, brids)?)
    }

    fn create_config_step_ancestry(
        step: &ConfigStep,
        steps: &HashMap<ConfigStepName, &ConfigStep>,
        dag: &Dag<ConfigStepName>,
    ) -> Result<ConfigStepAncestry, StudyPlanBuildError> {
        let name = step.name.clone();
        let parents = step.get_referenced_steps().clone();
        let variables = step.get_referenced_variables().clone();

        let mut ancestral_parents: HashSet<ConfigStepName> = HashSet::new();
        let mut ancestral_variables: HashSet<VariableName> = HashSet::new();

        for parent_name in dag.parents(&name)? {
            let parent_node = steps
                .get(parent_name)
                .ok_or_else(|| StudyPlanBuildError::AncestryBuildKeyError(parent_name.clone()))?;

            ancestral_parents.extend(parent_node.get_referenced_steps());
            ancestral_variables.extend(parent_node.get_referenced_variables());
        }

        Ok(ConfigStepAncestry {
            name,
            parents,
            ancestral_parents,
            variables,
            ancestral_variables,
        })
    }
}

/// Holds information regarding ancestry of ConfigStep (upstream VariableNames, and ConfigStepNames)
#[derive(Debug)]
pub struct ConfigStepAncestry {
    name: ConfigStepName,
    parents: HashSet<ConfigStepName>,
    ancestral_parents: HashSet<ConfigStepName>,
    variables: HashSet<VariableName>,
    ancestral_variables: HashSet<VariableName>,
}

#[derive(Debug, thiserror::Error)]
pub enum VarStepBuildError {
    #[error(transparent)]
    TemplatedStringError(#[from] TemplatedStringError),

    #[error(transparent)]
    VarStepUidError(#[from] UidError),
}

/// Helper struct that builds VarSteps based on ConfigStep information
struct VarStepBuilder<'a> {
    step: &'a ConfigStep,
    step_ancestry: &'a ConfigStepAncestry,
}

impl VarStepBuilder<'_> {
    /// Uses the branches to generate a VarStep with Variable information now converted into the TemplatedStrings. It is
    /// expected that many VarSteps will be generated from the VarStepBuilder
    fn build_realized_varstep(
        &self,
        branches: Vec<&VariableBranch>,
    ) -> Result<VarStep, VarStepBuildError> {
        let mut branch_map: HashMap<&VariableName, &VariableValue> =
            HashMap::with_capacity(branches.len());
        let branch_uids: Vec<BrId> = branches.iter().map(|x| x.uid).collect();
        for b in branches {
            branch_map.insert(&b.name, &b.value);
        }
        let uid = VarStepId::from_step_branches(&self.step.name, branch_uids.clone())?;

        let run_exe = self
            .step
            .run_exe
            .clone()
            .into_varstep_templated_string(&branch_map)?;
        let run_args = self
            .step
            .run_args
            .clone()
            .iter()
            .map(|x| x.clone().into_varstep_templated_string(&branch_map))
            .collect::<Result<Vec<_>, TemplatedStringError>>()?;

        let name = self.step.name.clone();

        let varstep = VarStep {
            name,
            uid,
            run_exe,
            run_args,
            branch_dependencies: branch_uids,
        };

        Ok(varstep)
    }
}
