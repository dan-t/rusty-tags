[![Build Status](https://travis-ci.org/dan-t/rusty-tags.svg?branch=master)](https://travis-ci.org/dan-t/rusty-tags)

rusty-tags
==========

A command line tool that creates tags - for source code navigation by
using [ctags](<http://ctags.sourceforge.net>) - for a cargo project and all
of its dependencies.

Prerequisites
=============

* [ctags](<http://ctags.sourceforge.net>) installed
* [git](<http://git-scm.com/>) installed if git dependencies are used

Installation
============

* get `rustc` and `cargo` from [here](<http://www.rust-lang.org/install.html>)
* `git clone https://github.com/dan-t/rusty-tags.git`
* `cd rusty-tags`
* `cargo build --release`

The build binary will be located at `target/release/rusty-tags`.

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

Currently `rusty-tags` doesn't support local dependencies and dependency overwrites.
For git dependencies it only searches inside of `~/.cargo/git/checkouts/` and for
crates.io dependencies inside of `~/.cargo/registry/src/github.com-*`.

Rust Standard Library Support
=============================

The source code of Rust already contains a script for creating tags, but
if you only want to jump into the standard library then reducing the directories
gives better results.

First get the Rust source code:

    $ git clone https://github.com/rust-lang/rust.git
    $ cd rust

And now execute the following script inside of the rust directory:

    #!/usr/bin/env bash
    
    src_dirs=`echo $PWD/src/{liballoc,libarena,libbacktrace,libcollections,libcore,libflate,libfmt_macros,libgetopts,libgraphviz,liblog,librand,librbml,libserialize,libstd,libsyntax,libterm}`
    
    ctags -f rusty-tags.vi --options=src/etc/ctags.rust --languages=Rust --recurse $src_dirs
    
    ctags -e -f rusty-tags.emacs --options=src/etc/ctags.rust --languages=Rust --recurse $src_dirs

Now add the created tags file to the list of tags files in your editor settings.

Vim Configuration
=================

Put this into your `~/.vim/after/ftplugin/rust.vim` file:

    setlocal tags=rusty-tags.vi;/,path-to-rust-source-code/rusty-tags.vi
    autocmd BufWrite *.rs :silent !rusty-tags vi

The first line (only supported by vim >= 7.4) ensures that vim will
automatically search for a `rusty-tags.vi` file upwards the directory hierarchy.

This tags setting is important if you want to jump to dependencies and
then further jump to theirs dependencies.

The second line ensures that your projects tag file gets updated if a file is written.

Normally you want to call the `rusty-tags` command in the backgroud by adding a `&`:

    autocmd BufWrite *.rs :silent !rusty-tags vi &

But I had sometimes strange behaviours this way that I couldn't track down
until now. So you can try using it with the `&`, and if it doesn't work,
if the tags aren't correctly updated, then you know the reason.

MacOS Issues
============

Mac OS users may encounter problems with the execution of `ctags` because the shipped version
of this program does not support the recursive flag. See [this posting](http://gmarik.info/blog/2010/10/08/ctags-on-OSX) 
for how to install a working version with homebrew.
