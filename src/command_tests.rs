    use super::*;

    #[test]
    fn empty() {
        assert_eq!(parse("  "), ExCommand::Empty);
    }

    #[test]
    fn write_variants() {
        assert_eq!(parse("w"), ExCommand::Write { path: None, force: false });
        assert_eq!(parse("w out.rs"), ExCommand::Write { path: Some("out.rs".into()), force: false });
        assert_eq!(parse("write foo"), ExCommand::Write { path: Some("foo".into()), force: false });
        assert_eq!(parse("w!"), ExCommand::Write { path: None, force: true });
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
            ExCommand::DeleteLines {
                range: SubRange::Range(LineAddr::Mark('<'), LineAddr::Mark('>')),
                register: None,
            }
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
        assert_eq!(
            parse("d"),
            ExCommand::DeleteLines { range: SubRange::CurrentLine, register: None }
        );
        assert_eq!(
            parse("1,5d"),
            ExCommand::DeleteLines {
                range: SubRange::Range(LineAddr::Num(1), LineAddr::Num(5)),
                register: None,
            }
        );
        assert_eq!(
            parse("%y"),
            ExCommand::YankLines { range: SubRange::WholeFile, register: None }
        );
        assert_eq!(
            parse("delete"),
            ExCommand::DeleteLines { range: SubRange::CurrentLine, register: None }
        );
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
    fn write_append_parses() {
        assert_eq!(
            parse("w >>log.txt"),
            ExCommand::WriteAppend { range: None, file: "log.txt".into() }
        );
        assert_eq!(
            parse("%w >>out"),
            ExCommand::WriteAppend { range: Some(SubRange::WholeFile), file: "out".into() }
        );
        // A plain write is unaffected.
        assert_eq!(
            parse("w file"),
            ExCommand::Write { path: Some("file".into()), force: false }
        );
    }

    #[test]
    fn write_filter_parses() {
        assert_eq!(
            parse("w !wc"),
            ExCommand::WriteFilter { range: None, cmd: "wc".into() }
        );
        assert_eq!(
            parse("%w !sort"),
            ExCommand::WriteFilter { range: Some(SubRange::WholeFile), cmd: "sort".into() }
        );
        // A real filename still writes to a file.
        assert_eq!(
            parse("w out.txt"),
            ExCommand::Write { path: Some("out.txt".into()), force: false }
        );
    }

    #[test]
    fn line_op_register_argument() {
        assert_eq!(
            parse("d a"),
            ExCommand::DeleteLines { range: SubRange::CurrentLine, register: Some('a') }
        );
        assert_eq!(
            parse("y b"),
            ExCommand::YankLines { range: SubRange::CurrentLine, register: Some('b') }
        );
        assert_eq!(
            parse("1,3y z"),
            ExCommand::YankLines {
                range: SubRange::Range(LineAddr::Num(1), LineAddr::Num(3)),
                register: Some('z'),
            }
        );
        // A multi-char trailing token is not a register, so `:d` doesn't match it.
        assert!(!matches!(parse("d foo"), ExCommand::DeleteLines { .. }));
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
    fn enew_parses() {
        assert_eq!(parse("enew"), ExCommand::Enew);
        assert_eq!(parse("ene"), ExCommand::Enew);
    }

    #[test]
    fn cd_and_pwd_parse() {
        assert_eq!(parse("pwd"), ExCommand::Pwd);
        assert_eq!(parse("cd"), ExCommand::ChangeDir(None));
        assert_eq!(parse("cd /tmp"), ExCommand::ChangeDir(Some("/tmp".into())));
        assert_eq!(parse("chdir src"), ExCommand::ChangeDir(Some("src".into())));
    }

    #[test]
    fn writequit_variants() {
        assert_eq!(parse("wq"), ExCommand::WriteQuit { path: None, force: false });
        assert_eq!(parse("x"), ExCommand::WriteQuit { path: None, force: false });
        assert_eq!(parse("wq file.txt"), ExCommand::WriteQuit { path: Some("file.txt".into()), force: false });
        assert_eq!(parse("wq!"), ExCommand::WriteQuit { path: None, force: true });
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
    fn tag_commands_parse() {
        assert_eq!(parse("tag foo"), ExCommand::Tag(Some("foo".into())));
        assert_eq!(parse("ta bar"), ExCommand::Tag(Some("bar".into())));
        assert_eq!(parse("pop"), ExCommand::TagPop);
        assert_eq!(parse("tags"), ExCommand::TagsList);
    }

    #[test]
    fn tag_match_commands_parse() {
        use crate::command::TagMove;
        assert_eq!(parse("tnext"), ExCommand::TagMatch(TagMove::Next));
        assert_eq!(parse("tn"), ExCommand::TagMatch(TagMove::Next));
        assert_eq!(parse("tprev"), ExCommand::TagMatch(TagMove::Prev));
        assert_eq!(parse("tN"), ExCommand::TagMatch(TagMove::Prev));
        assert_eq!(parse("tfirst"), ExCommand::TagMatch(TagMove::First));
        assert_eq!(parse("tlast"), ExCommand::TagMatch(TagMove::Last));
        assert_eq!(parse("tselect foo"), ExCommand::TagSelect(Some("foo".into())));
        assert_eq!(parse("ts"), ExCommand::TagSelect(None));
    }

    #[test]
    fn messages_command_parses() {
        assert_eq!(parse("messages"), ExCommand::Messages);
        assert_eq!(parse("mes"), ExCommand::Messages);
        assert_eq!(parse("message"), ExCommand::Messages);
    }

    #[test]
    fn set_gdefault() {
        assert_eq!(parse("set gdefault"), ExCommand::ToggleGdefault(true));
        assert_eq!(parse("set gd"), ExCommand::ToggleGdefault(true));
        assert_eq!(parse("set nogdefault"), ExCommand::ToggleGdefault(false));
        assert_eq!(parse("set nogd"), ExCommand::ToggleGdefault(false));
    }

    #[test]
    fn set_autoread() {
        assert_eq!(parse("set autoread"), ExCommand::ToggleAutoread(true));
        assert_eq!(parse("set ar"), ExCommand::ToggleAutoread(true));
        assert_eq!(parse("set noautoread"), ExCommand::ToggleAutoread(false));
        assert_eq!(parse("set noar"), ExCommand::ToggleAutoread(false));
    }

    #[test]
    fn set_autowrite() {
        assert_eq!(parse("set autowrite"), ExCommand::ToggleAutowrite(true));
        assert_eq!(parse("set aw"), ExCommand::ToggleAutowrite(true));
        assert_eq!(parse("set noautowrite"), ExCommand::ToggleAutowrite(false));
        assert_eq!(parse("set noaw"), ExCommand::ToggleAutowrite(false));
    }

    #[test]
    fn set_foldlevel() {
        assert_eq!(parse("set foldlevel=2"), ExCommand::SetFoldLevel(2));
        assert_eq!(parse("set fdl=0"), ExCommand::SetFoldLevel(0));
    }

    #[test]
    fn set_formatprg() {
        assert_eq!(parse("set formatprg=fmt"), ExCommand::SetFormatprg("fmt".into()));
        assert_eq!(parse("set fp=par"), ExCommand::SetFormatprg("par".into()));
        assert_eq!(parse("set formatprg="), ExCommand::SetFormatprg(String::new()));
    }

    #[test]
    fn set_virtualedit() {
        assert_eq!(
            parse("set virtualedit=onemore"),
            ExCommand::SetVirtualEdit("onemore".into())
        );
        assert_eq!(parse("set ve=all"), ExCommand::SetVirtualEdit("all".into()));
        assert_eq!(parse("set ve="), ExCommand::SetVirtualEdit(String::new()));
    }

    #[test]
    fn set_nrformats() {
        assert_eq!(
            parse("set nrformats=bin,hex,alpha"),
            ExCommand::SetNrformats("bin,hex,alpha".into())
        );
        assert_eq!(parse("set nf=bin,hex"), ExCommand::SetNrformats("bin,hex".into()));
    }

    #[test]
    fn set_equalprg() {
        assert_eq!(
            parse("set equalprg=clang-format"),
            ExCommand::SetEqualprg("clang-format".into())
        );
        assert_eq!(parse("set ep=rustfmt"), ExCommand::SetEqualprg("rustfmt".into()));
        assert_eq!(parse("set equalprg="), ExCommand::SetEqualprg(String::new()));
    }

    #[test]
    fn set_modeline() {
        assert_eq!(parse("set modeline"), ExCommand::ToggleModeline(true));
        assert_eq!(parse("set ml"), ExCommand::ToggleModeline(true));
        assert_eq!(parse("set nomodeline"), ExCommand::ToggleModeline(false));
        assert_eq!(parse("set noml"), ExCommand::ToggleModeline(false));
        assert_eq!(parse("set modelines=3"), ExCommand::SetModelines(3));
        assert_eq!(parse("set mls=0"), ExCommand::SetModelines(0));
    }

    #[test]
    fn set_list() {
        assert_eq!(parse("set list"), ExCommand::ToggleList(true));
        assert_eq!(parse("set nolist"), ExCommand::ToggleList(false));
    }

    #[test]
    fn set_colorcolumn() {
        assert_eq!(
            parse("set colorcolumn=80"),
            ExCommand::SetColorColumn { abs: vec![80], rel: vec![] }
        );
        assert_eq!(
            parse("set cc=80,120"),
            ExCommand::SetColorColumn { abs: vec![80, 120], rel: vec![] }
        );
        assert_eq!(
            parse("set cc=0"),
            ExCommand::SetColorColumn { abs: vec![], rel: vec![] }
        );
        assert_eq!(
            parse("set cc="),
            ExCommand::SetColorColumn { abs: vec![], rel: vec![] }
        );
        // Relative offsets (+N / -N) resolve against textwidth.
        assert_eq!(
            parse("set cc=+1"),
            ExCommand::SetColorColumn { abs: vec![], rel: vec![1] }
        );
        assert_eq!(
            parse("set cc=80,+1,-2"),
            ExCommand::SetColorColumn { abs: vec![80], rel: vec![1, -2] }
        );
    }

    #[test]
    fn set_listchars_parse() {
        assert_eq!(
            parse("set listchars=tab:>-,trail:~"),
            ExCommand::SetListchars("tab:>-,trail:~".into())
        );
        assert_eq!(parse("set lcs=trail:."), ExCommand::SetListchars("trail:.".into()));
    }

    #[test]
    fn set_cursorline() {
        assert_eq!(parse("set cursorline"), ExCommand::ToggleCursorLine(true));
        assert_eq!(parse("set cul"), ExCommand::ToggleCursorLine(true));
        assert_eq!(parse("set nocursorline"), ExCommand::ToggleCursorLine(false));
        assert_eq!(parse("set nocul"), ExCommand::ToggleCursorLine(false));
    }

    #[test]
    fn set_errorbells() {
        assert_eq!(parse("set errorbells"), ExCommand::ToggleErrorBells(true));
        assert_eq!(parse("set eb"), ExCommand::ToggleErrorBells(true));
        assert_eq!(parse("set noerrorbells"), ExCommand::ToggleErrorBells(false));
        assert_eq!(parse("set noeb"), ExCommand::ToggleErrorBells(false));
    }

    #[test]
    fn set_visualbell() {
        assert_eq!(parse("set visualbell"), ExCommand::ToggleVisualBell(true));
        assert_eq!(parse("set vb"), ExCommand::ToggleVisualBell(true));
        assert_eq!(parse("set novisualbell"), ExCommand::ToggleVisualBell(false));
        assert_eq!(parse("set novb"), ExCommand::ToggleVisualBell(false));
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
    fn set_show_options_parse() {
        assert_eq!(parse("set"), ExCommand::ShowOptions(false));
        assert_eq!(parse("set all"), ExCommand::ShowOptions(true));
    }

    #[test]
    fn write_range_parse() {
        assert_eq!(
            parse("1,5w out.txt"),
            ExCommand::WriteRange {
                range: SubRange::Range(LineAddr::Num(1), LineAddr::Num(5)),
                file: Some("out.txt".into())
            }
        );
        assert_eq!(
            parse("%w out.txt"),
            ExCommand::WriteRange { range: SubRange::WholeFile, file: Some("out.txt".into()) }
        );
        // A bare :w and :wq are NOT range writes.
        assert_eq!(parse("w out.txt"), ExCommand::Write { path: Some("out.txt".into()), force: false });
        assert!(!matches!(parse("1,5wq"), ExCommand::WriteRange { .. }));
    }

    #[test]
    fn read_command_parse() {
        assert_eq!(
            parse("r !ls"),
            ExCommand::ReadFile { addr: None, path: "!ls".into() }
        );
    }

    #[test]
    fn addressed_read_parse() {
        assert_eq!(
            parse("0r notes.txt"),
            ExCommand::ReadFile { addr: Some(LineAddr::Num(0)), path: "notes.txt".into() }
        );
        assert_eq!(
            parse("$r !date"),
            ExCommand::ReadFile { addr: Some(LineAddr::Last), path: "!date".into() }
        );
        assert_eq!(
            parse("3read f.txt"),
            ExCommand::ReadFile { addr: Some(LineAddr::Num(3)), path: "f.txt".into() }
        );
        // No address still parses as a plain read (handled by the bare `:r` arm).
        assert_eq!(
            parse("r x.txt"),
            ExCommand::ReadFile { addr: None, path: "x.txt".into() }
        );
    }

    #[test]
    fn filter_parse() {
        assert_eq!(
            parse("%!sort"),
            ExCommand::Filter { range: Some(SubRange::WholeFile), cmd: "sort".into() }
        );
        assert_eq!(
            parse("1,3!sort"),
            ExCommand::Filter {
                range: Some(SubRange::Range(LineAddr::Num(1), LineAddr::Num(3))),
                cmd: "sort".into()
            }
        );
        assert_eq!(parse("!ls"), ExCommand::Filter { range: None, cmd: "ls".into() });
    }

    #[test]
    fn retab_parse() {
        assert_eq!(parse("retab"), ExCommand::Retab(None));
        assert_eq!(parse("retab 2"), ExCommand::Retab(Some(2)));
        assert_eq!(parse("ret 8"), ExCommand::Retab(Some(8)));
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
        assert_eq!(
            parse("e main.rs"),
            ExCommand::Edit { path: "main.rs".into(), line: None }
        );
        assert_eq!(
            parse("e! main.rs"),
            ExCommand::Edit { path: "main.rs".into(), line: None }
        );
        assert_eq!(
            parse("e +42 main.rs"),
            ExCommand::Edit { path: "main.rs".into(), line: Some(42) }
        );
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
        assert_eq!(
            parse("r notes.txt"),
            ExCommand::ReadFile { addr: None, path: "notes.txt".into() }
        );
        assert_eq!(
            parse("read data.csv"),
            ExCommand::ReadFile { addr: None, path: "data.csv".into() }
        );
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
    fn nmap_parse() {
        assert_eq!(
            parse("nnoremap Y y$"),
            ExCommand::MapKey { lhs: 'Y', rhs: "y$".into() }
        );
        assert_eq!(parse("nnoremap"), ExCommand::MapList);
        assert_eq!(parse("nunmap Y"), ExCommand::Unmap('Y'));
    }

    #[test]
    fn abbrev_parse() {
        assert_eq!(
            parse("iabbrev teh the"),
            ExCommand::Abbrev { lhs: "teh".into(), rhs: "the".into() }
        );
        assert_eq!(
            parse("ab btw by the way"),
            ExCommand::Abbrev { lhs: "btw".into(), rhs: "by the way".into() }
        );
        assert_eq!(parse("iabbrev"), ExCommand::AbbrevList);
        assert_eq!(parse("unabbreviate teh"), ExCommand::Unabbrev("teh".into()));
    }

    #[test]
    fn delmarks_parse() {
        assert_eq!(parse("delmarks a b"), ExCommand::DelMarks("a b".into()));
        assert_eq!(parse("delm a-d"), ExCommand::DelMarks("a-d".into()));
        assert_eq!(parse("delmarks!"), ExCommand::DelMarks("!".into()));
    }

    #[test]
    fn history_command_parses_kinds() {
        use crate::command::HistoryKind;
        assert_eq!(parse("history"), ExCommand::History(HistoryKind::Cmd));
        assert_eq!(parse("his"), ExCommand::History(HistoryKind::Cmd));
        assert_eq!(parse("history :"), ExCommand::History(HistoryKind::Cmd));
        assert_eq!(parse("history /"), ExCommand::History(HistoryKind::Search));
        assert_eq!(parse("history search"), ExCommand::History(HistoryKind::Search));
        assert_eq!(parse("history all"), ExCommand::History(HistoryKind::All));
    }

    #[test]
    fn buffer_commands() {
        assert_eq!(parse("ls"), ExCommand::BufferList);
        assert_eq!(parse("buffers"), ExCommand::BufferList);
        assert_eq!(parse("bn"), ExCommand::BufferNext);
        assert_eq!(parse("bprev"), ExCommand::BufferPrev);
        // vim arglist aliases (case-sensitive: :n forward, :N backward).
        assert_eq!(parse("next"), ExCommand::BufferNext);
        assert_eq!(parse("n"), ExCommand::BufferNext);
        assert_eq!(parse("prev"), ExCommand::BufferPrev);
        assert_eq!(parse("N"), ExCommand::BufferPrev);
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
        use crate::command::SortNumeric::{Float, Hex, Int, None as NoNum};
        assert_eq!(parse("sort"), base(false, false, NoNum, false));
        assert_eq!(parse("sort!"), base(true, false, NoNum, false));
        assert_eq!(parse("sort u"), base(false, true, NoNum, false));
        assert_eq!(parse("sort! u"), base(true, true, NoNum, false));
        assert_eq!(parse("sort n"), base(false, false, Int, false));
        assert_eq!(parse("sort f"), base(false, false, Float, false));
        assert_eq!(parse("sort x"), base(false, false, Hex, false));
        assert_eq!(parse("sort i"), base(false, false, NoNum, true));
        assert_eq!(parse("sort! un"), base(true, true, Int, false));
        // A leading range is carried through, and `source` must not match.
        assert_eq!(
            parse("1,3sort"),
            ExCommand::Sort {
                range: SubRange::Range(LineAddr::Num(1), LineAddr::Num(3)),
                reverse: false,
                unique: false,
                numeric: NoNum,
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
                numeric: NoNum,
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
                numeric: NoNum,
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
    fn mksession_command() {
        assert_eq!(parse("mksession"), ExCommand::MkSession { file: None, force: false });
        assert_eq!(parse("mks s.vim"), ExCommand::MkSession { file: Some("s.vim".into()), force: false });
        assert_eq!(parse("mksession! s.vim"), ExCommand::MkSession { file: Some("s.vim".into()), force: true });
    }

    #[test]
    fn tohtml_command() {
        assert_eq!(parse("TOhtml"), ExCommand::ToHtml(None));
        assert_eq!(parse("tohtml out.html"), ExCommand::ToHtml(Some("out.html".into())));
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
    fn substitute_confirm_flag() {
        match parse("%s/foo/bar/gc") {
            ExCommand::SubstituteConfirm(spec) => {
                assert_eq!(spec.pattern, "foo");
                assert_eq!(spec.replacement, "bar");
                assert!(spec.global);
            }
            other => panic!("expected substitute-confirm, got {other:?}"),
        }
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
