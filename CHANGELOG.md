# Changelog

All notable changes to rvim are recorded here. Versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Navigation
- `gx` opens the web URL under the cursor with the system browser (http, https,
  ftp, and bare `www.` links, opened over https). The URL is extracted without its
  surrounding markdown/prose brackets and launched without a shell, so characters
  like `&` in a query string are safe; other schemes are declined.

### Editing
- `=` reindent operator: `==`, `=G`, `=ip`, `=i{`, and visual-mode `=` re-space
  lines to their brace nesting (honoring `shiftwidth`/`expandtab`), so `gg=G`
  tidies a whole file. Braces inside strings and `//` / `/* */` comments are
  ignored (block-comment state carries across lines, and comment-body lines keep
  their own indentation). It is a deliberate no-op for indentation-structured
  languages (Python, YAML, Ruby, Lua, Shell), where a brace rule would corrupt
  the layout. `:set equalprg=<cmd>` routes `=` through an external formatter
  (e.g. `clang-format`, `rustfmt`) instead of the built-in indenter.

### Configuration
- `:set` now accepts option values containing spaces when the space is escaped
  with a backslash, e.g. `:set equalprg=clang-format\ --style=Google`; previously
  the multi-option splitter broke such values apart. `\\` gives a literal
  backslash.

### UI
- `:messages` recalls recent status-line messages (the last 200), so a note or
  error that flashed by can be read again. It opens in a scratch buffer like the
  other listings.
- The status line shows `[paste]` while paste mode is on, next to the existing
  `[+]` / `[RO]` / `[dos]` flags (whose construction is now shared by the single
  and split status lines).

### Search
- `:s` replacements support vim case escapes: `\u` / `\l` (upper/lower the next
  character) and `\U` / `\L` … `\E` (case a run), e.g. `:%s/\w+/\U&/g`.
- vim's word-boundary atoms `\<` / `\>` now work in search and substitute patterns
  (translated to the regex engine's `\b`); previously they dropped the pattern to a
  literal match.
- Live substitute preview: typing `:s/pattern/...` or `:%s/...` highlights the
  pattern's matches as you type (like vim's `inccommand`), restoring the previous
  highlight on Esc. Honors `ignorecase`/`smartcase` and `\c`/`\C`.

### Interface language
- Templated messages now localize too, via placeholders filled after translation:
  file open/write/reload, `:bufdo`/`:windo` done, buffer switch/close, theme /
  filetype / language confirmations, and the unknown-command/buffer/filetype/
  language errors. (Rare messages carrying OS error text, and `:help`, stay English.)
- The UI can be localized: `:set language=vi` (Vietnamese), `:set language=fr`
  (French), `:set language=en` (English). Covers menus, mode indicators, the
  status-line scroll indicator (`Top`/`Bot`/`All`) and common messages. The English
  text is the translation key and untranslated strings fall back to English, so
  coverage and new languages can grow incrementally.

### Themes
- Added the Solarized Dark theme (`:theme solarized-dark`): the classic
  low-contrast blue-grey palette, a dark companion to the `paper` light theme.
- Added the Dracula theme (`:theme dracula`): soft dark background with vivid
  pink, purple and green accents.

### Windows
- Shared-buffer splits (#25): a plain `:split` / `:vsplit` now gives two windows
  on one shared buffer — edits in either appear in both instantly and undo is
  shared, while each window keeps its own cursor and scroll. Opening a different
  file in a split stays independent.

### Accessibility
- `:set list` now flags non-breaking spaces with their own `listchars` `nbsp:`
  marker (shown by default), and an optional `space:` marker reveals every
  ordinary space — making invisible characters visible.

### Command line
- `-c <command>` runs an ex command on startup (repeatable), after the file and
  config load — e.g. `rvim -c "set number" file`.
- `-R` / `--readonly` opens every loaded buffer read-only (`:w` refused unless
  forced with `:w!`).

### Configuration
- Modelines: rvim now reads a `vim:` options comment in a file's first/last lines
  on load and applies it (e.g. `# vim: set ts=2 sw=2 et ft=yaml:`), so per-project
  indentation travels with the file. Both the `set ... :` and bare `vi:opt:opt`
  forms are supported; only `:set` options are applied, never commands. Controlled
  by `:set modeline` / `:set nomodeline` (on by default) and `:set modelines=N`.
  Files opened in the background (extra command-line arguments, `:bn`/`:b N`) get
  their modeline applied the first time they are viewed, once per buffer.

### Files
- `:enew` opens a new empty, unnamed buffer in the current window; the previous
  buffer stays in the list (`:bn`/`:bp`) when it has a name or unsaved changes.
- `:cd [dir]` changes the working directory (with `~` expansion; no argument goes
  home) and `:pwd` prints it, so relative paths for `:e`/`:w`/`:r` resolve where
  you expect.

### Help
- The in-editor `:help` now documents the newer features: recursive folds
  (`zO`/`zC`/`zA`), `:set readonly`/`paste`/`language`, `:cd`/`:pwd`, `:s` case
  escapes and `\<`/`\>`, and syntax-fold indentation for brace-less languages.

### Folding
- Recursive fold commands `zO` / `zC` / `zA` open, close, or toggle the fold under
  the cursor together with every fold nested inside it.
- `:set foldmethod=syntax` now folds brace-less languages (Python, YAML, Ruby, Lua,
  Shell) by indentation instead of producing no folds; brace languages are
  unchanged.

### Syntax
- Scala highlighting (`.scala`, `.sc`): keywords, core types, common builtins,
  strings (including `"""` multi-line strings), and `//` / `/* */` comments.
- Dart highlighting (`.dart`): keywords, core types, common builtins, strings
  (including `'''` / `"""` multi-line strings), and `//` / `/* */` comments.
- R highlighting (`.r`): keywords, core types, common builtins, strings, numbers,
  and `#` comments.
- PHP highlighting (`.php`, `.phtml`): keywords, type hints, common builtins,
  strings, `$` variables, and `//` / `#` / `/* */` comments.
- Swift highlighting (`.swift`): keywords, core types, common builtins, strings
  (including `"""` multi-line strings), `#` compilation directives, and `//` /
  `/* */` comments.
- C# highlighting (`.cs`): keywords, types, common library builtins, strings
  (including `"""` raw strings across lines), `#` preprocessor directives, and
  `//` / `/* */` comments.
- Lua highlighting (`.lua`): keywords, stdlib builtins, strings, `--` line comments
  and `--[[ ... ]]` long comments that span lines. (Block-comment detection now
  runs before line comments so overlapping prefixes like `--[[`/`--` resolve
  correctly; unaffected for every other language.)
- Ruby highlighting (`.rb`): keywords, core-class types, common builtins, strings,
  `#` comments, and `@`/`@@`/`$` variables.
- Kotlin highlighting (`.kt`, `.kts`): keywords, types, stdlib builtins, strings
  (including triple-quoted raw strings across lines), and `//` / `/* */` comments.

### Editing
- `:set readonly` (`ro`) guards a file from accidental overwrite: `:w` is refused
  with `E45` until forced with `:w!`, and the status line shows `[RO]`. `:w!` and
  `:wq!` / `:x!` now parse as forced writes.
- `:set paste` / `:set nopaste`: paste mode suppresses autoindent, smartindent,
  abbreviation expansion and auto-wrap while inserting, so pasted text keeps its
  own layout instead of cascading indentation.

### UI
- The status line now shows a `[dos]` tag next to the file name when a buffer
  uses Windows CRLF line endings (Unix files stay untagged).
- Horizontal-scroll edge markers: a `>` at the right edge marks a line that
  continues off-screen and a `<` at the left edge marks text hidden by rightward
  scrolling, so truncated content is always signalled. Configurable through
  `:set listchars=extends:…,precedes:…` (a space hides either).
- Error messages (vim-style `E123:`) now render in a distinct per-theme error
  color on the command line, so failed commands stand out from status text.
- Expanded the Alt menu bar: a Window menu (splits), a Folding submenu (fold
  methods and fold column), more View options (smart-indent, whitespace, cursor
  line), and a Set Filetype list generated from every supported language.
- Status-line display toggles: `:set noshowmode`, `:set noshowcmd`, `:set noruler`
  trim the mode indicator, pending-command display, and cursor-position ruler
  (all on by default).

### Internal
- Rendering a closed fold no longer re-tokenizes its hidden lines each frame to
  advance syntax state; it uses the cheap `fast_block_state` short-circuit instead,
  so large folds in brace/multi-line languages are cheaper to scroll past.
- Shared-buffer splits (#25), step 1: the rendering layer now takes the buffer as
  an explicit parameter instead of reaching through the editor, decoupling drawing
  from buffer ownership ahead of the buffer/view separation.
- Shared-buffer splits (#25), step 2: each window now carries a `BufferId` — shared
  when you `:split` the current file, distinct when a window opens another file —
  the identity the shared-buffer wiring will route edits and rendering through.
- Syntax carry-over state now skips full tokenization for lines that cannot open
  a block comment or multi-line string (the common case), so recomputing the
  highlight state above the viewport is cheaper on large files.
- Added a UTF-8 regression suite (multibyte delete/replace/toggle-case, word
  motions, `$`, and visual-block delete) to guard the char/byte boundary.

### Fixed
- A file ending in a newline no longer loads with a phantom trailing blank line,
  and saving no longer appends an extra blank line each time (load/save now
  round-trips exactly, DOS line endings included).

### Files
- `:set backup` keeps the previous contents as `{name}~` before overwriting a file.

### Themes
- New `paper` theme: a light color scheme (dark text on a warm background) for
  bright environments — the first non-dark built-in. Switch with `:theme paper`.

### Internal
- Rendering cleanup: the per-line draw helper takes a small `RenderCtx` of
  frame-constant parameters instead of nineteen positional arguments (no behavior
  change).

### Buffers
- `:bufdo {cmd}` runs an ex command on every open buffer and returns to the one
  you started in (e.g. `:bufdo %s/old/new/g`).
- `:windo {cmd}` runs an ex command in every window, returning to the one you
  started in.

### UI
- `:set title` shows the current file name (with a modified marker) in the
  terminal window title, and the previous title is restored on exit.
- `:set colorcolumn` accepts several comma-separated columns (e.g. `cc=80,120`),
  each drawn as a guide including past the end of short lines.

### Command line
- `:command Name expansion` defines a user command (name must start uppercase),
  with typed arguments appended; `:delcommand` removes one and `:command` lists
  them. Alias resolution is bounded, so a self-referential alias can't loop.

### Editing
- `:set smartindent` adds an indent level after a line ending in `{` and dedents
  when `}` is typed as the first non-blank character.

### Files
- Line endings are preserved: a file loaded with Windows CRLF is written back with
  CRLF, Unix LF stays LF, and `:set fileformat=dos` / `=unix` chooses explicitly.

### Registers
- The read-only `"/` (last search pattern) and `":` (last command line) registers
  are now available for paste and `Ctrl-r` (e.g. `Ctrl-r /` while typing a `:s`).

### Performance
- Split windows memoize each pane's first-visible-line syntax state, so scrolling
  or moving the cursor no longer re-scans every pane from the top of the buffer
  each frame (large files with block comments / multiline strings redraw faster).

### UI
- The cursor shape now follows the mode: a block in Normal, a bar in Insert, an
  underline in Replace (terminals that lack DECSCUSR keep their usual cursor; the
  shape is restored on exit).

### Windows
- Resize splits: `Ctrl-w +` / `-` change the focused window's height, `Ctrl-w >`
  / `<` its width, and `Ctrl-w =` makes all windows equal again.
- `:set splitbelow` / `:set splitright` open new splits below / to the right of
  the current window instead of the default above / left.
- Rearrange splits: `Ctrl-w x` swaps the focused window with the next, and
  `Ctrl-w r` rotates every window's content one slot.

### Mouse
- Double-click selects the word under the pointer; triple-click selects the whole
  line (in any window when the screen is split).

### Command line
- `Tab` now completes filenames for commands that take a path (`:e`, `:w`, `:r`,
  `:split`, `:vsplit`, `:source`, …): it fills the first match, cycles with
  repeated `Tab`/`Shift-Tab`, and adds a trailing slash to directories.

### Folding
- `:set foldmethod=indent` builds folds automatically from indentation (nested
  blocks fold into nested folds); they refresh as you edit. `:set
  foldmethod=manual` (the default) is unchanged.
- `:set foldmethod=marker` builds folds from `{{{` / `}}}` markers in the text,
  nesting and refreshing the same way.
- `:set foldmethod=syntax` builds folds from brace-delimited blocks, using the
  tokenizer so braces inside strings, chars and comments are ignored; nests and
  refreshes on edit like the other automatic methods.
- Fold navigation: `zj`/`zk` jump to the next/previous fold, and `[z`/`]z` jump to
  the start/end of the current fold.
- `:set foldcolumn=N` draws a fold-indicator column that marks where folds start
  (`+` closed, `-`/`|` open) so fold structure is visible without probing.
- `H`, `M`, `L` and the page keys (`Ctrl-f`/`Ctrl-b`) now count a closed fold as a
  single screen row, so they land on the right line when folds are open.

### Windows
- Split-window polish: the mouse now focuses the pane under the pointer, clicks
  position the cursor within that pane, and the wheel scrolls the hovered pane;
  the command-line completion wildmenu is drawn in split mode too.
- Split windows: `:split`/`:vsplit` (and `Ctrl-w s`/`v`) divide the screen into
  panes, `Ctrl-w h/j/k/l` and `Ctrl-w w`/`W` move focus, and `:close`/`:only`
  (`Ctrl-w c`/`o`) tidy up. Each window keeps its own cursor and scroll position;
  `:q` closes the focused window until only one remains. Same-file splits are
  independent views for now (edits are not mirrored live).

## [1.0.0] — 2026-10-18

First stable release. rvim is now a comprehensive vim-emulating terminal editor:
modal editing with wide keystroke parity, multi-buffer workflows, incremental
search and substitution, registers and macros, visual and block modes, manual
code folding, syntax highlighting for seventeen languages, themes, mouse
support, and an Alt-activated menu bar. Backed by 681 tests. The sections below
collect everything landed since 0.2.0.

### Internal
- Window layout manager (foundation for split windows): a split tree that turns
  `:split`/`:vsplit` structure into per-window rectangles (siblings divide space
  evenly), with directional focus moves and close/only operations. Not yet wired
  to the editor — see the roadmap.

### Folding
- Manual code folding: `zf{motion}` / visual `zf` / `zF` create a fold, `zo`/`zc`/
  `za` open/close/toggle it, `zR`/`zM` open/close all, `zd`/`zE` delete one/all,
  and `zi` toggles folding on and off. Closed folds collapse to a dimmed
  `+-- N lines: …` header; `j`/`k` step over them and the cursor never rests
  inside a closed fold. Folds can be nested.

### Help
- `:help` is refreshed and the `:set ft=` language list is now generated from the
  language table (no more drift); it also covers the system clipboard, block-wise
  yank/paste, command-line `Ctrl-r`, `:n`/`:N`, and `shiftround`/`joinspaces`/
  `matchpairs`.

### Command line
- `+N` command-line option opens the file at line N (bare `+` opens at the last
  line), e.g. `rvim +42 file.rs`. The theme list in `--help` is also current.
- `+/PATTERN` command-line option opens the file at the first line matching the
  pattern, and leaves it as the active search for `n`/`N`.
- Multiple files on the command line (`rvim a.rs b.rs`) open the first as the
  active buffer and the rest as buffers reachable with `:bn`/`:bp`.
- `:next`/`:n` and `:prev`/`:N` are accepted as aliases for `:bn`/`:bp`.

### Editing
- Vertical motion remembers its goal column (vim's "curswant"): moving `j`/`k`
  through a shorter line and on to a longer one restores the original column, and
  block selections keep their width across short lines. After `$` the goal is the
  line end, so `j`/`k` follow each line's end.
- Block-wise yank and paste: `Ctrl-v` + `y` yanks a rectangle, and `p`/`P` pastes
  it back as a rectangle (padding short lines and adding lines past the end).
- `:set matchpairs` configures the bracket pairs `%` jumps between, so you can add
  pairs like `<:>` for HTML/XML (`:set matchpairs=(:),{:},[:],<:>`).

### Mouse
- Drag-to-select: with `:set mouse`, dragging the left button starts and extends
  a character-wise visual selection; a plain click clears it and repositions.

### Command line
- `Ctrl-r` inserts a register on the `:` / search line (e.g. `Ctrl-r "` pastes
  the last yank), and `Ctrl-r Ctrl-w` inserts the word under the cursor.

### Internal
- Language metadata (name, `:set ft` aliases, file extensions, comment marker) is
  now a single data table, so adding a language is one row plus its highlighter
  instead of edits across several match arms. No behavior change.

### Performance
- Rendering skips folding syntax state from the top of the buffer for languages
  with no line-spanning constructs (JSON, TOML, plain text, Z80), avoiding an
  unnecessary full-buffer scan per frame in large files.

### Syntax
- JSON highlighting (`.json`): strings, numbers, and the `true`/`false`/`null`
  literals are color-coded; detected by extension or `:set ft=json`.
- Python highlighting (`.py`, `.pyw`): keywords, builtin types and functions,
  strings, numbers, and `#` comments; detected by extension, a `python` shebang,
  or `:set ft=python`.
- TOML highlighting (`.toml`): strings, numbers, booleans and `#` comments;
  detected by extension or `:set ft=toml`.
- JavaScript highlighting (`.js`, `.mjs`, `.cjs`, `.jsx`): keywords, builtin
  objects/functions, strings, numbers, and `//` / `/* */` comments; detected by
  extension, a `node` shebang, or `:set ft=javascript`.
- Go highlighting (`.go`): keywords, builtin types and functions, strings, numbers,
  `//` / `/* */` comments, and multi-line raw-string backticks; detected by
  extension or `:set ft=go`.
- Shell highlighting (`.sh`, `.bash`, `.zsh`): keywords, builtin commands, strings,
  `$`-variables, and `#` comments; detected by extension, a shell shebang, or
  `:set ft=shell`.
- C/C++ highlighting (`.c`, `.h`, `.cpp`, `.cc`, `.hpp`): keywords, builtin types
  and library functions, strings, char literals, numbers, `//` / `/* */` comments,
  and `#` preprocessor directives; detected by extension or `:set ft=c`.
- Java highlighting (`.java`): keywords, builtin types and common classes, strings,
  char literals, numbers, and `//` / `/* */` comments; detected by extension or
  `:set ft=java`.
- YAML highlighting (`.yaml`, `.yml`): booleans/null, strings, numbers, `&anchor`
  / `*alias` references, and `#` comments; detected by extension or `:set ft=yaml`.
- TypeScript highlighting (`.ts`, `.tsx`, `.mts`, `.cts`): JavaScript plus TS
  keywords (`interface`, `type`, `enum`, …) and types, template literals, and
  `//` / `/* */` comments; detected by extension or `:set ft=typescript`.
- Preprocessor directives: a `preprocessor` character in a language spec colors a
  leading `#include` / `#define` etc. as a preprocessor token.
- Multi-line strings are tracked across lines (via `LangSpec.multiline_strings`),
  so Python triple-quoted strings and docstrings stay colored past the first
  line.
- Per-line syntax state widened to a `LineState` enum, so a language can track
  block comments and multi-line strings independently; JavaScript template
  literals now span lines correctly alongside `/* */` comments.
- Multi-line strings honor backslash escapes for single-character delimiters, so
  an escaped backtick inside a JS template literal no longer ends it early.

### Themes
- New built-in `gruvbox` dark theme (`:theme gruvbox`), the popular warm retro
  palette.
- New built-in `nord` theme (`:theme nord`), the cool, muted arctic palette.

### Indent
- `:set shiftround` rounds `>`/`<` to a multiple of `shiftwidth` (e.g. a 2-space
  indent snaps to 4 instead of 6). Off by default; `:set noshiftround` restores
  the plain add/remove behavior.

### Editing
- `:set joinspaces` makes `J` insert two spaces after a line ending in `.`, `!`,
  or `?` (vim's 'joinspaces'). Off by default.

### UI
- `listchars` gains an `eol:` marker: `:set listchars=...,eol:#` sets the
  end-of-line glyph shown under `:set list` (previously a hardcoded `$`).
- Status line scroll indicator: shows `Top`, `Bot`, or `All` when the top,
  bottom, or whole file is visible, and a file percentage otherwise (vim's ruler
  position), replacing the plain cursor percentage.

### Insert mode
- Auto-wrap: with `:set textwidth=N`, typing past the limit breaks the line at
  the last blank and carries the trailing word (plus autoindent) to a new line.
  The limit is measured in display columns, so tabs count by their width. A word
  with no blank to break on is left long. `textwidth=0` disables it.
- Literal / numeric entry with `Ctrl-v`: insert the next key verbatim (e.g.
  `Ctrl-v Tab` for a real tab under `expandtab`), or a character code via decimal
  digits, `u`+hex (unicode), `x`+hex, or `o`+octal.

### Registers
- `"*` now maps to the X11/Wayland PRIMARY selection (middle-click paste),
  distinct from `"+` (the system clipboard); both are the one clipboard on Windows
  and macOS.
- System clipboard: the `"+` and `"*` registers read and write the OS clipboard,
  so `"+y` copies to other applications and `"+p` pastes from them. Uses the
  platform clipboard tool (Windows, macOS, `wl-clipboard`/`xclip`/`xsel` on
  Linux); set `RVIM_CLIPBOARD` to a file path for a headless shared clipboard.

### Search
- `*` / `#` (and `g*` / `g#`) take a count, so `3*` jumps to the third next
  occurrence of the word under the cursor.

### Global command
- `:g//cmd` (and `:v//cmd`) with an empty pattern reuse the last search pattern,
  so `/foo` then `:g//d` deletes every `foo` line. Running `:g/pat/…` also records
  `pat` as the current search pattern.

### UI
- `:set listchars=tab:xy,trail:z` customises the `:set list` markers (the tab
  head/fill and the trailing-space glyph); `:set listchars?` shows the current
  values.

### Visual mode
- Ragged-right block append: in visual-block mode `$` extends the selection to
  each line's own end, so `$A` appends the typed text at the end of every line
  regardless of length (vim's `$`-block behavior).

### Customisation
- Key mappings: `:nnoremap {key} {keys}` remaps a single Normal-mode key to a
  sequence (e.g. `:nnoremap Y y$`), with `<CR>`/`<Esc>`/`<Space>`/`<Tab>`/`<BS>`/
  `<C-x>` notation in the right-hand side. Mappings are non-recursive; `:nunmap`
  removes one and bare `:nnoremap` lists them. Works from `~/.rvimrc`.

### Insert mode
- Abbreviations: `:iabbrev lhs rhs` (also `:ab`) defines an insert-mode
  abbreviation that expands when the next non-keyword char is typed;
  `:unabbreviate lhs` removes one and bare `:abbreviate` lists them.
- Whole-line completion: `Ctrl-x Ctrl-l` completes the current line from other
  buffer lines that start with the text before the cursor, with `Ctrl-n` /
  `Ctrl-p` cycling the candidates.
- Filename completion: `Ctrl-x Ctrl-f` completes the path before the cursor from
  the filesystem; directory candidates get a trailing `/`.

### UI
- Mode indicator (vim's `showmode`): the bottom line now shows `-- INSERT --`,
  `-- REPLACE --`, `-- VISUAL --`, `-- VISUAL LINE --`, or `-- VISUAL BLOCK --`
  while in those modes, so the current mode is always visible.

### Internal
- Factored the shared `:s` pattern resolution (empty-pattern reuse, the `i`
  flag, ignorecase/smartcase, regex build) into one `resolve_substitute` helper
  used by both the plain and interactive-confirm substitute paths, so they can no
  longer drift apart.

### Search & replace
- `:s/pat/rep/c` confirms each replacement interactively: the cursor stops on the
  match and `y` replaces, `n` skips, `a` does all remaining, `l` replaces one and
  stops, `q`/Esc quits. The whole run is a single undo.

### Editing
- `U` (normal mode) restores the most recently changed line to its state before
  that change, and repeating `U` toggles it back. The restore is itself undoable
  with `u` (vimtutor 2.5).

### Ex commands
- `:[range]w file` writes just the range's lines to a file (vimtutor 5.3).
- `:r !cmd` inserts a shell command's output below the cursor (vimtutor 5.4).

### Search
- `n` / `N` take a count, so `3n` jumps to the third next match and `2N` to the
  second previous one.

### Docs
- Reorganised the command-line reference in the README into short, grouped
  sections instead of one long run-on list.

### Editing
- External filter: `!{motion}` / `!!` (and visual `!`) prefill a range on the
  command line to pipe those lines through a shell command, replacing them with
  its output; `:[range]!cmd` does it directly and a bare `:!cmd` just runs the
  command. Input is sent with the platform-native line ending so tools like
  `sort` behave correctly on Windows.

### Ex commands
- `:delmarks {marks}` deletes the named marks (ranges like `a-d` are supported);
  `:delmarks!` clears all lowercase marks.

### Search & replace
- An empty `:s` pattern now reuses the last search pattern — `/foo` then
  `:%s//bar/g` replaces every `foo`. Running `:s` also sets the current search
  pattern, so a following `n` moves to the next match (matching vim).

### Text objects
- Tag text objects `it` / `at` select the innermost `<name>…</name>` element
  enclosing the cursor, across lines and honouring nesting (self-closing tags are
  skipped). `it` is the inner content, `at` the whole element — `cit`, `dat`,
  `yit`, `vat`, etc.
- Word and paragraph text objects now take a count: `d3iw` deletes three
  inner-word segments (word, space, word), `d2aw` two whole words with their
  whitespace, and `d2ap` two paragraphs. The count may also precede the operator
  (`2daw` == `d2aw`). Other objects still use the single unit.

### Navigation
- `gf` opens the file whose name is under the cursor, resolving it against the
  current working directory and then the current file's own directory. Reports
  `E446`/`E447` when there is no name under the cursor or the file can't be found.
- `gF` does the same but also jumps to a trailing `:line` number (e.g. on
  `src/editor.rs:120`), and `:edit +N file` opens a file directly at line `N`.

### Ex commands
- `:set` with no argument lists the options changed from their defaults, and
  `:set all` lists every option with its current value (in a scratch buffer, like
  `:marks` / `:changes`). Complements the existing `:set {option}?` query.
- `:retab [N]` normalises indentation to the current `tabstop` (set to `N` first
  when given) and `expandtab`: with `expandtab` every tab is expanded to spaces
  (column-aware); otherwise each line's leading whitespace is re-tabulated into
  tabs. Reports how many lines changed.

### Text objects
- Indentation text objects `ii` / `ai` (vim-indent-object style): `ii` selects the
  contiguous run of lines indented at least as far as the cursor's line, keeping
  blank lines that sit strictly inside the block; `ai` also includes the
  less-indented header line above. Works with every operator, the `>`/`<` indent
  operators, and visual mode — `dii`, `cii`, `>ai`, `vii`, `yii`, …

### Ex commands
- `:changes` lists the change list (the positions `g;` / `g,` navigate), with a
  `>` marking the current slot — a companion to the existing `:marks`,
  `:registers`, and `:jumps` listings.
- `:history` lists the command-line history; `:history /` (or `search`) lists the
  search-pattern history, and `:history all` shows both — matching vim's
  `:history`, with entries numbered oldest-first.

### Info
- `g Ctrl-g` reports the document's word, character, and byte counts along with
  the cursor's line and column.

### Navigation
- `gd` / `gD` jump to the definition of the identifier under the cursor — `gD`
  to its first whole-word occurrence in the file, `gd` to the nearest earlier
  occurrence (local-declaration heuristic). Records a jump for `Ctrl-o`.

### Ex commands
- `:[range]center [w]`, `:[range]right [w]`, `:[range]left [indent]` align the
  range's lines (width defaults to `textwidth`, or 80 when unset).
- `:set {option}?` reports an option's current value (e.g. `:set sw?` →
  `shiftwidth=4`, `:set nu?` → `number`/`nonumber`).
- `:sort /pat/` sorts by the text following the first match of `pat` on each line
  (non-matching lines sort first); the `r` flag (`:sort /pat/ r`) sorts by the
  matched text itself. Combines with the existing `n`/`i`/`u`/`!` flags.

### Editing
- `]p` / `[p` paste the register's lines below / above the current line,
  reindenting them so the first line matches the current line's indent and the
  rest keep their relative indent.

### Motions
- `f`/`F`/`t`/`T` (and `;`/`,`) take a count — `3fx` jumps to the 3rd `x` — and
  work as operator motions: `dfx`, `ct)`, `dFx`, `y2tn`, etc.
- `H` / `M` / `L` (top / middle / bottom of the screen) also work as line-wise
  operator motions — `dL`, `yH`, `cM`.

### Global command
- `:g/re/normal {keys}` (and `:v/…`) runs Normal-mode keystrokes on each
  matching line, processed bottom-to-top so line-count changes (e.g.
  `:g/re/normal dd`) stay correct.

### Undo
- `u` and `Ctrl-r` take a count (`3u`, `2Ctrl-r`), and `:earlier [N]` / `:later
  [N]` step back / forward through N changes.

### Search
- The search match the cursor is currently on is highlighted in a distinct
  colour (vim's CurSearch), so it stands out from the other matches. Added a
  `cur_search_bg` colour to every theme.
- Search offsets: `/pat/e[±N]` (end of match), `/pat/s[±N]` / `/pat/b[±N]`
  (start of match), and `/pat/±N` (N lines from the match). The offset is reused
  by `n` / `N`.
- Visual-mode `*` / `#` search for the selected text (regex-escaped, so it
  matches literally) forward / backward.

### Registers
- Uppercase register names append: `"Ayy` / `"Ad$` adds to register `a` instead
  of replacing it (linewise appends on a new line, charwise concatenates), and
  `"A` reads resolve to `"a`.
- Macros follow the same rule: `qA` appends to macro register `a` (rather than
  overwriting), and `@A` plays `@a`.

### Internal
- Consolidated the duplicated "operator target → row range" logic (used by `gc`,
  `gq`, and `>`/`<`) into a single `target_rows` helper, so a future `OpTarget`
  variant only needs handling in one place.

### UI
- `:set cursorcolumn` (`cuc`) highlights the column the cursor is on down the
  whole screen — a vertical companion to `cursorline` for tracking alignment.

### Motions & text objects
- Text objects work in visual mode: `i`/`a` + an object key selects it (`viw`,
  `vi(`, `vap`, `vi"`, …) instead of entering insert.
- Sentence motions `(` / `)` move backward / forward by sentence (counted,
  across lines), bounded by `.`/`!`/`?` punctuation and blank lines.
- Sentence text objects `is` / `as` (e.g. `das`, `cis`) select the sentence
  under the cursor, with `as` keeping the trailing whitespace.
- Bracket text objects (`i(`/`a(`, `i{`/`a{`, `i[`/`a[`, `i<`/`a<`) now span
  multiple lines, so `ci{` / `da(` / `yi[` work across a block or argument list
  that covers several rows (via a new charwise `OpTarget::Span`).
- `%` is now an operator motion: `d%` / `y%` / `c%` act from the cursor to the
  matching bracket, inclusive and across lines.
- The `>` / `<` indent operators accept motions and text objects — `>ip`, `>i{`
  (multi-line), `<ap` — not just the doubled `>>` / `<<` form.

### Marks
- The `` `[ `` / `` `] `` (and linewise `'[` / `']`) marks are now set to the
  start and end of the last changed, yanked, or put text — operator `d`/`c`/`y`
  with any motion or text object, visual yank/delete, and `p`/`P`/`gp`/`gP`/`:put`
  — so you can jump to or operate on the region you just touched.

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
- Wildmenu: when a `Tab` completion has more than one candidate, the candidates
  are shown just above the command line with the current one highlighted,
  sliding to keep the selection visible on a narrow terminal.

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
