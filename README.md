# Table of Contents

- [Intro](#intro)
    - [License](#license)
    - [Similar projects](#similar-projects)
      - [Comparison with `just`](docs/comparison.md)
- [Installation](#installation)
    - [Precompiled binaries](#precompiled-binaries)
- [Example with comments](#example-with-comments)

[Documentation](/docs/usage.md)

## Intro

`haku` is a simple command runner, kind of a `make` alternative but it is not a
`make` replacement.  It provides a limited set of internal commands(`for`,
`if`, `while` etc) that allow anyone to write cross-platform task files. If it
is not enough, `haku` has a Rust-like attributes for marking a block of a
script to run, e.g., only on a specific platform.

Warning: platform specific properties are detected at compile time, not at run time.
So, let's assume you build `haku` on Windows 32-bit, and run it on Linux 64-bit(e.g., using Wine).
The application will keep executing scripts with flags: `platform=windows` and `bit=32`.

Commands are stored in files named:

- Windows: `Hakufile`, and `Taskfile`
- Other Os: `Hakufile`, `Taskfile`, `hakufile`, and `taskfile`

When `haku` starts without task file name, first, it looks for `Taskfile`. If it does not exist,
`haku` looks for `Hakufile`.

All commands are either free ones(those must be in the file beginning), or grouped by sections -
a section is called a `recipe`. The file syntax is relaxed and simplified `makefile` one.

Internally `haku` uses `powershell` on Windows and `sh` on any other OS to execute an external
command. You can override the default using `shell` built-in function.

### License

Haku is released under Apache License Version 2.0

### Similar projects

`Haku` is heavily inspired by two great projects: [GNU make](https://www.gnu.org/software/make/)
and [just](https://github.com/casey/just). They do their job well but some things still are
a bit inconvenient to me. What made me to implement my own command runner:

- both utilities above are picky about whitespaces and indentation. And `make` sometimes have puzzling requirements
- it is not easy to create cross-platform makefiles. `just` provides a way but it is a limited one
- a set of built-in functions to manipulate file path: replace extension, add, create name with current time etc

At the same time, `haku` lacks some features that others have. See detailed comparison `haku` with
`just` in [docs](/docs/comparison.md). As for comparison with `make`, `haku` should not be used as
build system because `haku` does not check timestamps and does not try to minimize build time. So,
both tools are for different purposes and it does not make much sense to compare them.

## Installation

You can compile the application from sources, or install using cargo:

```shell
$ cargo install haku
```

You need Rust compiler that supports Rust 2018 edition (Rust 1.38 or newer) to do it. If you want
to upgrade existing haku, execute the following command:

```shell
$ cargo install haku --force
```

For rust 1.41 and newer flag `--force` can be omitted.

### Precompiled binaries

For Windows and Ubuntu you can download precompiled binaries from [Release page](https://github.com/VladimirMarkelov/haku/releases).

* Windows binary tested on Windows 10.
* Ubuntu binary tested on Ubuntu 18.
* musl-Linux build

## Example with comments

```
// Script header starts.

// select the correct name of the utility "rm" depending on OS type
#[not family(windows)]
rm = "rm"
#[family(windows)]
rm = "del"
app-name = "myapp"

// set flags for the "rm" utility. Use a bit different way: first, intialize with default
// value, and override if OS is windows
rm-flags = "-f"
#[os(windows)]
rm-flags = "/F"

// Script recipe section starts.

// Let's assume we support only linux 64-bit, and windows 64-bit.
// Stop execution in all other cases.
// "!" and "not" are synonyms
// This recipe does not have dependencies
#[!os(linux,windows)]
precheck-two:
  error "Only Windows and Linux are supported"

// default empty recipe for the rest cases: linux and windows 64-bit.
// This recipe has dependencies to do additional check for 32/64-bit
precheck: precheck-two

// it can be written shorter: "#[bit(32)]" because we are here only if parent recipe, that
// activates only on windows and linux, is executed. I wrote a full condition to make an
// example of how to do a few checks in one condition
#[os(linux,windows), bit(32)]
precheck-two:
  error "Only 64-bit OS is supported"

// empty default recipe. It is important to put default recipes without attrubutes last
precheck-two:

// build recipes
## linux build - this line is shown by command "--list"
#[os(linux), bit(64)]
@build: precheck
  println("Building on ", os(), "...")
  make -f linux.make

## this comment is doc comment but it is not show by "--list", only the last one is shown
## windows build - this line is shown in "--list" output
#[os(windows), bit(64)]
@build: precheck
  make -f win-gnu.make

// one script for all platforms. It is a bit wordy and has redundant operations that are
// used only as examples of haku features
@clean:
    rm_cmd = "${rm} ${rm-flags}"
    app-name = app
    // windows binary always has exe extension
    #[platform(windows)]
    app-name = add-ext($app-name, "exe")
    // prepend "-" to ignore any errors
    -${rm_cmd} *.o
    -${rm_cmd} *.obj
    -${rm_cmd} ${app-name}
```

The script above can be run without changes on any platform:

```
$ haku build
Building on Linux...
<here goes make output>

$ haku clean
rm -f *.o
<other rm calls displayed>
```
