# ActaTools

ActaTools is a set of lightweight command-line tools for managing computational studies with config-based setup, orchestration, and BLAKE3 evidence bundling inside existing workflows.

## Features

- Lightweight in scope and size
- File-based run and dependency tracking
- Provenance control through BLAKE3 backed evidence package bundling
- Repeatability by systematically defined Study Configurations

## Why Actatools?

Data provenance and workflow orchestration is crowded with many other existing tools. Where does ActaTools fit in?

### It is light

The Rust compiled binaries are small, and there are no dependencies. It is compiled which leads to relatively fast performance. Furthermore, it uses BLAKE3 hashing which is substantially faster than SHA-256.

### It is portable

The binaries are statically compiled with dependencies only on the CPU architecture and OS. This means that you can just
copy and paste it almost on any computer and it will likely work. There are no dependencies -- no Python needed, no git
calls underneath, and no docker.

Study configuration is a TOML file that can be placed at the top of any study. Run statuses are local file-based,
no general database needs to be installed.

It's not that batteries are included so much as batteries aren't needed.

### It is flexible

Being lightweight and command-line means it can be wrapped through whatever scripts or tools you usually for your projects. Want to include a manifest of your Python scripts? Just add a `subprocess.run()` call where you want. Using Bash scripts on your HPC? Drop the file onto your home directory and add `~/actarecords record my_file > record.json` at the end. Or if
you just want to generate a manifest of all your files at the end, just list and pipe into ActaTools,
`ls study/* | actarecords record - > study_manifest.json`

You can use it as part of a larger regulation compliance scheme, script version tracking, or regression testing.

### What about other tools?

By design, ActaTools sits in a comfortable niche that no other tools currently occupy. Most other tools are larger
and more cumbersome -- difficult to integrate into entrenched workflows or they make you buy into their workflow. Or the other end is a bunch of small tools that you would need to script around.

ActaTools is for the small research group that works mainly locally, and needs to have data and script provenance, or
a small tool to organize their studies. It is not replacing big orchestration tools like SnakeMake or distributed
database management like DataLad.

It is designed to be the step before reaching for those larger solutions.

## Binaries

ActaTools consists of two separate binaries that can be used independently of each other.

| Name             | Binary         | Primary Usage                                                         |
|------------------|----------------|-----------------------------------------------------------------------|
| ActaRecords      | `acta-records` | Evidence generation and tracking                                      |
| ActaStudy        | `acta-study`   | Study configuration, Step creation/dependency, and Study execution    |

## Status

ActaTools is an *early-stage project*. As such, there will be features that have *not* been thoroughly implemented or
tested. 

In general, ActaRecords is much more mature than ActaStudy.

### ActaRecords

Primary effort is going towards ActaRecords, as that is tightly scoped and has immediate benefit to teams that need
to add a provenance layer. The command-line interface allows evidence bundling ad-hoc with user scripts and can
be directly integrated into existing workflows, or batched at the end.

Status: Stable. Core features are implemented. Robustness and hardening is ongoing, and minor optional features are being added.

### ActaStudy

Minimal effort is currently going towards ActaStudy, as that is more ambitious use-case, but with possibly lower broad-appeal as it does require changes to workflows. As such, it cannot be easily inserted after a workflow has already been setup.

Status: Unstable. Core architecture and limited features are added. Robustness testing is lacking. Expect architectural changes to happen in future.

## Installation

Prebuilt binaries are published as assets under the GitHub Releases. The installation process is download
the binaries locally, then put them along your command line path, or call them directly. These binaries are
self-contained and portable.

Release Builds are provided for Linux, MacOS, and Windows. However, only Linux and MacOS are actively tested while Windows builds
are done through the GitHub CI and are provided "as-is".

## Examples

This section is a Quick-Start guide. Please see the associated [Acta-Records User Guide](/docs/acta-records%20user%20guide.md) or [Acta-Study User Guide](/docs/acta-study%20User%20Guide.md) for more detailed documentation.

### Record evidence and verify

Files can be hashed and recorded into a Record, which is a JSON file that stores the hashes associated with each file.

From the `/examples/acta-records` directory: 

``` console
$ acta-records record example_file_1.txt example_file_2.txt --output record_1.json
```

or more succinctly with using `stdin`

``` console
$ ls *.txt | acta-records record --output record_2.json
```
> Note this is a different Record than the last command because there are more files in this one.

This can be verified using the `verify` command:

```console
$ acta-records verify record_1.json record_2.json
record1.json                     VERIFIED 0ee13d328ff818...6069af8e44ffa3e
record2.json                     VERIFIED 818cfbdd9f7273...31cb4667963b8b4
```

which will print a verification summary for each input Record.

### Compare two Records for differences

You can compare two Records in detail using the `compare` command. Note that only two Records can be compared at once.

If you've ran the `record` commands above, you can compare the two Records.

``` console
$ acta-records compare record_1.json record_2.json
Record comparison
=================

Inputs
------

Record 1: record1.json
Record 2: record2.json

Summary
-------

  =  Same                   0
  ~  Changed                0
  !  Undetermined (1)       1
  !  Undetermined (2)       1
  -----------------------
     Total              2

Legend
------

  =  Same           record matched and digest is unchanged
  ~  Changed        record matched but digest changed
  !  Undetermined   matcher could not match record

No Change
---------
(None)

Changed
-------
(None)

Undetermined Record 1
---------------------

[0000] ! UNDETERMINED
  key:        (FileName)  example_file_1.txt
  Record:     example_file_1.txt
  digest:     756b2e6e302e051ac26eb904f3e3216c61b83933f5b2c9e349e525aef440ea0a 

Undetermined Record 2
---------------------

[0000] ! UNDETERMINED
  key:        (FileName)  example_file_2.txt
  Record:     example_file_2.txt
  digest:     a07f50a89a8b7cc7348c89f64545e84b8741022d6b937870a850c79a8119a3cb 
```

This attempts to match the files across the Records and then compares the hash values.

### ActaStudy - Study configuration and orchestration

> ActaStudy is less mature and should be considered experimental.

ActaStudy is the tool designed to manage orchestration of studies through a TOML-based Configuration File. An example
study can be found in `/examples/acta-study/`.

Two main files define a study:

- *[Configuration File](/examples/acta-study/config.toml)* : TOML formatted. Defines global parameters and Steps. Step dependencies 
and study variables are inferred from this file.

- *[Design File](/examples/acta-study/design.csv)* : CSV formatted. Defines the Study variables and their values. Each row corresponds
to a Variation.

The command `inspect` inspects a Configuration File, displaying detected files, detected dependencies, and constructs
a directed acyclic graph for inspection.

``` console
$ acta-study inspect config.toml
```

Example Step information, showing dependencies, variables, and check results:

``` console 
[Postprocess_stress]  PASS  
  Depends On: RunSolver
  Variables: 
```

Example directed acyclic graph showing dependency structures between defined Steps:

``` console
Directed Acyclic Graph
----------------------
      [Preprocess]
           ┌┘
           ↓
      [RunSolver]
           └┐
            ↓
  [Postprocess_stress]
```

The command `status` displays information regarding either the Study `status study`, a Variation `status Variation <Variation Id>`
or a Step `status Step <Step Id>`.

The sub-command `status study` is useful to see an overview of Variations and Steps along with their Ids and run status.

``` console
$ acta-study status study
Variations
----------
V1b42bb03936edb68
  sleep_time "10"
V03f41d5e9f2c560b
  sleep_time "1.5"

  VarStepId          Step Name      Run Status
----------------------------------------------------
  vs2d82b4a357a90634 RunSolver      Not-Initialized
  vsa99d15d276a20b43 Postprocess_st Not-Initialized
  vsd7f88979d321342c Preprocess     Not-Initialized
  vs8e80463615a48d20 Postprocess_st Not-Initialized
  vs789bff3b753fb971 RunSolver      Not-Initialized

```

The command `run` is used to execute a Step or a series of Steps continuously.

Use Sub-Command `next-step` to run the next available Step.

``` bash
acta-study run next-step
```

A particular Step can be run with its Id

``` bash
acta-study run step <Step_Id>
```

Steps can be run continuously according to a Variation or the entire Study

``` bash
acta-study run variation <Variation_Id>
acta-study run study
```

## Issues

If you encounter any issues or have any suggestions, please open an issue on GitHub.

## Author

Alexander Baker

## License

ActaTools is licensed under the GNU General Public License v3.0. See the [LICENSE](LICENSE) file for details.
