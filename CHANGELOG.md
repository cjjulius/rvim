# Changelog

All notable changes to rvim are recorded here. Versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Search
- `gn` / `gN` select the search match under or after/before the cursor, and work
  as operator targets (`cgn`, `dgn`, `ygn`). Combined with dot-repeat, `cgn`
  then `.` steps through and edits each match — the by-hand substitute workflow.

### Numbers
- `Ctrl-a` / `Ctrl-x` now recognize hexadecimal (`0x…`) and binary (`0b…`)
  literals in addition to decimal, preserving the prefix, digit width, and hex
  letter case. Fixes the previous behavior of bumping the leading `0` of a `0x`
  literal as if it were decimal.
- `g Ctrl-a` / `g Ctrl-x` over a visual selection build an incrementing
  sequence — the 1st changed line steps by `count`, the 2nd by `2·count`, and so
  on — so a column of equal numbers becomes 1, 2, 3, …

### Command line
- `Tab` completion: completes the ex-command name, or the option name after
  `:set `, with repeated `Tab` / `Shift-Tab` cycling forward / backward through
  the matches.

### Operators
- `g?` ROT13 operator — over a motion (`g?w`), a text object (`g?ip`), doubled
  for the whole line (`g??`), or applied to a visual selection. Reuses the case-
  operator plumbing, so it is its own inverse and leaves non-letters untouched.

### Insert mode
- Digraph input: `Ctrl-k` plus two characters composes a special character
  (accented Latin letters, ligatures, currency, and common math/arrow symbols),
  accepting the two keys in either order like vim. Composes within dot-repeat.

### Motions
- `[(` `[{` `])` `]}` jump to the (counted) unmatched enclosing bracket — the
  open bracket searching backward, the close searching forward, with inner
  balanced pairs skipped.

## [0.2.0] — 2026-10-01

A large round of vim-parity, UI, and robustness work on top of the 0.1
foundation. Highlights:

### Editing & operators
- `gq`/`gw` paragraph reflow with `:set textwidth=N`.
- `gI` (insert at column 0), `gp`/`gP` (paste leaving the cursor after).
- Counted `J`/`gJ`, counted dot-repeat (`3.`).
- Black-hole register `"_`, read-only file-name register `"%`.

### Visual mode
- `p`/`P` replace the selection; `r<c>` replaces every selected char.
- `J`/`gJ` join the selection; `Ctrl-a`/`Ctrl-x` bump numbers per line.
- `:` prefills the `:'<,'>` range; selection size shows in the showcmd corner.

### Motions & search
- `[[` `]]` `[]` `][` section motions, `{count}%` jump, `Ctrl-f`/`Ctrl-b` paging.
- Incremental search (`:set incsearch`), `[index/total]` match count,
  `:set ignorecase`/`smartcase`/`wrapscan`; `n`/`N` honor search direction.
- Change list (`g;`/`g,`), matching-bracket highlight, `scrolloff`.

### Ex commands
- Line ranges: `:move`, `:copy`/`:t`, `:delete`, `:yank`, `:>`/`:<`, `:join`,
  `:put`, range-aware `:sort` (with `n`/`i` flags), `:s///n` count.
- `:[range]normal {keys}`, `@:` repeat, mark/visual range addresses (`:'<,'>`,
  `:'a,'b`), multi-option `:set`, `:r[ead]`, `:e`/`:e!` reload,
  `:qa`/`:wa`/`:wqa`, alternate buffer (`Ctrl-^`/`:b#`).

### UI & accessibility
- Flicker-free rendering via synchronized terminal updates (DEC mode 2026).
- `showcmd` command preview, `:set list` whitespace view, matching-bracket
  highlight, `Ctrl-g`/`ga` info commands.
- Command-line history (Up/Down), insert-mode keyword completion
  (`Ctrl-n`/`Ctrl-p`), `ZZ`/`ZQ`.

### Fixes & internals
- `:help` no longer destroys the current buffer; menu flicker eliminated.
- Memoized block-comment folding for cheaper rendering of large files.
- Editor/command/app test suites split into sibling files; 374 tests.

## [0.1.0]

Initial foundation: modal editing, motions and operators, registers, marks,
macros, undo/redo, regex search & substitute, multiple buffers, the Alt menu
bar, four color themes, seven syntax highlighters, a plugin system, config
file, and mouse support.
