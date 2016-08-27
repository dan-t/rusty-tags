[![Build Status](https://travis-ci.org/dan-t/rusty-tags.svg?branch=master)](https://travis-ci.org/dan-t/rusty-tags)
[![](http://meritbadge.herokuapp.com/rusty-tags)](https://crates.io/crates/rusty-tags)

rusty-tags
==========

A command line tool that creates tags - for source code navigation by
using [ctags](<http://ctags.sourceforge.net>) - for a cargo project and all
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

`rusty-tags` will also put a `rusty-tags.vi / rusty-tags.emacs` file to the source
code of every dependency, so after jumping to a dependency, you're able to jump
further to its dependencies.

`rusty-tags` should also correctly handle the case if a dependency reexports
parts of its own dependencies.

Currently `rusty-tags` doesn't support dependency overrides and local path
dependencies are only supported if they're contained in your projects `Cargo.toml`.
For git dependencies it only searches inside of `~/.cargo/git/checkouts/` and for
crates.io dependencies inside of `~/.cargo/registry/src/github.com-*`.

Rust Standard Library Support
=============================

`rusty-tags` will create tags for the standard library if you supply
the rust source by defining the environment variable `$RUST_SRC_PATH`:

    $ git clone https://github.com/rust-lang/rust.git /home/you/rust
    $ cd /home/you/rust
    $ git checkout stable
    $ export RUST_SRC_PATH=/home/you/rust/src/   # should be defined in your ~/.bashrc

Configuration
=============

The current supported configuration at `~/.rusty-tags/config.toml` (defaults displayed):

    # the file name used for vi tags
    vi_tags = "rusty-tags.vi"

    # the file name used for emacs tags
    emacs_tags = "rusty-tags.emacs"

Vim Configuration
=================

Put this into your `~/.vim/after/ftplugin/rust.vim` file:

    setlocal tags=./rusty-tags.vi;/
    autocmd BufWrite *.rs :silent exec "!rusty-tags vi --start-dir=" . expand('%:p:h') . "&"

The first line (only supported by vim >= 7.4) ensures that vim will
automatically search for a `rusty-tags.vi` file upwards the directory hierarchy.

This tags setting is important if you want to jump to dependencies and
then further jump to theirs dependencies.

The second line ensures that your projects tag file gets updated if a file is written.

If you've supplied the rust source code by defining `$RUST_SRC_PATH`:

    setlocal tags=./rusty-tags.vi;/,$RUST_SRC_PATH/rusty-tags.vi

MacOS Issues
============

Mac OS users may encounter problems with the execution of `ctags` because the shipped version
of this program does not support the recursive flag. See [this posting](http://gmarik.info/blog/2010/10/08/ctags-on-OSX) 
for how to install a working version with homebrew.
