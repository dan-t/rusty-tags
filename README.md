[![Build Status](https://travis-ci.org/dan-t/rusty-tags.svg?branch=master)](https://travis-ci.org/dan-t/rusty-tags)
[![](http://meritbadge.herokuapp.com/rusty-tags)](https://crates.io/crates/rusty-tags)

rusty-tags
==========

A command line tool that creates tags - for source code navigation by
using [ctags](<http://ctags.sourceforge.net>) - for a [cargo](<https://github.com/rust-lang/cargo>) project and all
of its dependencies.

Prerequisites
=============

* [ctags](<http://ctags.sourceforge.net>) installed, needs a version with the `--recurse` flag
* [git](<http://git-scm.com/>) installed if git dependencies are used

Installation
============

    $ cargo install rusty-tags

The build binary will be located at `~/.cargo/bin/rusty-tags`.

Usage
=====

Just calling `rusty-tags vi` or `rusty-tags emacs` anywhere inside
of the cargo project should just work.

After its run a `rusty-tags.vi / rusty-tags.emacs` file should be beside of the
`Cargo.toml` file.

Additionally every dependency gets a tags file at its source directory, so
jumping further to its dependencies is possible.

If a dependency reexports parts of its own dependencies, then these reexported
parts are also contained in the tags file of the dependency.

Rust Standard Library Support
=============================

Tags for the standard library are created if the rust source is supplied by
defining the environment variable `RUST_SRC_PATH`.

If you're using [rustup](<https://www.rustup.rs/>) you can get the
rust source of the currently used compiler version by calling:

    $ rustup component add rust-src

And then setting `RUST_SRC_PATH` inside of e.g. `~/.bashrc`:

    $ export RUST_SRC_PATH=$(rustc --print sysroot)/lib/rustlib/src/rust/src/

Or without `rustup` by getting the rust source by yourself:

    $ git clone https://github.com/rust-lang/rust.git /home/you/rust
    $ cd /home/you/rust
    $ git checkout stable
    $ export RUST_SRC_PATH=/home/you/rust/src/   # should be defined in your ~/.bashrc

Using `rustup` is the recommended way, because the you will automatically get
the correct standard library tags of the currently used compiler version.

Configuration
=============

The current supported configuration at `~/.rusty-tags/config.toml` (defaults displayed):

    # the file name used for vi tags
    vi_tags = "rusty-tags.vi"

    # the file name used for emacs tags
    emacs_tags = "rusty-tags.emacs"

Vim Configuration
=================

Put this into your `~/.vimrc` file:

    autocmd BufRead *.rs :setlocal tags=./rusty-tags.vi;/
    autocmd BufWrite *.rs :silent! exec "!rusty-tags vi --quiet --start-dir=" . expand('%:p:h') . "&" | redraw!

The first line (only supported by vim >= 7.4) ensures that vim will
automatically search for a `rusty-tags.vi` file upwards the directory hierarchy.

This tags setting is important if you want to jump to dependencies and
then further jump to theirs dependencies.

The second line ensures that your projects tag file gets updated if a file is written.

If you've supplied the rust source code by defining `$RUST_SRC_PATH`:

    autocmd BufRead *.rs :setlocal tags=./rusty-tags.vi;/,$RUST_SRC_PATH/rusty-tags.vi

Sublime Configuration
=====================

The plugin [CTags](https://github.com/SublimeText/CTags) uses vi style tags, so
calling `rusty-tags vi` should work.

By default it expects tag files with the name `.tags`, which can be set
with `vi_tags = ".tags"` inside of `~/.rusty-tags/config.toml`.

MacOS Issues
============

Mac OS users may encounter problems with the execution of `ctags` because the shipped version
of this program does not support the recursive flag. See [this posting](<http://gmarik.info/blog/2010/10/08/ctags-on-OSX>)
for how to install a working version with homebrew.

Cygwin/Msys Issues
==================

If you're running [Cygwin](<https://www.cygwin.com/>) or [Msys](<http://www.mingw.org/wiki/MSYS>) under Windows,
you might have to set the environment variable `$CARGO_HOME` explicitly. Otherwise you might get errors
when the tags files are moved.
