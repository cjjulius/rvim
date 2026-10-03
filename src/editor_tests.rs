    use super::*;
    use crossterm::event::KeyEvent;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }
    fn special(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ed_with(text: &str) -> Editor {
        let mut ed = Editor::new();
        ed.buffer = Buffer::from_text(text);
        ed
    }

    #[test]
    fn basic_motion_hjkl() {
        let mut ed = ed_with("abc\ndef\nghi");
        ed.handle_key(key('l'));
        ed.handle_key(key('j'));
        assert_eq!(ed.cursor, Position::new(1, 1));
        ed.handle_key(key('h'));
        ed.handle_key(key('k'));
        assert_eq!(ed.cursor, Position::new(0, 0));
    }

    #[test]
    fn insert_text_and_escape() {
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        assert_eq!(ed.mode, Mode::Insert);
        for c in "hello".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.mode, Mode::Normal);
        assert_eq!(ed.buffer.line(0), Some("hello"));
    }

    #[test]
    fn mode_indicator_reflects_mode() {
        let mut ed = ed_with("hello");
        assert_eq!(ed.mode_indicator(), None); // Normal
        ed.handle_key(key('i'));
        assert_eq!(ed.mode_indicator(), Some("-- INSERT --"));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.mode_indicator(), None);
        ed.handle_key(key('v'));
        assert_eq!(ed.mode_indicator(), Some("-- VISUAL --"));
        ed.handle_key(special(KeyCode::Esc));
        ed.handle_key(special(KeyCode::Char('V')));
        assert_eq!(ed.mode_indicator(), Some("-- VISUAL LINE --"));
        ed.handle_key(special(KeyCode::Esc));
        ed.handle_key(ctrl('v'));
        assert_eq!(ed.mode_indicator(), Some("-- VISUAL BLOCK --"));
        ed.handle_key(special(KeyCode::Esc));
        ed.handle_key(key('R'));
        assert_eq!(ed.mode_indicator(), Some("-- REPLACE --"));
    }

    #[test]
    fn append_puts_cursor_after() {
        let mut ed = ed_with("ab");
        ed.handle_key(key('a'));
        ed.handle_key(key('X'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("aXb"));
    }

    #[test]
    fn dd_deletes_line_and_p_pastes() {
        let mut ed = ed_with("one\ntwo\nthree");
        ed.handle_key(key('d'));
        ed.handle_key(key('d'));
        assert_eq!(ed.buffer.line(0), Some("two"));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(1), Some("one"));
    }

    #[test]
    fn x_deletes_char() {
        let mut ed = ed_with("abc");
        ed.handle_key(key('x'));
        assert_eq!(ed.buffer.line(0), Some("bc"));
    }

    #[test]
    fn o_opens_line_below_in_insert() {
        let mut ed = ed_with("top");
        ed.handle_key(key('o'));
        assert_eq!(ed.mode, Mode::Insert);
        for c in "new".chars() {
            ed.handle_key(key(c));
        }
        assert_eq!(ed.buffer.line(1), Some("new"));
    }

    #[test]
    fn undo_after_insert() {
        let mut ed = ed_with("abc");
        ed.handle_key(key('x')); // delete 'a'
        assert_eq!(ed.buffer.line(0), Some("bc"));
        ed.handle_key(key('u'));
        assert_eq!(ed.buffer.line(0), Some("abc"));
    }

    #[test]
    fn line_undo_restores_line() {
        let mut ed = ed_with("hello world\nsecond");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('x')); // "ello world"
        ed.handle_key(key('x')); // "llo world"
        ed.handle_key(key('U')); // restore the whole line
        assert_eq!(ed.buffer.line(0), Some("hello world"));
    }

    #[test]
    fn line_undo_toggles() {
        let mut ed = ed_with("hello world");
        ed.handle_key(key('x'));
        ed.handle_key(key('x')); // "llo world"
        ed.handle_key(key('U')); // -> "hello world"
        assert_eq!(ed.buffer.line(0), Some("hello world"));
        ed.handle_key(key('U')); // toggle back -> "llo world"
        assert_eq!(ed.buffer.line(0), Some("llo world"));
    }

    #[test]
    fn plain_u_undoes_a_line_undo() {
        let mut ed = ed_with("hello world");
        ed.handle_key(key('x'));
        ed.handle_key(key('x')); // "llo world"
        ed.handle_key(key('U')); // "hello world"
        ed.handle_key(key('u')); // undo the U -> "llo world"
        assert_eq!(ed.buffer.line(0), Some("llo world"));
    }

    #[test]
    fn line_undo_without_changes_reports() {
        let mut ed = ed_with("untouched");
        ed.handle_key(key('U'));
        assert!(ed.message.contains("No line changes"));
    }

    #[test]
    fn count_undo_reverts_several_changes() {
        let mut ed = ed_with("abcdef");
        ed.handle_key(key('x'));
        ed.handle_key(key('x'));
        ed.handle_key(key('x')); // -> "def"
        assert_eq!(ed.buffer.line(0), Some("def"));
        ed.handle_key(key('3'));
        ed.handle_key(key('u')); // 3u undoes all three
        assert_eq!(ed.buffer.line(0), Some("abcdef"));
    }

    #[test]
    fn count_redo_reapplies_changes() {
        let mut ed = ed_with("abcdef");
        ed.handle_key(key('x'));
        ed.handle_key(key('x')); // -> "cdef"
        ed.undo_times(2); // back to "abcdef"
        assert_eq!(ed.buffer.line(0), Some("abcdef"));
        ed.redo_times(2); // forward to "cdef"
        assert_eq!(ed.buffer.line(0), Some("cdef"));
    }

    #[test]
    fn word_motion_forward() {
        let mut ed = ed_with("foo bar baz");
        ed.handle_key(key('w'));
        assert_eq!(ed.cursor.col, 4);
        ed.handle_key(key('w'));
        assert_eq!(ed.cursor.col, 8);
    }

    #[test]
    fn count_prefix_moves_multiple() {
        let mut ed = ed_with("abcdef");
        ed.handle_key(key('3'));
        ed.handle_key(key('l'));
        assert_eq!(ed.cursor.col, 3);
    }

    #[test]
    fn goto_gg_and_bottom() {
        let mut ed = ed_with("l0\nl1\nl2\nl3");
        ed.handle_key(key('G'));
        assert_eq!(ed.cursor.row, 3);
        ed.handle_key(key('g'));
        ed.handle_key(key('g'));
        assert_eq!(ed.cursor.row, 0);
    }

    #[test]
    fn visual_line_delete() {
        let mut ed = ed_with("a\nb\nc");
        ed.handle_key(key('V'));
        ed.handle_key(key('j'));
        ed.handle_key(key('d'));
        assert_eq!(ed.buffer.line(0), Some("c"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn search_forward_finds_next() {
        let mut ed = ed_with("alpha\nbeta\ngamma beta");
        ed.last_search = "beta".into();
        ed.search_repeat(true);
        assert_eq!(ed.cursor.row, 1);
        ed.search_repeat(true);
        assert_eq!(ed.cursor.row, 2);
    }

    #[test]
    fn counted_star_search() {
        let mut ed = ed_with("foo\nbar\nfoo\nbaz\nfoo");
        ed.cursor = Position::new(0, 0); // on the first "foo"
        ed.handle_key(key('2'));
        ed.handle_key(key('*')); // jump to the 2nd next occurrence
        assert_eq!(ed.cursor.row, 4);
    }

    #[test]
    fn counted_search_repeat() {
        let mut ed = ed_with("m0\nX\nX\nX\nm4");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('/'));
        ed.handle_key(key('X'));
        ed.handle_key(special(KeyCode::Enter));
        assert_eq!(ed.cursor.row, 1); // first match
        ed.handle_key(key('2'));
        ed.handle_key(key('n')); // two matches forward
        assert_eq!(ed.cursor.row, 3);
    }

    #[test]
    fn colon_enters_command_mode_and_returns_action() {
        let mut ed = ed_with("");
        ed.handle_key(key(':'));
        assert_eq!(ed.mode, Mode::Command);
        for c in "wq".chars() {
            ed.handle_key(key(c));
        }
        let action = ed.handle_key(special(KeyCode::Enter));
        assert_eq!(action, Action::RunEx("wq".into()));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn zz_writes_and_quits() {
        let mut ed = ed_with("hi");
        assert_eq!(ed.handle_key(key('Z')), Action::None); // first Z: pending
        assert_eq!(ed.handle_key(key('Z')), Action::RunEx("x".into()));
    }

    #[test]
    fn zq_quits_without_saving() {
        let mut ed = ed_with("hi");
        assert_eq!(ed.handle_key(key('Z')), Action::None);
        assert_eq!(ed.handle_key(key('Q')), Action::RunEx("q!".into()));
    }

    #[test]
    fn z_then_other_key_cancels() {
        let mut ed = ed_with("hi");
        ed.handle_key(key('Z'));
        assert_eq!(ed.handle_key(key('x')), Action::None); // not a quit; Z aborted
        assert!(!ed.pending_z_quit);
    }

    #[test]
    fn replace_char() {
        let mut ed = ed_with("cat");
        ed.handle_key(key('r'));
        ed.handle_key(key('b'));
        assert_eq!(ed.buffer.line(0), Some("bat"));
    }

    fn big_buffer(lines: usize) -> Editor {
        let text: Vec<String> = (0..lines).map(|i| format!("line{i}")).collect();
        let mut ed = ed_with(&text.join("\n"));
        ed.view_rows = 10;
        ed
    }

    #[test]
    fn z_dot_cr_dash_reposition_and_first_nonblank() {
        let mut ed = big_buffer(100); // view_rows = 10
        ed.cursor = Position::new(50, 3);
        ed.handle_key(key('z'));
        ed.handle_key(key('.')); // center + first non-blank
        assert_eq!(ed.top, 45);
        assert_eq!(ed.cursor.col, 0);

        ed.cursor = Position::new(50, 3);
        ed.handle_key(key('z'));
        ed.handle_key(special(KeyCode::Enter)); // line to top + first non-blank
        assert_eq!(ed.top, 50);
        assert_eq!(ed.cursor.col, 0);

        ed.cursor = Position::new(50, 3);
        ed.handle_key(key('z'));
        ed.handle_key(key('-')); // line to bottom + first non-blank
        assert_eq!(ed.top, 41);
        assert_eq!(ed.cursor.col, 0);
    }

    #[test]
    fn zz_zt_zb_position_viewport() {
        let mut ed = big_buffer(100);
        ed.cursor.row = 50;
        ed.handle_key(key('z'));
        ed.handle_key(key('z'));
        assert_eq!(ed.top, 45); // centered (50 - 10/2)

        ed.handle_key(key('z'));
        ed.handle_key(key('t'));
        assert_eq!(ed.top, 50); // line to top

        ed.handle_key(key('z'));
        ed.handle_key(key('b'));
        assert_eq!(ed.top, 41); // 50 + 1 - 10
    }

    #[test]
    fn sidescrolloff_keeps_horizontal_context() {
        let mut ed = ed_with(&"x".repeat(100));
        ed.view_cols = 20;
        ed.sidescrolloff = 4;
        ed.cursor.col = 50;
        ed.scroll_into_view();
        assert_eq!(ed.left, 35); // 4 cols of context to the right
        ed.cursor.col = 36;
        ed.scroll_into_view();
        assert_eq!(ed.left, 32); // 4 cols of context to the left
    }

    #[test]
    fn scrolloff_keeps_context_below_cursor() {
        let mut ed = big_buffer(100); // view_rows = 10
        ed.scrolloff = 3;
        ed.cursor.row = 8;
        ed.scroll_into_view();
        // Three lines must stay below the cursor (row 11 visible) -> top scrolls to 2.
        assert_eq!(ed.top, 2);
    }

    #[test]
    fn scrolloff_keeps_context_above_cursor() {
        let mut ed = big_buffer(100);
        ed.scrolloff = 3;
        ed.top = 20;
        ed.cursor.row = 21; // only one line of context above within the view
        ed.scroll_into_view();
        assert_eq!(ed.top, 18); // pulled up so three lines show above
    }

    #[test]
    fn scrolloff_shrinks_near_file_end() {
        let mut ed = big_buffer(10); // all 10 lines fit; cursor on the last line
        ed.scrolloff = 3;
        ed.cursor.row = 9;
        ed.scroll_into_view();
        assert_eq!(ed.top, 0); // never scrolls past the end to honor the margin
    }

    #[test]
    fn count_percent_jumps_to_line() {
        let mut ed = big_buffer(100); // 100 lines
        ed.handle_key(key('5'));
        ed.handle_key(key('0'));
        ed.handle_key(key('%')); // 50% -> line 50 -> row 49
        assert_eq!(ed.cursor.row, 49);
        ed.handle_key(key('1'));
        ed.handle_key(key('0'));
        ed.handle_key(key('0'));
        ed.handle_key(key('%')); // 100% -> last line
        assert_eq!(ed.cursor.row, 99);
    }

    #[test]
    fn percent_without_count_matches_bracket() {
        let mut ed = ed_with("a(b)c");
        ed.cursor = Position::new(0, 1); // on '('
        ed.handle_key(key('%'));
        assert_eq!(ed.cursor.col, 3); // matching ')'
    }

    #[test]
    fn ctrl_f_and_b_page_scroll() {
        let mut ed = big_buffer(100); // view_rows = 10 -> step = 8 (2-line overlap)
        ed.handle_key(ctrl('f'));
        assert_eq!(ed.top, 8);
        assert_eq!(ed.cursor.row, 8); // cursor on top line of new page
        ed.handle_key(ctrl('f'));
        assert_eq!(ed.top, 16);
        ed.handle_key(ctrl('b'));
        assert_eq!(ed.top, 8);
        assert_eq!(ed.cursor.row, 17); // cursor on bottom line of restored page
    }

    #[test]
    fn ctrl_f_honors_count() {
        let mut ed = big_buffer(100);
        ed.handle_key(key('2'));
        ed.handle_key(ctrl('f')); // two pages at once: 8 * 2
        assert_eq!(ed.top, 16);
    }

    #[test]
    fn h_l_honor_count() {
        let mut ed = big_buffer(100); // view_rows = 10
        ed.top = 20;
        ed.cursor.row = 25;
        ed.handle_key(key('3'));
        ed.handle_key(key('H')); // 3 lines below the top (20 + 2)
        assert_eq!(ed.cursor.row, 22);
        ed.handle_key(key('2'));
        ed.handle_key(key('L')); // 2 lines above the bottom (29 - 1)
        assert_eq!(ed.cursor.row, 28);
    }

    #[test]
    fn hml_jump_within_viewport() {
        let mut ed = big_buffer(100);
        ed.top = 20;
        ed.cursor.row = 25;
        ed.handle_key(key('H'));
        assert_eq!(ed.cursor.row, 20);
        ed.handle_key(key('M'));
        assert_eq!(ed.cursor.row, 25); // 20 + 10/2
        ed.handle_key(key('L'));
        assert_eq!(ed.cursor.row, 29); // 20 + 10 - 1
    }

    #[test]
    fn operator_screen_motion_d_l() {
        let mut ed = ed_with("0\n1\n2\n3\n4\n5\n6\n7\n8\n9");
        ed.view_rows = 5;
        ed.top = 0;
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('d'));
        ed.handle_key(key('L')); // delete cursor..bottom-of-screen (rows 0-4)
        assert_eq!(ed.buffer.line(0), Some("5"));
        assert_eq!(ed.buffer.line_count(), 5);
    }

    #[test]
    fn operator_screen_motion_d_h() {
        let mut ed = ed_with("0\n1\n2\n3\n4\n5\n6\n7\n8\n9");
        ed.view_rows = 5;
        ed.top = 2;
        ed.cursor = Position::new(4, 0);
        ed.handle_key(key('d'));
        ed.handle_key(key('H')); // delete top-of-screen..cursor (rows 2-4)
        assert_eq!(ed.buffer.line(2), Some("5"));
        assert_eq!(ed.buffer.line_count(), 7);
    }

    #[test]
    fn operator_screen_motion_d_m() {
        let mut ed = ed_with("0\n1\n2\n3\n4\n5");
        ed.view_rows = 5;
        ed.top = 0;
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('d'));
        ed.handle_key(key('M')); // delete cursor..middle-of-screen (rows 0-2)
        assert_eq!(ed.buffer.line(0), Some("3"));
        assert_eq!(ed.buffer.line_count(), 3);
    }

    #[test]
    fn ctrl_e_and_y_scroll_one_line() {
        let mut ed = big_buffer(100);
        ed.top = 10;
        ed.cursor.row = 15;
        ed.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL));
        assert_eq!(ed.top, 11);
        ed.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
        assert_eq!(ed.top, 10);
    }

    #[test]
    fn ctrl_e_pulls_cursor_into_view() {
        let mut ed = big_buffer(100);
        ed.top = 0;
        ed.cursor.row = 0;
        // Scroll down 5 lines; cursor (row 0) would be above the view, so it
        // should be pulled down to the new top.
        for _ in 0..5 {
            ed.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL));
        }
        assert_eq!(ed.top, 5);
        assert_eq!(ed.cursor.row, 5);
    }

    fn rust_ed(text: &str) -> Editor {
        let mut ed = ed_with(text);
        ed.language = Language::Rust;
        ed
    }

    #[test]
    fn comment_toggle_gcc() {
        let mut ed = rust_ed("let x = 1;");
        ed.handle_key(key('g'));
        ed.handle_key(key('c'));
        ed.handle_key(key('c')); // comment current line
        assert_eq!(ed.buffer.line(0), Some("// let x = 1;"));
        // Toggle back.
        ed.handle_key(key('g'));
        ed.handle_key(key('c'));
        ed.handle_key(key('c'));
        assert_eq!(ed.buffer.line(0), Some("let x = 1;"));
    }

    #[test]
    fn comment_toggle_preserves_indent() {
        let mut ed = rust_ed("    indented();");
        ed.handle_key(key('g'));
        ed.handle_key(key('c'));
        ed.handle_key(key('c'));
        assert_eq!(ed.buffer.line(0), Some("    // indented();"));
    }

    #[test]
    fn comment_toggle_range_with_motion() {
        let mut ed = rust_ed("a();\nb();\nc();");
        ed.handle_key(key('g'));
        ed.handle_key(key('c'));
        ed.handle_key(key('j')); // comment current + next line
        assert_eq!(ed.buffer.line(0), Some("// a();"));
        assert_eq!(ed.buffer.line(1), Some("// b();"));
        assert_eq!(ed.buffer.line(2), Some("c();"));
    }

    #[test]
    fn comment_toggle_visual_and_sql_marker() {
        let mut ed = ed_with("SELECT 1;\nFROM t;");
        ed.language = Language::PgSql;
        ed.handle_key(key('V'));
        ed.handle_key(key('j'));
        ed.handle_key(key('g'));
        ed.handle_key(key('c'));
        assert_eq!(ed.buffer.line(0), Some("-- SELECT 1;"));
        assert_eq!(ed.buffer.line(1), Some("-- FROM t;"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn case_op_gu_with_motion() {
        let mut ed = ed_with("HELLO WORLD");
        ed.handle_key(key('g'));
        ed.handle_key(key('u'));
        ed.handle_key(key('w')); // lowercase "HELLO " -> "hello "
        assert_eq!(ed.buffer.line(0), Some("hello WORLD"));
    }

    #[test]
    fn case_op_g_upper_with_text_object() {
        let mut ed = ed_with("foo bar baz");
        ed.handle_key(key('w')); // on "bar"
        ed.handle_key(key('g'));
        ed.handle_key(key('U'));
        ed.handle_key(key('i'));
        ed.handle_key(key('w')); // uppercase inner word
        assert_eq!(ed.buffer.line(0), Some("foo BAR baz"));
    }

    #[test]
    fn case_op_doubled_line() {
        let mut ed = ed_with("MixedCase Line");
        ed.handle_key(key('g'));
        ed.handle_key(key('u'));
        ed.handle_key(key('u')); // guu -> lowercase whole line
        assert_eq!(ed.buffer.line(0), Some("mixedcase line"));
    }

    #[test]
    fn case_op_toggle_with_dollar() {
        let mut ed = ed_with("aBcD");
        ed.handle_key(key('g'));
        ed.handle_key(key('~'));
        ed.handle_key(key('$')); // toggle to end of line
        assert_eq!(ed.buffer.line(0), Some("AbCd"));
    }

    #[test]
    fn rot13_doubled_line() {
        let mut ed = ed_with("Hello, World!");
        ed.handle_key(key('g'));
        ed.handle_key(key('?'));
        ed.handle_key(key('?')); // g?? -> ROT13 the whole line
        assert_eq!(ed.buffer.line(0), Some("Uryyb, Jbeyq!"));
    }

    #[test]
    fn rot13_inner_word() {
        let mut ed = ed_with("Hello world");
        ed.handle_key(key('g'));
        ed.handle_key(key('?'));
        ed.handle_key(key('i'));
        ed.handle_key(key('w')); // g?iw -> ROT13 the inner word
        assert_eq!(ed.buffer.line(0), Some("Uryyb world"));
    }

    #[test]
    fn rot13_visual_selection() {
        let mut ed = ed_with("abc");
        ed.handle_key(key('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // select "abc"
        ed.handle_key(key('g'));
        ed.handle_key(key('?')); // ROT13 the selection
        assert_eq!(ed.buffer.line(0), Some("nop"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn rot13_is_its_own_inverse() {
        let mut ed = ed_with("The Quick brown FOX 123");
        ed.handle_key(key('g'));
        ed.handle_key(key('?'));
        ed.handle_key(key('?')); // encode
        ed.handle_key(key('g'));
        ed.handle_key(key('?'));
        ed.handle_key(key('?')); // decode -> original (digits untouched)
        assert_eq!(ed.buffer.line(0), Some("The Quick brown FOX 123"));
    }

    #[test]
    fn text_object_diw() {
        let mut ed = ed_with("foo bar baz");
        ed.handle_key(key('w')); // cursor on "bar" (col 4)
        ed.handle_key(key('d'));
        ed.handle_key(key('i'));
        ed.handle_key(key('w')); // delete inner word "bar"
        assert_eq!(ed.buffer.line(0), Some("foo  baz"));
    }

    #[test]
    fn text_object_daw_removes_trailing_space() {
        let mut ed = ed_with("foo bar baz");
        ed.handle_key(key('w')); // on "bar"
        ed.handle_key(key('d'));
        ed.handle_key(key('a'));
        ed.handle_key(key('w')); // delete "bar " (a word)
        assert_eq!(ed.buffer.line(0), Some("foo baz"));
    }

    #[test]
    fn text_object_ci_parens() {
        let mut ed = ed_with("call(arg1, arg2)");
        ed.cursor = Position::new(0, 6); // inside parens (on 'r' of arg1)
        ed.handle_key(key('c'));
        ed.handle_key(key('i'));
        ed.handle_key(key('(')); // change inner parens
        assert_eq!(ed.mode, Mode::Insert);
        ed.handle_key(key('X'));
        assert_eq!(ed.buffer.line(0), Some("call(X)"));
    }

    #[test]
    fn text_object_di_quotes() {
        let mut ed = ed_with("say \"hello world\" now");
        // move cursor inside the quotes
        ed.cursor = Position::new(0, 8);
        ed.handle_key(key('d'));
        ed.handle_key(key('i'));
        ed.handle_key(key('"'));
        assert_eq!(ed.buffer.line(0), Some("say \"\" now"));
    }

    #[test]
    fn text_object_da_parens_includes_delims() {
        let mut ed = ed_with("x(inner)y");
        ed.cursor = Position::new(0, 3);
        ed.handle_key(key('d'));
        ed.handle_key(key('a'));
        ed.handle_key(key('(')); // delete "(inner)"
        assert_eq!(ed.buffer.line(0), Some("xy"));
    }

    #[test]
    fn text_object_di_brace_multiline() {
        let mut ed = ed_with("fn f() {\n    body;\n}");
        ed.cursor = Position::new(1, 4); // inside the braces
        ed.handle_key(key('d'));
        ed.handle_key(key('i'));
        ed.handle_key(key('{')); // delete the inner line
        assert_eq!(ed.buffer.line(0), Some("fn f() {"));
        assert_eq!(ed.buffer.line(1), Some(""));
        assert_eq!(ed.buffer.line(2), Some("}"));
        assert_eq!(ed.buffer.line_count(), 3);
    }

    #[test]
    fn text_object_da_paren_multiline_joins() {
        let mut ed = ed_with("foo(\n  a,\n  b\n)bar");
        ed.cursor = Position::new(1, 2); // inside the parens
        ed.handle_key(key('d'));
        ed.handle_key(key('a'));
        ed.handle_key(key('(')); // delete "(...)" across lines
        assert_eq!(ed.buffer.line(0), Some("foobar"));
        assert_eq!(ed.buffer.line_count(), 1);
    }

    #[test]
    fn text_object_ci_paren_multiline_enters_insert() {
        let mut ed = ed_with("call(\n  x\n)");
        ed.cursor = Position::new(1, 2);
        ed.handle_key(key('c'));
        ed.handle_key(key('i'));
        ed.handle_key(key('(')); // change inner across lines
        assert_eq!(ed.mode, Mode::Insert);
        assert_eq!(ed.buffer.line(1), Some(""));
    }

    #[test]
    fn text_object_single_line_paren_unchanged() {
        // The single-line fast path must still work exactly as before.
        let mut ed = ed_with("foo(bar)baz");
        ed.cursor = Position::new(0, 5);
        ed.handle_key(key('d'));
        ed.handle_key(key('i'));
        ed.handle_key(key('('));
        assert_eq!(ed.buffer.line(0), Some("foo()baz"));
    }

    #[test]
    fn percent_operator_deletes_to_match() {
        let mut ed = ed_with("foo(bar)baz");
        ed.cursor = Position::new(0, 3); // on '('
        ed.handle_key(key('d'));
        ed.handle_key(key('%')); // delete "(bar)"
        assert_eq!(ed.buffer.line(0), Some("foobaz"));
    }

    #[test]
    fn percent_operator_crosses_lines() {
        let mut ed = ed_with("x = foo(\n  a\n)");
        ed.cursor = Position::new(0, 7); // on '('
        ed.handle_key(key('d'));
        ed.handle_key(key('%')); // delete across lines to ')'
        assert_eq!(ed.buffer.line(0), Some("x = foo"));
        assert_eq!(ed.buffer.line_count(), 1);
    }

    #[test]
    fn percent_change_enters_insert() {
        let mut ed = ed_with("a(b)c");
        ed.cursor = Position::new(0, 1); // on '('
        ed.handle_key(key('c'));
        ed.handle_key(key('%'));
        assert_eq!(ed.mode, Mode::Insert);
        assert_eq!(ed.buffer.line(0), Some("ac"));
    }

    #[test]
    fn percent_yank_then_paste() {
        let mut ed = ed_with("(ab)");
        ed.cursor = Position::new(0, 0); // on '('
        ed.handle_key(key('y'));
        ed.handle_key(key('%')); // yank "(ab)"
        assert_eq!(ed.buffer.line(0), Some("(ab)"));
        ed.handle_key(key('$'));
        ed.handle_key(key('p')); // paste after -> "(ab)(ab)"
        assert_eq!(ed.buffer.line(0), Some("(ab)(ab)"));
    }

    #[test]
    fn case_op_upper_inner_brace_multiline() {
        let mut ed = ed_with("{\nabc\n}");
        ed.cursor = Position::new(1, 1);
        ed.handle_key(key('g'));
        ed.handle_key(key('U'));
        ed.handle_key(key('i'));
        ed.handle_key(key('{')); // uppercase the inner line
        assert_eq!(ed.buffer.line(1), Some("ABC"));
    }

    #[test]
    fn dot_repeats_x() {
        let mut ed = ed_with("abcdef");
        ed.handle_key(key('x')); // delete 'a' -> "bcdef"
        assert_eq!(ed.buffer.line(0), Some("bcdef"));
        ed.handle_key(key('.')); // repeat -> "cdef"
        assert_eq!(ed.buffer.line(0), Some("cdef"));
        ed.handle_key(key('.')); // -> "def"
        assert_eq!(ed.buffer.line(0), Some("def"));
    }

    #[test]
    fn dot_repeats_dd() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.handle_key(key('d'));
        ed.handle_key(key('d')); // delete "a"
        assert_eq!(ed.buffer.line(0), Some("b"));
        ed.handle_key(key('.')); // delete "b"
        assert_eq!(ed.buffer.line(0), Some("c"));
    }

    #[test]
    fn dot_repeats_insert_change() {
        let mut ed = ed_with("one\ntwo");
        // Insert "# " at the start of the line.
        ed.handle_key(key('I'));
        ed.handle_key(key('#'));
        ed.handle_key(key(' '));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("# one"));
        // Move to next line and repeat.
        ed.handle_key(key('j'));
        ed.handle_key(key('.'));
        assert_eq!(ed.buffer.line(1), Some("# two"));
    }

    #[test]
    fn dot_unchanged_by_navigation() {
        let mut ed = ed_with("abc\ndef");
        ed.handle_key(key('x')); // change: delete 'a'
        ed.handle_key(key('j')); // navigation (no change)
        ed.handle_key(key('0'));
        ed.handle_key(key('.')); // should repeat the delete, not the navigation
        assert_eq!(ed.buffer.line(1), Some("ef"));
    }

    #[test]
    fn macro_record_and_replay() {
        let mut ed = ed_with("a\nb\nc\nd");
        // Record into register q: delete a line (dd).
        ed.handle_key(key('q'));
        ed.handle_key(key('q')); // start recording into q
        assert_eq!(ed.recording_register(), Some('q'));
        ed.handle_key(key('d'));
        ed.handle_key(key('d')); // dd (recorded)
        ed.handle_key(key('q')); // stop recording
        assert_eq!(ed.recording_register(), None);
        assert_eq!(ed.buffer.line(0), Some("b"));
        // Replay: delete another line.
        ed.handle_key(key('@'));
        ed.handle_key(key('q'));
        assert_eq!(ed.buffer.line(0), Some("c"));
        // @@ repeats the last macro.
        ed.handle_key(key('@'));
        ed.handle_key(key('@'));
        assert_eq!(ed.buffer.line(0), Some("d"));
    }

    #[test]
    fn macro_uppercase_appends() {
        let mut ed = ed_with("abcdef");
        // Record `x` into register a.
        ed.handle_key(key('q'));
        ed.handle_key(key('a'));
        ed.handle_key(key('x')); // delete 'a'
        ed.handle_key(key('q'));
        assert_eq!(ed.buffer.line(0), Some("bcdef"));
        // Append another `x` via qA (records into the same register a).
        ed.handle_key(key('q'));
        ed.handle_key(key('A'));
        assert_eq!(ed.recording_register(), Some('a'));
        ed.handle_key(key('x')); // delete 'b'
        ed.handle_key(key('q'));
        assert_eq!(ed.buffer.line(0), Some("cdef"));
        // Register a now holds two x's; replaying deletes two chars.
        ed.handle_key(key('@'));
        ed.handle_key(key('a'));
        assert_eq!(ed.buffer.line(0), Some("ef"));
    }

    #[test]
    fn macro_uppercase_play_reads_lowercase() {
        let mut ed = ed_with("abc\nabc");
        ed.handle_key(key('q'));
        ed.handle_key(key('a'));
        ed.handle_key(key('x')); // delete first char
        ed.handle_key(key('q'));
        ed.handle_key(key('j'));
        ed.handle_key(key('0'));
        ed.handle_key(key('@'));
        ed.handle_key(key('A')); // @A resolves to register a
        assert_eq!(ed.buffer.line(1), Some("bc"));
    }

    #[test]
    fn macro_records_insert_sequence() {
        let mut ed = ed_with("x\ny");
        ed.handle_key(key('q'));
        ed.handle_key(key('a')); // record into a
        ed.handle_key(key('I')); // insert at line start
        ed.handle_key(key('>'));
        ed.handle_key(key(' '));
        ed.handle_key(special(KeyCode::Esc));
        ed.handle_key(key('q')); // stop
        assert_eq!(ed.buffer.line(0), Some("> x"));
        // Replay on the next line.
        ed.handle_key(key('j'));
        ed.handle_key(key('0'));
        ed.handle_key(key('@'));
        ed.handle_key(key('a'));
        assert_eq!(ed.buffer.line(1), Some("> y"));
    }

    #[test]
    fn mark_set_and_jump_exact() {
        let mut ed = ed_with("l0\nl1\nl2\nl3");
        ed.cursor = Position::new(2, 1);
        ed.handle_key(key('m'));
        ed.handle_key(key('a')); // set mark a at (2,1)
        ed.handle_key(key('g'));
        ed.handle_key(key('g')); // to top
        assert_eq!(ed.cursor.row, 0);
        ed.handle_key(key('`'));
        ed.handle_key(key('a')); // jump back to mark a
        assert_eq!(ed.cursor, Position::new(2, 1));
    }

    #[test]
    fn mark_jump_line_lands_on_first_nonblank() {
        let mut ed = ed_with("l0\n  indented\nl2");
        ed.cursor = Position::new(1, 5);
        ed.handle_key(key('m'));
        ed.handle_key(key('x'));
        ed.handle_key(key('g'));
        ed.handle_key(key('g'));
        ed.handle_key(key('\'')); // 'x -> line of mark, first non-blank
        ed.handle_key(key('x'));
        assert_eq!(ed.cursor.row, 1);
        assert_eq!(ed.cursor.col, 2); // first non-blank
    }

    #[test]
    fn backtick_backtick_returns_to_previous() {
        let mut ed = ed_with("a\nb\nc\nd\ne");
        ed.cursor = Position::new(1, 0);
        ed.handle_key(key('G')); // jump to last line, records previous (1,0)
        assert_eq!(ed.cursor.row, 4);
        ed.handle_key(key('`'));
        ed.handle_key(key('`')); // back to previous
        assert_eq!(ed.cursor.row, 1);
    }

    #[test]
    fn count_before_operator_3dd() {
        let mut ed = ed_with("a\nb\nc\nd\ne");
        ed.handle_key(key('3'));
        ed.handle_key(key('d'));
        ed.handle_key(key('d')); // delete 3 lines
        assert_eq!(ed.buffer.line(0), Some("d"));
        assert_eq!(ed.buffer.line_count(), 2);
    }

    #[test]
    fn count_between_operator_and_motion_d3w() {
        let mut ed = ed_with("one two three four");
        ed.handle_key(key('d'));
        ed.handle_key(key('3'));
        ed.handle_key(key('w')); // delete 3 words
        assert_eq!(ed.buffer.line(0), Some("four"));
    }

    #[test]
    fn multiplied_counts_2d3w() {
        let mut ed = ed_with("a b c d e f g");
        ed.handle_key(key('2'));
        ed.handle_key(key('d'));
        ed.handle_key(key('3'));
        ed.handle_key(key('w')); // 2*3 = 6 words deleted
        assert_eq!(ed.buffer.line(0), Some("g"));
    }

    #[test]
    fn count_paste_3p() {
        let mut ed = ed_with("x");
        ed.handle_key(key('y'));
        ed.handle_key(key('y')); // yank "x" linewise
        ed.handle_key(key('3'));
        ed.handle_key(key('p')); // paste 3 times
        assert_eq!(ed.buffer.line_count(), 4);
    }

    #[test]
    fn yank_word_and_paste() {
        let mut ed = ed_with("foo bar");
        ed.handle_key(key('y'));
        ed.handle_key(key('w')); // yank "foo "
        ed.handle_key(key('$'));
        ed.handle_key(key('p')); // paste after last char
        assert_eq!(ed.buffer.line(0), Some("foo barfoo "));
    }

    #[test]
    fn yank_to_eol() {
        let mut ed = ed_with("hello world");
        ed.handle_key(key('w')); // cursor at col 6 (start of "world")
        ed.handle_key(key('y'));
        ed.handle_key(key('$')); // yank "world"
        ed.handle_key(key('0'));
        ed.handle_key(key('P')); // paste before line start
        assert_eq!(ed.buffer.line(0), Some("worldhello world"));
    }

    #[test]
    fn delete_to_line_start() {
        let mut ed = ed_with("hello");
        ed.handle_key(key('3'));
        ed.handle_key(key('l')); // col 3
        ed.handle_key(key('d'));
        ed.handle_key(key('0')); // delete cols [0,3)
        assert_eq!(ed.buffer.line(0), Some("lo"));
    }

    #[test]
    fn delete_down_two_lines() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.handle_key(key('d'));
        ed.handle_key(key('j')); // delete current + next (a, b)
        assert_eq!(ed.buffer.line(0), Some("c"));
        assert_eq!(ed.buffer.line_count(), 2);
    }

    #[test]
    fn delete_to_end_with_d_g() {
        let mut ed = ed_with("l0\nl1\nl2\nl3");
        ed.handle_key(key('j')); // row 1
        ed.handle_key(key('d'));
        ed.handle_key(key('G')); // delete rows 1..=3
        assert_eq!(ed.buffer.line_count(), 1);
        assert_eq!(ed.buffer.line(0), Some("l0"));
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn cmdline_tab_completes_unique_command() {
        let mut ed = ed_with("");
        ed.handle_key(key(':'));
        for c in "sor".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Tab)); // :sor -> :sort
        assert_eq!(ed.cmdline, "sort");
    }

    #[test]
    fn cmdline_tab_cycles_matches() {
        let mut ed = ed_with("");
        ed.handle_key(key(':'));
        ed.handle_key(key('w'));
        ed.handle_key(special(KeyCode::Tab)); // wall
        assert_eq!(ed.cmdline, "wall");
        ed.handle_key(special(KeyCode::Tab)); // wq
        assert_eq!(ed.cmdline, "wq");
        ed.handle_key(special(KeyCode::BackTab)); // back to wall
        assert_eq!(ed.cmdline, "wall");
    }

    #[test]
    fn cmdline_tab_completes_set_option() {
        let mut ed = ed_with("");
        ed.handle_key(key(':'));
        for c in "set nu".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Tab)); // :set nu -> :set number
        assert_eq!(ed.cmdline, "set number");
    }

    #[test]
    fn cmdline_tab_noop_on_unknown_prefix() {
        let mut ed = ed_with("");
        ed.handle_key(key(':'));
        for c in "zzz".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Tab)); // no command starts with zzz
        assert_eq!(ed.cmdline, "zzz");
    }

    #[test]
    fn digraph_inserts_accented_letter() {
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        ed.handle_key(ctrl('k'));
        ed.handle_key(key('a'));
        ed.handle_key(key(':')); // Ctrl-k a : -> ä
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("ä"));
    }

    #[test]
    fn digraph_accepts_reversed_order() {
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        ed.handle_key(ctrl('k'));
        ed.handle_key(key(':'));
        ed.handle_key(key('a')); // reversed order still composes ä
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("ä"));
    }

    #[test]
    fn digraph_inserts_symbol_and_continues_typing() {
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        ed.handle_key(ctrl('k'));
        ed.handle_key(key('-'));
        ed.handle_key(key('>')); // Ctrl-k - > -> →
        ed.handle_key(key('x')); // normal typing resumes
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("→x"));
    }

    #[test]
    fn digraph_escape_cancels() {
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        ed.handle_key(ctrl('k'));
        ed.handle_key(special(KeyCode::Esc)); // cancel the digraph, stay in insert
        assert_eq!(ed.mode, Mode::Insert);
        ed.handle_key(key('z'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("z"));
    }

    #[test]
    fn digraph_repeats_with_dot() {
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        ed.handle_key(ctrl('k'));
        ed.handle_key(key('o'));
        ed.handle_key(key(':')); // insert ö
        ed.handle_key(special(KeyCode::Esc));
        ed.handle_key(key('.')); // dot-repeat the whole insert
        assert_eq!(ed.buffer.line(0), Some("öö"));
    }

    #[test]
    fn command_history_recall() {
        let mut ed = ed_with("x");
        // Run two ex commands to build history.
        ed.handle_key(key(':'));
        for c in "set number".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Enter));
        ed.handle_key(key(':'));
        for c in "noh".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Enter));
        // Open command line, Up recalls most recent, Up again older.
        ed.handle_key(key(':'));
        ed.handle_key(special(KeyCode::Up));
        assert_eq!(ed.cmdline, "noh");
        ed.handle_key(special(KeyCode::Up));
        assert_eq!(ed.cmdline, "set number");
        ed.handle_key(special(KeyCode::Down));
        assert_eq!(ed.cmdline, "noh");
    }

    #[test]
    fn search_history_is_separate() {
        let mut ed = ed_with("foo bar");
        ed.handle_key(key(':'));
        for c in "wq".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Esc)); // not recorded (esc), use a real run instead
        ed.handle_key(key('/'));
        for c in "bar".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Enter)); // search history gets "bar"
        ed.handle_key(key('/'));
        ed.handle_key(special(KeyCode::Up));
        assert_eq!(ed.cmdline, "bar");
    }

    #[test]
    fn gi_resumes_at_last_insert() {
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        for c in "abc".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Esc)); // insert ended at col 3
        ed.handle_key(key('0')); // move to col 0
        ed.handle_key(key('g'));
        ed.handle_key(key('i')); // resume at col 3
        assert_eq!(ed.mode, Mode::Insert);
        ed.handle_key(key('d'));
        assert_eq!(ed.buffer.line(0), Some("abcd"));
    }

    #[test]
    fn put_marks_bracket_linewise() {
        let mut ed = ed_with("one\ntwo\nthree");
        ed.handle_key(key('y'));
        ed.handle_key(key('y')); // yank "one" linewise
        ed.handle_key(key('j'));
        ed.handle_key(key('p')); // paste below line 1 -> row 2 is "one"
        ed.handle_key(key('`'));
        ed.handle_key(key('[')); // `[ -> start of put
        assert_eq!(ed.cursor, Position::new(2, 0));
        ed.handle_key(key('`'));
        ed.handle_key(key(']')); // `] -> end of put
        assert_eq!(ed.cursor, Position::new(2, 2));
    }

    #[test]
    fn put_marks_bracket_charwise() {
        let mut ed = ed_with("foo bar");
        ed.handle_key(key('y'));
        ed.handle_key(key('i'));
        ed.handle_key(key('w')); // yank "foo" charwise
        ed.cursor = Position::new(0, 4); // on "bar"
        ed.handle_key(key('P')); // paste before -> "foo foobar"
        assert_eq!(ed.buffer.line(0), Some("foo foobar"));
        ed.handle_key(key('`'));
        ed.handle_key(key('['));
        assert_eq!(ed.cursor, Position::new(0, 4));
        ed.handle_key(key('`'));
        ed.handle_key(key(']'));
        assert_eq!(ed.cursor, Position::new(0, 6));
    }

    #[test]
    fn put_mark_linewise_jump_lands_on_first_nonblank() {
        let mut ed = ed_with("  indented\nx");
        ed.handle_key(key('y'));
        ed.handle_key(key('y')); // yank "  indented"
        ed.handle_key(key('j'));
        ed.handle_key(key('p')); // paste below -> row 2
        ed.handle_key(key('\''));
        ed.handle_key(key('[')); // '[ is linewise -> first non-blank
        assert_eq!(ed.cursor, Position::new(2, 2));
    }

    #[test]
    fn yank_sets_bracket_marks_charwise() {
        let mut ed = ed_with("foo bar baz");
        ed.cursor = Position::new(0, 4); // on "bar"
        ed.handle_key(key('y'));
        ed.handle_key(key('i'));
        ed.handle_key(key('w')); // yiw -> yank "bar" (cols 4..6)
        ed.handle_key(key('0'));
        ed.handle_key(key('`'));
        ed.handle_key(key('['));
        assert_eq!(ed.cursor, Position::new(0, 4));
        ed.handle_key(key('`'));
        ed.handle_key(key(']'));
        assert_eq!(ed.cursor, Position::new(0, 6));
    }

    #[test]
    fn yank_sets_bracket_marks_linewise() {
        let mut ed = ed_with("one\ntwo\nthree");
        ed.handle_key(key('y'));
        ed.handle_key(key('j')); // yj -> yank lines 0-1 linewise
        ed.handle_key(key('G'));
        ed.handle_key(key('`'));
        ed.handle_key(key('['));
        assert_eq!(ed.cursor, Position::new(0, 0));
        ed.handle_key(key('`'));
        ed.handle_key(key(']'));
        assert_eq!(ed.cursor, Position::new(1, 2)); // last char of "two"
    }

    #[test]
    fn delete_sets_collapsed_bracket_marks() {
        let mut ed = ed_with("a\nb\nc");
        ed.cursor = Position::new(1, 0);
        ed.handle_key(key('d'));
        ed.handle_key(key('d')); // delete line "b"
        ed.handle_key(key('g'));
        ed.handle_key(key('g')); // jump to top
        ed.handle_key(key('`'));
        ed.handle_key(key('[')); // `[ -> where the deletion happened
        assert_eq!(ed.cursor, Position::new(1, 0));
    }

    #[test]
    fn visual_yank_sets_bracket_marks() {
        let mut ed = ed_with("hello");
        ed.handle_key(key('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // select "hel"
        ed.handle_key(key('y'));
        ed.handle_key(key('$'));
        ed.handle_key(key('`'));
        ed.handle_key(key('['));
        assert_eq!(ed.cursor, Position::new(0, 0));
        ed.handle_key(key('`'));
        ed.handle_key(key(']'));
        assert_eq!(ed.cursor, Position::new(0, 2));
    }

    #[test]
    fn mark_dot_tracks_last_change() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.cursor = Position::new(2, 0); // on "c"
        ed.handle_key(key('x')); // change on line 2
        ed.handle_key(key('g'));
        ed.handle_key(key('g')); // jump to top
        assert_eq!(ed.cursor.row, 0);
        ed.handle_key(key('`'));
        ed.handle_key(key('.')); // jump to last change
        assert_eq!(ed.cursor.row, 2);
    }

    #[test]
    fn ge_moves_to_previous_word_end() {
        let mut ed = ed_with("foo bar baz");
        ed.cursor = Position::new(0, 9); // on 'a' of "baz"
        ed.handle_key(key('g'));
        ed.handle_key(key('e')); // end of "bar" -> col 6
        assert_eq!(ed.cursor.col, 6);
        ed.handle_key(key('g'));
        ed.handle_key(key('e')); // end of "foo" -> col 2
        assert_eq!(ed.cursor.col, 2);
    }

    #[test]
    fn ge_stops_at_punctuation_but_big_e_spans() {
        let mut ed = ed_with("foo.bar baz");
        ed.cursor = Position::new(0, 8); // on 'b' of "baz"
        ed.handle_key(key('g'));
        ed.handle_key(key('e')); // small ge -> end of "bar" (col 6)
        assert_eq!(ed.cursor.col, 6);
        let mut ed2 = ed_with("foo.bar baz");
        ed2.cursor = Position::new(0, 8);
        ed2.handle_key(key('g'));
        ed2.handle_key(key('E')); // big gE -> end of WORD "foo.bar" (col 6 too here)
        assert_eq!(ed2.cursor.col, 6);
    }

    #[test]
    fn block_delete_removes_rectangle() {
        let mut ed = ed_with("abcd\nefgh\nijkl");
        // cursor at (0,1); block select to (2,2) -> columns 1..=2 over 3 rows
        ed.handle_key(key('l')); // col 1
        ed.handle_key(ctrl('v'));
        ed.handle_key(key('j'));
        ed.handle_key(key('j')); // row 2
        ed.handle_key(key('l')); // col 2
        ed.handle_key(key('d'));
        assert_eq!(ed.buffer.line(0), Some("ad"));
        assert_eq!(ed.buffer.line(1), Some("eh"));
        assert_eq!(ed.buffer.line(2), Some("il"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn block_insert_prepends_each_row() {
        let mut ed = ed_with("one\ntwo\nthree");
        ed.handle_key(ctrl('v'));
        ed.handle_key(key('j'));
        ed.handle_key(key('j')); // block over column 0, rows 0..2
        ed.handle_key(key('I'));
        ed.handle_key(key('#'));
        ed.handle_key(key(' '));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("# one"));
        assert_eq!(ed.buffer.line(1), Some("# two"));
        assert_eq!(ed.buffer.line(2), Some("# three"));
    }

    #[test]
    fn block_append_pads_short_rows() {
        let mut ed = ed_with("aa\nb\nccc");
        ed.handle_key(key('$')); // col 1 on "aa"
        ed.handle_key(ctrl('v'));
        ed.handle_key(key('j'));
        ed.handle_key(key('j')); // rows 0..2, col ~1
        ed.handle_key(key('A'));
        ed.handle_key(key('X'));
        ed.handle_key(special(KeyCode::Esc));
        // Append at column 2 (cmax+1); short rows get padded, longer rows get
        // the text inserted at that column.
        assert_eq!(ed.buffer.line(0), Some("aaX"));
        assert_eq!(ed.buffer.line(1), Some("b X"));
        assert_eq!(ed.buffer.line(2), Some("ccXc"));
    }

    #[test]
    fn block_append_ragged_right_with_dollar() {
        let mut ed = ed_with("a\nbbbb\ncc");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(ctrl('v')); // visual block
        ed.handle_key(key('j'));
        ed.handle_key(key('j')); // rows 0..2
        ed.handle_key(key('$')); // extend to each line's own end
        ed.handle_key(key('A'));
        ed.handle_key(key('X'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("aX"));
        assert_eq!(ed.buffer.line(1), Some("bbbbX"));
        assert_eq!(ed.buffer.line(2), Some("ccX"));
    }

    #[test]
    fn ctrl_a_increments_number() {
        let mut ed = ed_with("value = 41");
        ed.handle_key(ctrl('a')); // cursor at 0; finds 41 -> 42
        assert_eq!(ed.buffer.line(0), Some("value = 42"));
        assert_eq!(ed.cursor.col, 9); // on last digit
    }

    #[test]
    fn ctrl_x_decrements_with_count() {
        let mut ed = ed_with("x10y");
        ed.handle_key(key('5'));
        ed.handle_key(ctrl('x')); // 10 - 5 = 5
        assert_eq!(ed.buffer.line(0), Some("x5y"));
    }

    #[test]
    fn ctrl_a_handles_negative() {
        let mut ed = ed_with("n = -1");
        ed.handle_key(key('$')); // on '1'
        ed.handle_key(ctrl('a')); // -1 + 1 = 0
        assert_eq!(ed.buffer.line(0), Some("n = 0"));
    }

    #[test]
    fn ctrl_a_crosses_into_negative() {
        let mut ed = ed_with("3");
        ed.handle_key(key('5'));
        ed.handle_key(ctrl('x')); // 3 - 5 = -2
        assert_eq!(ed.buffer.line(0), Some("-2"));
    }

    #[test]
    fn ctrl_a_increments_hex_preserving_width() {
        let mut ed = ed_with("0x0f");
        ed.handle_key(ctrl('a')); // 0x0f + 1 = 0x10 (width kept)
        assert_eq!(ed.buffer.line(0), Some("0x10"));
    }

    #[test]
    fn ctrl_a_hex_keeps_uppercase_digits() {
        let mut ed = ed_with("0xFF");
        ed.handle_key(ctrl('a')); // 0xFF + 1 = 0x100, uppercased
        assert_eq!(ed.buffer.line(0), Some("0x100"));
    }

    #[test]
    fn ctrl_a_does_not_touch_hex_prefix_zero() {
        // The leading 0 of 0x1a must not be treated as a decimal number.
        let mut ed = ed_with("0x1a");
        ed.handle_key(ctrl('a')); // 0x1a + 1 = 0x1b
        assert_eq!(ed.buffer.line(0), Some("0x1b"));
    }

    #[test]
    fn ctrl_x_decrements_binary() {
        let mut ed = ed_with("0b0101");
        ed.handle_key(ctrl('x')); // 0b0101 - 1 = 0b0100 (width kept)
        assert_eq!(ed.buffer.line(0), Some("0b0100"));
    }

    #[test]
    fn ctrl_a_decimal_still_works_after_hex_support() {
        let mut ed = ed_with("items: 9");
        ed.handle_key(ctrl('a')); // plain decimal path unchanged
        assert_eq!(ed.buffer.line(0), Some("items: 10"));
    }

    #[test]
    fn insert_ctrl_r_pastes_register() {
        let mut ed = ed_with("word\ntarget");
        ed.handle_key(key('y'));
        ed.handle_key(key('i'));
        ed.handle_key(key('w')); // yiw -> unnamed = "word"
        ed.handle_key(key('j'));
        ed.handle_key(key('A')); // append at end of "target"
        ed.handle_key(ctrl('r'));
        ed.handle_key(key('"')); // paste unnamed register
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(1), Some("targetword"));
    }

    #[test]
    fn insert_ctrl_r_named_register() {
        let mut ed = ed_with("hi");
        ed.handle_key(key('"'));
        ed.handle_key(key('a'));
        ed.handle_key(key('y'));
        ed.handle_key(key('y')); // "ayy -> register a = "hi"
        ed.handle_key(key('A'));
        ed.handle_key(ctrl('r'));
        ed.handle_key(key('a'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("hihi"));
    }

    #[test]
    fn insert_ctrl_t_and_ctrl_d_indent() {
        let mut ed = ed_with("code");
        ed.shiftwidth = 2;
        ed.handle_key(key('A')); // insert at end, cursor col 4
        ed.handle_key(ctrl('t')); // indent -> "  code", cursor col 6
        assert_eq!(ed.buffer.line(0), Some("  code"));
        assert_eq!(ed.cursor.col, 6);
        ed.handle_key(ctrl('d')); // dedent -> "code", cursor col 4
        assert_eq!(ed.buffer.line(0), Some("code"));
        assert_eq!(ed.cursor.col, 4);
    }

    #[test]
    fn count_insert_repeats_text() {
        let mut ed = ed_with("");
        ed.handle_key(key('3'));
        ed.handle_key(key('i'));
        ed.handle_key(key('h'));
        ed.handle_key(key('i'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("hihihi"));
    }

    #[test]
    fn count_append_repeats() {
        let mut ed = ed_with("x");
        ed.handle_key(key('3'));
        ed.handle_key(key('a'));
        ed.handle_key(key('-'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("x---"));
    }

    #[test]
    fn count_open_creates_multiple_lines() {
        let mut ed = ed_with("top");
        ed.handle_key(key('3'));
        ed.handle_key(key('o'));
        ed.handle_key(key('z'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("top"));
        assert_eq!(ed.buffer.line(1), Some("z"));
        assert_eq!(ed.buffer.line(2), Some("z"));
        assert_eq!(ed.buffer.line(3), Some("z"));
        assert_eq!(ed.buffer.line_count(), 4);
    }

    #[test]
    fn plain_insert_not_repeated() {
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        ed.handle_key(key('a'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("a"));
    }

    #[test]
    fn replace_mode_overtypes() {
        let mut ed = ed_with("hello");
        ed.handle_key(key('R'));
        assert_eq!(ed.mode, Mode::Replace);
        ed.handle_key(key('J'));
        ed.handle_key(key('A'));
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("JAllo"));
    }

    #[test]
    fn replace_mode_appends_past_eol() {
        let mut ed = ed_with("ab");
        ed.handle_key(key('$')); // on 'b' (col 1)
        ed.handle_key(key('R'));
        ed.handle_key(key('X')); // overwrite 'b' -> "aX"
        ed.handle_key(key('Y')); // past EOL -> append
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("aXY"));
    }

    #[test]
    fn replace_mode_backspace_restores_original() {
        let mut ed = ed_with("cat");
        ed.handle_key(key('R'));
        ed.handle_key(key('X')); // c->X "Xat"
        ed.handle_key(key('Y')); // a->Y "XYt"
        ed.handle_key(special(KeyCode::Backspace)); // restore 'a' -> "Xat"
        ed.handle_key(special(KeyCode::Backspace)); // restore 'c' -> "cat"
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("cat"));
    }

    #[test]
    fn line_motions_plus_minus_underscore() {
        let mut ed = ed_with("a\n  b\n   c\nd");
        ed.handle_key(key('+')); // next line, first non-blank
        assert_eq!(ed.cursor, Position::new(1, 2));
        ed.handle_key(key('+'));
        assert_eq!(ed.cursor, Position::new(2, 3));
        ed.handle_key(key('-')); // prev line, first non-blank
        assert_eq!(ed.cursor, Position::new(1, 2));
        ed.handle_key(key('2'));
        ed.handle_key(key('_')); // down count-1 = 1 line, first non-blank
        assert_eq!(ed.cursor, Position::new(2, 3));
    }

    #[test]
    fn goto_column_bar() {
        let mut ed = ed_with("abcdef");
        ed.handle_key(key('4'));
        ed.handle_key(key('|')); // column 4 (0-based 3)
        assert_eq!(ed.cursor.col, 3);
        ed.handle_key(key('|')); // bare | -> column 1 (0-based 0)
        assert_eq!(ed.cursor.col, 0);
    }

    #[test]
    fn g_underscore_last_nonblank() {
        let mut ed = ed_with("hello   ");
        ed.handle_key(key('g'));
        ed.handle_key(key('_'));
        assert_eq!(ed.cursor.col, 4); // 'o', ignoring trailing spaces
    }

    #[test]
    fn delete_to_next_line_with_plus() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.handle_key(key('d'));
        ed.handle_key(key('+')); // delete current + next line
        assert_eq!(ed.buffer.line(0), Some("c"));
        assert_eq!(ed.buffer.line_count(), 2);
    }

    #[test]
    fn visual_o_swaps_ends() {
        let mut ed = ed_with("hello world");
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // cursor col 2
        ed.handle_key(key('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // anchor 2, cursor 4
        ed.handle_key(key('o')); // swap -> cursor 2, anchor 4
        assert_eq!(ed.cursor.col, 2);
        // Extend left; selection start moves with cursor.
        ed.handle_key(key('h'));
        let (s, e) = ed.selection().unwrap();
        assert_eq!(s.col, 1);
        assert_eq!(e.col, 4);
    }

    #[test]
    fn gv_reselects_last_visual() {
        let mut ed = ed_with("abcdef");
        ed.handle_key(key('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // select cols 0..=2
        ed.handle_key(special(KeyCode::Esc)); // exit visual
        assert_eq!(ed.mode, Mode::Normal);
        ed.handle_key(key('g'));
        ed.handle_key(key('v')); // reselect
        assert_eq!(ed.mode, Mode::Visual);
        let (s, e) = ed.selection().unwrap();
        assert_eq!((s.col, e.col), (0, 2));
    }

    #[test]
    fn gv_reselects_after_operation() {
        let mut ed = ed_with("HELLO");
        ed.handle_key(key('v'));
        ed.handle_key(key('l')); // select "HE"
        ed.handle_key(key('u')); // lowercase -> "heLLO", exits visual
        assert_eq!(ed.buffer.line(0), Some("heLLO"));
        ed.handle_key(key('g'));
        ed.handle_key(key('v')); // reselect same extent
        ed.handle_key(key('U')); // uppercase it back
        assert_eq!(ed.buffer.line(0), Some("HELLO"));
    }

    #[test]
    fn yank_register_zero() {
        let mut ed = ed_with("yanked\ndeleted\ntarget");
        ed.handle_key(key('y'));
        ed.handle_key(key('y')); // yank "yanked" -> "0 and unnamed
        ed.handle_key(key('j'));
        ed.handle_key(key('d'));
        ed.handle_key(key('d')); // delete "deleted" -> "1 and unnamed
        // Unnamed now holds the delete; "0 still holds the yank.
        ed.handle_key(key('"'));
        ed.handle_key(key('0'));
        ed.handle_key(key('p')); // paste "0 (the yank)
        assert_eq!(ed.buffer.line(2), Some("yanked"));
    }

    #[test]
    fn numbered_delete_registers_shift() {
        let mut ed = ed_with("one\ntwo\nthree\nfour");
        ed.handle_key(key('d'));
        ed.handle_key(key('d')); // delete "one" -> "1
        ed.handle_key(key('d'));
        ed.handle_key(key('d')); // delete "two" -> "1, "one" shifts to "2
        // "1 == most recent delete ("two"), "2 == older ("one").
        ed.handle_key(key('"'));
        ed.handle_key(key('1'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(1), Some("two"));
        ed.handle_key(key('"'));
        ed.handle_key(key('2'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(2), Some("one"));
    }

    #[test]
    fn small_delete_register_dash() {
        let mut ed = ed_with("abcdef");
        ed.handle_key(key('x')); // delete 'a' (small) -> "-
        ed.handle_key(key('$'));
        ed.handle_key(key('"'));
        ed.handle_key(key('-'));
        ed.handle_key(key('p')); // paste small-delete register
        assert_eq!(ed.buffer.line(0), Some("bcdefa"));
    }

    #[test]
    fn jumplist_back_and_forward() {
        let mut ed = ed_with("l0\nl1\nl2\nl3\nl4\nl5");
        // Jump around with G/gg (both record jumps).
        ed.handle_key(key('G')); // from (0,0) to last line (row 5); records 0
        assert_eq!(ed.cursor.row, 5);
        ed.handle_key(key('g'));
        ed.handle_key(key('g')); // to row 0; records 5
        assert_eq!(ed.cursor.row, 0);
        // Ctrl-o goes back to the previous jump origin (row 5).
        ed.handle_key(ctrl('o'));
        assert_eq!(ed.cursor.row, 5);
        // Ctrl-o again -> row 0 (the earlier origin).
        ed.handle_key(ctrl('o'));
        assert_eq!(ed.cursor.row, 0);
        // Ctrl-i goes forward again.
        ed.handle_key(ctrl('i'));
        assert_eq!(ed.cursor.row, 5);
    }

    #[test]
    fn jumplist_back_with_no_history_is_noop() {
        let mut ed = ed_with("a\nb\nc");
        ed.cursor = Position::new(1, 0);
        ed.handle_key(ctrl('o')); // nothing recorded yet
        assert_eq!(ed.cursor.row, 1);
    }

    #[test]
    fn shiftwidth_controls_indent() {
        let mut ed = ed_with("code");
        ed.shiftwidth = 2;
        ed.handle_key(key('>'));
        ed.handle_key(key('>'));
        assert_eq!(ed.buffer.line(0), Some("  code")); // 2 spaces
        ed.handle_key(key('<'));
        ed.handle_key(key('<'));
        assert_eq!(ed.buffer.line(0), Some("code"));
    }

    #[test]
    fn noexpandtab_indents_with_tab() {
        let mut ed = ed_with("code");
        ed.expandtab = false;
        ed.handle_key(key('>'));
        ed.handle_key(key('>'));
        assert_eq!(ed.buffer.line(0), Some("\tcode"));
    }

    #[test]
    fn insert_tab_respects_expandtab_and_tabstop() {
        let mut ed = ed_with("");
        ed.tabstop = 3;
        ed.handle_key(key('i'));
        ed.handle_key(special(KeyCode::Tab));
        assert_eq!(ed.buffer.line(0), Some("   ")); // 3 spaces
        let mut ed2 = ed_with("");
        ed2.expandtab = false;
        ed2.handle_key(key('i'));
        ed2.handle_key(special(KeyCode::Tab));
        assert_eq!(ed2.buffer.line(0), Some("\t"));
    }

    #[test]
    fn gj_joins_without_space() {
        let mut ed = ed_with("foo\nbar");
        ed.handle_key(key('g'));
        ed.handle_key(key('J'));
        assert_eq!(ed.buffer.line(0), Some("foobar"));
        // plain J inserts a space
        let mut ed2 = ed_with("foo\nbar");
        ed2.handle_key(key('J'));
        assert_eq!(ed2.buffer.line(0), Some("foo bar"));
    }

    #[test]
    fn paragraph_motions() {
        let mut ed = ed_with("a\nb\n\nc\nd\n\ne");
        ed.handle_key(key('}')); // to first blank (row 2)
        assert_eq!(ed.cursor.row, 2);
        ed.handle_key(key('}')); // to next blank (row 5)
        assert_eq!(ed.cursor.row, 5);
        ed.handle_key(key('{')); // back to blank (row 2)
        assert_eq!(ed.cursor.row, 2);
    }

    #[test]
    fn section_motion_open_and_close_braces() {
        // Allman-style braces in column 0 are vim's default section boundaries.
        // fn a()          row 0
        // {               row 1  (open boundary)
        //     body        row 2
        // }               row 3  (close boundary)
        // fn b()          row 4
        // {               row 5  (open boundary)
        //     body        row 6
        // }               row 7  (close boundary)
        let mut ed = ed_with("fn a()\n{\n    body\n}\nfn b()\n{\n    body\n}");
        ed.handle_key(key(']'));
        ed.handle_key(key(']')); // ]] -> next open-brace line (row 1)
        assert_eq!(ed.cursor.row, 1);
        assert_eq!(ed.cursor.col, 0);
        ed.handle_key(key(']'));
        ed.handle_key(key(']')); // ]] -> next open-brace line (row 5)
        assert_eq!(ed.cursor.row, 5);
        ed.handle_key(key('['));
        ed.handle_key(key('[')); // [[ -> previous open-brace line (row 1)
        assert_eq!(ed.cursor.row, 1);
        ed.handle_key(key(']'));
        ed.handle_key(key('[')); // ][ -> next close-brace line (row 3)
        assert_eq!(ed.cursor.row, 3);
        ed.handle_key(key('['));
        ed.handle_key(key(']')); // [] -> previous close-brace line (none above -> row 0)
        assert_eq!(ed.cursor.row, 0);
    }

    #[test]
    fn section_motion_counts_and_records_jump() {
        let mut ed = ed_with("{\na\n{\nb\n{\nc");
        ed.handle_key(key('2'));
        ed.handle_key(key(']'));
        ed.handle_key(key(']')); // 2]] -> skip to the third open brace (row 4)
        assert_eq!(ed.cursor.row, 4);
        ed.handle_key(ctrl('o')); // jump back to the start
        assert_eq!(ed.cursor.row, 0);
    }

    #[test]
    fn unmatched_paren_motions_single_line() {
        // foo(bar(baz), qux)
        //    ^3     ^7      ^17   open at 3 & 7, close at 11 & 17
        let mut ed = ed_with("foo(bar(baz), qux)");
        ed.cursor = Position::new(0, 9); // inside the inner parens (the 'a' of baz)
        ed.handle_key(key('['));
        ed.handle_key(key('(')); // [( -> inner open paren
        assert_eq!(ed.cursor.col, 7);
        ed.cursor = Position::new(0, 9);
        ed.handle_key(key('2'));
        ed.handle_key(key('['));
        ed.handle_key(key('(')); // 2[( -> outer open paren
        assert_eq!(ed.cursor.col, 3);
        ed.cursor = Position::new(0, 9);
        ed.handle_key(key(']'));
        ed.handle_key(key(')')); // ]) -> inner close paren
        assert_eq!(ed.cursor.col, 11);
        ed.cursor = Position::new(0, 9);
        ed.handle_key(key('2'));
        ed.handle_key(key(']'));
        ed.handle_key(key(')')); // 2]) -> outer close paren
        assert_eq!(ed.cursor.col, 17);
    }

    #[test]
    fn unmatched_brace_motions_multi_line() {
        // fn main() {   row 0, brace at col 10
        //     if x {    row 1, brace at col 9
        //         y;    row 2  (cursor here)
        //     }         row 3, brace at col 4
        // }             row 4, brace at col 0
        let mut ed = ed_with("fn main() {\n    if x {\n        y;\n    }\n}");
        ed.cursor = Position::new(2, 4);
        ed.handle_key(key('['));
        ed.handle_key(key('{')); // [{ -> enclosing open brace
        assert_eq!(ed.cursor, Position::new(1, 9));
        ed.cursor = Position::new(2, 4);
        ed.handle_key(key('2'));
        ed.handle_key(key('['));
        ed.handle_key(key('{')); // 2[{ -> outer open brace, records a jump
        assert_eq!(ed.cursor, Position::new(0, 10));
        ed.handle_key(ctrl('o')); // jump back
        assert_eq!(ed.cursor.row, 2);
        ed.cursor = Position::new(2, 4);
        ed.handle_key(key(']'));
        ed.handle_key(key('}')); // ]} -> enclosing close brace
        assert_eq!(ed.cursor, Position::new(3, 4));
        ed.cursor = Position::new(2, 4);
        ed.handle_key(key('2'));
        ed.handle_key(key(']'));
        ed.handle_key(key('}')); // 2]} -> outer close brace
        assert_eq!(ed.cursor, Position::new(4, 0));
    }

    #[test]
    fn sentence_motion_forward() {
        let mut ed = ed_with("One. Two. Three.");
        ed.handle_key(key(')')); // -> start of "Two"
        assert_eq!(ed.cursor.col, 5);
        ed.handle_key(key(')')); // -> start of "Three"
        assert_eq!(ed.cursor.col, 10);
    }

    #[test]
    fn sentence_motion_backward() {
        let mut ed = ed_with("One. Two. Three.");
        ed.cursor = Position::new(0, 12); // mid "Three"
        ed.handle_key(key('(')); // -> start of current sentence "Three"
        assert_eq!(ed.cursor.col, 10);
        ed.handle_key(key('(')); // already at start -> previous "Two"
        assert_eq!(ed.cursor.col, 5);
    }

    #[test]
    fn sentence_motion_crosses_lines() {
        let mut ed = ed_with("First.\nSecond.");
        ed.handle_key(key(')')); // -> start of the next sentence on line 1
        assert_eq!(ed.cursor, Position::new(1, 0));
    }

    #[test]
    fn das_deletes_sentence_with_trailing_space() {
        let mut ed = ed_with("One. Two. Three.");
        ed.handle_key(key('d'));
        ed.handle_key(key('a'));
        ed.handle_key(key('s')); // das on "One. "
        assert_eq!(ed.buffer.line(0), Some("Two. Three."));
    }

    #[test]
    fn dis_deletes_inner_sentence() {
        let mut ed = ed_with("One. Two.");
        ed.handle_key(key('d'));
        ed.handle_key(key('i'));
        ed.handle_key(key('s')); // dis on "One." keeps the trailing space
        assert_eq!(ed.buffer.line(0), Some(" Two."));
    }

    #[test]
    fn paragraph_text_object_dip() {
        let mut ed = ed_with("a\nb\n\nc");
        ed.handle_key(key('d'));
        ed.handle_key(key('i'));
        ed.handle_key(key('p')); // delete the paragraph "a","b"
        assert_eq!(ed.buffer.line(0), Some(""));
        assert_eq!(ed.buffer.line(1), Some("c"));
    }

    #[test]
    fn paragraph_text_object_dap_eats_trailing_blank() {
        let mut ed = ed_with("a\nb\n\nc");
        ed.handle_key(key('d'));
        ed.handle_key(key('a'));
        ed.handle_key(key('p')); // delete "a","b" + the blank line
        assert_eq!(ed.buffer.line(0), Some("c"));
        assert_eq!(ed.buffer.line_count(), 1);
    }

    #[test]
    fn indent_object_dii_inner_block() {
        let mut ed = ed_with("fn foo():\n    a = 1\n    b = 2\nbar");
        ed.cursor = Position::new(1, 4); // on the first indented line
        ed.handle_key(key('d'));
        ed.handle_key(key('i'));
        ed.handle_key(key('i')); // dii -> delete the indented block only
        assert_eq!(ed.buffer.line(0), Some("fn foo():"));
        assert_eq!(ed.buffer.line(1), Some("bar"));
        assert_eq!(ed.buffer.line_count(), 2);
    }

    #[test]
    fn indent_object_dai_includes_header() {
        let mut ed = ed_with("fn foo():\n    a = 1\n    b = 2\nbar");
        ed.cursor = Position::new(2, 4);
        ed.handle_key(key('d'));
        ed.handle_key(key('a'));
        ed.handle_key(key('i')); // dai -> block plus the `fn foo():` header above
        assert_eq!(ed.buffer.line(0), Some("bar"));
        assert_eq!(ed.buffer.line_count(), 1);
    }

    #[test]
    fn indent_object_keeps_interior_blank_line() {
        let mut ed = ed_with("    a\n\n    b\nc");
        ed.cursor = Position::new(0, 4);
        ed.handle_key(key('d'));
        ed.handle_key(key('i'));
        ed.handle_key(key('i')); // dii -> rows 0..2 incl. the interior blank line
        assert_eq!(ed.buffer.line(0), Some("c"));
        assert_eq!(ed.buffer.line_count(), 1);
    }

    #[test]
    fn indent_object_visual_vii_selects_block() {
        let mut ed = ed_with("def f():\n    x\n    y\nz");
        ed.cursor = Position::new(1, 4);
        ed.handle_key(key('v'));
        ed.handle_key(key('i'));
        ed.handle_key(key('i')); // vii -> visual-line-ish selection of the block
        let (s, e) = ed.selection().unwrap();
        assert_eq!(s.row, 1);
        assert_eq!(e.row, 2);
    }

    #[test]
    fn gf_opens_file_under_cursor() {
        // Tests run from the crate root, where Cargo.toml exists.
        let mut ed = ed_with("see Cargo.toml for config");
        ed.cursor = Position::new(0, 6); // inside "Cargo.toml"
        ed.handle_key(key('g'));
        let act = ed.handle_key(key('f'));
        assert_eq!(act, Action::RunEx("edit Cargo.toml".into()));
    }

    #[test]
    fn g_shift_f_opens_file_at_line() {
        // gF on "Cargo.toml:5" opens the file at line 5.
        let mut ed = ed_with("edit Cargo.toml:5 here");
        ed.cursor = Position::new(0, 7); // inside "Cargo.toml"
        ed.handle_key(key('g'));
        let act = ed.handle_key(special(KeyCode::Char('F')));
        assert_eq!(act, Action::RunEx("edit +5 Cargo.toml".into()));
    }

    #[test]
    fn gf_ignores_trailing_line_suffix() {
        // Plain gf on "Cargo.toml:5" opens the file, ignoring the :5.
        let mut ed = ed_with("edit Cargo.toml:5 here");
        ed.cursor = Position::new(0, 7);
        ed.handle_key(key('g'));
        let act = ed.handle_key(key('f'));
        assert_eq!(act, Action::RunEx("edit Cargo.toml".into()));
    }

    #[test]
    fn gf_reports_missing_file() {
        let mut ed = ed_with("open no_such_file_zzz.xyz now");
        ed.cursor = Position::new(0, 10); // inside the (nonexistent) name
        ed.handle_key(key('g'));
        let act = ed.handle_key(key('f'));
        assert_eq!(act, Action::None);
        assert!(ed.message.contains("Can't find file"));
    }

    #[test]
    fn gf_no_name_under_cursor() {
        let mut ed = ed_with("   ");
        ed.cursor = Position::new(0, 1); // on whitespace
        ed.handle_key(key('g'));
        let act = ed.handle_key(key('f'));
        assert_eq!(act, Action::None);
        assert!(ed.message.contains("No file name"));
    }

    #[test]
    fn tag_object_dit_single_line() {
        let mut ed = ed_with("<a>hi</a>");
        ed.cursor = Position::new(0, 3); // on 'h'
        ed.handle_key(key('d'));
        ed.handle_key(key('i'));
        ed.handle_key(key('t'));
        assert_eq!(ed.buffer.line(0), Some("<a></a>"));
    }

    #[test]
    fn tag_object_dat_single_line() {
        let mut ed = ed_with("<a>hi</a>");
        ed.cursor = Position::new(0, 3);
        ed.handle_key(key('d'));
        ed.handle_key(key('a'));
        ed.handle_key(key('t'));
        assert_eq!(ed.buffer.line(0), Some(""));
    }

    #[test]
    fn tag_object_dit_nested_multiline() {
        let mut ed = ed_with("<div>\n  <p>text</p>\n</div>");
        ed.cursor = Position::new(1, 5); // on 't' of "text", inside <p>
        ed.handle_key(key('d'));
        ed.handle_key(key('i'));
        ed.handle_key(key('t'));
        assert_eq!(ed.buffer.line(1), Some("  <p></p>"));
        assert_eq!(ed.buffer.line_count(), 3);
    }

    #[test]
    fn tag_object_dat_nested_removes_inner_pair() {
        let mut ed = ed_with("<div>\n  <p>text</p>\n</div>");
        ed.cursor = Position::new(1, 5);
        ed.handle_key(key('d'));
        ed.handle_key(key('a'));
        ed.handle_key(key('t'));
        assert_eq!(ed.buffer.line(1), Some("  "));
        assert_eq!(ed.buffer.line_count(), 3);
    }

    #[test]
    fn counted_text_object_d2aw() {
        let mut ed = ed_with("foo bar baz");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('d'));
        ed.handle_key(key('2'));
        ed.handle_key(key('a'));
        ed.handle_key(key('w')); // d2aw -> "foo bar " removed
        assert_eq!(ed.buffer.line(0), Some("baz"));
    }

    #[test]
    fn counted_text_object_d3iw() {
        let mut ed = ed_with("foo bar");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('d'));
        ed.handle_key(key('3'));
        ed.handle_key(key('i'));
        ed.handle_key(key('w')); // 3 segments: foo, space, bar
        assert_eq!(ed.buffer.line(0), Some(""));
    }

    #[test]
    fn counted_text_object_d2ap() {
        let mut ed = ed_with("a\n\nb\n\nc");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('d'));
        ed.handle_key(key('2'));
        ed.handle_key(key('a'));
        ed.handle_key(key('p')); // two paragraphs (+ their trailing blanks)
        assert_eq!(ed.buffer.line(0), Some("c"));
        assert_eq!(ed.buffer.line_count(), 1);
    }

    #[test]
    fn count_before_operator_also_applies_to_object() {
        // `2daw` is equivalent to `d2aw`.
        let mut ed = ed_with("foo bar baz");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('2'));
        ed.handle_key(key('d'));
        ed.handle_key(key('a'));
        ed.handle_key(key('w'));
        assert_eq!(ed.buffer.line(0), Some("baz"));
    }

    #[test]
    fn big_word_motions_w_b_e() {
        let mut ed = ed_with("foo.bar baz.qux");
        ed.handle_key(key('W')); // skip whole WORD "foo.bar" -> start of "baz.qux"
        assert_eq!(ed.cursor.col, 8);
        ed.handle_key(key('B')); // back to start of "foo.bar"
        assert_eq!(ed.cursor.col, 0);
        ed.handle_key(key('E')); // end of WORD "foo.bar"
        assert_eq!(ed.cursor.col, 6);
    }

    #[test]
    fn small_w_stops_at_punctuation() {
        let mut ed = ed_with("foo.bar");
        ed.handle_key(key('w')); // small word stops at '.'
        assert_eq!(ed.cursor.col, 3);
    }

    #[test]
    fn delete_big_word_d_w() {
        let mut ed = ed_with("foo.bar baz");
        ed.handle_key(key('d'));
        ed.handle_key(key('W')); // delete "foo.bar " (WORD + trailing space)
        assert_eq!(ed.buffer.line(0), Some("baz"));
    }

    #[test]
    fn change_big_word_like_ce() {
        let mut ed = ed_with("foo.bar baz");
        ed.handle_key(key('c'));
        ed.handle_key(key('W')); // like cE: change "foo.bar", keep the space
        ed.handle_key(key('X'));
        assert_eq!(ed.buffer.line(0), Some("X baz"));
    }

    #[test]
    fn capital_x_deletes_before_cursor() {
        let mut ed = ed_with("abcd");
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // col 2
        ed.handle_key(key('X')); // delete 'b'
        assert_eq!(ed.buffer.line(0), Some("acd"));
    }

    #[test]
    fn capital_y_yanks_lines() {
        let mut ed = ed_with("one\ntwo\nthree");
        ed.handle_key(key('2'));
        ed.handle_key(key('Y')); // yank 2 lines
        ed.handle_key(key('G'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(3), Some("one"));
        assert_eq!(ed.buffer.line(4), Some("two"));
    }

    #[test]
    fn count_gg_goes_to_line() {
        let mut ed = ed_with("l0\nl1\nl2\nl3\nl4");
        ed.handle_key(key('3'));
        ed.handle_key(key('g'));
        ed.handle_key(key('g')); // 3gg -> line 3 (row 2)
        assert_eq!(ed.cursor.row, 2);
    }

    #[test]
    fn operator_dgg_deletes_to_top() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.handle_key(key('G')); // last line (row 3)
        ed.handle_key(key('k')); // row 2
        ed.handle_key(key('d'));
        ed.handle_key(key('g'));
        ed.handle_key(key('g')); // delete rows 0..=2
        assert_eq!(ed.buffer.line_count(), 1);
        assert_eq!(ed.buffer.line(0), Some("d"));
    }

    #[test]
    fn count_replace_3r() {
        let mut ed = ed_with("aaaa");
        ed.handle_key(key('3'));
        ed.handle_key(key('r'));
        ed.handle_key(key('x')); // replace 3 chars
        assert_eq!(ed.buffer.line(0), Some("xxxa"));
    }

    #[test]
    fn count_tilde_toggles_n_chars() {
        let mut ed = ed_with("abcd");
        ed.handle_key(key('3'));
        ed.handle_key(key('~')); // toggle 3 chars
        assert_eq!(ed.buffer.line(0), Some("ABCd"));
        assert_eq!(ed.cursor.col, 3);
    }

    #[test]
    fn shift_operator_with_motion() {
        let mut ed = ed_with("a\nb\nc");
        ed.handle_key(key('>'));
        ed.handle_key(key('j')); // indent 2 lines
        assert_eq!(ed.buffer.line(0), Some("    a"));
        assert_eq!(ed.buffer.line(1), Some("    b"));
        assert_eq!(ed.buffer.line(2), Some("c"));
    }

    #[test]
    fn count_shift_lines() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.handle_key(key('3'));
        ed.handle_key(key('>'));
        ed.handle_key(key('>')); // 3>> indent 3 lines
        assert_eq!(ed.buffer.line(0), Some("    a"));
        assert_eq!(ed.buffer.line(2), Some("    c"));
        assert_eq!(ed.buffer.line(3), Some("d"));
    }

    #[test]
    fn target_rows_maps_each_variant() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.cursor = Position::new(2, 0);
        assert_eq!(ed.target_rows(OpTarget::Lines(1, 3)), (1, 3));
        assert_eq!(ed.target_rows(OpTarget::Chars(0, 4)), (2, 2)); // cursor row
        assert_eq!(
            ed.target_rows(OpTarget::Span(Position::new(0, 2), Position::new(3, 1))),
            (0, 3)
        );
    }

    #[test]
    fn shift_operator_with_paragraph_object() {
        let mut ed = ed_with("a\nb\n\nc");
        ed.handle_key(key('>'));
        ed.handle_key(key('i'));
        ed.handle_key(key('p')); // >ip indents the paragraph (lines 0-1)
        assert_eq!(ed.buffer.line(0), Some("    a"));
        assert_eq!(ed.buffer.line(1), Some("    b"));
        assert_eq!(ed.buffer.line(2), Some(""));
    }

    #[test]
    fn shift_operator_with_multiline_brace_object() {
        let mut ed = ed_with("{\nx\ny\n}");
        ed.cursor = Position::new(1, 0);
        ed.handle_key(key('>'));
        ed.handle_key(key('i'));
        ed.handle_key(key('{')); // >i{ indents the inner block
        assert_eq!(ed.buffer.line(0), Some("{"));
        assert_eq!(ed.buffer.line(1), Some("    x"));
        assert_eq!(ed.buffer.line(2), Some("    y"));
    }

    #[test]
    fn dedent_operator_with_paragraph_object() {
        let mut ed = ed_with("    a\n    b\n\nc");
        ed.handle_key(key('<'));
        ed.handle_key(key('i'));
        ed.handle_key(key('p')); // <ip dedents the paragraph
        assert_eq!(ed.buffer.line(0), Some("a"));
        assert_eq!(ed.buffer.line(1), Some("b"));
    }

    #[test]
    fn visual_x_deletes_selection() {
        let mut ed = ed_with("hello");
        ed.handle_key(key('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // select "hel"
        ed.handle_key(key('x'));
        assert_eq!(ed.buffer.line(0), Some("lo"));
    }

    #[test]
    fn visual_text_object_viw() {
        let mut ed = ed_with("foo bar baz");
        ed.cursor = Position::new(0, 4); // on "bar"
        ed.handle_key(key('v'));
        ed.handle_key(key('i'));
        ed.handle_key(key('w')); // viw selects "bar"
        assert_eq!(ed.mode, Mode::Visual);
        assert_eq!(ed.visual_anchor, Position::new(0, 4));
        assert_eq!(ed.cursor, Position::new(0, 6));
        ed.handle_key(key('d'));
        assert_eq!(ed.buffer.line(0), Some("foo  baz"));
    }

    #[test]
    fn visual_text_object_vi_parens() {
        let mut ed = ed_with("foo(bar)baz");
        ed.cursor = Position::new(0, 5); // inside parens
        ed.handle_key(key('v'));
        ed.handle_key(key('i'));
        ed.handle_key(key('(')); // vi( selects "bar"
        assert_eq!(ed.visual_anchor, Position::new(0, 4));
        assert_eq!(ed.cursor, Position::new(0, 6));
        ed.handle_key(key('d'));
        assert_eq!(ed.buffer.line(0), Some("foo()baz"));
    }

    #[test]
    fn visual_text_object_vap_spans_paragraph() {
        let mut ed = ed_with("a\nb\n\nc");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('v'));
        ed.handle_key(key('a'));
        ed.handle_key(key('p')); // vap selects the paragraph + trailing blank
        assert_eq!(ed.visual_anchor, Position::new(0, 0));
        assert_eq!(ed.cursor.row, 2);
    }

    #[test]
    fn text_object_a_big_w() {
        let mut ed = ed_with("foo.bar baz");
        ed.handle_key(key('d'));
        ed.handle_key(key('a'));
        ed.handle_key(key('W')); // delete a WORD "foo.bar " incl trailing space
        assert_eq!(ed.buffer.line(0), Some("baz"));
    }

    #[test]
    fn change_word_behaves_like_ce() {
        // vim: `cw` acts like `ce` — it does NOT eat the trailing space.
        let mut ed = ed_with("foo bar");
        ed.handle_key(key('c'));
        ed.handle_key(key('w'));
        assert_eq!(ed.mode, Mode::Insert);
        ed.handle_key(key('X'));
        assert_eq!(ed.buffer.line(0), Some("X bar"));
    }

    #[test]
    fn dd_and_yy_still_work() {
        let mut ed = ed_with("one\ntwo\nthree");
        ed.handle_key(key('d'));
        ed.handle_key(key('d'));
        assert_eq!(ed.buffer.line(0), Some("two"));
        ed.handle_key(key('y'));
        ed.handle_key(key('y'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(1), Some("two"));
    }

    #[test]
    fn autoindent_on_enter() {
        let mut ed = ed_with("    code");
        ed.handle_key(key('A')); // append at end of line
        ed.handle_key(special(KeyCode::Enter));
        ed.handle_key(key('x'));
        assert_eq!(ed.buffer.line(1), Some("    x"));
    }

    #[test]
    fn no_autoindent_when_disabled() {
        let mut ed = ed_with("    code");
        ed.autoindent = false;
        ed.handle_key(key('A'));
        ed.handle_key(special(KeyCode::Enter));
        ed.handle_key(key('x'));
        assert_eq!(ed.buffer.line(1), Some("x"));
    }

    #[test]
    fn insert_ctrl_w_deletes_word_before() {
        let mut ed = ed_with("foo bar");
        ed.handle_key(key('A')); // insert at end
        ed.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        assert_eq!(ed.buffer.line(0), Some("foo "));
    }

    #[test]
    fn insert_ctrl_u_deletes_to_line_start() {
        let mut ed = ed_with("hello");
        ed.handle_key(key('l'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // col 3
        ed.handle_key(key('i')); // insert before col 3
        ed.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(ed.buffer.line(0), Some("lo"));
        assert_eq!(ed.cursor.col, 0);
    }

    #[test]
    fn named_register_yank_and_paste() {
        let mut ed = ed_with("alpha\nbeta\ngamma");
        // Yank line 0 into register a.
        ed.handle_key(key('"'));
        ed.handle_key(key('a'));
        ed.handle_key(key('y'));
        ed.handle_key(key('y'));
        // Move down and paste from register a.
        ed.handle_key(key('j'));
        ed.handle_key(key('"'));
        ed.handle_key(key('a'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(2), Some("alpha"));
    }

    #[test]
    fn uppercase_register_appends_linewise() {
        let mut ed = ed_with("one\ntwo\nthree");
        ed.handle_key(key('"'));
        ed.handle_key(key('a'));
        ed.handle_key(key('y'));
        ed.handle_key(key('y')); // "ayy -> reg a = "one"
        ed.handle_key(key('j'));
        ed.handle_key(key('"'));
        ed.handle_key(key('A'));
        ed.handle_key(key('y'));
        ed.handle_key(key('y')); // "Ayy -> append "two"
        ed.handle_key(key('G'));
        ed.handle_key(key('"'));
        ed.handle_key(key('a'));
        ed.handle_key(key('p')); // paste both appended lines
        assert_eq!(ed.buffer.line(3), Some("one"));
        assert_eq!(ed.buffer.line(4), Some("two"));
    }

    #[test]
    fn uppercase_register_appends_charwise() {
        let mut ed = ed_with("abcdef");
        ed.handle_key(key('"'));
        ed.handle_key(key('a'));
        ed.handle_key(key('y'));
        ed.handle_key(key('l')); // "ayl -> reg a = "a"
        ed.handle_key(key('l'));
        ed.handle_key(key('"'));
        ed.handle_key(key('A'));
        ed.handle_key(key('y'));
        ed.handle_key(key('l')); // "Ayl -> append "b" => "ab"
        ed.handle_key(key('$'));
        ed.handle_key(key('"'));
        ed.handle_key(key('a'));
        ed.handle_key(key('p')); // paste "ab" after 'f'
        assert_eq!(ed.buffer.line(0), Some("abcdefab"));
    }

    #[test]
    fn uppercase_register_reads_lowercase() {
        let mut ed = ed_with("hello\nx");
        ed.handle_key(key('"'));
        ed.handle_key(key('a'));
        ed.handle_key(key('y'));
        ed.handle_key(key('y')); // reg a = "hello"
        ed.handle_key(key('j'));
        ed.handle_key(key('"'));
        ed.handle_key(key('A')); // reading "A resolves to register a
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(2), Some("hello"));
    }

    #[test]
    fn named_register_independent_from_unnamed() {
        let mut ed = ed_with("keep\nother");
        // Yank "keep" into register a.
        ed.handle_key(key('"'));
        ed.handle_key(key('a'));
        ed.handle_key(key('y'));
        ed.handle_key(key('y'));
        // Now yank "other" into the unnamed register.
        ed.handle_key(key('j'));
        ed.handle_key(key('y'));
        ed.handle_key(key('y'));
        // Unnamed paste yields "other"; register a still holds "keep".
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(2), Some("other"));
        ed.handle_key(key('"'));
        ed.handle_key(key('a'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(3), Some("keep"));
    }

    #[test]
    fn visual_uppercase_selection() {
        let mut ed = ed_with("hello world");
        ed.handle_key(key('v'));
        for _ in 0..4 {
            ed.handle_key(key('l')); // select "hello"
        }
        ed.handle_key(key('U'));
        assert_eq!(ed.buffer.line(0), Some("HELLO world"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn visual_line_lowercase_and_toggle() {
        let mut ed = ed_with("MixedCase");
        ed.handle_key(key('V'));
        ed.handle_key(key('u'));
        assert_eq!(ed.buffer.line(0), Some("mixedcase"));
        ed.handle_key(key('V'));
        ed.handle_key(key('~'));
        assert_eq!(ed.buffer.line(0), Some("MIXEDCASE"));
    }

    #[test]
    fn sort_buffer_ascending_and_reverse() {
        let mut ed = ed_with("banana\napple\ncherry");
        ed.sort_lines(SubRange::WholeFile, false, false, false, false, None, false);
        assert_eq!(ed.buffer.line(0), Some("apple"));
        assert_eq!(ed.buffer.line(1), Some("banana"));
        assert_eq!(ed.buffer.line(2), Some("cherry"));
        ed.sort_lines(SubRange::WholeFile, true, false, false, false, None, false);
        assert_eq!(ed.buffer.line(0), Some("cherry"));
        assert_eq!(ed.buffer.line(2), Some("apple"));
    }

    #[test]
    fn sort_buffer_unique_removes_duplicates() {
        let mut ed = ed_with("b\na\nb\nc\na");
        ed.sort_lines(SubRange::WholeFile, false, true, false, false, None, false);
        assert_eq!(ed.buffer.line_count(), 3);
        assert_eq!(ed.buffer.line(0), Some("a"));
        assert_eq!(ed.buffer.line(1), Some("b"));
        assert_eq!(ed.buffer.line(2), Some("c"));
    }

    #[test]
    fn sort_lines_range_only() {
        let mut ed = ed_with("z\nc\na\nb\nq");
        // :2,4sort -> sort only rows 1..=3 (c,a,b), leaving z and q in place
        ed.sort_lines(
            SubRange::Range(LineAddr::Num(2), LineAddr::Num(4)),
            false,
            false,
            false,
            false,
            None,
            false,
        );
        assert_eq!(ed.buffer.line(0), Some("z"));
        assert_eq!(ed.buffer.line(1), Some("a"));
        assert_eq!(ed.buffer.line(2), Some("b"));
        assert_eq!(ed.buffer.line(3), Some("c"));
        assert_eq!(ed.buffer.line(4), Some("q"));
    }

    #[test]
    fn sort_buffer_numeric() {
        let mut ed = ed_with("item 10\nitem 2\nitem 100\nitem 9");
        ed.sort_lines(SubRange::WholeFile, false, false, true, false, None, false);
        assert_eq!(ed.buffer.line(0), Some("item 2"));
        assert_eq!(ed.buffer.line(1), Some("item 9"));
        assert_eq!(ed.buffer.line(2), Some("item 10"));
        assert_eq!(ed.buffer.line(3), Some("item 100"));
    }

    #[test]
    fn sort_buffer_ignorecase() {
        let mut ed = ed_with("Banana\napple\nCherry");
        ed.sort_lines(SubRange::WholeFile, false, false, false, true, None, false);
        assert_eq!(ed.buffer.line(0), Some("apple"));
        assert_eq!(ed.buffer.line(1), Some("Banana"));
        assert_eq!(ed.buffer.line(2), Some("Cherry"));
    }

    #[test]
    fn sort_by_text_after_pattern() {
        let mut ed = ed_with("x9\nx1\nx5");
        // Sort by what follows "x".
        ed.sort_lines(SubRange::WholeFile, false, false, false, false, Some("x".into()), false);
        assert_eq!(ed.buffer.line(0), Some("x1"));
        assert_eq!(ed.buffer.line(1), Some("x5"));
        assert_eq!(ed.buffer.line(2), Some("x9"));
    }

    #[test]
    fn sort_by_matched_text_with_r_flag() {
        let mut ed = ed_with("3-zzz\n1-aaa\n2-mmm");
        // Sort on the matched digit itself (the `r` flag).
        ed.sort_lines(SubRange::WholeFile, false, false, false, false, Some("\\d".into()), true);
        assert_eq!(ed.buffer.line(0), Some("1-aaa"));
        assert_eq!(ed.buffer.line(1), Some("2-mmm"));
        assert_eq!(ed.buffer.line(2), Some("3-zzz"));
    }

    #[test]
    fn percent_matches_brackets() {
        let mut ed = ed_with("(a+b)");
        ed.handle_key(key('%'));
        assert_eq!(ed.cursor.col, 4); // ( -> )
        ed.handle_key(key('%'));
        assert_eq!(ed.cursor.col, 0); // ) -> (
    }

    #[test]
    fn percent_nested_brackets() {
        let mut ed = ed_with("(a(b)c)");
        ed.handle_key(key('%'));
        assert_eq!(ed.cursor.col, 6); // outer ( -> outer )
    }

    #[test]
    fn percent_scans_forward_to_bracket_on_line() {
        let mut ed = ed_with("x = (1)");
        ed.handle_key(key('%')); // cursor at 0, not a bracket -> finds ( then matches )
        assert_eq!(ed.cursor.col, 6);
    }

    #[test]
    fn current_match_identifies_match_under_cursor() {
        let mut ed = ed_with("foo bar foo");
        ed.set_search("foo".into());
        ed.cursor = Position::new(0, 8); // on the second "foo"
        assert_eq!(ed.current_match(), Some((8, 11)));
        ed.cursor = Position::new(0, 0); // on the first "foo"
        assert_eq!(ed.current_match(), Some((0, 3)));
        ed.cursor = Position::new(0, 4); // on "bar", not a match
        assert_eq!(ed.current_match(), None);
        ed.hlsearch = false; // highlighting off -> no current match
        ed.cursor = Position::new(0, 0);
        assert_eq!(ed.current_match(), None);
    }

    #[test]
    fn split_search_offset_parses_specs() {
        assert_eq!(split_search_offset("foo/e"), ("foo".into(), Some(SearchOffset::End(0))));
        assert_eq!(split_search_offset("foo/e+1"), ("foo".into(), Some(SearchOffset::End(1))));
        assert_eq!(split_search_offset("foo/+2"), ("foo".into(), Some(SearchOffset::Line(2))));
        assert_eq!(split_search_offset("foo/s-1"), ("foo".into(), Some(SearchOffset::Start(-1))));
        assert_eq!(split_search_offset("a/b"), ("a".into(), Some(SearchOffset::Start(0))));
        assert_eq!(split_search_offset("plain"), ("plain".into(), None));
    }

    #[test]
    fn search_offset_end_lands_on_last_char() {
        let mut ed = ed_with("xx foo yy");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('/'));
        for c in "foo/e".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Enter));
        assert_eq!(ed.cursor.col, 5); // last char of "foo"
    }

    #[test]
    fn search_offset_line_jumps_below() {
        let mut ed = ed_with("a\nbar\nc\nd");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('/'));
        for c in "bar/+1".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Enter));
        assert_eq!(ed.cursor.row, 2); // one line below the match
    }

    #[test]
    fn search_offset_reused_by_n() {
        let mut ed = ed_with("foo x\nfoo y\nfoo z");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('/'));
        for c in "foo/e".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Enter));
        assert_eq!(ed.cursor, Position::new(1, 2)); // first match after origin, at end
        ed.handle_key(key('n'));
        assert_eq!(ed.cursor, Position::new(2, 2)); // offset reapplied
    }

    #[test]
    fn percent_matches_across_lines() {
        let mut ed = ed_with("foo(\n  bar\n)");
        // move cursor onto the '(' at row 0 col 3
        ed.cursor = Position::new(0, 3);
        ed.handle_key(key('%'));
        assert_eq!(ed.cursor, Position::new(2, 0));
    }

    #[test]
    fn find_char_f_and_t() {
        let mut ed = ed_with("hello world");
        ed.handle_key(key('f'));
        ed.handle_key(key('o'));
        assert_eq!(ed.cursor.col, 4);
        let mut ed2 = ed_with("hello world");
        ed2.handle_key(key('t'));
        ed2.handle_key(key('o'));
        assert_eq!(ed2.cursor.col, 3);
    }

    #[test]
    fn find_char_f_backward() {
        let mut ed = ed_with("hello");
        ed.handle_key(key('$')); // col 4 ('o')
        ed.handle_key(key('F'));
        ed.handle_key(key('l'));
        assert_eq!(ed.cursor.col, 3);
    }

    #[test]
    fn repeat_find_semicolon_and_comma() {
        let mut ed = ed_with("o.o.o");
        ed.handle_key(key('f'));
        ed.handle_key(key('o')); // col 2
        assert_eq!(ed.cursor.col, 2);
        ed.handle_key(key(';')); // next o -> col 4
        assert_eq!(ed.cursor.col, 4);
        ed.handle_key(key(',')); // reverse -> col 2
        assert_eq!(ed.cursor.col, 2);
    }

    #[test]
    fn find_char_with_count() {
        let mut ed = ed_with("a1a2a3a4");
        ed.handle_key(key('3'));
        ed.handle_key(key('f'));
        ed.handle_key(key('a')); // 3fa -> 3rd 'a' after col 0 (col 6)
        assert_eq!(ed.cursor.col, 6);
    }

    #[test]
    fn till_char_with_count() {
        let mut ed = ed_with("xaxaxa");
        ed.handle_key(key('2'));
        ed.handle_key(key('t'));
        ed.handle_key(key('a')); // 2ta -> just before the 2nd 'a' (col 2)
        assert_eq!(ed.cursor.col, 2);
    }

    #[test]
    fn repeat_find_with_count() {
        let mut ed = ed_with("o.o.o.o");
        ed.handle_key(key('f'));
        ed.handle_key(key('o')); // col 2
        ed.handle_key(key('2'));
        ed.handle_key(key(';')); // 2; -> skip to the 3rd-from-here 'o' (col 6)
        assert_eq!(ed.cursor.col, 6);
    }

    #[test]
    fn operator_find_forward() {
        let mut ed = ed_with("foo(bar)baz");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('d'));
        ed.handle_key(key('f'));
        ed.handle_key(key(')')); // df) deletes "foo(bar)"
        assert_eq!(ed.buffer.line(0), Some("baz"));
    }

    #[test]
    fn operator_till_change_enters_insert() {
        let mut ed = ed_with("foo(bar)baz");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('c'));
        ed.handle_key(key('t'));
        ed.handle_key(key('(')); // ct( changes up to before '('
        assert_eq!(ed.mode, Mode::Insert);
        assert_eq!(ed.buffer.line(0), Some("(bar)baz"));
    }

    #[test]
    fn operator_find_backward() {
        let mut ed = ed_with("abcXdef");
        ed.cursor = Position::new(0, 6); // on 'f'
        ed.handle_key(key('d'));
        ed.handle_key(key('F'));
        ed.handle_key(key('X')); // dFX deletes "Xde" (X up to before cursor)
        assert_eq!(ed.buffer.line(0), Some("abcf"));
    }

    #[test]
    fn word_end_motion() {
        let mut ed = ed_with("foo bar");
        ed.handle_key(key('e'));
        assert_eq!(ed.cursor.col, 2);
        ed.handle_key(key('e'));
        assert_eq!(ed.cursor.col, 6);
    }

    #[test]
    fn delete_to_eol_with_d() {
        let mut ed = ed_with("hello world");
        ed.handle_key(key('5'));
        ed.handle_key(key('l')); // col 5
        ed.handle_key(key('D'));
        assert_eq!(ed.buffer.line(0), Some("hello"));
    }

    #[test]
    fn change_to_eol_with_c() {
        let mut ed = ed_with("hello world");
        ed.handle_key(key('5'));
        ed.handle_key(key('l')); // col 5
        ed.handle_key(key('C'));
        assert_eq!(ed.mode, Mode::Insert);
        ed.handle_key(key('!'));
        assert_eq!(ed.buffer.line(0), Some("hello!"));
    }

    #[test]
    fn toggle_case_tilde() {
        let mut ed = ed_with("aBc");
        ed.handle_key(key('~'));
        assert_eq!(ed.buffer.line(0), Some("ABc"));
        assert_eq!(ed.cursor.col, 1);
    }

    #[test]
    fn indent_and_dedent_line() {
        let mut ed = ed_with("code");
        ed.handle_key(key('>'));
        ed.handle_key(key('>'));
        assert_eq!(ed.buffer.line(0), Some("    code"));
        ed.handle_key(key('<'));
        ed.handle_key(key('<'));
        assert_eq!(ed.buffer.line(0), Some("code"));
    }

    #[test]
    fn visual_line_indent() {
        let mut ed = ed_with("a\nb\nc");
        ed.handle_key(key('V'));
        ed.handle_key(key('j'));
        ed.handle_key(key('>'));
        assert_eq!(ed.buffer.line(0), Some("    a"));
        assert_eq!(ed.buffer.line(1), Some("    b"));
        assert_eq!(ed.buffer.line(2), Some("c"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn substitute_char_s() {
        let mut ed = ed_with("cat");
        ed.handle_key(key('s'));
        assert_eq!(ed.mode, Mode::Insert);
        ed.handle_key(key('b'));
        assert_eq!(ed.buffer.line(0), Some("bat"));
    }

    #[test]
    fn substitute_line_s_keeps_indent() {
        let mut ed = ed_with("    keep");
        ed.handle_key(key('S'));
        assert_eq!(ed.mode, Mode::Insert);
        ed.handle_key(key('x'));
        assert_eq!(ed.buffer.line(0), Some("    x"));
    }

    #[test]
    fn substitute_regex_digits() {
        let mut ed = ed_with("item12 and item345");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: r"\d+".into(),
            replacement: "#".into(),
            global: true,
            ignorecase: false, count_only: false,
        };
        let (subs, _) = ed.substitute(&spec);
        assert_eq!(subs, 2);
        assert_eq!(ed.buffer.line(0), Some("item# and item#"));
    }

    fn confirm_spec(pattern: &str, replacement: &str, global: bool) -> SubstituteSpec {
        SubstituteSpec {
            range: SubRange::WholeFile,
            pattern: pattern.into(),
            replacement: replacement.into(),
            global,
            ignorecase: false,
            count_only: false,
        }
    }

    #[test]
    fn subst_confirm_yes_replaces_each() {
        let mut ed = ed_with("foo foo\nfoo");
        ed.substitute_confirm_start(&confirm_spec("foo", "X", true));
        assert!(ed.substitute_confirm_active());
        ed.handle_key(key('y'));
        ed.handle_key(key('y'));
        ed.handle_key(key('y'));
        assert!(!ed.substitute_confirm_active());
        assert_eq!(ed.buffer.line(0), Some("X X"));
        assert_eq!(ed.buffer.line(1), Some("X"));
    }

    #[test]
    fn subst_confirm_no_skips() {
        let mut ed = ed_with("foo foo");
        ed.substitute_confirm_start(&confirm_spec("foo", "X", true));
        ed.handle_key(key('n')); // skip first
        ed.handle_key(key('y')); // replace second
        assert_eq!(ed.buffer.line(0), Some("foo X"));
    }

    #[test]
    fn subst_confirm_quit_stops() {
        let mut ed = ed_with("foo foo");
        ed.substitute_confirm_start(&confirm_spec("foo", "X", true));
        ed.handle_key(key('q'));
        assert!(!ed.substitute_confirm_active());
        assert_eq!(ed.buffer.line(0), Some("foo foo"));
    }

    #[test]
    fn subst_confirm_all_replaces_remaining() {
        let mut ed = ed_with("foo foo\nfoo");
        ed.substitute_confirm_start(&confirm_spec("foo", "X", true));
        ed.handle_key(key('a'));
        assert!(!ed.substitute_confirm_active());
        assert_eq!(ed.buffer.line(0), Some("X X"));
        assert_eq!(ed.buffer.line(1), Some("X"));
    }

    #[test]
    fn subst_confirm_last_replaces_one_then_stops() {
        let mut ed = ed_with("foo foo");
        ed.substitute_confirm_start(&confirm_spec("foo", "X", true));
        ed.handle_key(key('l'));
        assert!(!ed.substitute_confirm_active());
        assert_eq!(ed.buffer.line(0), Some("X foo"));
    }

    #[test]
    fn subst_confirm_nonglobal_first_per_line() {
        let mut ed = ed_with("foo foo\nfoo foo");
        ed.substitute_confirm_start(&confirm_spec("foo", "X", false));
        ed.handle_key(key('y'));
        ed.handle_key(key('y'));
        assert_eq!(ed.buffer.line(0), Some("X foo"));
        assert_eq!(ed.buffer.line(1), Some("X foo"));
    }

    #[test]
    fn subst_confirm_zero_width_multibyte_terminates() {
        // A pattern that can match empty, on a line with a multibyte char, must
        // make forward progress on char boundaries and not panic or loop.
        let mut ed = ed_with("a\u{e9}b");
        ed.substitute_confirm_start(&confirm_spec("x*", "-", true));
        ed.handle_key(key('a')); // replace all remaining
        assert!(!ed.substitute_confirm_active());
    }

    #[test]
    fn subst_confirm_undo_reverts_all() {
        let mut ed = ed_with("foo foo");
        ed.substitute_confirm_start(&confirm_spec("foo", "X", true));
        ed.handle_key(key('y'));
        ed.handle_key(key('y'));
        assert_eq!(ed.buffer.line(0), Some("X X"));
        ed.handle_key(key('u')); // single undo reverts the whole :s///c
        assert_eq!(ed.buffer.line(0), Some("foo foo"));
    }

    #[test]
    fn substitute_empty_pattern_reuses_last_search() {
        let mut ed = ed_with("foo foo\nbar");
        ed.last_search = "foo".into();
        let spec = SubstituteSpec {
            range: SubRange::WholeFile,
            pattern: String::new(), // empty -> reuse "foo"
            replacement: "X".into(),
            global: true,
            ignorecase: false,
            count_only: false,
        };
        let (subs, _) = ed.substitute(&spec);
        assert_eq!(subs, 2);
        assert_eq!(ed.buffer.line(0), Some("X X"));
    }

    #[test]
    fn substitute_sets_last_search() {
        let mut ed = ed_with("alpha beta");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: "beta".into(),
            replacement: "Z".into(),
            global: false,
            ignorecase: false,
            count_only: false,
        };
        ed.substitute(&spec);
        assert_eq!(ed.last_search, "beta"); // :s updates the search pattern
    }

    #[test]
    fn global_delete_matching_lines() {
        let mut ed = ed_with("keep\nDROP me\nkeep\nDROP again");
        let affected = ed.global("DROP", false, "d");
        assert_eq!(affected, 2);
        assert_eq!(ed.buffer.line(0), Some("keep"));
        assert_eq!(ed.buffer.line(1), Some("keep"));
        assert_eq!(ed.buffer.line_count(), 2);
    }

    #[test]
    fn global_empty_pattern_reuses_last_search() {
        let mut ed = ed_with("keep\nDROP me\nkeep\nDROP again");
        ed.last_search = "DROP".into();
        let affected = ed.global("", false, "d"); // empty pattern -> reuse "DROP"
        assert_eq!(affected, 2);
        assert_eq!(ed.buffer.line_count(), 2);
    }

    #[test]
    fn global_sets_last_search() {
        let mut ed = ed_with("alpha\nbeta\nalpha");
        ed.global("alpha", false, "d");
        assert_eq!(ed.last_search, "alpha"); // :g records the search pattern
    }

    #[test]
    fn global_invert_delete() {
        let mut ed = ed_with("a\nkeep1\nb\nkeep2");
        ed.global("keep", true, "d"); // delete non-matching
        assert_eq!(ed.buffer.line(0), Some("keep1"));
        assert_eq!(ed.buffer.line(1), Some("keep2"));
        assert_eq!(ed.buffer.line_count(), 2);
    }

    #[test]
    fn global_substitute_on_matching_lines() {
        let mut ed = ed_with("foo 1\nbar 1\nfoo 1");
        let n = ed.global("foo", false, "s/1/9/");
        assert_eq!(n, 2);
        assert_eq!(ed.buffer.line(0), Some("foo 9"));
        assert_eq!(ed.buffer.line(1), Some("bar 1")); // not matched by g
        assert_eq!(ed.buffer.line(2), Some("foo 9"));
    }

    #[test]
    fn substitute_ignorecase_flag() {
        let mut ed = ed_with("Foo FOO foo");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: "foo".into(),
            replacement: "x".into(),
            global: true,
            ignorecase: true, count_only: false,
        };
        let (subs, _) = ed.substitute(&spec);
        assert_eq!(subs, 3);
        assert_eq!(ed.buffer.line(0), Some("x x x"));
    }

    #[test]
    fn substitute_vim_capture_group() {
        // vim-style backrefs: \1 \2 \3
        let mut ed = ed_with("2026-09-30");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: r"(\d+)-(\d+)-(\d+)".into(),
            replacement: r"\3/\2/\1".into(),
            global: false,
            ignorecase: false, count_only: false,
        };
        let (subs, _) = ed.substitute(&spec);
        assert_eq!(subs, 1);
        assert_eq!(ed.buffer.line(0), Some("30/09/2026"));
    }

    #[test]
    fn repeat_substitute_with_ampersand() {
        let mut ed = ed_with("foo foo\nfoo foo");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: "foo".into(),
            replacement: "bar".into(),
            global: true,
            ignorecase: false, count_only: false,
        };
        ed.substitute(&spec); // line 0 -> "bar bar"
        assert_eq!(ed.buffer.line(0), Some("bar bar"));
        ed.handle_key(key('j'));
        ed.handle_key(key('&')); // repeat on line 1
        assert_eq!(ed.buffer.line(1), Some("bar bar"));
    }

    #[test]
    fn substitute_invalid_regex_matches_literally() {
        let mut ed = ed_with("a (b) c");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: "(b".into(), // invalid regex -> literal
            replacement: "X".into(),
            global: false,
            ignorecase: false, count_only: false,
        };
        let (subs, _) = ed.substitute(&spec);
        assert_eq!(subs, 1);
        assert_eq!(ed.buffer.line(0), Some("a X) c"));
    }

    fn menu_ed() -> Editor {
        let mut ed = ed_with("hello");
        ed.open_menu(crate::menu::build_menus(&["matrix"], &["wordcount"]));
        ed
    }

    #[test]
    fn menu_open_select_pastes_into_command_line() {
        let mut ed = menu_ed();
        assert!(ed.is_menu_open());
        ed.handle_key(special(KeyCode::Down)); // open File dropdown
        ed.handle_key(special(KeyCode::Enter)); // select "Write" (w)
        assert!(!ed.is_menu_open());
        assert_eq!(ed.mode, Mode::Command);
        assert_eq!(ed.cmdline, "w");
    }

    #[test]
    fn menu_esc_backs_out_then_closes() {
        let mut ed = menu_ed();
        ed.handle_key(special(KeyCode::Down)); // dropdown open (depth 1)
        ed.handle_key(special(KeyCode::Esc)); // back to bar only
        assert!(ed.is_menu_open());
        ed.handle_key(special(KeyCode::Esc)); // close
        assert!(!ed.is_menu_open());
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn menu_mouse_select_runs_command() {
        let mut ed = menu_ed();
        ed.handle_key(special(KeyCode::Down)); // open File dropdown (level 0)
        ed.menu_mouse_select(0, 2); // click "Write & Quit" (wq)
        assert!(!ed.is_menu_open());
        assert_eq!(ed.mode, Mode::Command);
        assert_eq!(ed.cmdline, "wq");
    }

    #[test]
    fn menu_mouse_select_opens_submenu() {
        let mut ed = menu_ed();
        ed.menu_open_initial('v'); // View dropdown open at "Theme" (a submenu)
        ed.menu_mouse_select(0, 0); // click "Theme"
        assert!(ed.is_menu_open());
        assert_eq!(ed.menu().unwrap().depth(), 2); // submenu opened
    }

    #[test]
    fn menu_submenu_selection() {
        let mut ed = menu_ed();
        ed.menu_open_initial('v'); // View menu, dropdown open at "Theme"
        ed.handle_key(special(KeyCode::Enter)); // open Theme submenu
        ed.handle_key(special(KeyCode::Enter)); // first theme
        assert_eq!(ed.mode, Mode::Command);
        assert_eq!(ed.cmdline, "theme matrix");
    }

    #[test]
    fn star_searches_word_under_cursor() {
        let mut ed = ed_with("foo bar foo baz");
        // cursor on first "foo" (col 0)
        ed.handle_key(key('*'));
        assert_eq!(ed.cursor.col, 8); // second "foo"
    }

    #[test]
    fn hash_searches_backward() {
        let mut ed = ed_with("foo bar foo baz");
        ed.cursor = Position::new(0, 8); // on second "foo"
        ed.handle_key(key('#'));
        assert_eq!(ed.cursor.col, 0); // first "foo"
    }

    #[test]
    fn visual_star_searches_selection() {
        let mut ed = ed_with("foo bar foo bar");
        ed.cursor = Position::new(0, 4); // start of first "bar"
        ed.handle_key(key('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // select "bar"
        ed.handle_key(key('*'));
        assert_eq!(ed.mode, Mode::Normal);
        assert_eq!(ed.cursor.col, 12); // second "bar"
    }

    #[test]
    fn visual_star_escapes_regex_metachars() {
        let mut ed = ed_with("a.b a.b axb");
        ed.handle_key(key('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // select "a.b"
        ed.handle_key(key('*'));
        // Must match the literal "a.b" at col 4, not "axb" at col 8.
        assert_eq!(ed.cursor.col, 4);
    }

    #[test]
    fn visual_hash_searches_selection_backward() {
        let mut ed = ed_with("bar foo bar");
        ed.cursor = Position::new(0, 8); // start of last "bar"
        ed.handle_key(key('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // select "bar"
        ed.handle_key(key('#'));
        assert_eq!(ed.cursor.col, 0); // first "bar"
    }

    #[test]
    fn g_upper_d_jumps_to_first_occurrence() {
        let mut ed = ed_with("let foo = 1;\nbar();\nfoo + foo");
        ed.cursor = Position::new(2, 6); // on a later "foo"
        ed.handle_key(key('g'));
        ed.handle_key(key('D'));
        assert_eq!(ed.cursor, Position::new(0, 4)); // first "foo"
    }

    #[test]
    fn gd_jumps_to_nearest_occurrence_above() {
        let mut ed = ed_with("foo\nfoo\nfoo");
        ed.cursor = Position::new(2, 0); // on the 3rd "foo"
        ed.handle_key(key('g'));
        ed.handle_key(key('d'));
        assert_eq!(ed.cursor, Position::new(1, 0)); // nearest above
    }

    #[test]
    fn gd_records_a_jump() {
        let mut ed = ed_with("foo\nx\nx\nfoo");
        ed.cursor = Position::new(3, 0);
        ed.handle_key(key('g'));
        ed.handle_key(key('d')); // -> line 0
        assert_eq!(ed.cursor.row, 0);
        ed.handle_key(ctrl('o')); // jump back
        assert_eq!(ed.cursor.row, 3);
    }

    #[test]
    fn star_uses_word_boundaries() {
        let mut ed = ed_with("foo foobar foo");
        // whole-word "foo" is only at 0 and 11; from col 0, next is 11
        ed.handle_key(key('*'));
        assert_eq!(ed.cursor.col, 11);
    }

    #[test]
    fn g_star_ignores_word_boundaries() {
        let mut ed = ed_with("foo foobar");
        // g* matches the substring "foo" inside "foobar" (col 4)
        ed.handle_key(key('g'));
        ed.handle_key(key('*'));
        assert_eq!(ed.cursor.col, 4);
    }

    #[test]
    fn effective_ignorecase_logic() {
        let mut ed = ed_with("x");
        assert!(!ed.effective_ignorecase("foo")); // option off
        ed.ignorecase = true;
        assert!(ed.effective_ignorecase("foo")); // on, all lowercase
        assert!(ed.effective_ignorecase("FOO")); // on, no smartcase -> still insensitive
        ed.smartcase = true;
        assert!(ed.effective_ignorecase("foo")); // smartcase + lowercase -> insensitive
        assert!(!ed.effective_ignorecase("Foo")); // smartcase + uppercase -> sensitive
    }

    #[test]
    fn n_repeats_in_last_search_direction() {
        let mut ed = ed_with("foo\nbar\nfoo\nbar\nfoo");
        ed.cursor = Position::new(2, 0);
        ed.handle_key(key('?'));
        for c in "foo".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Enter)); // backward -> row 0
        assert_eq!(ed.cursor.row, 0);
        ed.handle_key(key('n')); // n repeats backward -> wraps to row 4
        assert_eq!(ed.cursor.row, 4);
        ed.handle_key(key('N')); // N reverses -> forward -> row 0
        assert_eq!(ed.cursor.row, 0);
    }

    #[test]
    fn nowrapscan_stops_at_buffer_end() {
        let mut ed = ed_with("foo\nbar\nfoo");
        ed.wrapscan = false;
        ed.set_search("foo".into());
        ed.search_repeat(true); // forward to the second "foo"
        assert_eq!(ed.cursor.row, 2);
        ed.search_repeat(true); // would wrap -> rejected
        assert_eq!(ed.cursor.row, 2);
        assert!(ed.message.contains("BOTTOM"));
    }

    #[test]
    fn wrapscan_on_wraps_around() {
        let mut ed = ed_with("foo\nbar\nfoo");
        ed.set_search("foo".into()); // wrapscan defaults on
        ed.cursor = Position::new(2, 0);
        ed.search_repeat(true); // wraps to the first "foo"
        assert_eq!(ed.cursor.row, 0);
    }

    #[test]
    fn gn_selects_match_under_cursor() {
        let mut ed = ed_with("foo bar foo");
        ed.set_search("foo".into());
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('g'));
        ed.handle_key(key('n'));
        assert_eq!(ed.mode, Mode::Visual);
        assert_eq!(ed.visual_anchor, Position::new(0, 0));
        assert_eq!(ed.cursor, Position::new(0, 2));
    }

    #[test]
    fn g_shift_n_selects_previous_match() {
        let mut ed = ed_with("foo bar foo");
        ed.set_search("foo".into());
        ed.cursor = Position::new(0, 7); // in the gap, before the last foo
        ed.handle_key(key('g'));
        ed.handle_key(key('N'));
        assert_eq!(ed.mode, Mode::Visual);
        assert_eq!(ed.visual_anchor, Position::new(0, 0));
        assert_eq!(ed.cursor, Position::new(0, 2));
    }

    #[test]
    fn dgn_deletes_next_match() {
        let mut ed = ed_with("a foo b");
        ed.set_search("foo".into());
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('d'));
        ed.handle_key(key('g'));
        ed.handle_key(key('n'));
        assert_eq!(ed.buffer.line(0), Some("a  b"));
    }

    #[test]
    fn cgn_change_repeats_with_dot() {
        let mut ed = ed_with("foo x foo x foo");
        ed.set_search("foo".into());
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('c'));
        ed.handle_key(key('g'));
        ed.handle_key(key('n')); // change the match under the cursor
        assert_eq!(ed.mode, Mode::Insert);
        for c in "bar".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("bar x foo x foo"));
        ed.handle_key(key('.')); // dot repeats on the next match
        assert_eq!(ed.buffer.line(0), Some("bar x bar x foo"));
    }

    #[test]
    fn ignorecase_search_finds_other_case() {
        let mut ed = ed_with("aaa\nBETA\nccc");
        ed.ignorecase = true;
        ed.set_search("beta".into());
        ed.search_repeat(true);
        assert_eq!(ed.cursor.row, 1); // matched BETA from a lowercase pattern
    }

    #[test]
    fn smartcase_uppercase_pattern_is_sensitive() {
        let mut ed = ed_with("aaa\nbeta\nBETA");
        ed.ignorecase = true;
        ed.smartcase = true;
        ed.set_search("BETA".into());
        ed.search_repeat(true);
        assert_eq!(ed.cursor.row, 2); // skips lowercase "beta", lands on exact "BETA"
    }

    #[test]
    fn substitute_count_only_leaves_buffer_unchanged() {
        let mut ed = ed_with("foo foo\nbar foo\nbaz");
        let spec = SubstituteSpec {
            range: SubRange::WholeFile,
            pattern: "foo".into(),
            replacement: "X".into(),
            global: true,
            ignorecase: false,
            count_only: true,
        };
        let (subs, lines) = ed.substitute(&spec);
        assert_eq!(subs, 3); // three matches
        assert_eq!(lines, 2); // across two lines
        assert_eq!(ed.buffer.line(0), Some("foo foo")); // nothing changed
    }

    #[test]
    fn ignorecase_applies_to_substitute() {
        let mut ed = ed_with("Foo foo FOO");
        ed.ignorecase = true;
        let spec = crate::command::SubstituteSpec {
            range: crate::command::SubRange::CurrentLine,
            pattern: "foo".into(),
            replacement: "x".into(),
            global: true,
            ignorecase: false, count_only: false, // no /i flag; the option should still apply
        };
        ed.substitute(&spec);
        assert_eq!(ed.buffer.line(0), Some("x x x"));
    }

    #[test]
    fn showcmd_tracks_pending_operator() {
        let mut ed = ed_with("hello world foo");
        ed.handle_key(key('2'));
        assert_eq!(ed.pending_command(), "2");
        ed.handle_key(key('d'));
        assert_eq!(ed.pending_command(), "2d");
        ed.handle_key(key('w'));
        assert_eq!(ed.pending_command(), ""); // command completed -> cleared
    }

    #[test]
    fn showcmd_tracks_operator_and_textobject() {
        let mut ed = ed_with("foo bar");
        ed.handle_key(key('d'));
        assert_eq!(ed.pending_command(), "d");
        ed.handle_key(key('i'));
        assert_eq!(ed.pending_command(), "di"); // awaiting the object char
        ed.handle_key(key('w'));
        assert_eq!(ed.pending_command(), "");
    }

    #[test]
    fn showcmd_cleared_when_leaving_normal_mode() {
        let mut ed = ed_with("hi");
        ed.handle_key(key('i'));
        assert_eq!(ed.pending_command(), ""); // insert mode shows nothing pending
    }

    #[test]
    fn search_count_reports_index_and_total() {
        let mut ed = ed_with("foo\nfoo\nfoo");
        ed.handle_key(key('/'));
        for c in "foo".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Enter)); // from (0,0) the next match is row 1
        assert!(ed.message.contains("[2/3]"), "{}", ed.message);
        ed.handle_key(key('n'));
        assert!(ed.message.contains("[3/3]"), "{}", ed.message);
        ed.handle_key(key('n')); // wraps back to the first
        assert!(ed.message.contains("[1/3]"), "{}", ed.message);
    }

    #[test]
    fn incsearch_previews_match_and_commits() {
        let mut ed = ed_with("alpha\nbravo\ncharlie");
        ed.handle_key(key('/'));
        for c in "charlie".chars() {
            ed.handle_key(key(c));
        }
        assert_eq!(ed.cursor.row, 2); // previewed live while typing
        ed.handle_key(special(KeyCode::Enter));
        assert_eq!(ed.cursor.row, 2); // committed to the same match
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn incsearch_esc_restores_cursor() {
        let mut ed = ed_with("alpha\nbravo\ncharlie");
        ed.handle_key(key('/'));
        for c in "charlie".chars() {
            ed.handle_key(key(c));
        }
        assert_eq!(ed.cursor.row, 2);
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.cursor, Position::new(0, 0)); // back to where search began
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn noincsearch_skips_preview_but_commits() {
        let mut ed = ed_with("alpha\nbravo\ncharlie");
        ed.incsearch = false;
        ed.handle_key(key('/'));
        for c in "charlie".chars() {
            ed.handle_key(key(c));
        }
        assert_eq!(ed.cursor.row, 0); // no live preview
        ed.handle_key(special(KeyCode::Enter));
        assert_eq!(ed.cursor.row, 2); // Enter still jumps
    }

    #[test]
    fn search_regex_finds_pattern() {
        let mut ed = ed_with("alpha1\nbeta22\ngamma333");
        ed.set_search(r"\d\d+".into()); // 2+ digits
        ed.search_repeat(true);
        assert_eq!(ed.cursor.row, 1); // beta22
        ed.search_repeat(true);
        assert_eq!(ed.cursor.row, 2); // gamma333
    }

    #[test]
    fn substitute_current_line_first_only() {
        let mut ed = ed_with("foo foo foo");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: "foo".into(),
            replacement: "bar".into(),
            global: false,
            ignorecase: false, count_only: false,
        };
        let (subs, lines) = ed.substitute(&spec);
        assert_eq!((subs, lines), (1, 1));
        assert_eq!(ed.buffer.line(0), Some("bar foo foo"));
    }

    #[test]
    fn substitute_global_whole_file() {
        let mut ed = ed_with("a x a\nx a x\nno match");
        let spec = SubstituteSpec {
            range: SubRange::WholeFile,
            pattern: "x".into(),
            replacement: "Q".into(),
            global: true,
            ignorecase: false, count_only: false,
        };
        let (subs, lines) = ed.substitute(&spec);
        assert_eq!((subs, lines), (3, 2));
        assert_eq!(ed.buffer.line(0), Some("a Q a"));
        assert_eq!(ed.buffer.line(1), Some("Q a Q"));
        assert_eq!(ed.buffer.line(2), Some("no match"));
    }

    #[test]
    fn substitute_numeric_range() {
        let mut ed = ed_with("z\nz\nz\nz");
        let spec = SubstituteSpec {
            range: SubRange::Range(LineAddr::Num(2), LineAddr::Num(3)),
            pattern: "z".into(),
            replacement: "Y".into(),
            global: false,
            ignorecase: false, count_only: false,
        };
        let (subs, lines) = ed.substitute(&spec);
        assert_eq!((subs, lines), (2, 2));
        assert_eq!(ed.buffer.line(0), Some("z"));
        assert_eq!(ed.buffer.line(1), Some("Y"));
        assert_eq!(ed.buffer.line(2), Some("Y"));
        assert_eq!(ed.buffer.line(3), Some("z"));
    }

    #[test]
    fn substitute_not_found_makes_no_change_and_no_undo() {
        let mut ed = ed_with("hello");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: "zzz".into(),
            replacement: "!".into(),
            global: true,
            ignorecase: false, count_only: false,
        };
        let (subs, _) = ed.substitute(&spec);
        assert_eq!(subs, 0);
        assert_eq!(ed.buffer.line(0), Some("hello"));
        // Nothing changed, so there should be nothing to undo.
        assert!(ed.buffer.undo(ed.cursor).is_none());
    }

    #[test]
    fn substitute_empty_replacement_deletes_text() {
        let mut ed = ed_with("re-mo-ve");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: "-".into(),
            replacement: "".into(),
            global: true,
            ignorecase: false, count_only: false,
        };
        let (subs, _) = ed.substitute(&spec);
        assert_eq!(subs, 2);
        assert_eq!(ed.buffer.line(0), Some("remove"));
    }

    #[test]
    fn gqip_reflows_paragraph() {
        let mut ed = ed_with("the quick brown fox\n\nnext para");
        ed.textwidth = 10;
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('g'));
        ed.handle_key(key('q'));
        ed.handle_key(key('i'));
        ed.handle_key(key('p')); // gqip — reflow the inner paragraph
        assert_eq!(ed.buffer.line(0), Some("the quick"));
        assert_eq!(ed.buffer.line(1), Some("brown fox"));
        assert_eq!(ed.buffer.line(2), Some("")); // blank separator preserved
        assert_eq!(ed.buffer.line(3), Some("next para"));
    }

    #[test]
    fn reflow_wraps_current_line_at_textwidth() {
        let mut ed = ed_with("the quick brown fox jumps over the lazy dog");
        ed.textwidth = 20;
        ed.handle_key(key('g'));
        ed.handle_key(key('q'));
        ed.handle_key(key('q')); // gqq
        for row in 0..ed.buffer.line_count() {
            assert!(ed.buffer.line(row).unwrap().chars().count() <= 20);
        }
        let joined: String = (0..ed.buffer.line_count())
            .map(|r| ed.buffer.line(r).unwrap().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(joined, "the quick brown fox jumps over the lazy dog");
    }

    #[test]
    fn reflow_joins_and_wraps_range() {
        let mut ed = ed_with("alpha beta\ngamma delta\nepsilon");
        ed.textwidth = 12;
        ed.handle_key(key('g'));
        ed.handle_key(key('q'));
        ed.handle_key(key('G')); // gqG
        assert_eq!(ed.buffer.line(0), Some("alpha beta"));
        assert_eq!(ed.buffer.line(1), Some("gamma delta"));
        assert_eq!(ed.buffer.line(2), Some("epsilon"));
    }

    #[test]
    fn reflow_preserves_indent() {
        let mut ed = ed_with("    one two three four five");
        ed.textwidth = 14;
        ed.handle_key(key('g'));
        ed.handle_key(key('q'));
        ed.handle_key(key('q'));
        // Each wrapped line keeps the four-space indent.
        for row in 0..ed.buffer.line_count() {
            assert!(ed.buffer.line(row).unwrap().starts_with("    "));
        }
    }

    #[test]
    fn percent_register_holds_filename() {
        let mut ed = ed_with("x");
        ed.buffer.set_path("notes.txt");
        ed.handle_key(key('"'));
        ed.handle_key(key('%'));
        ed.handle_key(key('p')); // "%p pastes the file name
        assert!(ed.buffer.line(0).unwrap().contains("notes.txt"), "{:?}", ed.buffer.line(0));
    }

    #[test]
    fn black_hole_register_preserves_unnamed() {
        let mut ed = ed_with("keep\ndrop");
        ed.handle_key(key('y'));
        ed.handle_key(key('y')); // yank "keep" into the unnamed register
        ed.cursor = Position::new(1, 0);
        // "_dd deletes "drop" into the black hole, leaving the unnamed register.
        ed.handle_key(key('"'));
        ed.handle_key(key('_'));
        ed.handle_key(key('d'));
        ed.handle_key(key('d'));
        assert_eq!(ed.buffer.line_count(), 1);
        assert_eq!(ed.buffer.line(0), Some("keep"));
        ed.handle_key(key('p')); // paste the still-intact unnamed register
        assert_eq!(ed.buffer.line(1), Some("keep"));
    }

    #[test]
    fn g_cap_i_inserts_at_column_zero() {
        let mut ed = ed_with("    code");
        ed.cursor = Position::new(0, 6);
        ed.handle_key(key('g'));
        ed.handle_key(key('I'));
        assert_eq!(ed.cursor.col, 0);
        assert_eq!(ed.mode, Mode::Insert);
        ed.handle_key(key('X'));
        assert_eq!(ed.buffer.line(0), Some("X    code"));
    }

    #[test]
    fn gp_charwise_leaves_cursor_after_paste() {
        let mut ed = ed_with("abc");
        ed.register = Register { text: "XY".into(), linewise: false };
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('g'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(0), Some("aXYbc"));
        assert_eq!(ed.cursor.col, 3); // one past the pasted "XY"
    }

    #[test]
    fn gp_linewise_moves_below_block() {
        let mut ed = ed_with("a\nb");
        ed.register = Register { text: "X\nY".into(), linewise: true };
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('g'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(1), Some("X"));
        assert_eq!(ed.buffer.line(2), Some("Y"));
        assert_eq!(ed.cursor.row, 3); // the line after the pasted block
    }

    #[test]
    fn bracket_p_reindents_to_current_line() {
        let mut ed = ed_with("        anchor");
        ed.register = Register { text: "code".into(), linewise: true };
        ed.cursor = Position::new(0, 8);
        ed.handle_key(key(']'));
        ed.handle_key(key('p')); // ]p -> paste below, indent to match "anchor"
        assert_eq!(ed.buffer.line(1), Some("        code"));
    }

    #[test]
    fn bracket_p_preserves_relative_indent() {
        let mut ed = ed_with("    anchor");
        ed.register = Register { text: "a\n  b".into(), linewise: true };
        ed.cursor = Position::new(0, 4);
        ed.handle_key(key(']'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(1), Some("    a"));
        assert_eq!(ed.buffer.line(2), Some("      b")); // 4 + its own 2
    }

    #[test]
    fn bracket_paste_above_with_indent() {
        let mut ed = ed_with("    anchor");
        ed.register = Register { text: "x".into(), linewise: true };
        ed.cursor = Position::new(0, 4);
        ed.handle_key(key('['));
        ed.handle_key(key('p')); // [p -> paste above, indent-adjusted
        assert_eq!(ed.buffer.line(0), Some("    x"));
        assert_eq!(ed.buffer.line(1), Some("    anchor"));
    }

    #[test]
    fn g_ampersand_repeats_substitute_over_file() {
        let mut ed = ed_with("foo\nfoo\nfoo");
        let spec = SubstituteSpec {
            range: SubRange::CurrentLine,
            pattern: "foo".into(),
            replacement: "bar".into(),
            global: false,
            ignorecase: false,
            count_only: false,
        };
        ed.substitute(&spec); // line 0 only
        assert_eq!(ed.buffer.line(0), Some("bar"));
        ed.handle_key(key('g'));
        ed.handle_key(key('&')); // repeat across the whole file
        assert_eq!(ed.buffer.line(1), Some("bar"));
        assert_eq!(ed.buffer.line(2), Some("bar"));
    }

    #[test]
    fn counted_dot_repeats_change_n_times() {
        let mut ed = ed_with("abcdef");
        ed.handle_key(key('x')); // delete 'a' -> "bcdef" (dot = x)
        ed.handle_key(key('3'));
        ed.handle_key(key('.')); // 3. -> delete b, c, d
        assert_eq!(ed.buffer.line(0), Some("ef"));
    }

    #[test]
    fn cmdline_ctrl_w_and_ctrl_u() {
        let mut ed = ed_with("");
        ed.handle_key(key(':'));
        for c in "set number".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(ctrl('w')); // delete the word "number"
        assert_eq!(ed.cmdline, "set ");
        ed.handle_key(ctrl('u')); // clear the line
        assert_eq!(ed.cmdline, "");
        assert_eq!(ed.mode, Mode::Command); // still editing
    }

    #[test]
    fn at_colon_repeats_last_ex_command() {
        let mut ed = ed_with("");
        ed.handle_key(key(':'));
        for c in "wq".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Enter)); // records "wq" in command history
        ed.handle_key(key('@'));
        let action = ed.handle_key(key(':'));
        assert_eq!(action, Action::RunEx("wq".into()));
    }

    #[test]
    fn at_colon_without_history_reports() {
        let mut ed = ed_with("");
        ed.handle_key(key('@'));
        let action = ed.handle_key(key(':'));
        assert_eq!(action, Action::None);
        assert!(ed.message.contains("No previous"));
    }

    #[test]
    fn visual_ctrl_a_increments_each_line() {
        let mut ed = ed_with("x 1\ny 5\nz 9");
        ed.handle_key(key('V'));
        ed.handle_key(key('j'));
        ed.handle_key(key('j'));
        ed.handle_key(ctrl('a'));
        assert_eq!(ed.buffer.line(0), Some("x 2"));
        assert_eq!(ed.buffer.line(1), Some("y 6"));
        assert_eq!(ed.buffer.line(2), Some("z 10"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn visual_ctrl_x_with_count() {
        let mut ed = ed_with("10\n20");
        ed.handle_key(key('V'));
        ed.handle_key(key('j'));
        ed.handle_key(key('3'));
        ed.handle_key(ctrl('x'));
        assert_eq!(ed.buffer.line(0), Some("7"));
        assert_eq!(ed.buffer.line(1), Some("17"));
    }

    #[test]
    fn visual_g_ctrl_a_builds_sequence() {
        let mut ed = ed_with("0\n0\n0");
        ed.handle_key(key('V'));
        ed.handle_key(key('j'));
        ed.handle_key(key('j'));
        ed.handle_key(key('g'));
        ed.handle_key(ctrl('a')); // g Ctrl-a -> 1, 2, 3
        assert_eq!(ed.buffer.line(0), Some("1"));
        assert_eq!(ed.buffer.line(1), Some("2"));
        assert_eq!(ed.buffer.line(2), Some("3"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn visual_g_ctrl_a_with_count_steps() {
        let mut ed = ed_with("10\n10\n10");
        ed.handle_key(key('V'));
        ed.handle_key(key('j'));
        ed.handle_key(key('j'));
        ed.handle_key(key('2'));
        ed.handle_key(key('g'));
        ed.handle_key(ctrl('a')); // 2g Ctrl-a -> +2, +4, +6
        assert_eq!(ed.buffer.line(0), Some("12"));
        assert_eq!(ed.buffer.line(1), Some("14"));
        assert_eq!(ed.buffer.line(2), Some("16"));
    }

    #[test]
    fn visual_g_ctrl_a_skips_numberless_lines() {
        let mut ed = ed_with("0\nfoo\n0");
        ed.handle_key(key('V'));
        ed.handle_key(key('j'));
        ed.handle_key(key('j'));
        ed.handle_key(key('g'));
        ed.handle_key(ctrl('a')); // rank advances only on changed lines
        assert_eq!(ed.buffer.line(0), Some("1"));
        assert_eq!(ed.buffer.line(1), Some("foo"));
        assert_eq!(ed.buffer.line(2), Some("2"));
    }

    #[test]
    fn visual_j_joins_selected_lines() {
        let mut ed = ed_with("one\ntwo\nthree\nfour");
        ed.handle_key(key('V'));
        ed.handle_key(key('j'));
        ed.handle_key(key('j')); // select lines 0-2
        ed.handle_key(key('J'));
        assert_eq!(ed.buffer.line(0), Some("one two three"));
        assert_eq!(ed.buffer.line(1), Some("four"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn normal_j_with_count_joins_several() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.handle_key(key('3'));
        ed.handle_key(key('J')); // 3J joins three lines
        assert_eq!(ed.buffer.line(0), Some("a b c"));
        assert_eq!(ed.buffer.line(1), Some("d"));
    }

    #[test]
    fn visual_r_replaces_selected_chars() {
        let mut ed = ed_with("hello");
        ed.handle_key(key('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // select h, e, l
        ed.handle_key(key('r'));
        ed.handle_key(key('x'));
        assert_eq!(ed.buffer.line(0), Some("xxxlo"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn visual_line_r_replaces_whole_lines() {
        let mut ed = ed_with("ab\ncd");
        ed.handle_key(key('V'));
        ed.handle_key(key('j'));
        ed.handle_key(key('r'));
        ed.handle_key(key('-'));
        assert_eq!(ed.buffer.line(0), Some("--"));
        assert_eq!(ed.buffer.line(1), Some("--"));
    }

    #[test]
    fn visual_block_r_replaces_rectangle() {
        let mut ed = ed_with("abc\ndef\nghi");
        ed.handle_key(ctrl('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('j'));
        ed.handle_key(key('r'));
        ed.handle_key(key('0'));
        assert_eq!(ed.buffer.line(0), Some("00c"));
        assert_eq!(ed.buffer.line(1), Some("00f"));
        assert_eq!(ed.buffer.line(2), Some("ghi")); // outside the block
    }

    #[test]
    fn visual_paste_replaces_charwise_selection() {
        let mut ed = ed_with("foo bar");
        ed.register = Register { text: "XYZ".into(), linewise: false };
        ed.cursor = Position::new(0, 4); // on "bar"
        ed.handle_key(key('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // select b, a, r
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(0), Some("foo XYZ"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn visual_paste_replaces_linewise_selection() {
        let mut ed = ed_with("a\nb\nc");
        ed.register = Register { text: "X".into(), linewise: true };
        ed.cursor = Position::new(1, 0); // on "b"
        ed.handle_key(key('V'));
        ed.handle_key(key('p'));
        assert_eq!(ed.buffer.line(0), Some("a"));
        assert_eq!(ed.buffer.line(1), Some("X"));
        assert_eq!(ed.buffer.line(2), Some("c"));
    }

    #[test]
    fn ga_shows_char_code() {
        let mut ed = ed_with("Abc");
        ed.cursor = Position::new(0, 0); // 'A'
        ed.handle_key(key('g'));
        ed.handle_key(key('a'));
        assert!(ed.message.contains("<A> 65"), "{}", ed.message);
        assert!(ed.message.contains("Hex 41"));
        assert!(ed.message.contains("Octal 101"));
    }

    #[test]
    fn ctrl_g_shows_file_info() {
        let mut ed = ed_with("one\ntwo\nthree");
        ed.cursor = Position::new(1, 0);
        ed.handle_key(ctrl('g'));
        assert!(ed.message.contains("3 lines"), "{}", ed.message);
        assert!(ed.message.contains("line 2 of 3"));
    }

    #[test]
    fn document_stats_counts() {
        let ed = ed_with("foo bar\nbaz");
        // 3 words; chars = 7 + newline + 3 = 11; bytes = 11.
        assert_eq!(ed.document_stats(), (3, 11, 11));
    }

    #[test]
    fn g_ctrl_g_reports_counts() {
        let mut ed = ed_with("foo bar\nbaz");
        ed.cursor = Position::new(1, 1);
        ed.handle_key(key('g'));
        ed.handle_key(ctrl('g'));
        assert!(ed.message.contains("3 words"), "{}", ed.message);
        assert!(ed.message.contains("11 chars"));
        assert!(ed.message.contains("line 2 of 2"));
    }

    #[test]
    fn gg_still_jumps_to_top() {
        let mut ed = ed_with("a\nb\nc");
        ed.cursor = Position::new(2, 0);
        ed.handle_key(key('g'));
        ed.handle_key(key('g'));
        assert_eq!(ed.cursor.row, 0);
    }

    #[test]
    fn changelist_navigates_edit_positions() {
        let mut ed = ed_with("one\ntwo\nthree\nfour");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('x')); // change on row 0
        ed.cursor = Position::new(2, 0);
        ed.handle_key(key('x')); // change on row 2
        ed.cursor = Position::new(3, 0); // wander away
        ed.handle_key(key('g'));
        ed.handle_key(key(';')); // g; -> most recent change (row 2)
        assert_eq!(ed.cursor.row, 2);
        ed.handle_key(key('g'));
        ed.handle_key(key(';')); // older (row 0)
        assert_eq!(ed.cursor.row, 0);
        ed.handle_key(key('g'));
        ed.handle_key(key(',')); // newer (row 2)
        assert_eq!(ed.cursor.row, 2);
    }

    #[test]
    fn changes_listing_shows_edit_rows() {
        let mut ed = ed_with("one\ntwo\nthree\nfour");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('x')); // change on row 0
        ed.cursor = Position::new(2, 0);
        ed.handle_key(key('x')); // change on row 2
        let listing = ed.changes_listing();
        assert!(listing.starts_with("changes —"));
        assert!(listing.contains("change  line  col  text"));
        // Both changed rows appear, by 1-based line number.
        assert!(listing.contains("   1  "));
        assert!(listing.contains("   3  "));
        // At the live position the `>` marker sits past the last entry.
        assert!(listing.trim_end().ends_with('>'));
    }

    #[test]
    fn changes_listing_marks_current_slot() {
        let mut ed = ed_with("one\ntwo\nthree\nfour");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('x'));
        ed.cursor = Position::new(2, 0);
        ed.handle_key(key('x'));
        ed.handle_key(key('g'));
        ed.handle_key(key(';')); // g; -> most recent change (row 2, idx 1)
        let listing = ed.changes_listing();
        // The `>` marker is on a data row now, not past the end.
        assert!(!listing.trim_end().ends_with('>'));
        assert!(listing.lines().any(|l| l.trim_start().starts_with('>')));
    }

    #[test]
    fn delmarks_removes_named_marks() {
        let mut ed = ed_with("one\ntwo\nthree\nfour");
        // Set marks a, b, c on different lines.
        for (c, row) in [('a', 0), ('b', 1), ('c', 2)] {
            ed.cursor = Position::new(row, 0);
            ed.handle_key(key('m'));
            ed.handle_key(key(c));
        }
        ed.delete_marks("a c");
        // a and c gone, b remains: jumping to b lands on row 1, a reports unset.
        ed.handle_key(key('`'));
        ed.handle_key(key('b'));
        assert_eq!(ed.cursor.row, 1);
        ed.handle_key(key('`'));
        ed.handle_key(key('a'));
        assert!(ed.message.contains("Mark not set"));
    }

    #[test]
    fn delmarks_range_and_bang() {
        let mut ed = ed_with("l0\nl1\nl2\nl3");
        for (c, row) in [('a', 0), ('b', 1), ('c', 2), ('d', 3)] {
            ed.cursor = Position::new(row, 0);
            ed.handle_key(key('m'));
            ed.handle_key(key(c));
        }
        ed.delete_marks("a-c"); // removes a, b, c; d remains
        ed.handle_key(key('`'));
        ed.handle_key(key('d'));
        assert_eq!(ed.cursor.row, 3);
        ed.delete_marks("!"); // clears all lowercase marks, including d
        ed.handle_key(key('`'));
        ed.handle_key(key('d'));
        assert!(ed.message.contains("Mark not set"));
    }

    #[test]
    fn history_listing_separates_cmd_and_search() {
        use crate::command::HistoryKind;
        let mut ed = ed_with("hello world\nfoo bar");
        // Submit a command-line entry.
        ed.handle_key(key(':'));
        for c in "set nu".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Enter));
        // Submit a search entry.
        ed.handle_key(key('/'));
        for c in "foo".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Enter));

        let cmd = ed.history_listing(HistoryKind::Cmd);
        assert!(cmd.contains(":set nu"));
        assert!(!cmd.contains("/foo"));

        let search = ed.history_listing(HistoryKind::Search);
        assert!(search.contains("/foo"));
        assert!(!search.contains(":set nu"));

        let all = ed.history_listing(HistoryKind::All);
        assert!(all.contains(":set nu") && all.contains("/foo"));
    }

    #[test]
    fn read_command_inserts_output_below() {
        let mut ed = ed_with("top\nbottom");
        ed.cursor = Position::new(0, 0);
        #[cfg(windows)]
        let cmd = "echo A& echo B";
        #[cfg(not(windows))]
        let cmd = "printf 'A\\nB\\n'";
        ed.read_command(cmd);
        assert_eq!(ed.buffer.line(0), Some("top"));
        assert_eq!(ed.buffer.line(1), Some("A"));
        assert_eq!(ed.buffer.line(2), Some("B"));
        assert_eq!(ed.buffer.line(3), Some("bottom"));
    }

    #[test]
    fn splice_lines_replaces_range() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.splice_lines(1, 2, &["X".into(), "Y".into(), "Z".into()]);
        assert_eq!(ed.buffer.line(0), Some("a"));
        assert_eq!(ed.buffer.line(1), Some("X"));
        assert_eq!(ed.buffer.line(2), Some("Y"));
        assert_eq!(ed.buffer.line(3), Some("Z"));
        assert_eq!(ed.buffer.line(4), Some("d"));
    }

    #[test]
    fn filter_range_through_sort() {
        let mut ed = ed_with("banana\napple\ncherry");
        ed.filter_range(Some(SubRange::WholeFile), "sort");
        assert_eq!(ed.buffer.line(0), Some("apple"));
        assert_eq!(ed.buffer.line(1), Some("banana"));
        assert_eq!(ed.buffer.line(2), Some("cherry"));
        assert_eq!(ed.buffer.line_count(), 3);
    }

    #[test]
    fn bang_bang_prefills_filter_cmdline() {
        let mut ed = ed_with("one\ntwo\nthree");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('!'));
        ed.handle_key(key('!'));
        assert_eq!(ed.mode, Mode::Command);
        assert_eq!(ed.cmdline, "1,1!");
    }

    #[test]
    fn visual_bang_prefills_selection_range() {
        let mut ed = ed_with("a\nb\nc");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(special(KeyCode::Char('V'))); // visual line
        ed.handle_key(key('j')); // extend to row 1
        ed.handle_key(key('!'));
        assert_eq!(ed.mode, Mode::Command);
        assert_eq!(ed.cmdline, "1,2!");
    }

    #[test]
    fn retab_expands_tabs_to_spaces() {
        let mut ed = ed_with("\tx\na\tb");
        ed.expandtab = true;
        ed.tabstop = 4;
        ed.retab(None);
        assert_eq!(ed.buffer.line(0), Some("    x")); // leading tab -> 4 spaces
        assert_eq!(ed.buffer.line(1), Some("a   b")); // tab at col 1 -> to col 4
    }

    #[test]
    fn retab_sets_tabstop_argument() {
        let mut ed = ed_with("\tx");
        ed.expandtab = true;
        ed.tabstop = 4;
        ed.retab(Some(2)); // narrows tabstop to 2 first
        assert_eq!(ed.tabstop, 2);
        assert_eq!(ed.buffer.line(0), Some("  x"));
    }

    #[test]
    fn retab_tabifies_leading_spaces_when_noexpandtab() {
        let mut ed = ed_with("        code"); // 8 leading spaces
        ed.expandtab = false;
        ed.tabstop = 4;
        ed.retab(None);
        assert_eq!(ed.buffer.line(0), Some("\t\tcode")); // 8 spaces -> 2 tabs
    }

    #[test]
    fn retab_reports_no_change() {
        let mut ed = ed_with("plain text");
        ed.expandtab = true;
        ed.tabstop = 4;
        ed.retab(None);
        assert!(ed.message.contains("no change"));
    }

    #[test]
    fn set_listchars_updates_markers() {
        let mut ed = ed_with("x");
        assert_eq!(ed.listchars(), ('▸', '·', '·')); // defaults
        ed.set_listchars("tab:>-,trail:~");
        assert_eq!(ed.listchars(), ('>', '-', '~'));
        assert_eq!(ed.option_value("listchars"), "listchars=tab:>-,trail:~");
    }

    #[test]
    fn set_listchars_ignores_bad_values() {
        let mut ed = ed_with("x");
        ed.set_listchars("tab:X,trail:YZ,bogus:Q"); // tab needs 2, trail needs 1
        // tab had only 1 char -> unchanged; trail had 2 -> unchanged; defaults kept
        assert_eq!(ed.listchars(), ('▸', '·', '·'));
    }

    #[test]
    fn options_listing_all_includes_every_option() {
        let ed = ed_with("hi");
        let all = ed.options_listing(true);
        assert!(all.contains("all options"));
        assert!(all.contains("shiftwidth=4"));
        assert!(all.contains("tabstop=")); // value option listed
        assert!(all.contains("noignorecase")); // a default-off boolean, shown in `all`
        // Every canonical option appears (plus title + blank line).
        assert!(all.lines().count() >= Editor::OPTION_NAMES.len());
    }

    #[test]
    fn options_listing_modified_shows_only_changes() {
        let mut ed = ed_with("hi");
        ed.shiftwidth = 2;
        ed.ignorecase = true;
        let listing = ed.options_listing(false);
        assert!(listing.contains("shiftwidth=2"));
        assert!(listing.contains("ignorecase"));
        // An unchanged default should not appear in the modified listing.
        assert!(!listing.contains("nonumber"));
    }

    #[test]
    fn changelist_empty_reports_message() {
        let mut ed = ed_with("hi");
        ed.handle_key(key('g'));
        ed.handle_key(key(';'));
        assert!(ed.message.contains("empty"));
    }

    #[test]
    fn visual_size_char_counts_columns_then_lines() {
        let mut ed = ed_with("hello world\nsecond\nthird");
        ed.handle_key(key('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('l')); // cols 0..2 on one line
        assert_eq!(ed.visual_size().as_deref(), Some("3"));
        ed.handle_key(key('j')); // now spans two lines -> line count
        assert_eq!(ed.visual_size().as_deref(), Some("2"));
    }

    #[test]
    fn visual_size_linewise_counts_lines() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.handle_key(key('V'));
        ed.handle_key(key('j'));
        ed.handle_key(key('j'));
        assert_eq!(ed.visual_size().as_deref(), Some("3"));
    }

    #[test]
    fn visual_size_block_is_rows_by_cols() {
        let mut ed = ed_with("abcd\nefgh\nijkl");
        ed.handle_key(ctrl('v'));
        ed.handle_key(key('l'));
        ed.handle_key(key('j'));
        assert_eq!(ed.visual_size().as_deref(), Some("2x2"));
    }

    #[test]
    fn visual_size_none_in_normal_mode() {
        let ed = ed_with("x");
        assert_eq!(ed.visual_size(), None);
    }

    #[test]
    fn dot_register_holds_last_inserted_text() {
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        for c in "foo".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Esc)); // line "foo", last insert "foo"
        ed.handle_key(key('"'));
        ed.handle_key(key('.'));
        ed.handle_key(key('p')); // ".p pastes "foo"
        assert_eq!(ed.buffer.line(0), Some("foofoo"));
    }

    #[test]
    fn insert_ctrl_e_copies_char_below() {
        let mut ed = ed_with("Z\nabc");
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('i'));
        ed.handle_key(ctrl('e')); // char below at col 0 is 'a'
        assert_eq!(ed.buffer.line(0), Some("aZ"));
    }

    #[test]
    fn insert_ctrl_y_copies_char_above() {
        let mut ed = ed_with("abc\nZ");
        ed.cursor = Position::new(1, 0);
        ed.handle_key(key('i'));
        ed.handle_key(ctrl('y')); // char above at col 0 is 'a'
        assert_eq!(ed.buffer.line(1), Some("aZ"));
    }

    #[test]
    fn insert_ctrl_e_noop_without_line_below() {
        let mut ed = ed_with("x");
        ed.handle_key(key('i'));
        ed.handle_key(ctrl('e')); // no line below -> nothing inserted
        assert_eq!(ed.buffer.line(0), Some("x"));
    }

    #[test]
    fn insert_ctrl_a_inserts_last_insert() {
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        for c in "ab".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Esc));
        ed.handle_key(key('o')); // open a line below, in insert
        ed.handle_key(ctrl('a')); // insert the previous insert "ab"
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(1), Some("ab"));
    }

    #[test]
    fn insert_ctrl_o_runs_one_normal_command() {
        let mut ed = ed_with("hello");
        ed.handle_key(key('A')); // append at end of line
        ed.handle_key(ctrl('o'));
        assert_eq!(ed.mode, Mode::Normal); // one Normal command coming
        ed.handle_key(key('0')); // move to column 0
        assert_eq!(ed.mode, Mode::Insert); // back to insert
        ed.handle_key(key('X'));
        assert_eq!(ed.buffer.line(0), Some("Xhello"));
    }

    #[test]
    fn insert_ctrl_o_multikey_command() {
        let mut ed = ed_with("one\ntwo");
        ed.handle_key(key('A'));
        ed.handle_key(ctrl('o'));
        ed.handle_key(key('d'));
        assert_eq!(ed.mode, Mode::Normal); // mid-command
        ed.handle_key(key('d')); // dd completes
        assert_eq!(ed.mode, Mode::Insert);
        assert_eq!(ed.buffer.line(0), Some("two"));
    }

    #[test]
    fn insert_completion_completes_prefix() {
        let mut ed = ed_with("function\nfun");
        ed.cursor = Position::new(1, 3); // end of "fun"
        ed.mode = Mode::Insert;
        ed.handle_key(ctrl('n'));
        assert_eq!(ed.buffer.line(1), Some("function"));
        assert_eq!(ed.cursor.col, 8);
    }

    #[test]
    fn insert_completion_cycles_candidates() {
        let mut ed = ed_with("apple apricot\nap");
        ed.cursor = Position::new(1, 2); // end of "ap"
        ed.mode = Mode::Insert;
        ed.handle_key(ctrl('n')); // first match in document order
        assert_eq!(ed.buffer.line(1), Some("apple"));
        ed.handle_key(ctrl('n')); // next candidate
        assert_eq!(ed.buffer.line(1), Some("apricot"));
        ed.handle_key(ctrl('p')); // back again
        assert_eq!(ed.buffer.line(1), Some("apple"));
    }

    #[test]
    fn insert_completion_no_match_leaves_text() {
        let mut ed = ed_with("hello\nzz");
        ed.cursor = Position::new(1, 2);
        ed.mode = Mode::Insert;
        ed.handle_key(ctrl('n'));
        assert_eq!(ed.buffer.line(1), Some("zz")); // unchanged
        assert!(ed.message.contains("No match"));
    }

    #[test]
    fn insert_line_completion_completes_whole_line() {
        let mut ed = ed_with("hello world\nhel");
        ed.cursor = Position::new(1, 3); // end of "hel"
        ed.mode = Mode::Insert;
        ed.handle_key(ctrl('x'));
        ed.handle_key(ctrl('l'));
        assert_eq!(ed.buffer.line(1), Some("hello world"));
    }

    #[test]
    fn nmap_expands_sequence() {
        let mut ed = ed_with("aaa\nbbb");
        ed.set_nmap('x', "dd"); // shadow x with dd (delete line)
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('x'));
        assert_eq!(ed.buffer.line(0), Some("bbb"));
    }

    #[test]
    fn nmap_is_non_recursive() {
        // Mapping x -> "xx" must not loop forever; the inner x deletes chars.
        let mut ed = ed_with("hello");
        ed.set_nmap('x', "xx"); // each inner x is the builtin delete-char
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('x'));
        assert_eq!(ed.buffer.line(0), Some("llo")); // two chars deleted, no loop
    }

    #[test]
    fn nmap_rhs_special_keys() {
        let mut ed = ed_with("hi");
        ed.set_nmap('q', "A!<Esc>"); // append '!' at end of line, then leave insert
        ed.cursor = Position::new(0, 0);
        ed.handle_key(key('q'));
        assert_eq!(ed.buffer.line(0), Some("hi!"));
        assert_eq!(ed.mode, Mode::Normal);
    }

    #[test]
    fn nmap_only_fires_at_rest() {
        // With a pending operator, the mapped key is taken literally by the op.
        let mut ed = ed_with("hello");
        ed.set_nmap('l', "0"); // map l -> 0 (line start)
        ed.cursor = Position::new(0, 2);
        ed.handle_key(key('d'));
        ed.handle_key(key('l')); // dl deletes one char (builtin l), mapping skipped
        assert_eq!(ed.buffer.line(0), Some("helo"));
    }

    #[test]
    fn abbrev_expands_on_nonword_char() {
        let mut ed = ed_with("");
        ed.set_abbrev("teh", "the");
        ed.mode = Mode::Insert;
        for c in "teh".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(key(' ')); // trigger
        assert_eq!(ed.buffer.line(0), Some("the "));
    }

    #[test]
    fn abbrev_multiword_rhs_on_enter() {
        let mut ed = ed_with("");
        ed.set_abbrev("btw", "by the way");
        ed.mode = Mode::Insert;
        for c in "btw".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Enter));
        assert_eq!(ed.buffer.line(0), Some("by the way"));
    }

    #[test]
    fn abbrev_only_matches_whole_word() {
        let mut ed = ed_with("");
        ed.set_abbrev("teh", "the");
        ed.mode = Mode::Insert;
        for c in "tehx".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(key(' '));
        assert_eq!(ed.buffer.line(0), Some("tehx ")); // no expansion
    }

    #[test]
    fn insert_file_completion_completes_path() {
        let dir = std::env::temp_dir().join(format!("rvim_fc_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("alpha.txt"), "x").unwrap();
        let base = dir.display().to_string().replace('\\', "/");
        let prefix = format!("{base}/al");
        let mut ed = ed_with(&prefix);
        ed.cursor = Position::new(0, prefix.chars().count());
        ed.mode = Mode::Insert;
        ed.handle_key(ctrl('x'));
        ed.handle_key(ctrl('f'));
        let got = ed.buffer.line(0).unwrap();
        assert!(got.ends_with("alpha.txt"), "got: {got}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn insert_line_completion_no_match_leaves_line() {
        let mut ed = ed_with("hello\nzzz");
        ed.cursor = Position::new(1, 3);
        ed.mode = Mode::Insert;
        ed.handle_key(ctrl('x'));
        ed.handle_key(ctrl('l'));
        assert_eq!(ed.buffer.line(1), Some("zzz"));
        assert!(ed.message.contains("No line completion"));
    }

    #[test]
    fn put_register_inserts_lines_below() {
        let mut ed = ed_with("a\nb\nc");
        ed.handle_key(key('y'));
        ed.handle_key(key('y')); // yank "a" linewise into the unnamed register
        ed.cursor = Position::new(2, 0); // on "c"
        ed.put_register(LineAddr::Current, None);
        assert_eq!(ed.buffer.line(3), Some("a"));
        assert_eq!(ed.buffer.line_count(), 4);
        assert_eq!(ed.cursor.row, 3);
    }

    #[test]
    fn put_register_at_top_with_zero() {
        let mut ed = ed_with("a\nb");
        ed.handle_key(key('y'));
        ed.handle_key(key('y')); // yank "a"
        ed.put_register(LineAddr::Num(0), None);
        assert_eq!(ed.buffer.line(0), Some("a"));
        assert_eq!(ed.buffer.line(1), Some("a"));
        assert_eq!(ed.buffer.line(2), Some("b"));
    }

    #[test]
    fn join_lines_collapses_range() {
        let mut ed = ed_with("foo\n  bar\n  baz\nkeep");
        // :1,3j -> join the first three lines with single spaces
        ed.join_lines(SubRange::Range(LineAddr::Num(1), LineAddr::Num(3)), false);
        assert_eq!(ed.buffer.line(0), Some("foo bar baz"));
        assert_eq!(ed.buffer.line(1), Some("keep"));
    }

    #[test]
    fn join_lines_raw_keeps_whitespace() {
        let mut ed = ed_with("foo\n  bar");
        // :1,2j! -> raw join, leading spaces preserved
        ed.join_lines(SubRange::Range(LineAddr::Num(1), LineAddr::Num(2)), true);
        assert_eq!(ed.buffer.line(0), Some("foo  bar"));
    }

    #[test]
    fn match_highlight_pairs_brackets() {
        let mut ed = ed_with("foo(bar)");
        ed.cursor = Position::new(0, 3); // on '('
        assert_eq!(ed.match_highlight(), Some(Position::new(0, 7)));
        ed.cursor = Position::new(0, 7); // on ')'
        assert_eq!(ed.match_highlight(), Some(Position::new(0, 3)));
    }

    #[test]
    fn match_highlight_none_off_bracket() {
        let mut ed = ed_with("foo(bar)");
        ed.cursor = Position::new(0, 1); // on 'o', not a bracket
        assert_eq!(ed.match_highlight(), None);
    }

    #[test]
    fn match_highlight_spans_lines() {
        let mut ed = ed_with("fn x() {\n  body\n}");
        ed.cursor = Position::new(0, 7); // on '{'
        assert_eq!(ed.match_highlight(), Some(Position::new(2, 0))); // matching '}'
    }

    #[test]
    fn read_lines_below_inserts_file_contents() {
        let mut ed = ed_with("a\nb\nc");
        ed.cursor = Position::new(0, 0); // on "a"
        ed.read_lines_below("X\nY\n"); // trailing newline must not add a blank line
        assert_eq!(ed.buffer.line(0), Some("a"));
        assert_eq!(ed.buffer.line(1), Some("X"));
        assert_eq!(ed.buffer.line(2), Some("Y"));
        assert_eq!(ed.buffer.line(3), Some("b"));
        assert_eq!(ed.buffer.line_count(), 5);
        assert_eq!(ed.cursor.row, 1); // on the first inserted line
    }

    #[test]
    fn marks_listing_includes_set_mark() {
        let mut ed = ed_with("alpha\nbeta");
        ed.cursor = Position::new(1, 0);
        ed.handle_key(key('m'));
        ed.handle_key(key('a')); // set mark a on line 2
        let listing = ed.marks_listing();
        assert!(listing.contains("beta")); // the mark's line text is shown
    }

    #[test]
    fn registers_listing_shows_yanked_text() {
        let mut ed = ed_with("hello");
        ed.handle_key(key('y'));
        ed.handle_key(key('y')); // yank the line
        let listing = ed.registers_listing();
        assert!(listing.contains("hello"));
    }

    #[test]
    fn mark_range_addresses_resolve() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.marks.insert('<', Position::new(1, 0));
        ed.marks.insert('>', Position::new(2, 0));
        ed.delete_lines(SubRange::Range(LineAddr::Mark('<'), LineAddr::Mark('>')));
        assert_eq!(ed.buffer.line(0), Some("a"));
        assert_eq!(ed.buffer.line(1), Some("d"));
        assert_eq!(ed.buffer.line_count(), 2);
    }

    #[test]
    fn visual_colon_prefills_selection_range() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.handle_key(key('V')); // visual-line on row 0
        ed.handle_key(key('j')); // extend to row 1
        ed.handle_key(key(':'));
        assert_eq!(ed.mode, Mode::Command);
        assert_eq!(ed.cmdline, "'<,'>");
        assert_eq!(ed.marks.get(&'<').map(|p| p.row), Some(0));
        assert_eq!(ed.marks.get(&'>').map(|p| p.row), Some(1));
    }

    #[test]
    fn delete_lines_removes_range() {
        let mut ed = ed_with("a\nb\nc\nd");
        ed.delete_lines(SubRange::Range(LineAddr::Num(2), LineAddr::Num(3)));
        assert_eq!(ed.buffer.line(0), Some("a"));
        assert_eq!(ed.buffer.line(1), Some("d"));
        assert_eq!(ed.buffer.line_count(), 2);
    }

    #[test]
    fn yank_lines_then_paste() {
        let mut ed = ed_with("a\nb\nc");
        ed.yank_lines(SubRange::Range(LineAddr::Num(1), LineAddr::Num(2)));
        ed.cursor = Position::new(2, 0); // on "c"
        ed.handle_key(key('p')); // paste the two yanked lines below
        assert_eq!(ed.buffer.line(3), Some("a"));
        assert_eq!(ed.buffer.line(4), Some("b"));
    }

    #[test]
    fn shift_lines_indents_range() {
        let mut ed = ed_with("a\nb\nc");
        ed.expandtab = true;
        ed.shiftwidth = 2;
        ed.shift_lines(SubRange::Range(LineAddr::Num(1), LineAddr::Num(2)), false, 1);
        assert_eq!(ed.buffer.line(0), Some("  a"));
        assert_eq!(ed.buffer.line(1), Some("  b"));
        assert_eq!(ed.buffer.line(2), Some("c")); // untouched
    }

    #[test]
    fn copy_lines_duplicates_range_at_dest() {
        let mut ed = ed_with("a\nb\nc");
        // :1,2t$ -> copy lines 1-2 to after the last line
        ed.copy_lines(
            SubRange::Range(LineAddr::Num(1), LineAddr::Num(2)),
            LineAddr::Last,
        );
        assert_eq!(ed.buffer.line(3), Some("a"));
        assert_eq!(ed.buffer.line(4), Some("b"));
        assert_eq!(ed.buffer.line_count(), 5);
        assert_eq!(ed.cursor.row, 4); // on the last copied line
    }

    #[test]
    fn move_lines_relocates_range() {
        let mut ed = ed_with("a\nb\nc\nd");
        // :1,2m$ -> move lines 1-2 to the end
        ed.move_lines(
            SubRange::Range(LineAddr::Num(1), LineAddr::Num(2)),
            LineAddr::Last,
        );
        assert_eq!(ed.buffer.line(0), Some("c"));
        assert_eq!(ed.buffer.line(1), Some("d"));
        assert_eq!(ed.buffer.line(2), Some("a"));
        assert_eq!(ed.buffer.line(3), Some("b"));
        assert_eq!(ed.buffer.line_count(), 4);
    }

    #[test]
    fn move_lines_to_top_with_zero() {
        let mut ed = ed_with("a\nb\nc");
        // :3m0 -> move line 3 to the top
        ed.move_lines(
            SubRange::Range(LineAddr::Num(3), LineAddr::Num(3)),
            LineAddr::Num(0),
        );
        assert_eq!(ed.buffer.line(0), Some("c"));
        assert_eq!(ed.buffer.line(1), Some("a"));
        assert_eq!(ed.buffer.line(2), Some("b"));
    }

    #[test]
    fn move_lines_into_itself_is_rejected() {
        let mut ed = ed_with("a\nb\nc");
        // :1,2m2 -> destination inside the moved block; nothing changes
        ed.move_lines(
            SubRange::Range(LineAddr::Num(1), LineAddr::Num(2)),
            LineAddr::Num(2),
        );
        assert_eq!(ed.buffer.line(0), Some("a"));
        assert_eq!(ed.buffer.line(1), Some("b"));
        assert_eq!(ed.buffer.line(2), Some("c"));
        assert!(ed.message.contains("E134"));
    }

    // The `"+` / `"*` registers go through the system clipboard. The tests
    // point `RVIM_CLIPBOARD` at a temp file so they never touch the real
    // clipboard; all clipboard cases live in one test to avoid racing on that
    // process-wide environment variable. `"*` shares the clipboard with `"+`.
    #[test]
    fn clipboard_registers_round_trip() {
        let mut path = std::env::temp_dir();
        path.push(format!("rvim_clip_test_{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("RVIM_CLIPBOARD", &path);

        // Char-wise yank to "+ writes the exact text, no trailing newline.
        let mut ed = ed_with("hello world");
        for c in "\"+yiw".chars() {
            ed.handle_key(key(c));
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");

        // "+p pastes the clipboard text back, char-wise.
        let mut ed = ed_with("XY");
        for c in "\"+p".chars() {
            ed.handle_key(key(c));
        }
        assert_eq!(ed.buffer.line(0), Some("XhelloY"));

        // Line-wise yank to "+ appends a newline so other apps get whole lines.
        let mut ed = ed_with("line1\nline2");
        for c in "\"+yy".chars() {
            ed.handle_key(key(c));
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "line1\n");

        // Reading it back detects the line-wise marker and pastes below.
        let mut ed = ed_with("top");
        for c in "\"+p".chars() {
            ed.handle_key(key(c));
        }
        assert_eq!(ed.buffer.line(1), Some("line1"));

        // "* is an alias for the same clipboard.
        let mut ed = ed_with("abc");
        for c in "\"*p".chars() {
            ed.handle_key(key(c));
        }
        assert_eq!(ed.buffer.line(1), Some("line1"));

        std::env::remove_var("RVIM_CLIPBOARD");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn scroll_indicator_all_top_bot_percent() {
        let mut ed = ed_with("a\nb\nc");
        ed.view_rows = 24;
        ed.top = 0;
        assert_eq!(ed.scroll_indicator(), "All");

        let big: String = (0..50).map(|n| format!("line{n}\n")).collect();
        let mut ed = ed_with(big.trim_end());
        ed.view_rows = 10;
        ed.top = 0;
        assert_eq!(ed.scroll_indicator(), "Top");
        ed.top = 40;
        assert_eq!(ed.scroll_indicator(), "Bot");
        ed.top = 20;
        assert_eq!(ed.scroll_indicator(), "40%");
    }

    #[test]
    fn textwidth_auto_wraps_on_insert() {
        let mut ed = ed_with("");
        ed.textwidth = 10;
        ed.handle_key(key('i'));
        for c in "hello world".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Esc));
        // "hello worl" is 10 chars; typing 'd' pushes past 10 and wraps the word.
        assert_eq!(ed.buffer.line(0), Some("hello"));
        assert_eq!(ed.buffer.line(1), Some("world"));
    }

    #[test]
    fn textwidth_zero_never_wraps() {
        let mut ed = ed_with("");
        ed.textwidth = 0;
        ed.handle_key(key('i'));
        for c in "hello world this is long".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("hello world this is long"));
        assert_eq!(ed.buffer.line(1), None);
    }

    #[test]
    fn textwidth_wrap_counts_tab_display_width() {
        // A leading tab (width 4) plus "ab cd" is 6 chars but 9 display columns;
        // with textwidth 8 it must wrap, which a char count would miss.
        let mut ed = ed_with("");
        ed.textwidth = 8;
        ed.tabstop = 4;
        ed.expandtab = false;
        ed.autoindent = false;
        ed.handle_key(key('i'));
        ed.handle_key(special(KeyCode::Tab));
        for c in "ab cde".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("\tab"));
        assert_eq!(ed.buffer.line(1), Some("cde"));
    }

    #[test]
    fn textwidth_leaves_unbreakable_word_long() {
        let mut ed = ed_with("");
        ed.textwidth = 5;
        ed.handle_key(key('i'));
        for c in "abcdefghij".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Esc));
        // No blank to break on, so the long word stays on one line.
        assert_eq!(ed.buffer.line(0), Some("abcdefghij"));
    }

    #[test]
    fn insert_ctrl_v_decimal_code() {
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        ed.handle_key(ctrl('v'));
        for c in "065".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("A"));
    }

    #[test]
    fn insert_ctrl_v_hex_unicode() {
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        ed.handle_key(ctrl('v'));
        for c in "u00e9".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("é"));
    }

    #[test]
    fn insert_ctrl_v_short_run_keeps_terminator() {
        // A 2-digit decimal run ended by a non-digit inserts the code (65 = 'A')
        // and then the terminating key.
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        ed.handle_key(ctrl('v'));
        for c in "65x".chars() {
            ed.handle_key(key(c));
        }
        ed.handle_key(special(KeyCode::Esc));
        assert_eq!(ed.buffer.line(0), Some("Ax"));
    }

    #[test]
    fn insert_ctrl_v_hex_byte() {
        let mut ed = ed_with("");
        ed.handle_key(key('i'));
        ed.handle_key(ctrl('v'));
        for c in "x41".chars() {
            ed.handle_key(key(c));
        }
        assert_eq!(ed.buffer.line(0), Some("A"));
    }

    #[test]
    fn insert_ctrl_v_literal_tab_ignores_expandtab() {
        let mut ed = ed_with("");
        ed.expandtab = true;
        ed.tabstop = 4;
        ed.handle_key(key('i'));
        ed.handle_key(ctrl('v'));
        ed.handle_key(special(KeyCode::Tab));
        assert_eq!(ed.buffer.line(0), Some("\t"));
    }
