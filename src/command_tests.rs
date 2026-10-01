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
    fn edit() {
        assert_eq!(parse("e main.rs"), ExCommand::Edit("main.rs".into()));
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
    fn buffer_commands() {
        assert_eq!(parse("ls"), ExCommand::BufferList);
        assert_eq!(parse("buffers"), ExCommand::BufferList);
        assert_eq!(parse("bn"), ExCommand::BufferNext);
        assert_eq!(parse("bprev"), ExCommand::BufferPrev);
        assert_eq!(parse("bd"), ExCommand::BufferDelete);
        assert_eq!(parse("b 3"), ExCommand::Buffer(3));
        assert_eq!(parse("buffer 2"), ExCommand::Buffer(2));
    }

    #[test]
    fn sort_variants() {
        assert_eq!(parse("sort"), ExCommand::Sort { reverse: false, unique: false });
        assert_eq!(parse("sort!"), ExCommand::Sort { reverse: true, unique: false });
        assert_eq!(parse("sort u"), ExCommand::Sort { reverse: false, unique: true });
        assert_eq!(parse("sort! u"), ExCommand::Sort { reverse: true, unique: true });
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
                ignorecase: false,
            })
        );
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
                ignorecase: true,
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
                ignorecase: false,
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
                ignorecase: false,
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
                ignorecase: false,
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
                ignorecase: false,
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
