# ActaRecords user guide

Version 0.3.0

## Introduction

ActaRecords is a lightweight tool to aid in provenance capture and evidence bundling. It is much lighter and has less features than version tracking systems such as DataLad or DVC, and specializes in local or mounted filesystems. The flip side is that it integrates very easily into any workflow. 

Specific use cases include:

- "There seems to be another workflow that seems the same, but is it?"
- "Did these files somehow change between runs?"
- "We just optimized the solver, but do the results change and at what point in the workflow?"

Specific use cases do *not* include:

- "Can we roll back that workflow to the one two months ago?" --> use version control Git
- "Who changed that file and why?" --> use version control Git
- "Can our team share the same dataset and track the changes?" --> use document tracking via DataLad
- "We need to document the runtime environment of our Python script so we can reproduce it later --> use Docker

In short, ActaRecords is for: 
 > Small research teams that have established workflows, and need to start tracking script/data integrity, bundle evidence, or implement workflow regression testing with minimal effort.

## Installation

Prebuilt binaries are published as assets under the GitHub Releases. The installation process is download
the binaries locally, then put them along your command line path, or call them directly. These binaries are
self-contained and portable.

Release Builds are provided for Linux, MacOS, and Windows. However, only Linux and MacOS are actively tested while Windows builds
are done through the Github CI and are provided "as-is".

### On Linux

On Linux, the personal folder is commonly at `~/bin` or `~/.local/bin`.

For ActaRecords:

``` bash
cp acta-records ~/bin
```

You can check that ActaRecords is on your path by trying to see its version

``` bash
acta-records --version
```

## System Support

Linux is the primary supported deployment. Release build are built with Ubuntu, however, it is expected that the compiled binaries will work on any modern Linux-based system.

MacOS is also tested and supported.

Windows is released "as-is" and is built only via Github.

## Commands

### `stdin` and `stdout`

Several commands can utilize `stdin` for path inputs all commands use `stdout` for displaying the results. Specifically, both `record`, `verify`, and `bundle` can use `stdin`. By convention, the argument after the command is then `-` but this is optional -- omitting this will still work.

Thus, these two are equivalent:
```
file1 file2 | acta-records record - --output output.json
file1 file2 | acta-records record --output output.json
```

If no `--output` is specified, the output is written to `stdout`. While convenient, note that all paths are relative to your current directory *not* where the output is written (as there is no way to know where you are redirecting the output). As such, it is recommended you use the `--output` option when possible.

### `record`

`record` is the primary command to hash file contents and construct Records (i.e., a file manifest). The most common usage is by passing it a list of files. If `--output` is not specified, it will print the JSON to the `stdout` (the screen).

``` bash
acta-records record example_file_1.txt example_file_1_copy.txt example_file_2.txt --output record.json
```

> You can also instead redirect the stdout to a file, but relative paths cannot be inferred if it is redirected to another folder.

`record` can also read from stdin, which allows for chaining through pipes.

``` bash
ls *.txt | acta-tools record - > record.json
```

`record` can also read an Includes File (with option `--includes-file`) which is a text file with a list of files to include. An example can be found in [/examples/acta-records/record.includes](/examples/acta-records/record.includes).

`record` creates a Record, in these examples named `record.json`. This file stores the content hash for each of the example files -- it is generated on the file contents, *not* metadata. For example, the hash values for `example_file_1.txt` and
`example_file_1_copy.txt` in `record.json` are the same, even though the filenames, and thus metadata, are different. The
content hash values are portable across filesystems and CPU architectures.

Hashes are 32 bytes (256 bit) in length, represented in these files as a 64 character Hex string.

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

Note that the `file` location is relative to where the Record is written, *not* to the current directory. This increases portability, as long as the Record and its hashed files are moved together, the Record remains well-defined.

### `verify`

The command `verify` re-hashes the files in a Record(s) and verifies that they have not changed. A `--verbose` option 
lists a full report for each input. Multiple Records can be verified at once and each input is given a line in the on
the output. Outputs are written to `stdout`.

``` bash
acta-records verify record.json
```

Multiple files and reading in via `stdin` is supported as welll.

```bash
acta-records verify record.json record1.json
ls *.json | acta-records verify
```




### `compare`

The command `compare` compares two Records, aligning files and checking their hash values against one another. File alignment is a potential issue as different records, likely being generated at different times, can have different paths -- making it difficult to match each entry to each other.

Entries are aligned using a tiered approach, attempting to align by:

1. hash-value
2. full-path
3. filename

Entries that cannot be uniquely matched are grouped together as "ambiguous" in the comparison report. 

``` bash
acta-study compare record.json test/manifest.json
```

This prints a summary of the comparison -- there should be no differences since recorded identical data.





### `bundle`

The command `bundle` bundles a set of a file together into a directory. It is intended as an easy way to create an evidence or archival bundle of files.

Contrary to the other commands, the argument `--output-dir` is *not* optional, as there needs to be a target directory.

Like the `record` command, files can be "piped" in, or an specified in an Includes File (via `--includes-file`). Importantly, the files are "flattened" -- no directory information is preserved. A single Record is created within the directory, call `record.json`, which records the hash values for each included file.

From the `examples/acta-records/` folder, each are equivalent from a Bash terminal:

``` bash
acta-records bundle example_file_1_copy.txt example_file1.txt example_file_2.txt --output-dir bundle_dir
acta-records bundle --includes-file record.includes --output-dir bundle_dir
ls *.txt | acta-records bundle --output-dir bundle_dir
cat record.includes | acta-records --output-dir bundle_dir
```

##