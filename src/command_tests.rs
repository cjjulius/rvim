    use super::*;

    #[test]
    fn empty() {
        assert_eq!(parse("  "), ExCommand::Empty);
    }

    #[test]
    fn write_variants() {
        assert_eq!(parse("w"), ExCommand::Write(None));
        assert_eq!(parse("w out.rs"), ExCommand::Write(Some("out.rs".into())));
        assert_eq!(parse("write foo"), ExCommand::Write(Some("foo".into())));
    }

    #[test]
    fn move_and_copy_variants() {
        assert_eq!(
            parse("m0"),
            ExCommand::MoveLines { range: SubRange::CurrentLine, dest: LineAddr::Num(0) }
        );
        assert_eq!(
            parse("1,3m$"),
            ExCommand::MoveLines {
                range: SubRange::Range(LineAddr::Num(1), LineAddr::Num(3)),
                dest: LineAddr::Last
            }
        );
        assert_eq!(
            parse("t."),
            ExCommand::CopyLines { range: SubRange::CurrentLine, dest: LineAddr::Current }
        );
        assert_eq!(
            parse("2,4copy0"),
            ExCommand::CopyLines {
                range: SubRange::Range(LineAddr::Num(2), LineAddr::Num(4)),
                dest: LineAddr::Num(0)
            }
        );
        // `colo`/`colorscheme` must still reach the theme command, not copy.
        assert_eq!(parse("colo"), ExCommand::SetTheme(None));
    }

    #[test]
    fn mark_and_visual_range_addresses() {
        assert_eq!(
            parse("'<,'>d"),
            ExCommand::DeleteLines(SubRange::Range(
                LineAddr::Mark('<'),
                LineAddr::Mark('>')
            ))
        );
        assert_eq!(
            parse("'a,'bm0"),
            ExCommand::MoveLines {
                range: SubRange::Range(LineAddr::Mark('a'), LineAddr::Mark('b')),
                dest: LineAddr::Num(0)
            }
        );
        // A marked range also drives :s.
        match parse("'<,'>s/x/y/") {
            ExCommand::Substitute(spec) => assert_eq!(
                spec.range,
                SubRange::Range(LineAddr::Mark('<'), LineAddr::Mark('>'))
            ),
            other => panic!("expected substitute, got {other:?}"),
        }
    }

    #[test]
    fn line_op_variants() {
        assert_eq!(parse("d"), ExCommand::DeleteLines(SubRange::CurrentLine));
        assert_eq!(
            parse("1,5d"),
            ExCommand::DeleteLines(SubRange::Range(LineAddr::Num(1), LineAddr::Num(5)))
        );
        assert_eq!(parse("%y"), ExCommand::YankLines(SubRange::WholeFile));
        assert_eq!(parse("delete"), ExCommand::DeleteLines(SubRange::CurrentLine));
        assert_eq!(
            parse(">>"),
            ExCommand::ShiftLines { range: SubRange::CurrentLine, dedent: false, times: 2 }
        );
        assert_eq!(
            parse("1,3<"),
            ExCommand::ShiftLines {
                range: SubRange::Range(LineAddr::Num(1), LineAddr::Num(3)),
                dedent: true,
                times: 1
            }
        );
        // Longer d-/y-words must not be swallowed by :d / :y.
        assert!(matches!(parse("diffsplit"), ExCommand::Passthrough { .. }));
        assert_eq!(parse("bd"), ExCommand::BufferDelete);
        assert_eq!(
            parse("1,3j"),
            ExCommand::JoinLines {
                range: SubRange::Range(LineAddr::Num(1), LineAddr::Num(3)),
                raw: false
            }
        );
        assert_eq!(
            parse("join!"),
            ExCommand::JoinLines { range: SubRange::CurrentLine, raw: true }
        );
    }

    #[test]
    fn write_quit_all_variants() {
        assert_eq!(parse("qa"), ExCommand::QuitAll { force: false });
        assert_eq!(parse("qall"), ExCommand::QuitAll { force: false });
        assert_eq!(parse("qa!"), ExCommand::QuitAll { force: true });
        assert_eq!(parse("wa"), ExCommand::WriteAll);
        assert_eq!(parse("wall"), ExCommand::WriteAll);
        assert_eq!(parse("wqa"), ExCommand::WriteQuitAll { force: false });
        assert_eq!(parse("xa"), ExCommand::WriteQuitAll { force: false });
        assert_eq!(parse("wqa!"), ExCommand::WriteQuitAll { force: true });
    }

    #[test]
    fn quit_variants() {
        assert_eq!(parse("q"), ExCommand::Quit { force: false });
        assert_eq!(parse("q!"), ExCommand::Quit { force: true });
    }

    #[test]
    fn writequit_variants() {
        assert_eq!(parse("wq"), ExCommand::WriteQuit(None));
        assert_eq!(parse("x"), ExCommand::WriteQuit(None));
        assert_eq!(parse("wq file.txt"), ExCommand::WriteQuit(Some("file.txt".into())));
    }

    #[test]
    fn goto_line() {
        assert_eq!(parse("42"), ExCommand::Goto(42));
    }

    #[test]
    fn theme() {
        assert_eq!(parse("theme cobalt"), ExCommand::SetTheme(Some("cobalt".into())));
        assert_eq!(parse("colorscheme"), ExCommand::SetTheme(None));
        assert_eq!(parse("colo matrix"), ExCommand::SetTheme(Some("matrix".into())));
    }

    #[test]
    fn set_number() {
        assert_eq!(parse("set number"), ExCommand::ToggleNumbers(true));
        assert_eq!(parse("set nonu"), ExCommand::ToggleNumbers(false));
    }

    #[test]
    fn set_relativenumber() {
        assert_eq!(parse("set relativenumber"), ExCommand::ToggleRelativeNumbers(true));
        assert_eq!(parse("set rnu"), ExCommand::ToggleRelativeNumbers(true));
        assert_eq!(parse("set nornu"), ExCommand::ToggleRelativeNumbers(false));
    }

    #[test]
    fn set_autoindent() {
        assert_eq!(parse("set autoindent"), ExCommand::ToggleAutoIndent(true));
        assert_eq!(parse("set ai"), ExCommand::ToggleAutoIndent(true));
        assert_eq!(parse("set noai"), ExCommand::ToggleAutoIndent(false));
    }

    #[test]
    fn set_indentation_options() {
        assert_eq!(parse("set expandtab"), ExCommand::ToggleExpandTab(true));
        assert_eq!(parse("set noet"), ExCommand::ToggleExpandTab(false));
        assert_eq!(parse("set shiftwidth=2"), ExCommand::SetShiftWidth(2));
        assert_eq!(parse("set sw=8"), ExCommand::SetShiftWidth(8));
        assert_eq!(parse("set tabstop=4"), ExCommand::SetTabStop(4));
        assert_eq!(parse("set ts=2"), ExCommand::SetTabStop(2));
    }

    #[test]
    fn set_scrolloff() {
        assert_eq!(parse("set scrolloff=5"), ExCommand::SetScrollOff(5));
        assert_eq!(parse("set so=3"), ExCommand::SetScrollOff(3));
        assert_eq!(parse("set scrolloff=0"), ExCommand::SetScrollOff(0));
    }

    #[test]
    fn set_sidescrolloff() {
        assert_eq!(parse("set sidescrolloff=5"), ExCommand::SetSideScrollOff(5));
        assert_eq!(parse("set siso=3"), ExCommand::SetSideScrollOff(3));
    }

    #[test]
    fn set_textwidth() {
        assert_eq!(parse("set textwidth=40"), ExCommand::SetTextWidth(40));
        assert_eq!(parse("set tw=72"), ExCommand::SetTextWidth(72));
    }

    #[test]
    fn set_list() {
        assert_eq!(parse("set list"), ExCommand::ToggleList(true));
        assert_eq!(parse("set nolist"), ExCommand::ToggleList(false));
    }

    #[test]
    fn set_colorcolumn() {
        assert_eq!(parse("set colorcolumn=80"), ExCommand::SetColorColumn(80));
        assert_eq!(parse("set cc=0"), ExCommand::SetColorColumn(0));
    }

    #[test]
    fn set_cursorline() {
        assert_eq!(parse("set cursorline"), ExCommand::ToggleCursorLine(true));
        assert_eq!(parse("set cul"), ExCommand::ToggleCursorLine(true));
        assert_eq!(parse("set nocursorline"), ExCommand::ToggleCursorLine(false));
        assert_eq!(parse("set nocul"), ExCommand::ToggleCursorLine(false));
    }

    #[test]
    fn align_parse() {
        assert_eq!(
            parse("center 10"),
            ExCommand::Align { range: SubRange::CurrentLine, kind: AlignKind::Center, width: Some(10) }
        );
        assert_eq!(
            parse("ce"),
            ExCommand::Align { range: SubRange::CurrentLine, kind: AlignKind::Center, width: None }
        );
        assert_eq!(
            parse("1,5right 60"),
            ExCommand::Align {
                range: SubRange::Range(LineAddr::Num(1), LineAddr::Num(5)),
                kind: AlignKind::Right,
                width: Some(60),
            }
        );
        assert_eq!(
            parse("le 4"),
            ExCommand::Align { range: SubRange::CurrentLine, kind: AlignKind::Left, width: Some(4) }
        );
        // Not an alignment command.
        assert!(matches!(parse("centern"), ExCommand::Passthrough { .. }));
    }

    #[test]
    fn set_query_parse() {
        assert_eq!(parse("set sw?"), ExCommand::SetQuery("sw".into()));
        assert_eq!(parse("set number?"), ExCommand::SetQuery("number".into()));
    }

    #[test]
    fn earlier_later_parse() {
        assert_eq!(parse("earlier 3"), ExCommand::Earlier(3));
        assert_eq!(parse("earlier"), ExCommand::Earlier(1));
        assert_eq!(parse("ea 5"), ExCommand::Earlier(5));
        assert_eq!(parse("later 2"), ExCommand::Later(2));
        assert_eq!(parse("later"), ExCommand::Later(1));
    }

    #[test]
    fn set_cursorcolumn() {
        assert_eq!(parse("set cursorcolumn"), ExCommand::ToggleCursorColumn(true));
        assert_eq!(parse("set cuc"), ExCommand::ToggleCursorColumn(true));
        assert_eq!(parse("set nocursorcolumn"), ExCommand::ToggleCursorColumn(false));
        assert_eq!(parse("set nocuc"), ExCommand::ToggleCursorColumn(false));
    }

    #[test]
    fn set_wrapscan() {
        assert_eq!(parse("set wrapscan"), ExCommand::ToggleWrapScan(true));
        assert_eq!(parse("set ws"), ExCommand::ToggleWrapScan(true));
        assert_eq!(parse("set nowrapscan"), ExCommand::ToggleWrapScan(false));
        assert_eq!(parse("set nows"), ExCommand::ToggleWrapScan(false));
    }

    #[test]
    fn set_case_options() {
        assert_eq!(parse("set ignorecase"), ExCommand::ToggleIgnoreCase(true));
        assert_eq!(parse("set ic"), ExCommand::ToggleIgnoreCase(true));
        assert_eq!(parse("set noignorecase"), ExCommand::ToggleIgnoreCase(false));
        assert_eq!(parse("set smartcase"), ExCommand::ToggleSmartCase(true));
        assert_eq!(parse("set scs"), ExCommand::ToggleSmartCase(true));
        assert_eq!(parse("set noscs"), ExCommand::ToggleSmartCase(false));
        assert_eq!(parse("set incsearch"), ExCommand::ToggleIncSearch(true));
        assert_eq!(parse("set is"), ExCommand::ToggleIncSearch(true));
        assert_eq!(parse("set noincsearch"), ExCommand::ToggleIncSearch(false));
    }

    #[test]
    fn set_filetype() {
        assert_eq!(parse("set ft=rust"), ExCommand::SetFiletype("rust".into()));
        assert_eq!(parse("set filetype=tsql"), ExCommand::SetFiletype("tsql".into()));
    }

    #[test]
    fn passthrough_for_plugins() {
        assert_eq!(
            parse("wordcount"),
            ExCommand::Passthrough {
                name: "wordcount".into(),
                args: String::new()
            }
        );
    }

    #[test]
    fn edit_and_reload() {
        assert_eq!(parse("e main.rs"), ExCommand::Edit("main.rs".into()));
        assert_eq!(parse("e! main.rs"), ExCommand::Edit("main.rs".into()));
        assert_eq!(parse("e"), ExCommand::Reload { force: false });
        assert_eq!(parse("e!"), ExCommand::Reload { force: true });
        assert_eq!(parse("edit!"), ExCommand::Reload { force: true });
    }

    #[test]
    fn put_variants() {
        assert_eq!(
            parse("put"),
            ExCommand::PutRegister { dest: LineAddr::Current, register: None }
        );
        assert_eq!(
            parse("pu a"),
            ExCommand::PutRegister { dest: LineAddr::Current, register: Some('a') }
        );
        assert_eq!(
            parse("0put"),
            ExCommand::PutRegister { dest: LineAddr::Num(0), register: None }
        );
        assert_eq!(
            parse("3put x"),
            ExCommand::PutRegister { dest: LineAddr::Num(3), register: Some('x') }
        );
    }

    #[test]
    fn read_file_variants() {
        assert_eq!(parse("r notes.txt"), ExCommand::ReadFile("notes.txt".into()));
        assert_eq!(parse("read data.csv"), ExCommand::ReadFile("data.csv".into()));
        // No file name falls through to the plugin passthrough, not a crash.
        assert!(matches!(parse("r"), ExCommand::Passthrough { .. }));
    }

    #[test]
    fn marks_registers_jumps_commands() {
        assert_eq!(parse("marks"), ExCommand::Marks);
        assert_eq!(parse("reg"), ExCommand::Registers);
        assert_eq!(parse("registers"), ExCommand::Registers);
        assert_eq!(parse("jumps"), ExCommand::Jumps);
        assert_eq!(parse("changes"), ExCommand::Changes);
    }

    #[test]
    fn buffer_commands() {
        assert_eq!(parse("ls"), ExCommand::BufferList);
        assert_eq!(parse("buffers"), ExCommand::BufferList);
        assert_eq!(parse("bn"), ExCommand::BufferNext);
        assert_eq!(parse("bprev"), ExCommand::BufferPrev);
        assert_eq!(parse("bd"), ExCommand::BufferDelete);
        assert_eq!(parse("b#"), ExCommand::BufferAlternate);
        assert_eq!(parse("e#"), ExCommand::BufferAlternate);
        assert_eq!(parse("b #"), ExCommand::BufferAlternate);
        assert_eq!(parse("b 3"), ExCommand::Buffer(3));
        assert_eq!(parse("buffer 2"), ExCommand::Buffer(2));
    }

    #[test]
    fn sort_variants() {
        let base = |reverse, unique, numeric, ignorecase| ExCommand::Sort {
            range: SubRange::WholeFile,
            reverse,
            unique,
            numeric,
            ignorecase,
            pattern: None,
            use_match: false,
        };
        assert_eq!(parse("sort"), base(false, false, false, false));
        assert_eq!(parse("sort!"), base(true, false, false, false));
        assert_eq!(parse("sort u"), base(false, true, false, false));
        assert_eq!(parse("sort! u"), base(true, true, false, false));
        assert_eq!(parse("sort n"), base(false, false, true, false));
        assert_eq!(parse("sort i"), base(false, false, false, true));
        assert_eq!(parse("sort! un"), base(true, true, true, false));
        // A leading range is carried through, and `source` must not match.
        assert_eq!(
            parse("1,3sort"),
            ExCommand::Sort {
                range: SubRange::Range(LineAddr::Num(1), LineAddr::Num(3)),
                reverse: false,
                unique: false,
                numeric: false,
                ignorecase: false,
                pattern: None,
                use_match: false,
            }
        );
        assert!(matches!(parse("source foo"), ExCommand::Source(_)));

        // `:sort /pat/` parses the pattern; the `r` flag sorts on the match.
        assert_eq!(
            parse("sort /\\d\\+/"),
            ExCommand::Sort {
                range: SubRange::WholeFile,
                reverse: false,
                unique: false,
                numeric: false,
                ignorecase: false,
                pattern: Some("\\d\\+".into()),
                use_match: false,
            }
        );
        assert_eq!(
            parse("sort /x/ r"),
            ExCommand::Sort {
                range: SubRange::WholeFile,
                reverse: false,
                unique: false,
                numeric: false,
                ignorecase: false,
                pattern: Some("x".into()),
                use_match: true,
            }
        );
    }

    #[test]
    fn nohlsearch_variants() {
        assert_eq!(parse("noh"), ExCommand::ToggleHlSearch(false));
        assert_eq!(parse("nohlsearch"), ExCommand::ToggleHlSearch(false));
        assert_eq!(parse("set hlsearch"), ExCommand::ToggleHlSearch(true));
        assert_eq!(parse("set nohls"), ExCommand::ToggleHlSearch(false));
    }

    #[test]
    fn source_command() {
        assert_eq!(parse("source ~/.rvimrc"), ExCommand::Source("~/.rvimrc".into()));
        assert_eq!(parse("so init.vim"), ExCommand::Source("init.vim".into()));
    }

    #[test]
    fn substitute_current_line() {
        assert_eq!(
            parse("s/foo/bar/"),
            ExCommand::Substitute(SubstituteSpec {
                range: SubRange::CurrentLine,
                pattern: "foo".into(),
                replacement: "bar".into(),
                global: false,
                ignorecase: false, count_only: false,
            })
        );
    }

    #[test]
    fn substitute_count_flag() {
        match parse("%s/foo//n") {
            ExCommand::Substitute(spec) => {
                assert!(spec.count_only);
                assert_eq!(spec.pattern, "foo");
            }
            other => panic!("expected substitute, got {other:?}"),
        }
    }

    #[test]
    fn normal_command_parses() {
        match parse("normal A;") {
            ExCommand::Normal { range, keys } => {
                assert_eq!(range, None);
                assert_eq!(keys, "A;");
            }
            other => panic!("expected normal, got {other:?}"),
        }
        match parse("%normal I#") {
            ExCommand::Normal { range, keys } => {
                assert_eq!(range, Some(SubRange::WholeFile));
                assert_eq!(keys, "I#");
            }
            other => panic!("expected normal, got {other:?}"),
        }
        // `normalize` is not `:normal`.
        assert!(!matches!(parse("normalize"), ExCommand::Normal { .. }));
    }

    #[test]
    fn global_commands() {
        assert_eq!(
            parse("g/foo/d"),
            ExCommand::Global { pattern: "foo".into(), invert: false, command: "d".into() }
        );
        assert_eq!(
            parse("v/foo/d"),
            ExCommand::Global { pattern: "foo".into(), invert: true, command: "d".into() }
        );
        assert_eq!(
            parse("g!/bar/d"),
            ExCommand::Global { pattern: "bar".into(), invert: true, command: "d".into() }
        );
        assert_eq!(
            parse("g/x/s/a/b/g"),
            ExCommand::Global { pattern: "x".into(), invert: false, command: "s/a/b/g".into() }
        );
    }

    #[test]
    fn global_does_not_hijack_other_commands() {
        assert!(!matches!(parse("version"), ExCommand::Global { .. }));
        assert!(!matches!(parse("wq"), ExCommand::Global { .. }));
    }

    #[test]
    fn substitute_ignorecase_flag() {
        assert_eq!(
            parse("s/foo/bar/gi"),
            ExCommand::Substitute(SubstituteSpec {
                range: SubRange::CurrentLine,
                pattern: "foo".into(),
                replacement: "bar".into(),
                global: true,
                ignorecase: true, count_only: false,
            })
        );
    }

    #[test]
    fn substitute_whole_file_global() {
        assert_eq!(
            parse("%s/foo/bar/g"),
            ExCommand::Substitute(SubstituteSpec {
                range: SubRange::WholeFile,
                pattern: "foo".into(),
                replacement: "bar".into(),
                global: true,
                ignorecase: false, count_only: false,
            })
        );
    }

    #[test]
    fn substitute_numeric_range() {
        assert_eq!(
            parse("2,5s/x/y/"),
            ExCommand::Substitute(SubstituteSpec {
                range: SubRange::Range(LineAddr::Num(2), LineAddr::Num(5)),
                pattern: "x".into(),
                replacement: "y".into(),
                global: false,
                ignorecase: false, count_only: false,
            })
        );
    }

    #[test]
    fn substitute_symbolic_range() {
        assert_eq!(
            parse(".,$s/a/b/g"),
            ExCommand::Substitute(SubstituteSpec {
                range: SubRange::Range(LineAddr::Current, LineAddr::Last),
                pattern: "a".into(),
                replacement: "b".into(),
                global: true,
                ignorecase: false, count_only: false,
            })
        );
    }

    #[test]
    fn substitute_empty_replacement_deletes() {
        assert_eq!(
            parse("s/drop//"),
            ExCommand::Substitute(SubstituteSpec {
                range: SubRange::CurrentLine,
                pattern: "drop".into(),
                replacement: "".into(),
                global: false,
                ignorecase: false, count_only: false,
            })
        );
    }

    #[test]
    fn substitute_does_not_hijack_other_commands() {
        // These start with 's' or contain digits but are not substitutions.
        assert!(!matches!(parse("set number"), ExCommand::Substitute(_)));
        assert!(!matches!(parse("42"), ExCommand::Substitute(_)));
        assert!(!matches!(parse("w"), ExCommand::Substitute(_)));
        assert!(!matches!(parse("x"), ExCommand::Substitute(_)));
    }
