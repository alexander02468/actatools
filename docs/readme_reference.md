---
title: ActaTools Technical Overview and Evaluation (DRAFT)
subtitle: Revision 0.1.1
date: March 2nd, 2026
output: pdf_document
geometry: margin=1in
header-includes:
  - \usepackage{fancyhdr}
  - \pagestyle{fancy}
  - \fancyfoot[CO,CE]{CONFIDENTIAL EVALUATION DRAFT}
  - \fancyfoot[LE,RO]{\thepage}
  - \rhead{\textit{Revision 0.1.1}}
  - \usepackage[section]{placeins}
  - |
      \pretocmd{\subsection}{\FloatBarrier}{}{\errmessage{Failed to patch \subsection}}
---

## 0. Preface

*IMPORTANT: this document/tool is a working draft and its content may change in future releases.
As such, please do not distribute.*

This document serves to create an outline for the scope and interface of the ActaStudy software.

## 1. Summary

Computational study management is a tedious task that is often neglected, performed inconsistently, or
addressed post-hoc after problems are discovered. This may lead to inconsistent and non-reproducible code, or
create extra work as simulations need to be verified/re-run post-hoc when inconsistencies
are discovered.

Existing tools include enterprise solutions such as simulation process and data management solutions like Ansys
Minerva, orchestration managers such as Dagster or Airflow, or quality management systems such as Veeva.
These are often invasive, overbuilt with enterprise-level features, and costly for small to medium sized teams.

ActaTools is a lightweight computational study management tool that provides a scaffold around existing computational
pipelines to improve reproducibility, traceability, and data management. It is specifically designed for
engineers and researchers in small to medium sized teams that use multi-stepped workflows so that they can manage
large scale studies and their associated workflows. ActaTools is a command line interface tool that uses
small configuration files to set up a study. It includes a lightweight orchestration manager, optional study designer,
and SHA-256 hash based input/execution/output evidence bundler.

These features represent a minimal setup to achieve reproducibility, traceability, and data management without the
overhead needed by other systems.

## 2. Problem statement

The primary pain in computational workflow study management is that management systems cost time and energy
to set up, use, and maintain. Additionally, the cost of not having a clean system is delayed and probabilistic:
problems are only encountered later and sometimes not at all. However, when problems occur, they are costly in time,
effort, and resources.

For example, the following can occur:

- Codebase drift: as improvements and features are added, it is realized that the results from previous studies
have changed and can no longer be replicated. Additionally, it is not clear which change or changes caused this.

- Untraceable results: results are examined from an older study. It is not clear which code or inputs created the study
and it cannot be reproduced.

- Multiple versions found in the wild: when examining a workflow from a co-worker, it is discovered that you both
are working on different versions of the codebase. It is unclear what is different between these versions, what
implications there are, and why these differences exist.

- Different management systems are found between team members: sometimes team members create their own solutions for study
management which, while each sufficient on their own, make comparisons across the systems difficult. This can occur
when one wants to compare the inputs and outputs of one study run by one team member with the inputs and outputs
of another study that was run by a different team member.

Each of these example problems require costly fixes. Often time has to be spent reviewing old code, running `diff`
between different codebase files, and reconstructing a timeline of changes.

## 3. Scope

ActaTools is designed to:

- Be minimally invasive to existing workflows
- Have a relatively straightforward and script-friendly interface
- Track which inputs and execution environment correspond with which results
- Create and package evidence of each result
- Provide tools for management of study design
- Provide tools to detect differences between outputs across different studies
- Provide tools for straightforward workflow orchestration or interface with existing orchestrators such as SLURM or
LSF

ActaTools is *not* designed to:

- Be a resource-aware optimized task scheduler such as SLURM or LSF
- Create a centralized database of study results such as simulation and process management systems
- Be a quality management system with approvals and regulatory checks
- Replace traditional software development tools such as Git, Docker containers, or Continuous Integration/Continous
 Deployment services
- Determine which key output metrics should be tracked and compared across studies
- Analyze or provide commentary of results

A successful ActaTools deployment would allow for the following scenarios:

- Edge case errors can be immediately localized to particular parts of a workflow that failed
- Differences in code/outputs can be localized to particular parts of the workflow
- During development, a particular part of the workflow can be iterated without the cost of running the entire
workflow
- An immediate output difference with an existing simulation is detected when a new feature is added
- A reviewer can see the inputs and execution environment that created any tracked output upon request

## 4. Core concepts

### 4.1 Step

A Step is the smallest element of a Study. It consists of inputs (defined by a set of TOML
files), an executor script + executable (command call), and an output path. Dependencies can be specified
indicating that other steps must be finished before the indicated step can be run.

![Components of a Step, inputs/outputs/dependencies](docs/Step.png){ width=3.5in }

### 4.2 Variation

A Variation is an isolated directed graph with chosen inputs at each branching point. This corresponds to a single line
in the `design.csv` file. A single Variation can share Steps with other variations, depending on how the Steps are set
up and where in the Variation a branching occurs. This reduces unnecessary computations.

![Example Variation (Solver Step is highlighted) which consists of many steps.](docs/Variation.svg)

### 4.3 Study

A Study is all the Variations combined and represents the largest tracked entity by ActaTools. Study level
configuration, which describes the Variation directories and the inter-dependencies between the Steps. This description
is given in the `config.toml` file which should be placed at the study root directory.

The list of Variations contained in the study is described in the `design.csv` file. The `design.csv` file can be
created manually, or via the `acta design` interface which is described in more detail in the Command Line
Interface section of this document.

![Full example Study with 3 Variations. The branching occurs at the `Preprocess 3` Step. This means that `Preprocess 3`
has different inputs, depending on which Variation is run, and these inputs are specified in the `design.csv` file.
Note how `Preprocess 1` and `Preprocess 2` are shared across all 3 Variations and are only ran once.](docs/Variations.png)

### 4.5 The `config.toml` file

The `config.toml` is the heart of any study. It contains all the settings and run information related to the study,
which includes where things should be run, what should be run, and what inputs/outputs are. It is formatted in
Tom's Obvious Minimal Language (TOML) which is a minimal, unambiguous, and readable format.

Study level configuration is done using variable as the root directory, while steps are defined in an array of Steps.
A straightforward example is below, with only 3 steps:

```
study_name = "Example Study Configuration"
run_dir = "runs/"  # Where runs are ran

#####    STEPS   #####
## Step 1: Preprocess
[[steps]]
    name = "Preprocess1"
    depends_on  = []  # no dependencies
    run_exe = "bash.exe"
    run_script = "run.sh"
    output_files = ["output.json"] # these output files are for version tracking and evidence control

    [steps.inputs]
        preprocess_filepath = "input_Preprocess1.json"
        input_mesh = "$STUDY$/input_mesh.vtu"
        sleep_time = 100

## Step 2: Solver
[[steps]]
    name = 'RunSolver'
    depends_on = ["Preprocess"] # based on the names of the steps
    run_exe = "bash.exe"
    run_script = "run.sh"
    outputs = ["output_RunSolver.json"]

    [steps.inputs]
        Preprocess1_results = "$Preprocess1$/output.json"
        Solver_settings = "solver_settings.json"

## Step 3: Postprocess results
[[steps]]
    name = "Postprocess"
    depends_on = ["RunSolver"]   # based on the names of the steps
    run_exe = "bash.exe"
    run_script  = "run.sh"
    outputs = ["output.json", "report.md"]

    [steps.inputs]
    RunSolver_output = "$RunSolver$/output_RunSolver.json"
```

### 4.6 The `design.csv` file

The `design.csv` is a minimal file that conveys the design of the experiment. Column headers are used to indicate
the variable and the corresponding row values represent changes to that variable. Variables that are not found in the
`config.toml` are ignored and can be used by the user for other values associated with that row.

Each row corresponds to a Variation. The entire `design.csv` represents all the Variations in the study.

## 5. Command line interface overview

The command line interface uses a `git`-like operation where commands are chained with sub-commands to access
the different functions of the software.

The base commands are: `inspect`, `design`, `run`, and `status` and can chained with additional command-specific
sub-commands. For example, to run a particular step, one can use the call `acta run step <variation> <step_name>`.

![diagram of commands](docs/commands.svg)

## 6. Outputs

### 6.1 Evidence and provenance

Evidence is captured at the file level and stored in bundles at the step level. Each file's metadata is
cryptographically hashed using SHA-256 and then a Step manifest is created that stores the filenames and their
associated hash values. Hashing the metadata detects non-malicious file changes as every time a file is edited, the
metadata (for example, the modified time or the file size) changes. When comparing Steps, files
can be associated by both filename and also contents. What exactly is different or why they are different
would be determinable solely from this system.

If more tamper-resistance is needed such as for a regulatory submission, then the file contents themselves
can be hashed and the manifest can be cryptographically signed. In this scenario, any file changes can be detected
by changes in the file's hash values, or if a malicious actor also creates a new hash value, these hash value 
changes can be detected by the bundle's signed hash value. Only malicious tampering at the bundle level can be
detected. Only detection of a change is possible, not what is changed.

The bundle then stores a list of filenames, their associated cryptographic hash values along with a final
cryptographic hash value of the bundle itself. This manifest and its associated files can be moved around
and verified as a group across filesystems.

### 6.2 Drift detection and regression testing

Drift is when key outputs change as the codebase is slowly changed over time. Changes to the codebase are often due
to features added or bug fixes. These changes are often both unintended and unexpected. Early detection of changes
is important because if detected late, after many codebase changes are made, that increases the time needed to find
when and why the changes occurred.

Detection of the key output changes are best achieved through regression testing. With this, a reference study is
identified and the key outputs from any study can be compared. Steps are aligned as best as possible so that
differences, if they exist, can be localized to particular Steps. Furthermore, thresholds can be set for
quantitative outputs so that minor differences, which are common in finite element solvers even across identical
inputs, are ignored.

## 7. Typical usage scenarios

### 7.1 Example: Prospective research study (in-silico trial)

The research problem.

You have a working pipeline and just want to change the source dataset to perform an in-silico trial -- that is,
see something happening on a set of virtual patients.

In this case, the Variations are branched very early, often in the first step. Additionally, the input filenames are
likely hard-coded (for example, directed towards particular datasets). Using a `design` script is not particularly
useful here and it is easier to make the `design.csv` manually. The `design.csv` can be as minimal as a list of
input files.

A minimal `config.toml` might look something like this.

```
study_name = "In-Silico Trial"
run_dir = "runs/"  # Where runs are ran

#####    STEPS   #####
## Step 1: Preprocess
[[steps]]
    name = "Preprocess1"
    depends_on  = []  # no dependencies
    run_exe = "bash.exe"
    run_script = "run.sh"
    output_files = ["subject.mesh"] 

    [steps.inputs]
        subject_filepath = "$VARIABLE$"

## Step 2: Solver
[[steps]]
    name = 'RunSolver'
    depends_on = ["Preprocess"] # based on the names of the steps
    run_exe = "solve.exe"
    run_script = "run_subject.sh"
    outputs = ["results.json"]

    [steps.inputs]
        Preprocess1_results = "$Preprocess1$/subject.mesh"
        Solver_settings = "solver_settings.json"

## Step 3: Postprocess results
[[steps]]
    name = "Postprocess"
    depends_on = ["RunSolver"]   # based on the names of the steps
    run_exe = "bash.exe"
    run_script  = "run.sh"
    outputs = ["output.json", "report.md"]

    [steps.inputs]
    RunSolver_output = "$RunSolver$/output_RunSolver.json"
```

The corresponding `design.csv` can look as minimal as this:

```
subject_filepath
subject1.vti
subject2.vti
subject3.vti
```

Note that the `subject_filepath` corresponds to the input in the first Step ("Preprocess").

### 7.2 Example: Sensitivity/uncertainty analysis

The computational problem.

Since simulations are just a model, you want to see how your model responds to input parameters
that were perhaps arbitrary or estimated.

In this case, the commands from `design` can be helpful in constructing the Variations that you would want tested.
A minimal workflow example could look like this:

```
study_name = "In-Silico Trial"
run_dir = "runs/"  # Where runs are ran

#####    STEPS   #####
## Step 1: Preprocess
[[steps]]
    name = "Preprocess1"
    depends_on  = []  # no dependencies
    run_exe = "bash.exe"
    run_script = "run.sh"
    output_files = ["subject.mesh"] 

    [steps.inputs]
        applied_force = "$VARIABLE$"

## Step 2: Solver
[[steps]]
    name = 'RunSolver'
    depends_on = ["Preprocess"] # based on the names of the steps
    run_exe = "solve.exe"
    run_script = "run_subject.sh"
    outputs = ["results.json"]

    [steps.inputs]
        Preprocess1_results = "$Preprocess1$/subject.mesh"
        Solver_settings = "solver_settings.json"

## Step 3: Postprocess results
[[steps]]
    name = "Postprocess"
    depends_on = ["RunSolver"]   # based on the names of the steps
    run_exe = "bash.exe"
    run_script  = "run.sh"
    outputs = ["output.json", "report.md"]

    [steps.inputs]
    RunSolver_output = "$RunSolver$/output_RunSolver.json"
```

In Step 1 (Preprocess) there is an input parameter that controls a boundary condition, `applied_force`.
The `design` command can generate a sweep of different `applied_force` values.

`acta design sweep applied_force 100 500 5`

This appends to the `design.csv` and results in the following `design.csv` file:

```
applied_force
100
200
300
400
500
```

Now, with the `config.toml` and the `design.csv` specified, the study can be run using `acta run step next` or
`acta run variation next` until all Steps are complete.

### 7.3 Example: Regression testing [Draft]

The software development problem.

Since you are actively developing and adding methods to your workflow, your codebase and process can drift over time.
To capture and address any drift, it is common to perform regression testing. In this scenario, a baseline (already
run) Study is designated and serves as a reference. Then, the newer workflow is ran on the same inputs, and the 
results are checked for validity/drift against the reference.

### 7.4 Example: The regulatory submission [Draft]

The regulatory problem.

When submitting a study such as one of the previous examples, the regulatory agency demands that
things must be well-docuemnted. This generally means that there needs to be a system of provenance control --
documentation of what was run which includes the execution environment and the inputs/outputs. Additionally, these need
to be tamper resistant.

## 8. Tradeoffs and known limitations

ActaTools tries to walk the line between offering the tools and capabilities of enterprise solutions, which can be
large codebase and system overhauls, while not being invasive or forcing a particular way of working. This
is a balance.

By allowing each user to customize their own study workflow, this makes it more difficult to offer general
features for all users as a one-stop function. For example, automated regression testing would require custom
scripts to create new studies in the same configuration as existing, and pre-selection of key outputs.

The known tradeoff and limitations can be summarized below:

- Lightweight means less predictable setups which makes one-stop functions difficult to make work for all users
- Custom solver deployments requires effort to interface with the solver and likely needs custom
  wrapper scripts. These may be solver and machine dependent
- Integration with some orchestration managers such as SLURM is both manager specific (that is, SLURM would
be different than LSF), and also the specific to the settings on the cluster itself
- It is difficult to capture execution environments with dynamically typed and interpreted languages such as Python
and R. Better accuracy would be to track Docker deployments, however this is a heavier change if users
are not using containers. Additionally, containers are often restricted on cluster environments.
- A file-based tracking system is used for orchestration management which means that it is portable as no software
specific files are used at all, and no long-running processes are needed to manage everything. However, it also
means that results are more accidentally editable, and also that large studies of greater than 100,000 Variations
are generally not easily supported (but this may be changed in the future).

Each of these limitations can be mitigated with custom scripts or altered workflows, but difficult to offer a
solution within the scope of ActaTools.
