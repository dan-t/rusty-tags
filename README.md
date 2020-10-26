[![Build Status](https://travis-ci.org/dan-t/rusty-tags.svg?branch=master)](https://travis-ci.org/dan-t/rusty-tags)
[![](http://meritbadge.herokuapp.com/rusty-tags)](https://crates.io/crates/rusty-tags)

rusty-tags
==========

A command line tool that creates [tags](https://en.wikipedia.org/wiki/Ctags) - for source code navigation by
using [ctags](<http://ctags.sourceforge.net>) - for a [cargo](<https://github.com/rust-lang/cargo>) project, all
of its direct and indirect dependencies and the rust standard library.

Prerequisites
=============

* [ctags](<http://ctags.sourceforge.net>) installed, needs a version with the `--recurse` flag

On a linux system the package is most likely called `exuberant-ctags`.

Otherwise you can get the sources directly from [here](http://ctags.sourceforge.net/) or use the newer and alternative
[universal-ctags](https://github.com/universal-ctags/ctags).

Only `universal-ctags` will add tags for struct fields and enum variants.

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

Rust Standard Library Support
=============================

Tags for the standard library are created if the rust source is supplied by
defining the environment variable `RUST_SRC_PATH`.

These tags aren't automatically added to the tags of the cargo project and have
to be added manually with the path `$RUST_SRC_PATH/rusty-tags.vi` or
`$RUST_SRC_PATH/rusty-tags.emacs`.

If you're using [rustup](<https://www.rustup.rs/>) you can get the
rust source of the currently used compiler version by calling:

    $ rustup component add rust-src

And then setting `RUST_SRC_PATH` inside of e.g. `~/.bashrc`.
For `rustc >= 1.47.0`:
    $ export RUST_SRC_PATH=$(rustc --print sysroot)/lib/rustlib/src/rust/library/

For `rustc < 1.47.0`:
    $ export RUST_SRC_PATH=$(rustc --print sysroot)/lib/rustlib/src/rust/src/

Configuration
=============

The current supported configuration at `~/.rusty-tags/config.toml` (defaults displayed):

    # the file name used for vi tags
    vi_tags = "rusty-tags.vi"

    # the file name used for emacs tags
    emacs_tags = "rusty-tags.emacs"

    # the name or path to the ctags executable, by default executables with names
    # are searched in the following order: "ctags", "exuberant-ctags", "exctags", "universal-ctags", "uctags"
    ctags_exe = ""

    # options given to the ctags executable
    ctags_options = ""

Vim Configuration
=================

Put this into your `~/.vimrc` file:

    autocmd BufRead *.rs :setlocal tags=./rusty-tags.vi;/

Or if you've supplied the rust source code by defining `RUST_SRC_PATH`:

    autocmd BufRead *.rs :setlocal tags=./rusty-tags.vi;/,$RUST_SRC_PATH/rusty-tags.vi

And:

    autocmd BufWritePost *.rs :silent! exec "!rusty-tags vi --quiet --start-dir=" . expand('%:p:h') . "&" | redraw!

Emacs Configuration
===================

Install [counsel-etags](https://github.com/redguardtoo/counsel-etags).

Create file `.dir-locals.el` in rust project root (please note the line to set `counsel-etags-extra-tags-files` is optional):

    ((nil . ((counsel-etags-update-tags-backend . (lambda (src-dir) (shell-command "rusty-tags emacs")))
             (counsel-etags-extra-tags-files . ("~/third-party-lib/rusty-tags.emacs" "$RUST_SRC_PATH/rusty-tags.emacs"))
             (counsel-etags-tags-file-name . "rusty-tags.emacs"))))

Use `M-x counsel-etags-find-tag-at-point` for code navigation.

`counsel-etags` will automatically detect and update tags file in project root. So no extra setup is required.

Sublime Configuration
=====================

The plugin [CTags](https://github.com/SublimeText/CTags) uses vi style tags, so
calling `rusty-tags vi` should work.

By default it expects tag files with the name `.tags`, which can be set
in `~/.rusty-tags/config.toml`:

    vi_tags = ".tags"

Or by calling `rusty-tags vi --output=".tags"`.

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
