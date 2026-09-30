//! rvim command-line entry point.

use rvim::app::App;
use std::process::ExitCode;

const USAGE: &str = "\
rvim — a modular, vim-emulating terminal editor

USAGE:
    rvim [OPTIONS] [FILE]

OPTIONS:
    --theme <name>   start with a color theme (matrix, retrowave, cobalt)
    --no-config      skip loading ~/.rvimrc
    --version        print version and exit
    --help, -h       print this help and exit

Inside the editor, press :help for keybindings, :q to quit.";

fn main() -> ExitCode {
    let mut theme: Option<String> = None;
    let mut file: Option<String> = None;
    let mut no_config = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
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

    if let Err(e) = app.run() {
        eprintln!("rvim: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
