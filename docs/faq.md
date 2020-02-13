# Table of Contents

- [What's wrong with `make`](#whats-wrong-with-make)
- [What does 'haku' mean](#what-does-haku-mean)
- [Why only variable names are case-sensitive](#why-only-variable-names-are-case-sensitive)

### What's wrong with `make`

`make` is a great tool. It may save a lot of time by building only changed files. But it does not
do a good job in some of my cases:

- to do something that does not depend on files, I have has to mark a target .PHONY. It is not a
  big deal, just a little inconvenient;
- to rerun a task in case of no file has changed, I have to touch files or `make` does nothing;
- run a task on an arbitrary file: e.g., convert one of text files in a directory from
  UTF8 to UTF16. I use bash/cmd scripts for it now;
- sometimes I need to share a value(and change it on the fly) between few targets. I do not now
  a way to do it in make file;
- it would good to have a cross-platform make file but with `make` I have to create two different
- make files and select them manually with `-f` option.

### What does 'haku' mean

At first sight, `haku` sounds like a distorted `hacker`. But these two words have nothing in
common. `Haku` in Quechua is an exclamation like `let's go!` or `let's do it!`. It looked like
a good word for a command runner.


### Why only variable names are case-sensitive

I wanted to make everything case-insensitive, so people can write in any way they like. But
variables are a special case. If a variable is not initialized by a script, its value is read
from environment variable (transparent usage of environment variables). E.g., in Linux
the name of environment variable is case sensitive. Hence there is discrepancy.
