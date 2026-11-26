# SyncTeX parser

The reference SyncTeX parser (`synctex_parser.c` and its headers), version 1.21,
copied unmodified from TeX Live's `texk/web2c/synctexdir`. It is the library
Okular, Zathura and TeXworks use. MIT-style licence, see the header of
`synctex_parser.c`.

`crates/latex/build.rs` compiles it. It needs a C compiler and the zlib
development files (`zlib-devel` on Fedora, `zlib1g-dev` on Debian).
