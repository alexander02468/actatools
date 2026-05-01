# ActaTools

ActaTools is a set of lightweight command-line tools for managing reproducible, and traceable computational studies by scaffolding 
workflows with config-based setup, orchestration, study design, and BLAKE3 evidence bundling.

## Features

- Lightweight in scope and size
- File-based run and dependency tracking
- Provenance control through BLAKE3 backed evidence package bundling
- Repeatability by systematically defined Study Configurations

## Binaries

ActaTools consists of two separate binaries that can be used independently of each other.

| Name             | Binary         | Primary Usage                                                         |
|------------------|----------------|-----------------------------------------------------------------------|
| ActaRecords      | `acta-records` | Evidence generation and tracking                                      |
| ActaStudy        | `acta-study`   | Study configuration, Step creation/dependency, and Study execution    |

## Installation

Binaries are pre-compiled for Linux, MacOSX, and Windows. Download the latest appropriate binary from [Dist], and
copy it along your Path.

### On Linux

On Linux, the personal folder is commonly at `~/bin` or `~/.local/bin`. For ActaRecords:

``` bash
cp /dist/0.2/acta-records ~/bin
```

You can check that ActaRecords is on your path by trying to see its version

``` bash
acta-study --version
```

### On Windows

Windows binaries are provided on a best-effort basis. They are built by CI and basic tests are run, but most development and support effort is focused on Linux.

[Not yet implemented]

## Examples

This section is designed as a Quick-Start guide. Please see the associated Acta-Records or Acta-Study User Guide for more detailed documentation.

### ActaRecords - Evidence bundling, verification, and comparison

ActaRecords is the tool designed for lightweight evidence generation and tracking. It uses a minimal Includes File to create
a record of the files that should be hashed and recorded.

Because ActaRecords is lightweight, it is straightforward to integrate into existing workflows. A minimal includes
file and a command-line call can bundle the artifacts from existing workflows.

An example can be found in `examples/acta-records/`.

Here you should find the following:

```bash
example_file_1.txt
example_file_1_copy.txt
example_file_2.txt
record.includes
```

The example [Includes File](/examples/acta-records/record.includes) includes all the example files in the folder. 
To create a manifest, use the `record` command.

``` bash
acta-records record record.includes record.json
```

This creates a Record, `record.json`. This file stores the hash for each of the example files -- it
is generated on the file contents themselves, not metadata. For example, the hash values for `example_file_1.txt` and
`example_file_1_copy.txt` `record.json` are the same, even though the filenames, and thus metadata, is different. These
hash values are portable across filesystems and CPU architectures.

``` bash
  "record_entries": [
    {
      "file": "./example_file_1.txt",
      "digest": "756b2e6e302e051ac26eb904f3e3216c61b83933f5b2c9e349e525aef440ea0a"
    },
    {
      "file": "./example_file_1_copy.txt",
      "digest": "756b2e6e302e051ac26eb904f3e3216c61b83933f5b2c9e349e525aef440ea0a"
    },
  ]
```

The command `bundle` bundles the files referenced in the Includes File to a folder. It also creates a Record in that folder
named `manifest.json`.

``` bash
acta-study bundle record.includes foo/
```

The folder `foo/` will have all the files from `record.includes` along with the Record `manifest.json`.

The command `compare` compares two Records, aligning files and checking their hash values against one another.

``` bash
acta-study compare record.json test/manifest.json
```

This prints a summary of the comparison -- there should be no differences since recorded identical data.

The command `verify` verifies an existing Record by re-hashing all the files and creating a comparison report against the
new Record. This will expose file changes since the Record was originally recorded.

``` bash
acta-study verify record.json
```

### ActaStudy - Study configuration and orchestration

ActaStudy is the tool designed to manage orchestration of studies through a TOML-based Configuration File. An example
study can be found in `/examples/acta-study/`.

Two main files define a study:

- *[Configuration File](/examples/acta-study/config.toml)* : TOML formatted. Defines global parameters and Steps. Step dependencies 
and study variables are inferred from this file.

- *[Design File](/examples/acta-study/design.csv)* : CSV formatted. Defines the Study variables and their values. Each row corresponds
to a Variation.

The command `inspect` inspects a Configuration File, displaying detected files, detected dependencies, and constructs
a directed acyclic graph for inspection.

``` bash
acta-study inspect config.toml
```

Example Step information, showing dependencies, variables, and check results:

``` bash 
[Postprocess_stress]  PASS  
  Depends On: RunSolver
  Variables: 
```

Example directed acyclic graph showing dependency structures between defined Steps:

``` bash
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

The subcommand `status study` is useful to see an overview of Variations and Steps along with their Ids and run status.

``` bash
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

## Limitations

ActaTools is in active development and new features are being implemented. Additionally, robustness, documentation, and 
test coverage is currently weak, so please be aware that certain functionalities may be brittle. Linux workflows
are being prioritized so Windows based edge-cases (for example, path handling) are not well supported.

Known limitations include:

### ActaRecords

- Older SHA-256 hashing is not implemented.
- File-matching for Record comparison is done by filename only. More sophisticated matching such as using the
  extended path is not implemented.
- Report (for example, from `compare` or `verify`) output to files is not yet implemented. As a workaround, the stdout can be 
redirected to a file.

### ActaStudy

- Detached running of Steps is not implemented.
- Parsing of template strings is not robust against ill-formed Strings in Step definitions.
- Only TOML and CSV files are currently supported.
- As relative paths have not been robustly tested, it is recommended to run studies from the Study root, which is defined
by the `config.toml`.
- Configurable status logging is not supported.
- Depends on Pola.rs, which is overkill and inflates the binary size.

## Issues

If you encounter any issues or have any suggestions, please open an issue on GitHub.

## Author

Alexander Baker

## License

ActaTools is licensed under the GNU General Public License v3.0. See the [LICENSE](LICENSE) file for details.
