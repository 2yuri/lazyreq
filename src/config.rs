pub enum Mode {
    Run,
    ExportCurl,
    List,
    History(HistoryOpts),
    Retry(String),
    Import(String),
    /// Interactive terminal UI; the path is the directory to scan for .lreq
    /// files (defaults to the current directory).
    Tui(Option<String>),
    Update,
    Version,
}

#[derive(PartialEq)]
pub enum HistoryFilter {
    All,
    Success,
    Failed,
    Status(u16),
}

pub struct HistoryOpts {
    pub last: Option<usize>,
    pub verbose: bool,
    pub show_headers: bool,
    pub filter: HistoryFilter,
    /// Address a single run by its run id.
    pub req: Option<String>,
}

pub struct Config {
    pub filename: String,
    pub target: String,
    pub mode: Mode,
}

const USAGE: &str = "usage: lazyreq                      # interactive UI, scans the current directory
       lazyreq --path <dir>         # interactive UI, scans <dir>
       lazyreq <file.lreq> <request-id> [--curl]
       lazyreq <file.lreq> --list
       lazyreq <file.lreq> [request-id] --history [--last N] [-v] [--show-headers] [--req RUN-ID] [--success|--failed|--status CODE]
       lazyreq <file.lreq> --retry <run-id>
       lazyreq import '<curl command>'
       lazyreq update               # self-update to the latest release";

impl Config {
    pub fn new(args: &[String]) -> Result<Config, String> {
        if args.iter().any(|a| a == "--version" || a == "-V") {
            return Ok(Config {
                filename: String::new(),
                target: String::new(),
                mode: Mode::Version,
            });
        }

        if matches!(args.get(1).map(|s| s.as_str()), Some("update") | Some("--update")) {
            return Ok(Config {
                filename: String::new(),
                target: String::new(),
                mode: Mode::Update,
            });
        }

        if args.get(1).map(|s| s.as_str()) == Some("import") {
            let command = args[2..].join(" ");
            if command.trim().is_empty() {
                return Err(format!(
                    "`import` needs a curl command, e.g. lazyreq import 'curl https://api.example.com'\n{}",
                    USAGE
                ));
            }
            return Ok(Config {
                filename: String::new(),
                target: String::new(),
                mode: Mode::Import(command),
            });
        }

        let mut mode = Mode::Run;
        let mut filename = String::new();
        let mut target = String::new();

        let mut history = false;
        let mut retry: Option<String> = None;
        let mut scan_path: Option<String> = None;
        let mut opts = HistoryOpts {
            last: None,
            verbose: false,
            show_headers: false,
            filter: HistoryFilter::All,
            req: None,
        };

        let mut iter = args.iter().skip(1);
        while let Some(arg) = iter.next() {
            let mut value_of = |flag: &str| -> Result<String, String> {
                iter.next()
                    .cloned()
                    .ok_or(format!("`{}` needs a value\n{}", flag, USAGE))
            };
            let set_filter = |current: &mut HistoryFilter, new: HistoryFilter| {
                if *current != HistoryFilter::All {
                    return Err(format!(
                        "--success, --failed and --status are mutually exclusive\n{}",
                        USAGE
                    ));
                }
                *current = new;
                Ok(())
            };

            if arg == "--curl" {
                mode = Mode::ExportCurl;
            } else if arg == "--list" {
                mode = Mode::List;
            } else if arg == "--history" {
                history = true;
            } else if arg == "--retry" {
                retry = Some(value_of("--retry")?);
            } else if arg == "--path" {
                scan_path = Some(value_of("--path")?);
            } else if arg == "--req" {
                opts.req = Some(value_of("--req")?);
            } else if arg == "--last" {
                let raw = value_of("--last")?;
                opts.last = Some(raw.parse().map_err(|_| {
                    format!("invalid value `{}` for --last (expected a number)\n{}", raw, USAGE)
                })?);
            } else if arg == "--status" {
                let raw = value_of("--status")?;
                let code = raw.parse().map_err(|_| {
                    format!("invalid value `{}` for --status (expected e.g. 401)\n{}", raw, USAGE)
                })?;
                set_filter(&mut opts.filter, HistoryFilter::Status(code))?;
            } else if arg == "--success" {
                set_filter(&mut opts.filter, HistoryFilter::Success)?;
            } else if arg == "--failed" {
                set_filter(&mut opts.filter, HistoryFilter::Failed)?;
            } else if arg == "-v" || arg == "--verbose" {
                opts.verbose = true;
            } else if arg == "--show-headers" {
                // seeing headers only makes sense in the detailed view
                opts.show_headers = true;
                opts.verbose = true;
            } else if arg.starts_with("--") {
                return Err(format!("unknown flag `{}`\n{}", arg, USAGE));
            } else if filename.is_empty() {
                filename = arg.clone();
            } else if target.is_empty() {
                target = arg.clone();
            } else {
                return Err(format!("unexpected argument `{}`\n{}", arg, USAGE));
            }
        }

        if filename.is_empty() {
            // No file → interactive UI, as long as no file-scoped flags came along.
            let file_flags = history
                || retry.is_some()
                || !matches!(mode, Mode::Run)
                || opts.last.is_some()
                || opts.verbose
                || opts.show_headers
                || opts.req.is_some()
                || opts.filter != HistoryFilter::All;
            if file_flags {
                return Err(USAGE.to_string());
            }
            return Ok(Config {
                filename: String::new(),
                target: String::new(),
                mode: Mode::Tui(scan_path),
            });
        }

        if let Some(path) = scan_path {
            return Err(format!(
                "`--path {}` opens the interactive UI and cannot be combined with a file\n{}",
                path, USAGE
            ));
        }

        if history {
            if !matches!(mode, Mode::Run) || retry.is_some() {
                return Err(format!(
                    "--history cannot be combined with --curl, --list or --retry\n{}",
                    USAGE
                ));
            }
            mode = Mode::History(opts);
        } else if opts.last.is_some()
            || opts.verbose
            || opts.show_headers
            || opts.req.is_some()
            || opts.filter != HistoryFilter::All
        {
            return Err(format!(
                "--last, --status, --success, --failed, --req, -v and --show-headers only work with --history\n{}",
                USAGE
            ));
        } else if let Some(run_id) = retry {
            if !matches!(mode, Mode::Run) {
                return Err(format!(
                    "--retry cannot be combined with --curl or --list\n{}",
                    USAGE
                ));
            }
            if !target.is_empty() {
                return Err(format!(
                    "--retry replays a run id, not a request id — drop `{}`\n{}",
                    target, USAGE
                ));
            }
            mode = Mode::Retry(run_id);
        }

        if !filename.ends_with(".lreq") {
            return Err(format!("`{}` is not a .lreq file\n{}", filename, USAGE));
        }

        if target.is_empty() && !matches!(mode, Mode::List | Mode::History(_) | Mode::Retry(_)) {
            return Err(format!("missing request id\n{}", USAGE));
        }

        Ok(Config {
            filename,
            target,
            mode,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Config, String> {
        let mut full = vec!["lazyreq".to_string()];
        full.extend(args.iter().map(|s| s.to_string()));
        Config::new(&full)
    }

    #[test]
    fn run_mode() {
        let config = parse(&["api.lreq", "login"]).unwrap();
        assert!(matches!(config.mode, Mode::Run));
        assert_eq!(config.filename, "api.lreq");
        assert_eq!(config.target, "login");
    }

    #[test]
    fn run_requires_a_request_id_but_list_and_history_do_not() {
        assert!(parse(&["api.lreq"]).is_err());
        assert!(parse(&["api.lreq", "--list"]).is_ok());
        assert!(parse(&["api.lreq", "--history"]).is_ok());
    }

    #[test]
    fn history_flags() {
        let config = parse(&[
            "api.lreq", "login", "--history", "--last", "3", "--failed", "-v",
        ])
        .unwrap();
        assert_eq!(config.target, "login");
        match config.mode {
            Mode::History(opts) => {
                assert_eq!(opts.last, Some(3));
                assert!(opts.verbose);
                assert!(!opts.show_headers);
                assert!(opts.filter == HistoryFilter::Failed);
            }
            _ => panic!("expected history mode"),
        }
    }

    #[test]
    fn show_headers_implies_verbose() {
        let config = parse(&["api.lreq", "--history", "--show-headers"]).unwrap();
        match config.mode {
            Mode::History(opts) => assert!(opts.verbose && opts.show_headers),
            _ => panic!("expected history mode"),
        }
    }

    #[test]
    fn status_filter_parses_a_code() {
        let config = parse(&["api.lreq", "--history", "--status", "401"]).unwrap();
        match config.mode {
            Mode::History(opts) => assert!(opts.filter == HistoryFilter::Status(401)),
            _ => panic!("expected history mode"),
        }
        assert!(parse(&["api.lreq", "--history", "--status", "teapot"]).is_err());
        assert!(parse(&["api.lreq", "--history", "--status"]).is_err());
    }

    #[test]
    fn history_filters_are_mutually_exclusive() {
        assert!(parse(&["api.lreq", "--history", "--success", "--failed"]).is_err());
        assert!(parse(&["api.lreq", "--history", "--failed", "--status", "500"]).is_err());
    }

    #[test]
    fn history_flags_require_history_mode() {
        assert!(parse(&["api.lreq", "login", "--last", "3"]).is_err());
        assert!(parse(&["api.lreq", "login", "-v"]).is_err());
        assert!(parse(&["api.lreq", "login", "--failed"]).is_err());
    }

    #[test]
    fn history_rejects_other_modes_and_unknown_flags() {
        assert!(parse(&["api.lreq", "--history", "--curl"]).is_err());
        assert!(parse(&["api.lreq", "--history", "--list"]).is_err());
        assert!(parse(&["api.lreq", "login", "--nope"]).is_err());
    }

    #[test]
    fn req_addresses_a_run_within_history() {
        let config = parse(&["api.lreq", "--history", "--req", "a3f2c1d0"]).unwrap();
        match config.mode {
            Mode::History(opts) => assert_eq!(opts.req.as_deref(), Some("a3f2c1d0")),
            _ => panic!("expected history mode"),
        }
        assert!(parse(&["api.lreq", "login", "--req", "a3f2c1d0"]).is_err()); // needs --history
    }

    #[test]
    fn retry_takes_a_run_id_and_nothing_else() {
        let config = parse(&["api.lreq", "--retry", "a3f2c1d0"]).unwrap();
        assert!(matches!(config.mode, Mode::Retry(id) if id == "a3f2c1d0"));

        assert!(parse(&["api.lreq", "--retry"]).is_err()); // missing value
        assert!(parse(&["api.lreq", "login", "--retry", "a3f2c1d0"]).is_err()); // no request id
        assert!(parse(&["api.lreq", "--retry", "a3f2c1d0", "--history"]).is_err());
        assert!(parse(&["api.lreq", "--retry", "a3f2c1d0", "--curl"]).is_err());
    }

    #[test]
    fn non_lreq_files_are_rejected() {
        assert!(parse(&["api.yaml", "login"]).is_err());
    }

    #[test]
    fn no_args_opens_the_tui() {
        assert!(matches!(parse(&[]).unwrap().mode, Mode::Tui(None)));
    }

    #[test]
    fn path_scans_a_directory_in_tui_mode() {
        let config = parse(&["--path", "~/projects"]).unwrap();
        assert!(matches!(config.mode, Mode::Tui(Some(p)) if p == "~/projects"));

        assert!(parse(&["--path"]).is_err()); // missing value
        assert!(parse(&["api.lreq", "login", "--path", "~"]).is_err()); // not with a file
    }

    #[test]
    fn file_scoped_flags_without_a_file_are_rejected() {
        assert!(parse(&["--history"]).is_err());
        assert!(parse(&["--list"]).is_err());
        assert!(parse(&["--retry", "a3f2c1d0"]).is_err());
        assert!(parse(&["-v"]).is_err());
    }

    #[test]
    fn import_and_version_still_short_circuit() {
        assert!(matches!(
            parse(&["import", "curl", "https://x.dev"]).unwrap().mode,
            Mode::Import(_)
        ));
        assert!(matches!(parse(&["--version"]).unwrap().mode, Mode::Version));
    }
}
