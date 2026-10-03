# Changelog

All notable changes to rvim are recorded here. Versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Registers
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
