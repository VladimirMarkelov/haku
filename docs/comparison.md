## Comparison with `just`

As of `haku` v0.3 and `just` v0.5.

### Features

Both scripts use their own syntax heavily inspired by Makefile one.

| Feature | haku | just |
| --- | --- | --- |
| Indentation | optional | required by syntax |
| Comments | + | + |
| Doc comments | + | + |
| [Multi-line command](#multi-line-commands) | + | + |
| [Multi-line strings](#multi-line-strings) |   | + |
| [Multi-line constructs](#multi-line-constructs) | + |   |
| String escapes | + | + |
| [Strings](#string-types) | one type | few types |
| [Math operators](#math-operators) | very limited | very limited |
| [Environment variable support](#environment-variables) | + | + |
| [Variables](#variables) | + | + |
| [Variable substitution](#variable-substitution) | + | + |
| [Recipe parameters](#recipe-parameters) | + | + |
| [Recipe attributes](#recipe-attributes) | + |   |
| Recipe dependencies | + | + |
| [Recipe in other language](#recipe-in-other-language) |   | + |
| [Private recipe](#private-recipe) | limited | + |
| Import recipes from other files | + |   |
| [Built-in flow control](#built-in-flow-control) | + |   |
| [Built-in functions](#built-in-functions) | + | limited |
| Dotenv integration |   | + |
| Single line/recipe modes |   | + |
| Aliases |   | + |
| Default shell | `powershell` on Windows, <br /> `sh` on other OS | `sh` |
| Custom shell | + | + |
| [Shell execution flags](#shell-execution-flags) | + | + |
| [Error reporting](#error-reporting) | + | + |

#### Multi-line commands

Both applications allow a user to divide a long recipe line into a few smaller lines. In this case, every line
except the last one must end with `\`.

#### Multi-line strings

While one can imitate multi-line string in `haku` using `\` character, it is not the same feature as `just` provides.

#### Multi-line constructs

By default both applications executes a script line by line. But there is difference:

In `just` one cannot put, e.g., `while` and its body to separate lines: the entire `while` with its body must be a 
single line. Yes, it can be separated with `\` but, anyway, it is executed by a shell as a single huge command.

`Haku` provides built-in control flow statements, so it is possible to run a real multi-line loop.

#### String types

`Just` has two string types: double-quoted one that supports escaped characters, and  single-quotes raw strings.

`haku` treats both type of string in the same way.

#### Math operators

`Just` supports only `+` to concatenate strings,

`Haku` supports only logical operators `&&`, `||`, `!` in logical conditions

#### Environment variables

Both allows a user to read and set environment variables values. The only difference: `just` uses a separate function
`env_var` to read the existing variable, while in `haku` environment variable are used in the same way as any internal
variables: `echo ${PATH}`. `Haku` automatically reads a value from environment variable if a script variable with the
same name does not exist.

#### Variables

A slight difference in name conventions: `just` requires all letters to be Latin letters, while `haku` allows any
Unicode letters.

Assignment in `just` is `:=`, in `haku` it is `=` or `?=`.

#### Variable substitution

`Just` uses double curly braces and allows a script to use concatenation when a string is interpolated: 
`running {{date + testname}}`

`Haku` used Perl/Bash-style and only variable names are allowed - no operations or function calls:
`running ${date} ${testname}`

#### Recipe parameters

In both scripts one can use single value or list parameter for a recipe:
`recipe param_single +param_list:`.

`Just` allows setting default values for parameters: `recipe param=default:`. In `haku` it can be simulated with
assign-if-empty operator:

```
recipe param:
  param ?= default
```

`Just` may pass parameters to dependency: `recipe: (build "main")`. It is not supported in `haku`.

#### Recipe attributes

For `just` recipe attributes are in roadmap and are not supported yet.

`Haku` provides Rust-like attributes that can be applied to the entire recipe or to a single code block. That allows
a user to write cross-platform scripts:

```
#[family(windows)]
build-internal:
    echo Building Windows version...
    make -f mingw.makefile build
#[family(linux)]
build-internal:
    echo Building Linux version...
    make build

build: build-internal
```

#### Recipe in other language

There is no plans to support recipes in other languages in `haku`. At this moment it can simulated by temporary
switching the current shell:

```
shell("python")
print 2+2
shell("bash", "-cu")
```

#### Private recipe

In `haku` a user cannot create private recipes. Only few built-in private recipes exist, e.g, `_default`.

#### Built-in flow control

`Just` delegates all control statements(`if`, `for` etc) to external shell. So, the code that uses them is a
shell-dependent one.

`Haku` introduces built-in `for`, `while`, `if` and a few more control statements that works the same on any
platform and in any shell.

#### Built-in functions

No application allows a user to create custom functions. But the number of built-in functions differs.

`Just` provides only several functions that return various system information and works with environment variables.

`Haku`, besides mentioned in `just`, includes functions to format date and time, process strings, and file/directory paths.

#### Shell execution flags

Both applications supports `@` to suppress printing  a command before executing it in external shell.

In addition to it, `haku` provides a flag `-` that suppresses command errors and keep the script running even if 
the command with this flag fails.

#### Error reporting

Both applications show the line number and the line that generates an error. Since `haku` supports script import,
in case of more than one script is loaded, the error should show the name of the script which contain the failed line.
