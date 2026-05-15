# ActaRecords user guide

Version 0.3.0

[1. Introduction](#1-introduction) \
[2. Installation](#2-installation) \
[3. System Support](#3-system-support) \
[4. Hashing, Reproducibility, and Provenance](#4-hashing-reproducibility-and-provenance) \
[5. Commands](#5-commands)

## 1. Introduction

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

## 2. Installation

Prebuilt binaries are published as assets under the GitHub Releases. The installation process is download
the binaries locally, then put them along your command line path, or call them directly. These binaries are
self-contained and portable.

Release Builds are provided for Linux, MacOS, and Windows. However, only Linux and MacOS are actively tested while Windows builds
are done through the GitHub CI and are provided "as-is".

### 2.1 On Linux

On Linux, the personal folder is commonly at `~/bin` or `~/.local/bin`.

For ActaRecords:

``` console
$ cp acta-records ~/bin
```

You can check that ActaRecords is on your path by trying to see its version

``` console
$ acta-records --version
```

## 3. System Support

Linux is the primary supported deployment. Release build are built with Ubuntu, however, it is expected that the compiled binaries will work on any modern Linux-based system.

MacOS is also tested and supported.

Windows is released "as-is" and is built only via GitHub.

## 4. Hashing and reproducibility

### 4.1 What are Hashes? Digests?

Hashes are the results of a Hash Function, which maps values onto another set of value using a Hash Function. They are commonly used in Hash Tables (e.g., Python `dict`), and also creation of a digest or cryptographic fingerprint.

For our usage, the idea is quite simple: can we use a number (or set of bytes) that uniquely identifies an object, and is portable across sessions, computers, storage systems, etc. In ActaRcords, we use a BLAKE3 hashing algorithm to go through all the bytes of a file and create a unique identifier. This single value, which represents the contents of the file compressed to a single byte array, is referred to as a *digest*. Hash function such as SHA-256 or BLAKE3 are designed to be very sensitive to the bytes -- single byte changes will intentionally create wildly different hashes. Therefore, small changes in a file are easily detectable.

A digest is a hash that is used to represent the data inside a file. 

### 4.2 Reproducibility and verification

One of the main uses for hashes and digests is to verify that a file is indeed, the same file as it purports to be. This is commonly used in data transfer, once the file is transfer, their hashes are checked against an independently sent digest. Sometimes, bytes can flip in transit and this prevents it.

From a study point of view, digests are used to detect changes in files. This can be input, results, or even the scripts themselves. Once the file is hashed and the digest is recorded, any changes to those files are easily detected. Having a digest of a file allows for the following scenarios:

1. Changes in a file are trivially detected. Any change to the file (intentional or not) is easily discovered by comparing the digest on record against a newly computed digest against the file.
2. Renamed files can be correctly matched against hashes. If a file is accidentally (or intentionally but forgotten) renamed, this can be detected by comparing the digest of an unknown file against all the digests of the old files.
3. Digests for the files can be compared across filesystems and architectures. The hashing algorithms (e.g. BLAKE3 or SHA-256) are all based solely on the bytes of the file and has nothing to do with the way it is stored (e.g. metadata). Therefore, a file on a completely different system can be checked with the hash values against an original digest. 

If the digest is also cryptographically signed, that means that a 3rd party injects their own hash-like function to create another value -- the fingerprint. This is to prevent malicious tampering of the files -- without it, a malicious actor can change the files and then just re-do the hash and create a new digest. Once the digest is signed, it is "sealed" and tampering can be detected. This is common in evidence bundles -- once the evidence is created for regulatory or archival purposes, it can be sealed and any tampering with those files would be immediately evident. 

## 5. Commands

### 5.1 `stdin` and `stdout`

Several commands can utilize `stdin` for path inputs all commands use `stdout` for displaying the results. Specifically, both `record`, `verify`, and `bundle` can use `stdin`. By convention, the argument after the command is then `-` but this is optional -- omitting this will still work.

Thus, these two are equivalent:
``` console
$ file1 file2 | acta-records record - --output output.json
$ file1 file2 | acta-records record --output output.json
```

If no `--output` is specified, the output is written to `stdout`. While convenient, note that all paths are relative to your current directory *not* where the output is written (as there is no way to know where you are redirecting the output). As such, it is recommended you use the `--output` option when possible.

### 5.2 `record`

`record` is the primary command to hash file contents and construct Records (i.e., a file manifest). The most common usage is by passing it a list of files. If `--output` is not specified, it will print the JSON to the `stdout` (the screen).

``` console
$ acta-records record example_file_1.txt example_file_1_copy.txt example_file_2.txt --output record.json
```

> You can also instead redirect the stdout to a file, but relative paths cannot be inferred if it is redirected to another folder.

`record` can also read from stdin, which allows for chaining through pipes.

``` console
$ ls *.txt | acta-tools record - > record.json
```

`record` can also read an Includes File (with option `--includes-file`) which is a text file with a list of files to include. An example can be found in [/examples/acta-records/record.includes](/examples/acta-records/record.includes).

`record` creates a Record, in these examples named `record.json`. This file stores the content hash for each of the example files -- it is generated on the file contents, *not* metadata. For example, the hash values for `example_file_1.txt` and
`example_file_1_copy.txt` in `record.json` are the same, even though the filenames, and thus metadata, are different. The
content hash values are portable across filesystems and CPU architectures.

Hashes are 32 bytes (256 bit) in length, represented in these files as a 64 character Hex string.

``` console
{
  "metadata": {
    "record_format": 1,
    "generated_by": "actatools",
    "library_version": "0.3.0",
    "digest_algorithm": "BLAKE3",
    "generated_at_utc": "2026-05-15 0:39:04.652917 +00:00:00",
    "meta_digest": "8aaae94c0881c59847caed389f4bb99d333f2d692becd0194210d2aaaca77fef"
  },
  "record_entries": [
    {
      "file": "example_file_1_copy.txt",
      "data_digest": "756b2e6e302e051ac26eb904f3e3216c61b83933f5b2c9e349e525aef440ea0a"
    },
    {
      "file": "example_file_1.txt",
      "data_digest": "756b2e6e302e051ac26eb904f3e3216c61b83933f5b2c9e349e525aef440ea0a"
    },
    {
      "file": "example_file_2.txt",
      "data_digest": "a07f50a89a8b7cc7348c89f64545e84b8741022d6b937870a850c79a8119a3cb"
    }
  ],
  "digest": "9007586de5dc6d0067aa6642beba182d860305108c3011c27d8709abb69d62ad"
}
```

Note that the `file` location is relative to where the Record is written, *not* to the current directory. This increases portability, as long as the Record and its hashed files are moved together, the Record remains well-defined.

### 5.3 `verify`

The command `verify` re-hashes the files in a Record(s) and verifies that they have not changed. A `--verbose` option 
lists a full report for each input. Multiple Records can be verified at once and each input is given a line in the on
the output. Outputs are written to `stdout`.

``` console
$ acta-records verify record.json
record.json                      VERIFIED 9007586de5dc6d...d8709abb69d62ad
```

which says that the contents of the record.json are all verified. The files are individually hashed and then those digests are again hashed to create a record digest. That is what is being shown -- but all files need to be the same in order for the final hash to be the same.

If you want to see the longer audit-like verification, you can pass the --long option.

``` console
$ acta-records verify record.json --long
Record Verification
===================

Summary
-------
record.json

               VERIFIED
     Expected  9007586de5dc6d0067aa6642beba182d860305108c3011c27d8709abb69d62ad
   --> Actual  9007586de5dc6d0067aa6642beba182d860305108c3011c27d8709abb69d62ad

  Num Records Verified  3
    Num Records Failed  0
  -----------------------------
                 Total  3

Record Entries
--------------
  File  example_file_1_copy.txt
                   VERIFIED
         Expected  756b2e6e302e051ac26eb904f3e3216c61b83933f5b2c9e349e525aef440ea0a
       --> Actual  756b2e6e302e051ac26eb904f3e3216c61b83933f5b2c9e349e525aef440ea0a

  File  example_file_1.txt
                   VERIFIED
         Expected  756b2e6e302e051ac26eb904f3e3216c61b83933f5b2c9e349e525aef440ea0a
       --> Actual  756b2e6e302e051ac26eb904f3e3216c61b83933f5b2c9e349e525aef440ea0a

  File  example_file_2.txt
                   VERIFIED
         Expected  a07f50a89a8b7cc7348c89f64545e84b8741022d6b937870a850c79a8119a3cb
       --> Actual  a07f50a89a8b7cc7348c89f64545e84b8741022d6b937870a850c79a8119a3cb
```

Here, all the files are shown with their actual digest and recalculated digests.

Multiple files read in via `stdin` are supported as well.

```console
$ acta-records verify record.json record1.json
$ ls *.json | acta-records verify
```

``` console
record_wrong.json                FAILED   90075...9d62ad -> 90075...9d62ad
record.json                      VERIFIED 9007586de5dc6d...d8709abb69d62ad
record1.json                     VERIFIED 0ee13d328ff818...6069af8e44ffa3e
record2.json                     VERIFIED 818cfbdd9f7273...31cb4667963b8b4
```

Note that a `record_wrong.json` is intentionally included here with the final digest slightly changed.

### 5.4 `compare`

The command `compare` compares two Records, aligning files and checking their hash values against one another. File alignment is a potential issue as different records, likely being generated at different times, can have different paths -- making it difficult to match each entry to each other.

Entries are aligned using a tiered approach, attempting to align by:

1. hash-value
2. filename

Entries that cannot be uniquely matched are grouped together as "undetermined" in the comparison report.

``` console
$ acta-study compare record.json record1.json

Record comparison
=================

Inputs
------

Record 1: record.json
Record 2: record1.json

Summary
-------

  =  Same                   1
  ~  Changed                0
  !  Undetermined (1)       2
  !  Undetermined (2)       0
  -----------------------
     Total              3

Legend
------

  =  Same           record matched and digest is unchanged
  ~  Changed        record matched but digest changed
  !  Undetermined   matcher could not match record

No Change
---------

[0000] = SAME
  key:        (FileName)  example_file_1.txt
  Record 1:     example_file_1.txt
  Record 2:     example_file_1.txt
  digest:     756b2e6e302e051ac26eb904f3e3216c61b83933f5b2c9e349e525aef440ea0a

Changed
-------
(None)

Undetermined Record 1
---------------------

[0000] ! UNDETERMINED
  key:        (FileName)  example_file_1_copy.txt
  Record:     example_file_1_copy.txt
  digest:     756b2e6e302e051ac26eb904f3e3216c61b83933f5b2c9e349e525aef440ea0a 
[0001] ! UNDETERMINED
  key:        (FileName)  example_file_2.txt
  Record:     example_file_2.txt
  digest:     a07f50a89a8b7cc7348c89f64545e84b8741022d6b937870a850c79a8119a3cb 

Undetermined Record 2
---------------------
(None)

```

Here, `record.json` had three files and `record1.json` had only one of those files. Thus, we see that one file is matched -- by filename. The hash values could not work because two values in `record.json` had the same digest.

The other two files in `record.json` (input 1) are unable to be matched against `record1.json` (input 2) and are printed at the end. It also prints the last attempt to match and what key it was using -- here it is trying to match based on filename.

### 5.5 `bundle`

The command `bundle` bundles a set of a file together into a directory. It is intended as an easy way to create an evidence or archival bundle of files.

Contrary to the other commands, the argument `--output-dir` is *not* optional, as there needs to be a target directory.

Like the `record` command, files can be "piped" in, or an specified in an Includes File (via `--includes-file`). Importantly, the files are "flattened" -- no directory information is preserved. A single Record is created within the directory, call `record.json`, which records the hash values for each included file.

From the `examples/acta-records/` folder, each are equivalent from a Bash terminal:

``` console
acta-records bundle example_file_1_copy.txt example_file1.txt example_file_2.txt --output-dir bundle_dir
acta-records bundle --includes-file record.includes --output-dir bundle_dir
ls *.txt | acta-records bundle --output-dir bundle_dir
cat record.includes | acta-records --output-dir bundle_dir
```
