    use super::*;

    #[test]
    fn run_ex_theme_switch() {
        let mut app = App::new();
        app.run_ex("theme cobalt");
        assert_eq!(app.themes.current().name, "cobalt");
    }

    #[test]
    fn run_ex_unknown_theme_message() {
        let mut app = App::new();
        app.run_ex("theme nope");
        assert!(app.editor.message.contains("Unknown theme"));
    }

    #[test]
    fn run_ex_toggle_numbers() {
        let mut app = App::new();
        app.run_ex("set nonumber");
        assert!(!app.editor.show_line_numbers);
        app.run_ex("set number");
        assert!(app.editor.show_line_numbers);
    }

    #[test]
    fn run_ex_set_filetype() {
        let mut app = App::new();
        app.run_ex("set ft=rust");
        assert_eq!(app.editor.language, Language::Rust);
    }

    #[test]
    fn run_ex_goto_line() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("a\nb\nc\nd");
        app.run_ex("3");
        assert_eq!(app.editor.cursor.row, 2);
    }

    #[test]
    fn run_ex_plugin_passthrough() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("hello world");
        app.run_ex("wordcount");
        assert!(app.editor.message.contains("words"));
    }

    #[test]
    fn run_ex_quit_blocked_when_dirty() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("x");
        app.editor.buffer.insert_char(crate::buffer::Position::new(0, 1), 'y');
        app.run_ex("q");
        assert!(!app.quit);
        app.run_ex("q!");
        assert!(app.quit);
    }

    #[test]
    fn alternate_buffer_switches_back_and_forth() {
        let dir = std::env::temp_dir();
        let pid = std::process::id();
        let a = dir.join(format!("rvim_alt_a_{pid}.txt"));
        let b = dir.join(format!("rvim_alt_b_{pid}.txt"));
        std::fs::write(&a, "AAA\n").unwrap();
        std::fs::write(&b, "BBB\n").unwrap();
        let mut app = App::open(a.to_str().unwrap()).unwrap();
        app.run_ex(&format!("e {}", b.to_str().unwrap())); // -> B, alternate = A
        assert_eq!(app.editor.buffer.line(0), Some("BBB"));
        app.run_ex("b#"); // -> A
        assert_eq!(app.editor.buffer.line(0), Some("AAA"));
        app.run_ex("b#"); // -> B again
        assert_eq!(app.editor.buffer.line(0), Some("BBB"));
        std::fs::remove_file(&a).ok();
        std::fs::remove_file(&b).ok();
    }

    #[test]
    fn open_additional_loads_extra_buffers() {
        let dir = std::env::temp_dir();
        let pid = std::process::id();
        let a = dir.join(format!("rvim_multi_a_{pid}.txt"));
        let b = dir.join(format!("rvim_multi_b_{pid}.txt"));
        std::fs::write(&a, "AAA\n").unwrap();
        std::fs::write(&b, "BBB\n").unwrap();
        let mut app = App::open(a.to_str().unwrap()).unwrap();
        app.open_additional(b.to_str().unwrap()).unwrap();
        assert_eq!(app.editor.buffer.line(0), Some("AAA")); // first stays active
        app.run_ex("bn"); // cycle to the second
        assert_eq!(app.editor.buffer.line(0), Some("BBB"));
        std::fs::remove_file(&a).ok();
        std::fs::remove_file(&b).ok();
    }

    #[test]
    fn alternate_buffer_without_alternate_reports() {
        let mut app = App::new();
        app.run_ex("b#");
        assert!(app.editor.message.contains("No alternate"));
    }

    #[test]
    fn normal_command_appends_to_each_line() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("a\nb\nc");
        app.run_ex("%normal A;");
        assert_eq!(app.editor.buffer.line(0), Some("a;"));
        assert_eq!(app.editor.buffer.line(1), Some("b;"));
        assert_eq!(app.editor.buffer.line(2), Some("c;"));
    }

    #[test]
    fn normal_command_runs_once_without_range() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("hello");
        app.run_ex("normal x"); // delete the first character
        assert_eq!(app.editor.buffer.line(0), Some("ello"));
    }

    #[test]
    fn colorcolumn_sets_value() {
        let mut app = App::new();
        app.run_ex("set colorcolumn=80");
        assert_eq!(app.editor.colorcolumns, vec![80]);
        app.run_ex("set cc=80,120");
        assert_eq!(app.editor.colorcolumns, vec![80, 120]);
        app.run_ex("set cc=0");
        assert_eq!(app.editor.colorcolumns, Vec::<usize>::new());
    }

    #[test]
    fn global_normal_appends_to_matching_lines() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("foo\nbar\nfoo baz");
        app.run_ex("g/foo/normal A!");
        assert_eq!(app.editor.buffer.line(0), Some("foo!"));
        assert_eq!(app.editor.buffer.line(1), Some("bar"));
        assert_eq!(app.editor.buffer.line(2), Some("foo baz!"));
    }

    #[test]
    fn global_normal_dd_deletes_matching_lines() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("x1\nkeep\nx2\nx3\ndone");
        app.run_ex("g/x/normal dd");
        assert_eq!(app.editor.buffer.line(0), Some("keep"));
        assert_eq!(app.editor.buffer.line(1), Some("done"));
        assert_eq!(app.editor.buffer.line_count(), 2);
    }

    #[test]
    fn earlier_later_undo_redo() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("abcdef");
        app.feed_normal_keys("xx"); // delete two chars -> "cdef"
        assert_eq!(app.editor.buffer.line(0), Some("cdef"));
        app.run_ex("earlier 2"); // undo both
        assert_eq!(app.editor.buffer.line(0), Some("abcdef"));
        app.run_ex("later 1"); // redo one
        assert_eq!(app.editor.buffer.line(0), Some("bcdef"));
    }

    #[test]
    fn align_center_right_left() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("hi");
        app.run_ex("center 10");
        assert_eq!(app.editor.buffer.line(0), Some("    hi")); // (10-2)/2 = 4
        app.editor.buffer = crate::buffer::Buffer::from_text("hi");
        app.run_ex("right 10");
        assert_eq!(app.editor.buffer.line(0), Some("        hi")); // 10-2 = 8
        app.editor.buffer = crate::buffer::Buffer::from_text("    hi");
        app.run_ex("left 2");
        assert_eq!(app.editor.buffer.line(0), Some("  hi"));
        app.editor.buffer = crate::buffer::Buffer::from_text("    hi");
        app.run_ex("left");
        assert_eq!(app.editor.buffer.line(0), Some("hi"));
    }

    #[test]
    fn align_center_over_range() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("a\nbb\nccc");
        app.run_ex("1,3center 5");
        assert_eq!(app.editor.buffer.line(0), Some("  a"));
        assert_eq!(app.editor.buffer.line(1), Some(" bb"));
        assert_eq!(app.editor.buffer.line(2), Some(" ccc"));
    }

    #[test]
    fn set_query_reports_values() {
        let mut app = App::new();
        app.run_ex("set shiftwidth=3");
        app.run_ex("set sw?");
        assert_eq!(app.editor.message, "shiftwidth=3");
        app.run_ex("set nonumber");
        app.run_ex("set nu?");
        assert_eq!(app.editor.message, "nonumber");
        app.run_ex("set wibble?");
        assert!(app.editor.message.contains("Unknown option"));
    }

    #[test]
    fn cursorcolumn_toggles() {
        let mut app = App::new();
        assert!(!app.editor.cursorcolumn);
        app.run_ex("set cursorcolumn");
        assert!(app.editor.cursorcolumn);
        app.run_ex("set nocuc");
        assert!(!app.editor.cursorcolumn);
    }

    #[test]
    fn cursorline_toggles() {
        let mut app = App::new();
        assert!(app.editor.cursorline); // on by default
        app.run_ex("set nocursorline");
        assert!(!app.editor.cursorline);
        app.run_ex("set cursorline");
        assert!(app.editor.cursorline);
    }

    #[test]
    fn set_multiple_options_in_one_command() {
        let mut app = App::new();
        app.editor.show_line_numbers = false;
        app.run_ex("set number shiftwidth=2 tabstop=8 nonumber");
        assert_eq!(app.editor.shiftwidth, 2);
        assert_eq!(app.editor.tabstop, 8);
        assert!(!app.editor.show_line_numbers); // last of number/nonumber wins
    }

    #[test]
    fn marks_command_opens_listing_and_preserves_buffer() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("x\ny");
        app.run_ex("marks");
        assert!(app.editor.buffer.line(0).unwrap().contains("marks"));
        assert_eq!(app.others.len(), 1); // original buffer preserved
    }

    #[test]
    fn delmarks_command_clears_mark() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("a\nb\nc");
        app.feed_normal_keys("maj"); // set mark a, move down
        app.run_ex("delmarks a");
        app.feed_normal_keys("`a");
        assert!(app.editor.message.contains("Mark not set"));
    }

    #[test]
    fn changes_command_opens_listing_and_preserves_buffer() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("x\ny");
        app.run_ex("changes");
        assert!(app.editor.buffer.line(0).unwrap().contains("changes"));
        assert_eq!(app.others.len(), 1); // original buffer preserved
    }

    #[test]
    fn history_command_opens_listing_and_preserves_buffer() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("x\ny");
        app.run_ex("history");
        assert!(app.editor.buffer.line(0).unwrap().contains("history"));
        assert_eq!(app.others.len(), 1); // original buffer preserved
    }

    #[test]
    fn nmap_end_to_end() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("one\ntwo");
        app.run_ex("nnoremap x dd"); // shadow x -> delete line
        app.feed_normal_keys("x");
        assert_eq!(app.editor.buffer.line(0), Some("two"));
    }

    #[test]
    fn abbrev_end_to_end() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("");
        app.run_ex("iabbrev teh the");
        app.feed_normal_keys("iteh "); // insert, type the lhs + trigger space
        assert_eq!(app.editor.buffer.line(0), Some("the "));
    }

    #[test]
    fn substitute_confirm_end_to_end() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("foo foo");
        app.run_ex("s/foo/X/gc"); // current line, global, confirm
        assert!(app.editor.substitute_confirm_active());
        app.feed_normal_keys("yy"); // confirm both matches
        assert!(!app.editor.substitute_confirm_active());
        assert_eq!(app.editor.buffer.line(0), Some("X X"));
    }

    #[test]
    fn substitute_empty_pattern_reuses_previous() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("foo bar foo");
        app.run_ex("s/foo/X/"); // first foo -> X, last pattern = foo
        app.run_ex("s//Y/"); // empty pattern reuses "foo" -> next foo
        assert_eq!(app.editor.buffer.line(0), Some("X bar Y"));
    }

    #[test]
    fn write_range_writes_only_those_lines() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("l1\nl2\nl3\nl4");
        let path = std::env::temp_dir().join(format!("rvim_wr_{}.txt", std::process::id()));
        app.run_ex(&format!("2,3w {}", path.display()));
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "l2\nl3\n");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn filter_command_sorts_lines() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("b\na\nc");
        app.run_ex("%!sort");
        assert_eq!(app.editor.buffer.line(0), Some("a"));
        assert_eq!(app.editor.buffer.line(1), Some("b"));
        assert_eq!(app.editor.buffer.line(2), Some("c"));
    }

    #[test]
    fn retab_command_expands_tabs() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("\thi");
        app.editor.expandtab = true;
        app.editor.tabstop = 4;
        app.run_ex("retab");
        assert_eq!(app.editor.buffer.line(0), Some("    hi"));
    }

    #[test]
    fn set_all_opens_options_listing() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("x\ny");
        app.run_ex("set all");
        assert!(app.editor.buffer.line(0).unwrap().contains("options"));
        assert_eq!(app.others.len(), 1); // original buffer preserved
    }

    #[test]
    fn help_preserves_current_buffer() {
        let path = std::env::temp_dir().join(format!("rvim_help_{}.txt", std::process::id()));
        std::fs::write(&path, "my work\n").unwrap();
        let mut app = App::open(path.to_str().unwrap()).unwrap();
        app.run_ex("help");
        assert!(app.editor.buffer.line(0).unwrap().contains("quick help"));
        assert_eq!(app.others.len(), 1); // original buffer preserved, not destroyed
        app.run_ex("bd"); // close help -> back to the file
        assert_eq!(app.editor.buffer.line(0), Some("my work"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn help_lists_languages_dynamically() {
        let mut app = App::new();
        app.run_ex("help");
        let text: String = (0..app.editor.buffer.line_count())
            .filter_map(|r| app.editor.buffer.line(r))
            .collect::<Vec<_>>()
            .join("\n");
        // The :set ft= line is built from the language table, so recent additions
        // appear without editing the help text.
        assert!(text.contains("yaml"));
        assert!(text.contains("java"));
        assert!(text.contains("clipboard")); // the "+ / "* help line
        // Recently added features are documented.
        assert!(text.contains("readonly"));
        assert!(text.contains("language=<code>"));
        assert!(text.contains(":cd"));
        assert!(text.contains("recursive")); // zO/zC/zA line
    }

    #[test]
    fn help_does_not_stack() {
        let mut app = App::new();
        app.run_ex("help");
        let n = app.others.len();
        app.run_ex("help"); // already in help -> no extra copy
        assert_eq!(app.others.len(), n);
    }

    #[test]
    fn reload_discards_changes_with_force() {
        let path = std::env::temp_dir()
            .join(format!("rvim_reload_{}.txt", std::process::id()));
        std::fs::write(&path, "original\n").unwrap();
        let mut app = App::open(path.to_str().unwrap()).unwrap();
        app.editor
            .buffer
            .insert_char(crate::buffer::Position::new(0, 0), 'X');
        assert!(app.editor.buffer.is_dirty());
        app.run_ex("e"); // blocked: unsaved changes
        assert!(app.editor.buffer.is_dirty());
        assert!(app.editor.message.contains("No write"));
        app.run_ex("e!"); // forced reload
        assert_eq!(app.editor.buffer.line(0), Some("original"));
        assert!(!app.editor.buffer.is_dirty());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn block_state_top_memoizes_and_recomputes() {
        let mut app = App::new();
        app.editor.buffer =
            crate::buffer::Buffer::from_text("/* comment\nstill in\n*/\ncode");
        app.editor.set_language(crate::syntax::Language::Rust);
        app.editor.top = 1;
        use crate::syntax::LineState;
        assert_eq!(app.block_state_top(), LineState::Block); // line 1 inside the comment
        assert!(app.block_memo.is_some()); // result cached
        assert_eq!(app.block_state_top(), LineState::Block); // cached path returns the same
        app.editor.top = 3;
        assert_eq!(app.block_state_top(), LineState::Normal); // line 3 ("code") is outside
    }

    #[test]
    fn run_ex_quit_all_respects_dirty() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("x");
        app.editor.buffer.insert_char(crate::buffer::Position::new(0, 1), 'y');
        app.run_ex("qa");
        assert!(!app.quit); // blocked: unsaved changes
        app.run_ex("qa!");
        assert!(app.quit); // forced
    }

    #[test]
    fn run_ex_write_all_reports_unnamed_buffer() {
        let mut app = App::new();
        // A fresh scratch buffer has no file name, so it can't be written.
        app.editor.buffer = crate::buffer::Buffer::from_text("scratch");
        app.run_ex("wa");
        assert!(app.editor.message.contains("failed"), "{}", app.editor.message);
        // write-quit-all without force must not quit when a buffer can't be saved.
        app.run_ex("wqa");
        assert!(!app.quit);
        app.run_ex("wqa!");
        assert!(app.quit);
    }

    #[test]
    fn run_ex_substitute_whole_file() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("cat\ncat\ndog");
        app.run_ex("%s/cat/COW/g");
        assert_eq!(app.editor.buffer.line(0), Some("COW"));
        assert_eq!(app.editor.buffer.line(1), Some("COW"));
        assert_eq!(app.editor.buffer.line(2), Some("dog"));
        assert!(app.editor.message.contains("2 substitutions"));
    }

    #[test]
    fn run_ex_substitute_not_found_message() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("hello");
        app.run_ex("s/zzz/x/");
        assert!(app.editor.message.contains("Pattern not found"));
    }

    #[test]
    fn run_ex_mouse_toggle() {
        let mut app = App::new();
        app.run_ex("set mouse");
        assert!(app.want_mouse);
        app.run_ex("set nomouse");
        assert!(!app.want_mouse);
    }

    #[test]
    fn run_ex_relativenumber_forces_gutter_on() {
        let mut app = App::new();
        app.run_ex("set nonumber");
        assert!(!app.editor.show_line_numbers);
        app.run_ex("set relativenumber");
        assert!(app.editor.relative_numbers);
        assert!(app.editor.show_line_numbers); // forced back on
        app.run_ex("set nornu");
        assert!(!app.editor.relative_numbers);
    }

    #[test]
    fn multiple_buffers_open_and_navigate() {
        let mut app = App::new();
        app.run_ex("e foo.rs"); // active foo.rs (initial scratch discarded)
        app.run_ex("e bar.tsql"); // active bar.tsql, foo in others
        assert!(app.editor.buffer.path().unwrap().ends_with("bar.tsql"));
        assert_eq!(app.others.len(), 1);
        // Language autodetected on switch.
        assert_eq!(app.editor.language, Language::TSql);

        app.run_ex("bn"); // rotate -> foo.rs
        assert!(app.editor.buffer.path().unwrap().ends_with("foo.rs"));
        app.run_ex("bp"); // back -> bar.tsql
        assert!(app.editor.buffer.path().unwrap().ends_with("bar.tsql"));
    }

    #[test]
    fn buffer_switch_when_already_open() {
        let mut app = App::new();
        app.run_ex("e a.rs");
        app.run_ex("e b.rs");
        // Re-opening a.rs should switch, not create a duplicate.
        app.run_ex("e a.rs");
        assert!(app.editor.buffer.path().unwrap().ends_with("a.rs"));
        assert_eq!(app.others.len(), 1);
    }

    #[test]
    fn buffer_list_and_delete() {
        let mut app = App::new();
        app.run_ex("e one.rs");
        app.run_ex("e two.rs");
        app.run_ex("ls");
        assert!(app.editor.message.contains("one.rs"));
        assert!(app.editor.message.contains("two.rs"));
        // Delete current (two.rs); one.rs becomes active.
        app.run_ex("bd");
        assert!(app.editor.buffer.path().unwrap().ends_with("one.rs"));
        assert!(app.others.is_empty());
        // Can't delete the last buffer.
        app.run_ex("bd");
        assert!(app.editor.message.contains("cannot close last buffer"));
    }

    #[test]
    fn run_ex_high_contrast_theme() {
        let mut app = App::new();
        app.run_ex("theme high-contrast");
        assert_eq!(app.themes.current().name, "high-contrast");
    }

    #[test]
    fn apply_config_lines_applies_settings() {
        let mut app = App::new();
        let lines = vec![
            "theme cobalt".to_string(),
            "set nonumber".to_string(),
            "set mouse".to_string(),
        ];
        app.apply_config_lines(&lines);
        assert_eq!(app.themes.current().name, "cobalt");
        assert!(!app.editor.show_line_numbers);
        assert!(app.want_mouse);
    }

    #[test]
    fn source_command_runs_file() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("rvim_rc_{}.rc", std::process::id()));
        std::fs::write(&path, "\" comment\ntheme retrowave\nset nonumber\n").unwrap();
        let mut app = App::new();
        app.run_ex(&format!("source {}", path.display()));
        assert_eq!(app.themes.current().name, "retrowave");
        assert!(!app.editor.show_line_numbers);
        assert!(app.editor.message.contains("sourced"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn source_missing_file_reports_error() {
        let mut app = App::new();
        app.run_ex("source /no/such/rvimrc-xyz");
        assert!(app.editor.message.contains("Can't open"));
    }

    // ---- split windows ---------------------------------------------------

    use crate::window::{Move, Rect};

    fn kev(c: char) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(c),
            crossterm::event::KeyModifiers::NONE,
        )
    }

    #[test]
    fn vsplit_and_split_add_windows() {
        let mut app = App::new();
        assert_eq!(app.window_count(), 1);
        app.run_ex("vsplit");
        assert_eq!(app.window_count(), 2);
        assert_eq!(app.panes.len(), 1);
        app.run_ex("split");
        assert_eq!(app.window_count(), 3);
    }

    #[test]
    fn split_duplicates_the_current_view() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("alpha\nbeta\ngamma");
        app.editor.cursor.row = 2;
        app.run_ex("split");
        // New focused window shows the same text and cursor row.
        assert_eq!(app.editor.buffer.line(0), Some("alpha"));
        assert_eq!(app.editor.cursor.row, 2);
        // The old window is preserved as a pane with the same content.
        assert_eq!(app.panes[0].editor.buffer.line(2), Some("gamma"));
    }

    #[test]
    fn split_with_file_opens_it_in_the_new_window() {
        let mut app = App::new();
        let dir = std::env::temp_dir();
        let path = dir.join("rvim_split_test.txt");
        std::fs::write(&path, "one\ntwo\n").unwrap();
        app.run_ex(&format!("vsplit {}", path.display()));
        assert_eq!(app.window_count(), 2);
        assert_eq!(app.editor.buffer.line(0), Some("one"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn plain_split_shares_buffer_id_file_split_does_not() {
        let mut app = App::new();
        let base = app.editor.buf_id;
        // A plain `:split` is another view of the same buffer: same id.
        app.run_ex("split");
        assert_eq!(app.editor.buf_id, base);
        assert_eq!(app.panes[0].editor.buf_id, base);
        // Splitting in a different file is a distinct buffer: fresh id.
        let dir = std::env::temp_dir();
        let path = dir.join("rvim_bufid_test.txt");
        std::fs::write(&path, "one\ntwo\n").unwrap();
        app.run_ex(&format!("split {}", path.display()));
        assert_ne!(app.editor.buf_id, base, "a different file gets a new buffer id");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn split_mirrors_edits_live_across_windows() {
        use crate::buffer::{Buffer, Position};
        let mut app = App::new();
        app.editor.buffer = Buffer::from_text("alpha\nbeta");
        app.run_ex("split"); // two windows, one shared buffer
        let other = app.panes[0].id;
        assert_eq!(app.buffer_for(other).line(0), Some("alpha"));
        // Edit in the focused window; the other window sees it immediately.
        app.editor.buffer.insert_str(Position::new(0, 0), "X");
        assert_eq!(app.buffer_for(other).line(0), Some("Xalpha"));
    }

    #[test]
    fn shared_edits_follow_focus_switch() {
        use crate::buffer::{Buffer, Position};
        let mut app = App::new();
        app.editor.buffer = Buffer::from_text("alpha");
        app.run_ex("split");
        let other = app.panes[0].id;
        app.editor.buffer.insert_str(Position::new(0, 0), "X"); // focused edit
        app.activate_window(other); // focus the other view of the same buffer
        // The authoritative (edited) content follows focus, so edits aren't lost.
        assert_eq!(app.editor.buffer.line(0), Some("Xalpha"));
    }

    #[test]
    fn set_language_switches_ui_locale() {
        use crate::i18n::Locale;
        use crate::mode::Mode;
        let mut app = App::new();
        assert_eq!(app.editor.locale, Locale::English);
        app.run_ex("set language=vi");
        assert_eq!(app.editor.locale, Locale::Vietnamese);
        // Mode indicators now localize.
        app.editor.mode = Mode::Insert;
        assert_eq!(app.editor.mode_indicator(), Some("-- CHÈN --"));
        // The scroll indicator and common messages localize too.
        app.editor.set_viewport(24, 80); // whole buffer fits on screen
        assert_eq!(app.editor.scroll_indicator(), "Tất cả"); // "All"
        app.run_ex("close"); // only one window -> localized E444
        assert!(app.editor.message.contains("cửa sổ cuối"));
        // An unknown code is reported, locale unchanged.
        app.run_ex("set language=xx");
        assert!(app.editor.message.contains("E474"));
        assert_eq!(app.editor.locale, Locale::Vietnamese);
    }

    #[test]
    fn templated_messages_localize_and_fill_placeholders() {
        let dir = std::env::temp_dir();
        let path = dir.join("rvim_i18n_msg.txt");
        std::fs::write(&path, "x\n").unwrap();
        let mut app = App::open(path.to_str().unwrap()).unwrap();
        app.run_ex("set language=vi");
        app.run_ex("w");
        // Vietnamese write message ("đã ghi \"{name}\" {bytes}B") with placeholders filled.
        assert!(app.editor.message.contains("đã ghi"), "localized: {}", app.editor.message);
        assert!(app.editor.message.contains('B'), "byte count kept");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn enew_opens_empty_buffer_keeping_the_old() {
        use crate::buffer::Buffer;
        let mut app = App::new();
        app.editor.buffer = Buffer::from_text("important");
        app.editor.buffer.set_path("keep.txt");
        app.run_ex("enew");
        // The new buffer is empty and unnamed.
        assert!(app.editor.buffer.path().is_none());
        assert_eq!(app.editor.buffer.line(0), Some(""));
        // The old named buffer is preserved in the buffer list.
        assert!(app.others.iter().any(|e| e
            .buffer
            .path()
            .map(|p| p.display().to_string())
            .as_deref()
            == Some("keep.txt")));
    }

    #[test]
    fn set_readonly_all_marks_every_buffer() {
        let dir = std::env::temp_dir();
        let extra = dir.join("rvim_ro_all.txt");
        std::fs::write(&extra, "x\n").unwrap();
        let mut app = App::new();
        app.open_additional(extra.to_str().unwrap()).unwrap();
        app.set_readonly_all();
        assert!(app.editor.readonly);
        assert!(app.others.iter().all(|e| e.readonly));
        let _ = std::fs::remove_file(&extra);
    }

    #[test]
    fn pwd_reports_cwd_and_bad_cd_errors_without_changing_it() {
        // Deliberately avoids a successful `:cd`, which would change the
        // process-wide working directory and race other tests.
        let mut app = App::new();
        app.run_ex("pwd");
        let cwd = std::env::current_dir().unwrap().display().to_string();
        assert_eq!(app.editor.message, cwd);
        app.run_ex("cd /no/such/dir/rvim-xyz");
        assert!(app.editor.message.contains("E344"));
        assert_eq!(std::env::current_dir().unwrap().display().to_string(), cwd);
    }

    #[test]
    fn readonly_blocks_write_until_forced() {
        let dir = std::env::temp_dir();
        let path = dir.join("rvim_ro_test.txt");
        std::fs::write(&path, "x\n").unwrap();
        let mut app = App::open(path.to_str().unwrap()).unwrap();
        app.run_ex("set readonly");
        assert!(app.editor.readonly);
        app.run_ex("w");
        assert!(app.editor.message.contains("E45"), "readonly :w is refused");
        app.run_ex("w!");
        assert!(!app.editor.message.contains("E45"), ":w! overrides readonly");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn close_window_returns_to_single_and_refuses_last() {
        let mut app = App::new();
        app.run_ex("split");
        assert_eq!(app.window_count(), 2);
        app.run_ex("close");
        assert_eq!(app.window_count(), 1);
        assert!(app.panes.is_empty());
        // Closing the last window is refused.
        app.run_ex("close");
        assert_eq!(app.window_count(), 1);
        assert!(app.editor.message.contains("Cannot close last window"));
    }

    #[test]
    fn only_window_drops_the_rest() {
        let mut app = App::new();
        app.run_ex("split");
        app.run_ex("vsplit");
        assert_eq!(app.window_count(), 3);
        app.run_ex("only");
        assert_eq!(app.window_count(), 1);
        assert!(app.panes.is_empty());
    }

    #[test]
    fn focus_cycles_through_windows() {
        let mut app = App::new();
        app.run_ex("split"); // focus = new window (id 1), pane = id 0
        let first = app.focused;
        app.focus_cycle(true);
        assert_ne!(app.focused, first);
        app.focus_cycle(true);
        assert_eq!(app.focused, first); // wrapped back
    }

    #[test]
    fn directional_focus_moves_between_panes() {
        let mut app = App::new();
        app.run_ex("vsplit"); // [new | old], focus on the left (new)
        let left = app.focused;
        let area = Rect::new(0, 0, 80, 24);
        app.focus_dir(Move::Right, area);
        let right = app.focused;
        assert_ne!(right, left);
        app.focus_dir(Move::Left, area);
        assert_eq!(app.focused, left);
    }

    #[test]
    fn edits_in_one_window_do_not_touch_the_other_view() {
        // Independent-view model: splitting the same buffer gives separate copies.
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("x");
        app.run_ex("split");
        // Type into the focused window.
        app.editor.handle_key(kev('i'));
        app.editor.handle_key(kev('Z'));
        app.editor
            .handle_key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Esc,
                crossterm::event::KeyModifiers::NONE,
            ));
        assert!(app.editor.buffer.line(0).unwrap().contains('Z'));
        // The other window's view is unchanged.
        assert_eq!(app.panes[0].editor.buffer.line(0), Some("x"));
    }

    #[test]
    fn q_closes_window_when_split_then_quits_on_last() {
        let mut app = App::new();
        app.run_ex("split");
        assert_eq!(app.window_count(), 2);
        app.run_ex("q");
        assert_eq!(app.window_count(), 1);
        assert!(!app.quit);
        app.run_ex("q");
        assert!(app.quit);
    }

    #[test]
    fn ctrl_w_key_dispatches_window_commands() {
        let mut app = App::new();
        let area = Rect::new(0, 0, 80, 24);
        app.handle_window_key(kev('v'), area); // Ctrl-w v
        assert_eq!(app.window_count(), 2);
        app.handle_window_key(kev('o'), area); // Ctrl-w o
        assert_eq!(app.window_count(), 1);
    }

    fn mouse(kind: MouseEventKind, col: u16, row: u16) -> event::MouseEvent {
        event::MouseEvent { kind, column: col, row, modifiers: KeyModifiers::NONE }
    }

    #[test]
    fn mouse_click_focuses_and_positions_in_the_hovered_pane() {
        let mut app = App::new();
        app.editor.show_line_numbers = false;
        app.editor.buffer =
            crate::buffer::Buffer::from_text("aaaaaaaa\nbbbbbbbb\ncccccccc\ndddddddd");
        app.run_ex("vsplit"); // left = new (focused), right = id 0
        assert_ne!(app.focused, WinId(0));
        let layout = Layout::compute(80, 24, 4, false, false);
        // Click in the right pane (x >= 41), on row 2.
        app.handle_mouse_split(mouse(MouseEventKind::Down(MouseButton::Left), 50, 2), &layout);
        assert_eq!(app.focused, WinId(0), "click should focus the hovered pane");
        assert_eq!(app.editor.cursor.row, 2);
        assert!(app.editor.cursor.col <= 7); // clamped to the line
    }

    #[test]
    fn mouse_scroll_scrolls_the_hovered_pane_without_focusing() {
        let mut app = App::new();
        app.editor.show_line_numbers = false;
        let text: String = (0..50).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
        app.editor.buffer = crate::buffer::Buffer::from_text(&text);
        app.run_ex("vsplit");
        let focused_before = app.focused;
        let layout = Layout::compute(80, 24, 50, false, false);
        // Scroll over the right (non-focused) pane.
        app.handle_mouse_split(mouse(MouseEventKind::ScrollDown, 60, 3), &layout);
        // Focus is unchanged, and the right pane (id 0) scrolled down.
        assert_eq!(app.focused, focused_before);
        let right = app.panes.iter().find(|p| p.id == WinId(0)).unwrap();
        assert_eq!(right.editor.top, 3);
    }

    #[test]
    fn with_foldcolumn_widens_gutter_and_shrinks_text() {
        let l = Layout::compute(80, 24, 10, true, false); // gutter 4, text 76
        let l2 = l.with_foldcolumn(2);
        assert_eq!(l2.fold_width, 2);
        assert_eq!(l2.gutter_width, 6);
        assert_eq!(l2.text_cols, 74);
    }

    #[test]
    fn set_foldcolumn_option_clamps_and_reports() {
        let mut app = App::new();
        app.run_ex("set foldcolumn=3");
        assert_eq!(app.editor.foldcolumn, 3);
        app.run_ex("set fdc=99"); // clamped to 12
        assert_eq!(app.editor.foldcolumn, 12);
    }

    #[test]
    fn click_count_detects_double_and_triple() {
        let mut app = App::new();
        let m = mouse(MouseEventKind::Down(MouseButton::Left), 5, 2);
        assert_eq!(app.click_count(&m), 1);
        assert_eq!(app.click_count(&m), 2);
        assert_eq!(app.click_count(&m), 3);
        assert_eq!(app.click_count(&m), 1); // resets after a triple
        let elsewhere = mouse(MouseEventKind::Down(MouseButton::Left), 9, 2);
        assert_eq!(app.click_count(&elsewhere), 1); // different cell resets
    }

    // ---- window rearrangement (Ctrl-w x / r) -----------------------------

    /// Three vertical windows whose buffers are "A","B","C"; returns the app.
    /// Layout order is [id2="C" (focused), id1="B", id0="A"].
    fn three_windows() -> App {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("A");
        app.run_ex("vsplit");
        app.editor.buffer = crate::buffer::Buffer::from_text("B");
        app.run_ex("vsplit");
        app.editor.buffer = crate::buffer::Buffer::from_text("C");
        app
    }

    fn buf_at(app: &App, id: WinId) -> String {
        let ed = if id == app.focused {
            &app.editor
        } else {
            &app.panes.iter().find(|p| p.id == id).unwrap().editor
        };
        ed.buffer.line(0).unwrap().to_string()
    }

    #[test]
    fn window_exchange_swaps_with_next() {
        let mut app = three_windows();
        assert_eq!(buf_at(&app, WinId(2)), "C");
        assert_eq!(buf_at(&app, WinId(1)), "B");
        app.window_exchange(); // focused id2 swaps with next (id1)
        assert_eq!(app.focused, WinId(2));
        assert_eq!(buf_at(&app, WinId(2)), "B");
        assert_eq!(buf_at(&app, WinId(1)), "C");
        assert_eq!(buf_at(&app, WinId(0)), "A"); // untouched
    }

    #[test]
    fn window_rotate_shifts_all_contents() {
        let mut app = three_windows();
        app.window_rotate(); // [C,B,A] -> rotate_right -> [A,C,B] over ids [2,1,0]
        assert_eq!(buf_at(&app, WinId(2)), "A");
        assert_eq!(buf_at(&app, WinId(1)), "C");
        assert_eq!(buf_at(&app, WinId(0)), "B");
        assert_eq!(app.window_count(), 3);
    }

    #[test]
    fn window_rearrange_noop_with_one_window() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("solo");
        app.window_exchange();
        app.window_rotate();
        assert_eq!(app.window_count(), 1);
        assert_eq!(app.editor.buffer.line(0), Some("solo"));
    }

    // ---- per-window syntax-state memo ------------------------------------

    #[test]
    fn refresh_window_syntax_memoizes_and_prunes() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("a\nb\nc");
        app.run_ex("split");
        let area = Rect::new(0, 0, 80, 24);
        assert!(app.win_block_memo.is_empty());
        app.refresh_window_syntax(area);
        assert_eq!(app.win_block_memo.len(), 2);
        app.run_ex("only"); // back to a single window
        app.refresh_window_syntax(area);
        assert_eq!(app.win_block_memo.len(), 1); // stale entry pruned
    }

    #[test]
    fn window_views_top_state_matches_direct_computation() {
        let mut app = App::new();
        app.editor.language = Language::Rust;
        app.editor.buffer = crate::buffer::Buffer::from_text("/* open\nx\ny\nz");
        app.run_ex("vsplit");
        let area = Rect::new(0, 0, 80, 24);
        app.refresh_window_syntax(area);
        let views = app.window_views(area);
        assert!(!views.is_empty());
        for v in &views {
            let direct = app.syntax.block_state_at(
                v.editor.language,
                v.editor.buffer.lines(),
                v.editor.top,
            );
            assert_eq!(v.top_state, direct);
        }
    }

    // ---- syntax folding (foldmethod=syntax) ------------------------------

    #[test]
    fn set_foldmethod_syntax_folds_braces() {
        let mut app = App::new();
        app.editor.language = Language::Rust;
        app.editor.buffer = crate::buffer::Buffer::from_text("fn f() {\n  x;\n  y;\n}");
        app.run_ex("set foldmethod=syntax");
        assert_eq!(app.editor.foldmethod, crate::editor::FoldMethod::Syntax);
        assert!(app.editor.folds().all().iter().any(|f| f.start == 0 && f.end == 3));
    }

    #[test]
    fn syntax_folds_recompute_after_edit() {
        let mut app = App::new();
        app.editor.language = Language::Rust;
        app.editor.buffer = crate::buffer::Buffer::from_text("fn f() {\n  x;\n  y;\n}");
        app.run_ex("set foldmethod=syntax");
        assert!(app.editor.folds().all().iter().any(|f| f.start == 0 && f.end == 3));
        // Open a new line inside the block, growing it by one line.
        app.editor.cursor.row = 2;
        app.editor.handle_key(kev('o'));
        app.editor.handle_key(kev('z'));
        app.editor.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.editor.recompute_syntax_folds(&app.syntax); // app does this each frame
        assert!(app.editor.folds().all().iter().any(|f| f.start == 0 && f.end == 4));
    }

    // ---- splitbelow / splitright -----------------------------------------

    #[test]
    fn default_split_opens_above_and_left() {
        let mut app = App::new();
        app.run_ex("split");
        assert_eq!(app.layout.ids(), vec![WinId(1), WinId(0)]); // new above
    }

    #[test]
    fn splitbelow_opens_new_window_below() {
        let mut app = App::new();
        app.run_ex("set splitbelow");
        assert!(app.editor.splitbelow);
        app.run_ex("split");
        assert_eq!(app.layout.ids(), vec![WinId(0), WinId(1)]); // new below
        assert_eq!(app.focused, WinId(1));
    }

    #[test]
    fn splitright_opens_new_window_right() {
        let mut app = App::new();
        app.run_ex("set splitright");
        app.run_ex("vsplit");
        assert_eq!(app.layout.ids(), vec![WinId(0), WinId(1)]); // new to the right
    }

    // ---- window resizing (Ctrl-w +/-/</>/=) ------------------------------

    #[test]
    fn ctrl_w_plus_grows_focused_window_height() {
        let mut app = App::new();
        app.run_ex("split"); // [1, 0] stacked; focus = new window (1)
        let area = Rect::new(0, 0, 80, 24);
        app.handle_window_key(kev('+'), area);
        let r = app.layout.rects(area);
        let focused_h = r.iter().find(|(id, _)| *id == app.focused).unwrap().1.h;
        assert!(focused_h > 12, "focused window should have grown past even");
    }

    #[test]
    fn ctrl_w_equals_restores_even_windows() {
        let mut app = App::new();
        app.run_ex("split");
        let area = Rect::new(0, 0, 80, 24);
        app.handle_window_key(kev('+'), area);
        app.handle_window_key(kev('='), area);
        let r = app.layout.rects(area);
        assert_eq!(r.iter().map(|(_, rc)| rc.h).collect::<Vec<_>>(), vec![12, 12]);
    }

    #[test]
    fn set_fileformat_option_switches_line_ending() {
        let mut app = App::new();
        app.run_ex("set fileformat=dos");
        assert_eq!(app.editor.option_value("fileformat"), "fileformat=dos");
        app.run_ex("set ff=unix");
        assert_eq!(app.editor.option_value("fileformat"), "fileformat=unix");
    }

    // ---- user-defined commands (:command) --------------------------------

    #[test]
    fn user_command_aliases_a_builtin() {
        let mut app = App::new();
        app.run_ex("command W set number");
        app.editor.show_line_numbers = false;
        app.run_ex("W");
        assert!(app.editor.show_line_numbers);
    }

    #[test]
    fn user_command_appends_arguments() {
        let mut app = App::new();
        app.run_ex("command Theme theme");
        app.run_ex("Theme cobalt");
        assert_eq!(app.themes.current().name, "cobalt");
    }

    #[test]
    fn user_command_requires_uppercase_name() {
        let mut app = App::new();
        app.run_ex("command foo w");
        assert!(app.editor.message.contains("uppercase"));
        assert!(app.user_commands.is_empty());
    }

    #[test]
    fn delcommand_removes_a_user_command() {
        let mut app = App::new();
        app.run_ex("command W w");
        assert_eq!(app.user_commands.len(), 1);
        app.run_ex("delcommand W");
        assert!(app.user_commands.is_empty());
    }

    #[test]
    fn self_referential_user_command_terminates() {
        let mut app = App::new();
        app.run_ex("command W W"); // alias to itself must not loop forever
        app.run_ex("W"); // returns (bounded resolution), no hang
    }

    #[test]
    fn set_title_option_toggles() {
        let mut app = App::new();
        app.run_ex("set title");
        assert!(app.editor.title);
        app.run_ex("set notitle");
        assert!(!app.editor.title);
    }

    // ---- :bufdo ----------------------------------------------------------

    #[test]
    fn bufdo_runs_command_on_every_buffer() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("a");
        let mut b1 = Editor::new();
        b1.buffer = crate::buffer::Buffer::from_text("a");
        let mut b2 = Editor::new();
        b2.buffer = crate::buffer::Buffer::from_text("a");
        app.others.push(b1);
        app.others.push(b2);
        app.run_ex("bufdo %s/a/b/");
        assert_eq!(app.editor.buffer.line(0), Some("b"));
        assert_eq!(app.others[0].buffer.line(0), Some("b"));
        assert_eq!(app.others[1].buffer.line(0), Some("b"));
    }

    #[test]
    fn bufdo_single_buffer_runs_once() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("a");
        app.run_ex("bufdo %s/a/b/");
        assert_eq!(app.editor.buffer.line(0), Some("b"));
    }

    #[test]
    fn bufdo_returns_to_starting_buffer() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("start");
        let mut b1 = Editor::new();
        b1.buffer = crate::buffer::Buffer::from_text("other");
        app.others.push(b1);
        app.run_ex("bufdo set number");
        assert_eq!(app.editor.buffer.line(0), Some("start")); // focus restored
    }

    // ---- :windo ----------------------------------------------------------

    #[test]
    fn windo_runs_command_in_every_window() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("a");
        app.run_ex("vsplit"); // two windows sharing one buffer
        app.run_ex("windo %s/a/b/");
        assert_eq!(app.editor.buffer.line(0), Some("b"));
        // Both windows are views of the same buffer, so each resolves to "b".
        let other = app.panes[0].id;
        assert_eq!(app.buffer_for(other).line(0), Some("b"));
    }

    #[test]
    fn windo_restores_the_starting_window() {
        let mut app = App::new();
        app.run_ex("vsplit");
        let start = app.focused;
        app.run_ex("windo set number");
        assert_eq!(app.focused, start);
    }

    #[test]
    fn windo_single_window_runs_once() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("a");
        app.run_ex("windo %s/a/b/");
        assert_eq!(app.editor.buffer.line(0), Some("b"));
    }

    // ---- :set backup -----------------------------------------------------

    #[test]
    fn backup_keeps_previous_contents_on_write() {
        let p = std::env::temp_dir().join(format!("rvim_bk_{}.txt", std::process::id()));
        std::fs::write(&p, "orig\n").unwrap();
        let mut app = App::open(p.to_str().unwrap()).unwrap();
        app.run_ex("set backup");
        app.editor.handle_key(kev('x')); // "orig" -> "rig"
        app.run_ex("w");
        let bak = format!("{}~", p.display());
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), "orig\n");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "rig\n");
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(&bak);
    }

    #[test]
    fn no_backup_by_default() {
        let p = std::env::temp_dir().join(format!("rvim_nobk_{}.txt", std::process::id()));
        std::fs::write(&p, "orig\n").unwrap();
        let mut app = App::open(p.to_str().unwrap()).unwrap();
        app.editor.handle_key(kev('x'));
        app.run_ex("w");
        let bak = format!("{}~", p.display());
        assert!(!std::path::Path::new(&bak).exists());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn status_display_toggles() {
        let mut app = App::new();
        assert!(app.editor.showmode && app.editor.showcmd && app.editor.ruler);
        app.run_ex("set noshowmode");
        assert!(!app.editor.showmode);
        app.run_ex("set noruler");
        assert!(!app.editor.ruler);
        app.run_ex("set noshowcmd");
        assert!(!app.editor.showcmd);
        app.run_ex("set showmode");
        assert!(app.editor.showmode);
    }

    #[test]
    fn apply_modelines_sets_buffer_options() {
        let mut app = App::new();
        app.editor.buffer =
            crate::buffer::Buffer::from_text("// vim: set ts=4 sw=2 et ft=rust:\ncode\n");
        app.editor.message = "loaded".into();
        app.apply_modelines();
        assert_eq!(app.editor.tabstop, 4);
        assert_eq!(app.editor.shiftwidth, 2);
        assert!(app.editor.expandtab);
        assert_eq!(app.editor.language, Language::Rust);
        // The pre-existing (file-load) message survives modeline application.
        assert_eq!(app.editor.message, "loaded");
    }

    #[test]
    fn apply_modelines_respects_nomodeline() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("// vim: ts=4\ncode\n");
        app.editor.modeline = false;
        let before = app.editor.tabstop;
        app.apply_modelines();
        assert_eq!(app.editor.tabstop, before);
    }
