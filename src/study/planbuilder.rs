// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::{HashMap, HashSet};

use crate::{
    study::{
        configuration::{ConfigStep, ConfigStepName, StudyConfiguration},
        dag::{ActaDag, DagBuilder, DagError},
        design::{BrId, StudyDesign, VId, VariableBranch, VariableName, VariableValue},
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

    #[error(transparent)]
    VarStepDependencyBuildError(#[from] VarStepDependencyBuildError),

    #[error("Variable {} is missing", .0)]
    VarStepVariableMissing(VariableName),
}

struct VarstepGenerationReturn {
    variation_varsteps: HashMap<VId, Vec<VarStepId>>,
    varsteps: HashMap<VarStepId, VarStep>,
}

#[derive(Debug)]
pub struct StudyPlanBuilder;

impl StudyPlanBuilder {
    pub fn build_study_plan(
        study_config: StudyConfiguration,
        design: StudyDesign,
    ) -> Result<StudyPlan, StudyPlanBuildError> {
        // DAG verification

        let mut steps_map: HashMap<ConfigStepName, &ConfigStep> =
            HashMap::with_capacity(study_config.steps.len());
        for s in &study_config.steps {
            steps_map.insert(s.name.clone(), s);
        }

        let config_step_dag = Self::build_step_dag(&study_config)?;

        let config_step_ancestries_map =
            Self::generate_step_ancestries(&study_config, steps_map, config_step_dag)?;

        // generate all the VarSteps using the ConfigStep + ConfigStepAncestry
        let varstep_generation_result =
            Self::generate_varsteps(&study_config, &config_step_ancestries_map, &design)?;

        let study_varsteps = varstep_generation_result.varsteps;
        // get the varstep dependencies using the dag + generating the VarStepId
        let varstep_dependencies = Self::generate_varstep_dependencies(
            &study_varsteps,
            &design,
            &config_step_ancestries_map,
        )?;

        // Build the varstep_dag
        let vs_dag = Self::build_varstep_dag(&study_varsteps, &varstep_dependencies)?;

        // Copy/Move everything into the StudyPlan
        Ok(StudyPlan {
            settings: study_config.settings,
            steps: study_config.steps,
            varsteps: study_varsteps,
            variations: design.variations,
            variation_varsteps: varstep_generation_result.variation_varsteps,
            varstep_dependencies,
            dag: vs_dag,
        })
    }

    fn build_step_dag(
        study_config: &StudyConfiguration,
    ) -> Result<ActaDag<ConfigStepName>, StudyPlanBuildError> {
        let mut dag_builder = DagBuilder::<ConfigStepName>::new();
        // build the dependency tree --> each Step needs to know what Branches it depends on (even upstream)
        for step in &study_config.steps {
            dag_builder.add_node(step.name.clone(), step.get_dependent_steps())?;
        }

        let config_step_dag = dag_builder.into_dag()?;
        Ok(config_step_dag)
    }

    fn build_varstep_dag(
        study_varsteps: &HashMap<VarStepId, VarStep>,
        varstep_dependencies: &HashMap<VarStepId, Vec<VarStepId>>,
    ) -> Result<ActaDag<VarStepId>, StudyPlanBuildError> {
        let mut vs_dag_builder: DagBuilder<VarStepId> = DagBuilder::new();
        for (uid, _) in study_varsteps {
            let dependent_vsids = &varstep_dependencies[uid];
            vs_dag_builder.add_node(uid.clone(), dependent_vsids.clone())?;
        }
        let vs_dag = vs_dag_builder.into_dag()?;
        Ok(vs_dag)
    }

    fn generate_step_ancestries(
        study_config: &StudyConfiguration,
        steps_map: HashMap<ConfigStepName, &ConfigStep>,
        config_step_dag: ActaDag<ConfigStepName>,
    ) -> Result<HashMap<ConfigStepName, ConfigStepAncestry>, StudyPlanBuildError> {
        // generate the ancestry
        let mut config_step_ancestries_map: HashMap<ConfigStepName, ConfigStepAncestry> =
            HashMap::new();
        for step in &study_config.steps {
            let ancestry = Self::create_config_step_ancestry(step, &steps_map, &config_step_dag)?;
            config_step_ancestries_map.insert(ancestry.name.clone(), ancestry);
        }
        Ok(config_step_ancestries_map)
    }

    fn generate_varsteps(
        study_config: &StudyConfiguration,
        config_step_ancestries_map: &HashMap<ConfigStepName, ConfigStepAncestry>,
        design: &StudyDesign,
    ) -> Result<VarstepGenerationReturn, StudyPlanBuildError> {
        let mut study_varsteps: HashMap<VarStepId, VarStep> = HashMap::new();
        let mut variation_varsteps_map: HashMap<VId, Vec<VarStepId>> =
            HashMap::with_capacity(design.variations.len());
        for variation in &design.variations {
            let mut variation_varsteps: Vec<VarStepId> =
                Vec::with_capacity(study_config.steps.len());
            let variation_branches: Vec<&VariableBranch> = variation
                .branch_ids
                .iter()
                .map(|x| &design.branches[x])
                .collect::<Vec<&VariableBranch>>();

            // build the ConfigStepName to VarstepId map. Every variation will have only one ConfigStepName
            //  doing this way will "overbuild" the Varsteps (i.e. looping through the variations), but we can
            //  do a presence check to see if that particular varstep has already been built.
            //  For now, just "overbuild", the varsteps constructions are likely pretty cheap.
            let mut configstepname_to_varstepid: HashMap<ConfigStepName, VarStepId> =
                HashMap::new();
            for step in &study_config.steps {
                let ancestry = &config_step_ancestries_map[&step.name];

                // get the varstepId for this step, add to the map
                let varstep_uid = resolve_uid(ancestry, variation_branches.clone())?;
                configstepname_to_varstepid.insert(step.name.clone(), varstep_uid);
            }

            for step in &study_config.steps {
                let ancestry = &config_step_ancestries_map[&step.name];
                let varstep_builder = VarStepBuilder { step };
                // loop through the related variables, get the associated branch with each one
                let variation_branches: HashSet<&VariableBranch> = variation
                    .branch_ids
                    .iter()
                    .map(|x| &design.branches[x])
                    .collect::<HashSet<&VariableBranch>>();

                let mut varstep_branches: Vec<&VariableBranch> =
                    Vec::with_capacity(ancestry.get_related_variables().len());
                for variable_name in &ancestry.get_related_variables() {
                    let branch = variation_branches
                        .iter()
                        .find(|x| x.name == *variable_name)
                        .ok_or_else(|| {
                            StudyPlanBuildError::VarStepVariableMissing(variable_name.clone())
                        })?;
                    varstep_branches.push(branch);
                }

                let varstep = varstep_builder
                    .build_realized_varstep(&configstepname_to_varstepid, varstep_branches)?;

                variation_varsteps.push(varstep.uid.clone());
                study_varsteps.insert(varstep.uid.clone(), varstep);
            }
            variation_varsteps_map.insert(variation.uid.clone(), variation_varsteps);
        }

        let varstep_generation_result = VarstepGenerationReturn {
            varsteps: study_varsteps,
            variation_varsteps: variation_varsteps_map,
        };

        Ok(varstep_generation_result)
    }

    fn generate_varstep_dependencies(
        study_varsteps: &HashMap<VarStepId, VarStep>,
        design: &StudyDesign,
        config_step_ancestries_map: &HashMap<ConfigStepName, ConfigStepAncestry>,
    ) -> Result<HashMap<VarStepId, Vec<VarStepId>>, StudyPlanBuildError> {
        let mut varstep_dependencies: HashMap<VarStepId, Vec<VarStepId>> = HashMap::new();
        for (vsid, varstep) in study_varsteps {
            let dependent_vsids =
                get_varstep_dependencies(varstep, &config_step_ancestries_map, &design.branches)?;
            varstep_dependencies.insert(*vsid, dependent_vsids);
        }
        Ok(varstep_dependencies)
    }

    fn create_config_step_ancestry(
        step: &ConfigStep,
        steps: &HashMap<ConfigStepName, &ConfigStep>,
        dag: &ActaDag<ConfigStepName>,
    ) -> Result<ConfigStepAncestry, StudyPlanBuildError> {
        let name = step.name.clone();
        let parents = step.get_dependent_steps().clone();
        let variables = step.get_referenced_variables().clone();

        let mut ancestral_parents: HashSet<ConfigStepName> = HashSet::new();
        let mut ancestral_variables: HashSet<VariableName> = HashSet::new();

        for parent_name in dag.collect_ancestors(&name)? {
            let parent_node = steps
                .get(&parent_name)
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

#[derive(Debug, thiserror::Error)]
pub enum VarStepDependencyBuildError {
    #[error(
        "Unable to retrieve ancestry for {}, while building varstep dependencies for {}",
        config_step_name,
        varstep_uid
    )]
    StepAncestryRetrievalError {
        config_step_name: ConfigStepName,
        varstep_uid: VarStepId,
    },

    #[error(transparent)]
    UidError(#[from] UidError),

    #[error("Resolution of the VarStep did not happen cleanly")]
    ResolveUidError,
}

/// Given a VarStep, the full ancestries map, and full branch map, determines and returns all the dependent VarStepIds
fn get_varstep_dependencies(
    varstep: &VarStep,
    step_ancestries: &HashMap<ConfigStepName, ConfigStepAncestry>,
    branch_map: &HashMap<BrId, VariableBranch>,
) -> Result<Vec<VarStepId>, VarStepDependencyBuildError> {
    let step_name = &varstep.name;
    let dependent_step_names = &step_ancestries
        .get(step_name)
        .ok_or_else(|| VarStepDependencyBuildError::StepAncestryRetrievalError {
            config_step_name: step_name.clone(),
            varstep_uid: varstep.uid.clone(),
        })?
        .parents;

    let mut branches: Vec<&VariableBranch> = Vec::with_capacity(varstep.branch_dependencies.len());
    for brid in &varstep.branch_dependencies {
        branches.push(&branch_map[&brid]);
    }
    // resolve the dependent step names into their uid --> need to know their dependent branches
    let mut dependent_varstep_uids: Vec<VarStepId> = Vec::with_capacity(dependent_step_names.len());
    for dependent_step_name in dependent_step_names {
        let dependent_step_ancestry =
            step_ancestries.get(dependent_step_name).ok_or_else(|| {
                VarStepDependencyBuildError::StepAncestryRetrievalError {
                    config_step_name: step_name.clone(),
                    varstep_uid: varstep.uid.clone(),
                }
            })?;

        let dependent_step_uid = resolve_uid(dependent_step_ancestry, branches.clone())?;
        dependent_varstep_uids.push(dependent_step_uid);
    }

    Ok(dependent_varstep_uids)
}

/// uses the step_name + the ancestral_variables to filter the branches and build a varstep_uid. This should be filtered
/// in that there are no "duplicate" variables
fn resolve_uid(
    ancestry: &ConfigStepAncestry,
    branches: Vec<&VariableBranch>,
) -> Result<VarStepId, VarStepDependencyBuildError> {
    let step_name = &ancestry.name;

    let related_variables = ancestry.get_related_variables();

    let mut brids: HashSet<BrId> = HashSet::with_capacity(related_variables.len());
    for b in branches {
        if related_variables.contains(&b.name) {
            brids.insert(b.uid);
        }
    }

    let brids_vec: Vec<BrId> = brids.iter().cloned().collect();

    match brids.len() == related_variables.len() {
        true => Ok(VarStepId::from_step_branches(step_name, brids_vec)?),
        false => Err(VarStepDependencyBuildError::ResolveUidError),
    }
}

/// Holds information regarding ancestry of ConfigStep (upstream VariableNames, and ConfigStepNames)
/// ancestral means not including this step. Use function get_related_[steps/variables] if you want both
/// step + ancestors
#[derive(Debug)]
pub struct ConfigStepAncestry {
    name: ConfigStepName,
    parents: HashSet<ConfigStepName>,
    ancestral_parents: HashSet<ConfigStepName>,
    variables: HashSet<VariableName>,
    ancestral_variables: HashSet<VariableName>,
}

impl ConfigStepAncestry {
    pub fn get_related_parents(&self) -> HashSet<ConfigStepName> {
        HashSet::from_iter(self.parents.union(&self.ancestral_parents).cloned())
    }

    pub fn get_related_variables(&self) -> HashSet<VariableName> {
        HashSet::from_iter(self.variables.union(&self.ancestral_variables).cloned())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VarStepBuildError {
    #[error(transparent)]
    TemplatedStringError(#[from] TemplatedStringError),

    #[error(transparent)]
    VarStepUidError(#[from] UidError),
}

/// Helper struct that builds VarSteps based on ConfigStep information. Because references need to be filled in (e.g.,
/// ConfigStepName needs to be filled into VarStepIds) all the VarStepIds need to be referenced against their VariableBranchIds
/// (BrId)
#[derive(Debug)]
struct VarStepBuilder<'a> {
    step: &'a ConfigStep,
}

impl VarStepBuilder<'_> {
    /// Uses the branches to generate a VarStep with Variable information now converted into the TemplatedStrings. It is
    /// expected that many VarSteps will be generated from the VarStepBuilder
    fn build_realized_varstep(
        &self,
        step_to_vsid_map: &HashMap<ConfigStepName, VarStepId>,
        branches: Vec<&VariableBranch>,
    ) -> Result<VarStep, VarStepBuildError> {
        let mut branch_map: HashMap<&VariableName, &VariableValue> =
            HashMap::with_capacity(branches.len());
        let branch_uids: Vec<BrId> = branches.iter().map(|x| x.uid).collect();
        for b in branches {
            branch_map.insert(&b.name, &b.value);
        }
        let uid = VarStepId::from_step_branches(&self.step.name, branch_uids.clone())?;

        // each of the referenced steps should be converted into VarStepIds
        let run_exe = self
            .step
            .run_exe
            .clone()
            .try_into_varstep_templated_string(step_to_vsid_map, &branch_map)?;
        let run_args = self
            .step
            .run_args
            .clone()
            .iter()
            .map(|x| {
                x.clone()
                    .try_into_varstep_templated_string(step_to_vsid_map, &branch_map)
            })
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

#[cfg(test)]
mod test {

    // what to test here:
    // Note: Dag is tested already in DAG
    //
    // 1. Proper varstep building
    // 2. Stable hashids during build process
    // 3. Dependency structure is correctly linked using varstep uid
    // 4. Proper ancestry construction (do i need it still?)

    use std::{fs::File, path::PathBuf};

    use crate::study::{configreader::ConfigReader, design::StudyDesignBuilder};

    use super::*;

    fn build_study_configuration() -> StudyConfiguration {
        let mut f = File::open(PathBuf::from("tests/fixtures/config.toml")).unwrap();
        ConfigReader::build_from_reader(&mut f).unwrap()
    }

    fn get_steps_map(study_config: &StudyConfiguration) -> HashMap<ConfigStepName, &ConfigStep> {
        let mut steps_map: HashMap<ConfigStepName, &ConfigStep> =
            HashMap::with_capacity(study_config.steps.len());
        for s in &study_config.steps {
            steps_map.insert(s.name.clone(), s);
        }
        steps_map
    }

    fn build_study_design() -> StudyDesign {
        let f = File::open(PathBuf::from("tests/fixtures/design.csv")).unwrap();
        let variables = vec!["sleep_time", "solve_time"];
        StudyDesignBuilder { variables }
            .build_from_reader(f)
            .unwrap()
    }

    /// typical cases
    #[test]
    fn test_build_study_plan() {
        let study_config = build_study_configuration();
        let study_design = build_study_design();
        let study_plan = StudyPlanBuilder::build_study_plan(study_config, study_design);
        assert!(study_plan.is_ok());
    }

    #[test]
    fn test_build_step_dag() {
        let study_config = build_study_configuration();
        let step_dag = StudyPlanBuilder::build_step_dag(&study_config);
        assert!(step_dag.is_ok());
    }

    #[test]
    fn test_build_ancestry() {
        let study_config = build_study_configuration();
        let steps_dag = StudyPlanBuilder::build_step_dag(&study_config).unwrap();

        let config_step_1 = &study_config.steps[0];
        let steps_map = get_steps_map(&study_config);

        let ancestry =
            StudyPlanBuilder::create_config_step_ancestry(config_step_1, &steps_map, &steps_dag);
        assert!(ancestry.is_ok());
        assert_eq!(ancestry.unwrap().name, ConfigStepName::from("preprocess"));
    }

    #[test]
    fn test_build_ancestry_multiple_dependecies() {
        let study_config = build_study_configuration();
        let steps_dag = StudyPlanBuilder::build_step_dag(&study_config).unwrap();

        let config_step_postprocess = &study_config.steps[2];
        let steps_map = get_steps_map(&study_config);

        let ancestry = StudyPlanBuilder::create_config_step_ancestry(
            config_step_postprocess,
            &steps_map,
            &steps_dag,
        );

        assert!(ancestry.is_ok());
        let ancestry = ancestry.unwrap();
        assert_eq!(ancestry.name, ConfigStepName::from("postprocess"));
        let expected_variable_dependencies = vec![
            VariableName::new("sleep_time"),
            VariableName::new("solve_time"),
        ];
        let expected_variable_dependencies: HashSet<VariableName> =
            HashSet::from_iter(expected_variable_dependencies.iter().cloned());
        assert_eq!(
            ancestry.get_related_variables(),
            expected_variable_dependencies
        )
    }

    #[test]
    fn test_generate_step_ancestries() {
        let study_config = build_study_configuration();
        let config_step_dag = StudyPlanBuilder::build_step_dag(&study_config).unwrap();
        let steps_map = get_steps_map(&study_config);

        let ancestries =
            StudyPlanBuilder::generate_step_ancestries(&study_config, steps_map, config_step_dag);
        assert!(ancestries.is_ok());
    }

    // #[test]
    // fn test_varstep_build() {
    //     let study_config = build_study_configuration();
    //     let study_design = build_study_design();
    //     let config_step = &study_config.steps[0];

    //     let mut configstepname_to_varstepid : HashMap<ConfigStepName, VarStepId> = HashMap::new();
    //     for step in &study_config.steps {
    //         let ancestry = &config_step_ancestries_map[&config_step.name];

    //         // get the varstepId for this step, add to the map
    //         let varstep_uid = resolve_uid(ancestry, variation_branches.clone())?;
    //         configstepname_to_varstepid.insert(step.name.clone(), varstep_uid);
    //     }

    //     let variation = &study_design.variations[0];
    //     let variation_branches = variation
    //         .branch_ids
    //         .iter()
    //         .map(|x| &study_design.branches[x])
    //         .collect::<Vec<&VariableBranch>>();

    //     let varstep_builder = VarStepBuilder { step: config_step };
    //     let varstep = varstep_builder.build_realized_varstep(variation_branches);

    //     assert!(varstep.is_ok());
    // }

    #[test]
    fn test_generate_varsteps() {
        let study_config = build_study_configuration();
        let study_design = build_study_design();
        let config_step_dag = StudyPlanBuilder::build_step_dag(&study_config).unwrap();
        let steps_map = get_steps_map(&study_config);

        let config_step_ancestries_map =
            StudyPlanBuilder::generate_step_ancestries(&study_config, steps_map, config_step_dag)
                .unwrap();

        let varstep_generation_return = StudyPlanBuilder::generate_varsteps(
            &study_config,
            &config_step_ancestries_map,
            &study_design,
        );

        assert!(varstep_generation_return.is_ok());
        assert_eq!(varstep_generation_return.unwrap().varsteps.len(), 6)
    }

    #[test]
    fn test_get_varstep_dependencies() {
        let study_config = build_study_configuration();
        let study_design = build_study_design();
        let config_step_dag = StudyPlanBuilder::build_step_dag(&study_config).unwrap();
        let steps_map = get_steps_map(&study_config);

        let config_step_ancestries_map =
            StudyPlanBuilder::generate_step_ancestries(&study_config, steps_map, config_step_dag)
                .unwrap();

        let varstep_generation_return = StudyPlanBuilder::generate_varsteps(
            &study_config,
            &config_step_ancestries_map,
            &study_design,
        )
        .unwrap();

        let dependencies = StudyPlanBuilder::generate_varstep_dependencies(
            &varstep_generation_return.varsteps,
            &study_design,
            &config_step_ancestries_map,
        );

        assert!(dependencies.is_ok())
    }

    #[test]
    fn test_build_varstep_dag() {
        let study_config = build_study_configuration();
        let study_design = build_study_design();
        let config_step_dag = StudyPlanBuilder::build_step_dag(&study_config).unwrap();
        let steps_map = get_steps_map(&study_config);

        let config_step_ancestries_map =
            StudyPlanBuilder::generate_step_ancestries(&study_config, steps_map, config_step_dag)
                .unwrap();

        let varstep_generation_return = StudyPlanBuilder::generate_varsteps(
            &study_config,
            &config_step_ancestries_map,
            &study_design,
        )
        .unwrap();

        // get the varstep dependencies using the dag + generating the VarStepId
        let varstep_dependencies = StudyPlanBuilder::generate_varstep_dependencies(
            &varstep_generation_return.varsteps,
            &study_design,
            &config_step_ancestries_map,
        )
        .unwrap();

        let vs_dag = StudyPlanBuilder::build_varstep_dag(
            &varstep_generation_return.varsteps,
            &varstep_dependencies,
        );

        assert!(vs_dag.is_ok())
    }

    #[test]
    fn test_generate_varstep_ancestries() {
        let study_config = build_study_configuration();
        let study_design = build_study_design();
        let config_step_dag = StudyPlanBuilder::build_step_dag(&study_config).unwrap();
        let steps_map = get_steps_map(&study_config);

        let config_step_ancestries_map =
            StudyPlanBuilder::generate_step_ancestries(&study_config, steps_map, config_step_dag)
                .unwrap();

        let varstep_generation_return = StudyPlanBuilder::generate_varsteps(
            &study_config,
            &config_step_ancestries_map,
            &study_design,
        )
        .unwrap();

        // get the varstep dependencies using the dag + generating the VarStepId
        let varstep_dependencies = StudyPlanBuilder::generate_varstep_dependencies(
            &varstep_generation_return.varsteps,
            &study_design,
            &config_step_ancestries_map,
        );

        assert!(varstep_dependencies.is_ok())
    }

    #[test]
    fn test_resolve_uid() {
        let study_config = build_study_configuration();
        let steps_dag = StudyPlanBuilder::build_step_dag(&study_config).unwrap();
        let study_design = build_study_design();

        let config_step = &study_config.steps[0];
        let steps_map = get_steps_map(&study_config);
        let variation = &study_design.variations[0];

        let variation_branches = variation
            .branch_ids
            .iter()
            .map(|x| &study_design.branches[x])
            .collect::<Vec<&VariableBranch>>();

        let ancestry =
            StudyPlanBuilder::create_config_step_ancestry(config_step, &steps_map, &steps_dag)
                .unwrap();

        let uid = resolve_uid(&ancestry, variation_branches);
        assert!(uid.is_ok())
    }

    // Error cases
    #[test]
    fn test_resolve_uid_duplicate_branches() {
        let study_config = build_study_configuration();
        let steps_dag = StudyPlanBuilder::build_step_dag(&study_config).unwrap();

        let config_step = &study_config.steps[0];
        let steps_map = get_steps_map(&study_config);

        let branches = vec![
            VariableBranch::new(VariableName::new("sleep_time"), VariableValue::new("10")).unwrap(),
            VariableBranch::new(VariableName::new("solve_time"), VariableValue::new("A")).unwrap(),
            VariableBranch::new(VariableName::new("sleep_time"), VariableValue::new("20")).unwrap(),
        ];

        let ancestry =
            StudyPlanBuilder::create_config_step_ancestry(config_step, &steps_map, &steps_dag)
                .unwrap();

        let uid = resolve_uid(&ancestry, branches.iter().collect());
        assert!(uid.is_err());
    }
}
