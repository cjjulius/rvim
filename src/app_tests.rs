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
        assert_eq!(app.editor.colorcolumn, 80);
        app.run_ex("set cc=0");
        assert_eq!(app.editor.colorcolumn, 0);
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
    fn substitute_empty_pattern_reuses_previous() {
        let mut app = App::new();
        app.editor.buffer = crate::buffer::Buffer::from_text("foo bar foo");
        app.run_ex("s/foo/X/"); // first foo -> X, last pattern = foo
        app.run_ex("s//Y/"); // empty pattern reuses "foo" -> next foo
        assert_eq!(app.editor.buffer.line(0), Some("X bar Y"));
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
        assert!(app.block_state_top()); // line 1 is inside the block comment
        assert!(app.block_memo.is_some()); // result cached
        assert!(app.block_state_top()); // cached path returns the same
        app.editor.top = 3;
        assert!(!app.block_state_top()); // line 3 ("code") is outside the block
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
