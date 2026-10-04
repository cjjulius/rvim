//! rvim command-line entry point.

use rvim::app::App;
use std::process::ExitCode;

const USAGE: &str = "\
rvim — a modular, vim-emulating terminal editor

USAGE:
    rvim [OPTIONS] [+N] [FILE]

OPTIONS:
    +N               open FILE at line N (bare + opens at the last line)
    +/PATTERN        open FILE at the first line matching PATTERN
    --theme <name>   start with a color theme (matrix, retrowave, cobalt,
                     gruvbox, nord, high-contrast)
    --no-config      skip loading ~/.rvimrc
    --version        print version and exit
    --help, -h       print this help and exit

Inside the editor, press :help for keybindings, :q to quit.";

fn main() -> ExitCode {
    let mut theme: Option<String> = None;
    let mut file: Option<String> = None;
    let mut no_config = false;
    let mut start_line: Option<usize> = None;
    let mut start_search: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        // `+N` opens at line N; bare `+` opens at the last line; `+/pat` opens at
        // the first match of `pat` (vim).
        if let Some(rest) = arg.strip_prefix('+') {
            if let Some(pat) = rest.strip_prefix('/') {
                start_search = Some(pat.to_string());
            } else if rest.is_empty() {
                start_line = Some(usize::MAX); // bare `+`: last line (clamped)
            } else {
                match rest.parse::<usize>() {
                    Ok(n) => start_line = Some(n.max(1)),
                    Err(_) => {
                        eprintln!("error: invalid line number '+{rest}'");
                        return ExitCode::FAILURE;
                    }
                }
            }
            continue;
        }
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--no-config" => no_config = true,
            "--version" | "-V" => {
                println!("rvim {}", rvim::VERSION);
                return ExitCode::SUCCESS;
            }
            "--theme" => match args.next() {
                Some(t) => theme = Some(t),
                None => {
                    eprintln!("error: --theme requires a value");
                    return ExitCode::FAILURE;
                }
            },
            other if other.starts_with('-') => {
                eprintln!("error: unknown option '{other}'\n\n{USAGE}");
                return ExitCode::FAILURE;
            }
            other => file = Some(other.to_string()),
        }
    }

    let mut app = match file {
        Some(ref path) => match App::open(path) {
            Ok(app) => app,
            Err(e) => {
                eprintln!("rvim: cannot open '{path}': {e}");
                return ExitCode::FAILURE;
            }
        },
        None => App::new(),
    };

    // Config first, so an explicit --theme on the CLI wins over the rvimrc.
    if !no_config {
        app.load_config();
    }
    if let Some(t) = theme {
        app.set_theme(&t);
    }

    // `+N` / `+/pat` position the cursor once the file is loaded (the first frame
    // scrolls it into view). Ignored when no file was given.
    if file.is_some() {
        if let Some(line) = start_line {
            app.editor.goto_line(line);
        } else if let Some(pat) = start_search {
            app.editor.search_from_start(&pat);
        }
    }

    if let Err(e) = app.run() {
        eprintln!("rvim: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
