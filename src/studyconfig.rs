// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{Context, Error, anyhow};
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    path::PathBuf,
};
use toml::Table;

use crate::configparsing::{ParsedString, TemplatedString, TemplatedStringPart};

/// Holds data related to the configuration parameters at the study level, no optional arguments. Represents
/// the loaded version of the config.toml
#[derive(Debug)]
pub struct StudyConfiguration {
    pub name: String,                                        // name of the study
    pub design_path: PathBuf,                                // path to the design.csv
    pub run_dir: PathBuf,                                    // directory where runs are at
    pub evidence_dir: PathBuf,                               // where evidence is stored
    pub study_dir: PathBuf,                                  // top directory of the study
    pub shared_dir: PathBuf,                                 // where shared files live
    pub steps: Vec<ConfigStep>,                              // step definitions
    pub step_dependencies: HashMap<String, HashSet<String>>, // Maps the step_uid to all the branch name dependencies
}

impl StudyConfiguration {
    pub fn from_config_path(config_path: &PathBuf) -> Result<Self, Error> {
        // first load in the config file
        let config_path_str = config_path.as_os_str().to_string_lossy();
        let content = std::fs::read_to_string(&config_path)
            .context(format!("unable to read config file at {config_path_str}"))?;
        let config: toml::Table = toml::from_str(&content)
            .context(format!("Unable to parse TOML in {config_path_str}"))?;

        let default_design_path = PathBuf::from("design.csv");
        let default_run_dir = PathBuf::from("./run");
        let default_evidence_dir = PathBuf::from("./evidence");
        let default_shared_dir = PathBuf::from("./shared");

        let name = extract_string_from_table(&config, "study_name")?;
        let design_path = extract_string_from_table_optional(&config, "design_path")
            .map(PathBuf::from)
            .unwrap_or(default_design_path);
        let run_dir = extract_string_from_table_optional(&config, "run_dir")
            .map(PathBuf::from)
            .unwrap_or(default_run_dir);
        let evidence_dir = extract_string_from_table_optional(&config, "evidence_dir")
            .map(PathBuf::from)
            .unwrap_or(default_evidence_dir);
        let shared_dir = extract_string_from_table(&config, "shared")
            .map(PathBuf::from)
            .unwrap_or(default_shared_dir);

        // now extract the steps
        // extract the steps table
        let steps_array = config
            .get("steps")
            .context("Steps need to be provided")?
            .as_table()
            .context("Steps need to be a TOML table")?;

        let mut steps: Vec<ConfigStep> = Vec::new();
        for (k, v) in steps_array.iter() {
            let step_table = v.as_table().context("Step needs to be in table form")?;

            steps.push(ConfigStep::from_toml_table(k, step_table)?);
        }

        let mut step_dependencies_map: HashMap<String, HashSet<String>> = HashMap::new();
        // Build the step dependency map using ascii_dag
        for step in &steps {
            let start = vec![step.uid.clone()];

            let step_dependencies =
                ascii_dag::layout::generic::traversal::collect_all_nodes_fn(&start, |x| {
                    steps
                        .iter()
                        .find(|y| *y.uid == **x)
                        .expect("Depends on contains step that doesn't exist")
                        .depends_on
                        .clone()
                });

            // convert to hashset
            let step_dependencies_set: HashSet<String> =
                step_dependencies.into_iter().collect::<HashSet<String>>();

            // add to hashmap
            step_dependencies_map.insert(step.uid.clone(), step_dependencies_set);
        }

        // Now everything is loaded, create the StudyConfig, the Design dataframe, and the StudyController
        let study_config = StudyConfiguration {
            name: name,                 // name of the study
            design_path: design_path,   // path to the design.csv
            run_dir: run_dir,           // directory where runs are at
            evidence_dir: evidence_dir, //
            study_dir: config_path
                .parent()
                .ok_or_else(|| anyhow!("Unable to find study directory"))?
                .to_path_buf(),
            step_dependencies: step_dependencies_map,
            shared_dir: shared_dir,
            steps: steps,
        };
        Ok(study_config)
    }

    /// Returns all the variables in this step + dependent steps
    /// This is useful in the controller to figure out all the Branch dependencies
    pub fn get_all_dependent_step_variables(
        &self,
        step: &ConfigStep,
    ) -> Result<HashSet<String>, Error> {
        let mut step_dependent_variables: HashSet<String> = HashSet::new();

        // first add the own step.variables
        for v in &step.variables {
            step_dependent_variables.insert(v.clone());
        }

        // now do the same with all the upstream steps
        for c_step_uid in self
            .step_dependencies
            .get(&step.uid)
            .ok_or(anyhow!("unable to find step"))?
        {
            let c_step = self
                .get_step_by_uid(&c_step_uid)
                .ok_or(anyhow!("unable to find step {c_step_uid}"))?;

            for v in &c_step.variables {
                step_dependent_variables.insert(v.clone());
            }
        }

        Ok(step_dependent_variables)
    }

    /// Returns an ASCII-based Directed Acyclic Graph from visualization purposes
    fn get_ascii_dag(&self) -> String {
        // construct the step dependencies
        let mut graph = ascii_dag::DAG::new();
        let mut string2idx: HashMap<String, usize> = HashMap::new();

        // first go through and add all steps
        let mut idx: usize = 1;
        for step in &self.steps {
            let uid = &step.uid;

            graph.add_node(idx, uid);
            string2idx.insert(step.uid.clone(), idx);

            idx = idx + 1;
        }

        // now add the connections
        for step in &self.steps {
            let cur_i = string2idx
                .get(&step.uid)
                .expect("Key is missing, likely bug");

            for s in &step.depends_on {
                // get the indices
                let i = string2idx.get(s).expect("Key is missing, likely bug");

                // Ascii_dag does not like it when a node points to itself
                if &i != &cur_i {
                    graph.add_edge(i.clone(), cur_i.clone(), None);
                }
            }
        }

        let ir = graph.compute_layout();

        ir.render_scanline()
    }

    /// Renders the ASCII version of the DAG
    pub fn render_ascii_dag<W: Write>(&self, out: &mut W) -> Result<(), Error> {
        let ascii_dag_string = self.get_ascii_dag();
        write!(out, "{ascii_dag_string}")?;

        Ok(())
    }

    /// Returns a set of all input variables that exist
    pub fn get_input_variables(&self) -> HashSet<String> {
        let mut input_variables: HashSet<String> = HashSet::new();
        for s in &self.steps {
            for i in &s.variables {
                input_variables.insert(String::from(i));
            }
        }

        input_variables
    }

    // find and return step by name
    pub fn get_step_by_uid(&self, step_uid: &str) -> Option<&ConfigStep> {
        for step in &self.steps {
            if step.uid == step_uid {
                return Some(step);
            }
        }
        return None;
    }

    pub fn render_inspect_to_screen<W: Write>(&self, out: &mut W) -> Result<(), Error> {
        // perform checks

        // step check
        let mut config_step_checks: HashMap<&String, ConfigStepVerification> =
            HashMap::with_capacity(self.steps.len());
        for config_step in &self.steps {
            let v = ConfigStepVerification::check(&self, &config_step);
            config_step_checks.insert(&config_step.uid, v);
        }

        writeln!(out, "")?;
        writeln!(out, "Study")?;
        writeln!(out, "=====")?;

        self.render_header(out)?;

        writeln!(out, "Steps")?;
        writeln!(out, "-----")?;
        for config_step in &self.steps {
            let step_verification = &config_step_checks[&config_step.uid];
            Self::render_step(out, config_step, step_verification)?;
        }

        writeln!(out, "Directed Acyclic Graph")?;
        writeln!(out, "----------------------")?;
        self.render_ascii_dag(out)?;

        Ok(())
    }

    pub fn render_header<W: Write>(&self, out: &mut W) -> Result<(), Error> {
        let study_name = &self.name;
        let run_dir = &self.run_dir.to_string_lossy();
        let evidence_dir = &self.evidence_dir.to_string_lossy();
        let design_path = &self.design_path.to_string_lossy();

        writeln!(out, "  Study name:  {study_name}")?;
        writeln!(out, "  Run directory: {run_dir}")?;
        writeln!(out, "  Evidence directory: {evidence_dir}")?;
        writeln!(out, "  Design File path: {design_path}")?;
        writeln!(out, "")?;

        Ok(())
    }

    /// Writes Step information, including the results of checks
    fn render_step<W: Write>(
        out: &mut W,
        step: &ConfigStep,
        verification: &ConfigStepVerification,
    ) -> Result<(), Error> {
        let step_uid = &step.uid;
        write!(out, "[{step_uid}]")?;
        match verification {
            ConfigStepVerification::Passed => writeln!(out, "  PASS  ")?,
            ConfigStepVerification::Failed(config_step_failures) => {
                writeln!(out, "  FAIL  ")?;
                for config_failure in config_step_failures {
                    writeln!(out, "    {config_failure}")?;
                }
            }
        }
        let depends_on_str = step.depends_on.join(", ");
        writeln!(out, "  Depends On: {depends_on_str}")?;
        let variables_str = step.variables.join(", ");
        writeln!(out, "  Variables: {variables_str}")?;
        writeln!(out, "")?;
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq)]
enum ConfigStepVerification {
    Passed,
    Failed(Vec<ConfigStepFailure>),
}

#[derive(Clone, PartialEq, Eq)]
enum ConfigStepFailure {
    MissingDependentStep(String),
    MissingVariable(String),
}

impl ConfigStepVerification {
    /// Perfoms Verification of ConfigStep based on the StudyConfiguration.
    /// Checks:
    ///     - StudyConfigution contains all steps defined in depends_on
    ///     - Variables are recognized in the StudyConfiguration
    fn check(
        study_config: &StudyConfiguration,
        config_step: &ConfigStep,
    ) -> ConfigStepVerification {
        let mut failures: Vec<ConfigStepFailure> = Vec::with_capacity(config_step.depends_on.len());

        // check depends_on
        for dependent_config_step_uid in &config_step.depends_on {
            match study_config.get_step_by_uid(dependent_config_step_uid) {
                Some(_) => {}
                None => failures.push(ConfigStepFailure::MissingDependentStep(
                    dependent_config_step_uid.clone(),
                )),
            };
        }

        // check variables
        for variable in &config_step.variables {
            match study_config.get_input_variables().contains(variable) {
                true => {}
                false => failures.push(ConfigStepFailure::MissingVariable(variable.clone())),
            }
        }

        match failures.is_empty() {
            true => ConfigStepVerification::Passed,
            false => ConfigStepVerification::Failed(failures),
        }
    }
}

impl std::fmt::Display for ConfigStepFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigStepFailure::MissingDependentStep(step) => {
                write!(f, "Missing Dependent Step: {step}")
            }
            ConfigStepFailure::MissingVariable(variable) => {
                write!(f, "Missing Dependent Step: {variable}")
            }
        }
    }
}

/// Holds a step definition from the configuration TOML
#[derive(Debug)]
pub struct ConfigStep {
    pub uid: String,
    pub run_exe: TemplatedString,
    pub run_args: Vec<TemplatedString>,
    pub depends_on: Vec<String>,
    pub variables: Vec<String>,
}
impl ConfigStep {
    /// Extracts a ConfigStep from the TOML Table in a Config.toml file. Because the step name is in the Step entry, it
    /// needs to be passed in, and cannot be extracted from the Step table itself
    ///
    fn from_toml_table(key_name: &str, table: &Table) -> Result<ConfigStep, Error> {
        // parse
        let uid = String::from(key_name);
        let run_exe_path = TemplatedString::from_parsed_string_with_context(
            &ParsedString::from_string(&extract_string_from_table(table, "run_exe")?)?,
            &uid,
        )?;

        /// This function converts and checks an optional TOML entry that should be Vec<String>,
        /// If there is a parsing error, returns Error. Otherwise returns Option<Vec<String>>, which
        /// is empty if nothing is provided
        fn extract_vector_str(
            table: &toml::Table,
            key_name: &str,
        ) -> Result<Option<Vec<ParsedString>>, Error> {
            let res = match table.get(key_name) {
                None => Ok(None),

                Some(x) => {
                    let arr = x
                        .as_array()
                        .context(format!("Values in {key_name} need to be Strings"))?;

                    // explicit, loop through the array making sure everything is a string
                    let mut items = Vec::with_capacity(arr.len());
                    for (i, item) in arr.iter().enumerate() {
                        let s = item.as_str().with_context(|| {
                            format!("field `{key_name}` element {i} was not a string")
                        })?;
                        let parsed_str = ParsedString::from_string(s)?;
                        items.push(parsed_str);
                    }

                    Ok(Some(items))
                }
            };
            res
        }

        // extract the string vector, let it be empty Vec if not provided
        let run_args_no_context: Option<Vec<ParsedString>> = extract_vector_str(table, "run_args")?;
        let run_args = run_args_no_context
            .unwrap_or_default()
            .iter()
            .map(|x| TemplatedString::from_parsed_string_with_context(x, &uid))
            .collect::<Result<Vec<TemplatedString>, Error>>()?;

        // use the run_args to extract any dependencies and variable dependecies
        let mut depends_on: Vec<String> = Vec::new();
        let mut variables: Vec<String> = Vec::new();
        for parsed_string in &run_args {
            for string_part in &parsed_string.parts {
                match string_part {
                    TemplatedStringPart::Step { name, loc: _ } => {
                        // don't include itself
                        if name != key_name {
                            depends_on.push(String::from(name))
                        }
                    }
                    TemplatedStringPart::StudyVariable(v) => variables.push(String::from(v)),
                    _ => {}
                }
            }
        }

        // make the step and return
        let step = ConfigStep {
            uid: uid,
            run_exe: run_exe_path,
            run_args: run_args,
            depends_on: depends_on,
            variables: variables,
        };

        return Ok(step);
    }
}

impl std::fmt::Display for ConfigStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let step_name = &self.uid;

        let mut run_args_str = String::from("Run Args: ");
        for ts in &self.run_args {
            run_args_str.push_str(&format!("{ts} \n          "));
        }

        let depends_on_str = &self.depends_on.join(", ");
        let variables_str = &self.variables.join(", ");

        writeln!(f, "Step Name: {step_name}")?;
        writeln!(f, "{run_args_str}")?;
        writeln!(f, "Depends On: {depends_on_str}")?;
        writeln!(f, "Variables: {variables_str}")?;
        Ok(())
    }
}

// Extracts a string from the table, returning an Option if any problems encountered
fn extract_string_from_table_optional(table: &Table, key_name: &str) -> Option<String> {
    table
        .get(key_name)
        .map(|x| x.as_str().unwrap_or_default())
        .map(String::from)
}

// Extracts a string from the table, returns a Result to track any problems are encountered
fn extract_string_from_table(table: &Table, key_name: &str) -> Result<String, Error> {
    let x = table
        .get(key_name)
        .with_context(|| format!("{key_name} must be provided"))?
        .as_str()
        .with_context(|| format!("{key_name} must be a TOML string"))?
        .to_string();
    Ok(x)
}
