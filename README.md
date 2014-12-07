rusty-tags
==========

A command line tool that creates tags for a cargo project and all
of its dependencies.

Prerequisites
=============

* `ctags` installed
* `git` installed if git dependencies are used

Installation
============

* get `rustc` and `cargo` from [here](<http://www.rust-lang.org/install.html>)
* `git clone https://github.com/dan-t/rusty-tags.git`
* `cd rusty-tags`
* `cargo build --release`

The build binary will be located at `target/release/rusty-tags`.

Usage
=====

`cargo build` has to be called at least once, to download the source code of
the dependencies. If a dependency gets added or updated, then most likely
`cargo build` has to be called again.

Just calling `rusty-tags` then anywhere inside of a cargo project should
just work and after its run a `rusty-tags.vi` file should be beside
of the `Cargo.toml` file.

`rusty-tags` will also put a `rusty-tags.vi` to the source code of
every dependency, so after jumping to a dependency, you're able
to jump further to its dependencies.

`rusty-tags` should also correctly handle the case if a dependency
reexports parts of its own dependencies.

Currently `rusty-tags` doesn't support cargo dependency overwrites and
only searches for git dependencies inside of `~/.cargo/git/checkouts/`
and for crates.io dependencies inside of `~/.cargo/registry/src/github.com-*`.

Rust Standard Library Support
=============================

`rusty-tags` can automatically add tags for the standard library to
every tags file it's creating.

Creating tags for the standard library:

    $ git clone https://github.com/rust-lang/rust.git
    $ cd rust
    $ ./configure
    $ make TAGS.vi

You might want to modify `rust/mk/ctags.mk` to only consider rust files,
otherwise you're also getting tags for all of rustcs c code:

    -CTAGS_OPTS=--options="${CFG_SRC_DIR}src/etc/ctags.rust" --languages=-javascript --recurse ${CTAGS_LOCATIONS}
    +CTAGS_OPTS=--options="${CFG_SRC_DIR}src/etc/ctags.rust" --languages=Rust --recurse ${CTAGS_LOCATIONS}

Renaming the tags file, that if you've jumped to the standard
library, that you're able to jump further inside of it, and copying a
version of the tags, that `rusty-tags` can add it to every tags file it creates: 

    $ mv TAGS.vi rusty-tags.vi
    $ mkdir -p ~/.rusty-tags
    $ cp rusty-tags.vi ~/.rusty-tags/rust-std-lib.vi

Vim Configuration
=================

Put this into your `~/.vim/after/ftplugin/rust.vim` file:

    set tags=rusty-tags.vi;/
    autocmd BufWrite *.rs :silent !rusty-tags

The first line (only supported by vim >= 7.4) ensures that vim will
automatically search for a `rusty-tags.vi` file upwards the directory hierarchy.

This tags setting is important if you want to jump to dependencies and
then further jump to theirs dependencies.

The second line ensures that your projects tag file gets updated if a file is written.

Normally you want to call the `rusty-tags` command in the backgroud by adding a `&`:

    autocmd BufWrite *.rs :silent !rusty-tags &

But I had sometimes strange behaviours this way that I couldn't track down
until now. So you can try using it with the `&`, and if it doesn't work,
if the tags aren't correctly updated, then you know the reason.

Emacs Support
=============

I wanted to support vi and emacs style tags by default and almost everything
in `rusty-tags` is already prepared to support emacs tags, but then, after
looking at an emacs tags file the first time, I saw, that it's completely
differently constructed than a vi one.

`rusty-tags` caches the tags for every dependency and then merges them.

This works nicely for the vi tags, because each entry is in a separate line
and starts with the keyword, so merging and then sorting them is quite simple.

So, if there're emacs user that want a working `rusty-tags` and can give some
hints, then please step forward!
