// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{Context, Error, anyhow};
use polars::{
    frame::DataFrame,
    io::SerReader,
    prelude::{CsvReadOptions, DataType, Field, PlSmallStr, Scalar, Schema},
};

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::configparsing::TemplatedString;
use crate::studyconfig::{ConfigStep, StudyConfiguration};
use crate::uid::{BrId, UidDigest, VARSTEPID_DIGEST_LEN, VId, VarStepId};

/// StudyController is the main controller of the configuration and management of study activities
/// and keeps the (mostly) raw input configurations.
/// It generates the the DAG and realized lists of variation steps (VarStep) which are based on the template
/// ConfigSteps held in the config.toml (which is held here)
#[derive(Debug, Clone)]
pub struct StudyController {
    // study_config: StudyConfiguration, // Struct that holds everything in the config.toml
    // design: DataFrame,                // Dataframe that holds everything in the design.csv
    pub variations_ordered: Vec<VId>, // keeps track of how the variations were input, can also be used for a logical ordering
    pub variations: HashMap<VId, Variation>,
    pub branches: HashMap<BrId, Branch>,
    pub varstep_direct_dependencies: HashMap<VarStepId, Vec<VarStepId>>, // direct link to the dependent VarSteps, by Ids
    pub varstep_upstream_branches_map: HashMap<VarStepId, Vec<BrId>>,
    pub varsteps: HashMap<VarStepId, VarStep>, // Vector of all Variation Specific Steps (VarSteps)
    pub varsteps_by_vid: HashMap<VId, Vec<VarStepId>>, //  tracks which VarSteps are part of each Variation
}
impl StudyController {
    /// convenvenience that wraps the study configuration construction, then immediately uses that to build the
    /// study controller. The study config is not saved and is lost when using this
    pub fn from_config_path(config_path: &PathBuf) -> Result<(Self, StudyConfiguration), Error> {
        let study_config = StudyConfiguration::from_config_path(config_path)?;
        let study = Self::from_study_config(&study_config)?;
        let out = (study, study_config);
        Ok(out)
    }

    /// Reads in a CSV with selected column headers to strings, as a Polars::dataframe
    pub fn read_csv_columns_as_strings<P: AsRef<Path>>(
        path: P,
        columns: Vec<String>,
    ) -> Result<DataFrame, Error> {
        let selected: Arc<[PlSmallStr]> = columns
            .iter()
            .map(|name| PlSmallStr::from_string(name.clone()))
            .collect::<Vec<_>>()
            .into();

        let schema_overwrite = columns
            .into_iter()
            .map(|name| {
                let name = PlSmallStr::from_string(name);
                Field::new(name, DataType::String)
            })
            .collect::<Schema>();

        let df = CsvReadOptions::default()
            .with_has_header(true)
            .with_columns(Some(selected))
            .with_schema_overwrite(Some(Arc::new(schema_overwrite)))
            .try_into_reader_with_file_path(Some(path.as_ref().into()))?
            .finish()?;

        Ok(df)
    }

    /// Creates a study from the study configuration
    pub fn from_study_config(study_config: &StudyConfiguration) -> Result<Self, Error> {
        // Load in the design
        // Reads in a design file using a CSV reader
        let column_variables = study_config.get_input_variables();
        let df = Self::read_csv_columns_as_strings(
            study_config.design_path.clone(),
            column_variables
                .iter()
                .map(|x| x.clone())
                .collect::<Vec<String>>(),
        )?;

        // Now construct the Variations by looping through the dataframe
        let num_rows = df.height();

        let num_cols = column_variables.len();
        let mut branches: HashMap<BrId, Branch> = HashMap::with_capacity(num_rows * num_cols); // will be less than this as some branches may be repeated
        let mut variations_ordered: Vec<VId> = Vec::new();
        let mut variations: HashMap<VId, Variation> = HashMap::with_capacity(num_rows);
        let mut variation_branches: Vec<HashSet<BrId>> = Vec::with_capacity(num_rows);
        // initialize the variation hashset<BrId>
        for i in 0..num_rows {
            variation_branches.insert(i, HashSet::new());
        }
        // now populate by going through columns (more efficient than by rows)
        for var in column_variables {
            let col = df.column(&var)?;
            for row_idx in 0..num_rows {
                let av = col.get(row_idx)?;
                let scalar = Scalar::new(av.dtype(), av.into_static());
                let branch = Branch::new(var.clone(), scalar)?;
                variation_branches[row_idx].insert(branch.uid.clone()); // record that it belongs in this variation
                branches.insert(branch.uid, branch); // move the new branch into the study level container
            }
        }

        // create a variation for each row
        for variation_brid_set in variation_branches {
            let branches: Vec<&Branch> = variation_brid_set
                .into_iter()
                .map(|x| &branches[&x])
                .collect();
            let variation = Variation::from_branches(branches.into_iter())?;
            variations_ordered.push(variation.uid.clone());
            variations.insert(variation.uid, variation);
        }

        let mut varsteps: HashMap<VarStepId, VarStep> = HashMap::new();
        let mut varstep_direct_dependencies: HashMap<VarStepId, Vec<VarStepId>> = HashMap::new();
        let mut varsteps_by_vid: HashMap<VId, Vec<VarStepId>> = HashMap::new();
        let mut varstep_upstream_branches_map: HashMap<VarStepId, Vec<BrId>> = HashMap::new();

        // For each variation, generate all the VarSteps, keeping track of the dependencies and adding as needed
        //
        // This is quick and dirty now, would need to see if large studies still perform acceptably; this
        // is a good area to prioritize refactoring.
        // Additional easy gains could probably be through caching some of the maps as they are reconstructed in the
        // hot loop
        for variation in variations.values() {
            let mut variation_varsteps: Vec<VarStepId> = Vec::new();

            for step in &study_config.steps {
                let variation_branches = variation.get_branches_into_iter(&branches);

                // create a hashset of all the branches upstream
                // First extract all upstream variables, then filter the full variation branches by this
                let mut step_upstream_variables: HashSet<String> = HashSet::new();
                for upstream_step in study_config
                    .step_dependencies
                    .get(&step.uid)
                    .ok_or_else(|| anyhow!("Error finding step in study_config"))?
                {
                    for v in &study_config
                        .get_step_by_uid(upstream_step)
                        .ok_or_else(|| anyhow!("Unable to find {upstream_step} in study config"))?
                        .variables
                    {
                        step_upstream_variables.insert(v.clone());
                    }
                }

                // now filter
                let mut varstep_upstream_branches: Vec<&Branch> = Vec::new();
                for b in variation_branches {
                    if step_upstream_variables.contains(&b.variable_name) {
                        varstep_upstream_branches.push(b);
                    }
                }

                // now generate the VarStepId and make the VarStep
                let varstep_uid =
                    VarStepId::from_uid_branches(&step.uid, varstep_upstream_branches.clone())?;

                // Store the upstream branches against the new VarStepId, convert each Branch into it's BrId, not
                // a &Branch
                let varstep_upstream_brids : Vec<BrId> = varstep_upstream_branches
                .iter()
                .map(|x| x.uid.clone())
                .collect();
                varstep_upstream_branches_map.insert(varstep_uid.clone(), varstep_upstream_brids);

                // use only upstream branches as it should be less costly than the full Variation Branches
                let varstep =
                    VarStep::from_configstep_var(&varstep_uid, &step, varstep_upstream_branches)
                        .context("Unable to create VarStep")?;

                // insert into the VarSteps, indexed by the VarStepId
                variation_varsteps.push(varstep_uid.clone());
                varsteps.insert(varstep_uid, varstep);
            }

            // within each variation, build each direct dependency structure
            for varstep_uid in &variation_varsteps {
                // only add the direct dependencies if the varstep has not already been added
                if varstep_direct_dependencies.contains_key(varstep_uid) == false {
                    let mut c_varstep_dendencies: Vec<VarStepId> = Vec::new();

                    let varstep = varsteps
                        .get(varstep_uid)
                        .ok_or_else(|| anyhow!("Varstep UID not found, something went wrong"))?;

                    let config_step = study_config
                        .get_step_by_uid(&varstep.configstep_uid)
                        .ok_or_else(|| anyhow!("Config Step missing"))?;

                    // get the names of the config step dependents, then find them in the variation_varsteps
                    for config_step_dependent in &config_step.depends_on {
                        for v in &variation_varsteps {
                            if varsteps[v].configstep_uid == *config_step_dependent {
                                c_varstep_dendencies.push(v.clone())
                            }
                        }
                    }
                    // insert the vector in the bigger map
                    varstep_direct_dependencies.insert(*varstep_uid, c_varstep_dendencies);
                }
            }
            // add to the variation mapping
            varsteps_by_vid.insert(variation.uid.clone(), variation_varsteps);
        }

        let study = StudyController {
            // study_config: study_config,
            variations: variations,
            variations_ordered: variations_ordered,
            branches: branches,
            varsteps: varsteps,
            varstep_direct_dependencies: varstep_direct_dependencies,
            varsteps_by_vid: varsteps_by_vid,
            varstep_upstream_branches_map,
        };

        Ok(study)
    }
}

#[derive(Debug, Clone)]
pub struct Variation {
    // each branch id of the variation. While the branch set here is unique to the variation,
    // individual branches are not necessariliy, so we keep the actual Branch owned
    // by the study itself
    pub branch_uids: HashSet<BrId>,
    pub uid: VId, // Hashed internal uid value
                  // design_uid: String, // value from the design, optional maybe in the future
}
impl Variation {
    pub fn from_branches<'a, I>(branches: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = &'a Branch>,
    {
        // normalize the iterator so we can use the contained refs multiple times
        let branches: Vec<&Branch> = branches.into_iter().into_iter().collect();

        let mut branch_uids: HashSet<BrId> = HashSet::with_capacity(branches.len());
        for b in &branches {
            branch_uids.insert(b.uid.clone());
        }

        let uid = VId {
            id: UidDigest::from_branches_with_prefix("variation", branches.into_iter())?,
        };

        let v = Variation {
            branch_uids: branch_uids,
            uid: uid,
        };
        Ok(v)
    }

    /// convenience function that gets the branches in this Variation (which only holds BrIds) by accessing
    /// through the branches map (branches), usually off the study controller
    pub fn get_branches_into_iter<'a>(
        &self,
        branches: &'a HashMap<BrId, Branch>,
    ) -> impl IntoIterator<Item = &'a Branch> {
        self.branch_uids
            .iter()
            .map(|branch_id| branches.get(branch_id).expect("Branch key is missing"))
    }

    /// Return the varsteps that are needed to complete this Variation. Note the VarSteps can
    pub fn get_varstep_uids(
        &self,
        study: &StudyController,
        study_config: &StudyConfiguration,
    ) -> Result<Vec<VarStepId>, Error> {
        let mut uids: Vec<VarStepId> = Vec::new();

        // loop through the study config, generate the varstep uids using the branches contained in the study
        for step in &study_config.steps {
            // loop through the branch names, and pull the actual branches from the variation branches
            let dependent_branches =
                VarStep::get_dependent_branches(&step.uid, study_config, study.branches.values());

            // using the full varstep branches, compute the ID
            let hash_id: UidDigest<VARSTEPID_DIGEST_LEN> =
                UidDigest::from_branches_with_prefix(&step.uid, dependent_branches)?;
            let varstep_uid = VarStepId { id: hash_id };
            println!("{varstep_uid}");
            uids.push(varstep_uid);
        }

        Ok(uids)
    }
}

/// Small struct that represents a branch realization (variable name + value of the variable)
#[derive(Debug, Clone)]
pub struct Branch {
    pub uid: BrId,
    pub variable_name: String, // String version of the column name in the design
    pub value: Scalar,
}
impl Branch {
    fn new(variable_name: String, value: Scalar) -> Result<Self, Error> {
        let uid = BrId {
            id: UidDigest::from_str_value(&variable_name, &value)?,
        };

        let branch = Self {
            uid: uid,
            variable_name: variable_name,
            value: value,
        };
        Ok(branch)
    }
}

/// VarStepKind keeps track of whether this is a normal step or a step that needs to run detached (e.g. over
/// something remote like an HPC cluster). This mainly changes how the status is tracked as stale file tracking
/// is no longer reliable.
#[derive(Debug, Clone, Copy)]
pub enum VarStepRunStyle {
    /// The submitting process will own the step until completion
    Blocking,

    /// Another process will own the step to completion (e.g., HPC clusters)
    Detached,
}

/// Variation Step, holds a Variation version of the Config step. Branches are associated and it is uniquely
/// identified (usually from a coordinating structure above)
/// It does not track dependencies directly, and these need to be resolved at the coordinating level.
#[derive(Debug, Clone)]
pub struct VarStep {
    /// VarStep unique ID, usually (but not required) created off the input upstream branches.
    /// The info contained in this struct is necessariliy unique as the VarStep input files are often derived from
    /// upstream branches, hence the VarStep UID should be created at a higher coordinating level.
    pub varstep_uid: VarStepId,

    /// original config step uid it's based on
    pub configstep_uid: String,

    /// Branches that are directly used in this VarStep
    pub branches: HashSet<BrId>,

    /// Run framework, tracks how it needs to be managed
    pub kind: VarStepRunStyle,

    /// Templated run arguments. These are resolved during actual execuation when exact filepaths are realized
    pub run_args: Vec<TemplatedString>,
}
impl VarStep {
    /// returns all the dependent branches that are contained in input branches
    fn get_dependent_branches<'a, B>(
        config_step_uid: &str,
        study_config: &StudyConfiguration,
        branches: B,
    ) -> Vec<&'a Branch>
    where
        B: IntoIterator<Item = &'a Branch>,
    {
        let mut branch_dependencies: Vec<&Branch> = Vec::new();
        let config_step_uid = config_step_uid.to_string();
        let branches: Vec<&Branch> = branches.into_iter().collect();

        for dependent_step_uid in &study_config.step_dependencies[&config_step_uid] {
            let dependent_step_branch_variables = &study_config
                .get_step_by_uid(dependent_step_uid)
                .expect("Problem extracting config step")
                .variables;

            for variation_branch in &branches {
                if dependent_step_branch_variables.contains(&variation_branch.variable_name) {
                    branch_dependencies.push(variation_branch);
                    break;
                }
            }
        }

        branch_dependencies
    }

    /// Creates a Variation Step using the base Config Step (step), a variation id (varstep_uid),
    /// and the branches that make up the Variation (variation_branches). Only the relevant branches are carried over
    pub fn from_configstep_var<'a, B>(
        varstep_uid: &VarStepId,
        step: &ConfigStep,
        variation_branches: B,
    ) -> Result<Self, Error>
    where
        B: IntoIterator<Item = &'a Branch>,
    {
        // put the BrIds from the variation branches in here
        let mut branches: HashSet<BrId> = HashSet::new();
        for b in variation_branches {
            if step.variables.contains(&b.variable_name) {
                branches.insert(b.uid.clone());
            }
        }

        let varstep: VarStep = VarStep {
            varstep_uid: varstep_uid.clone(),
            run_args: step.run_args.clone(),
            kind: VarStepRunStyle::Blocking,
            branches: branches,
            configstep_uid: step.uid.clone(),
        };

        Ok(varstep)
    }
}
