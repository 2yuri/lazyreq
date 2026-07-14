pub enum Mode {
    Run,
    ExportCurl,
    List,
    History(HistoryOpts),
    Import(String),
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
}

pub struct Config {
    pub filename: String,
    pub target: String,
    pub mode: Mode,
}

const USAGE: &str = "usage: lazyreq <file.lreq> <request-id> [--curl]
       lazyreq <file.lreq> --list
       lazyreq <file.lreq> [request-id] --history [--last N] [-v] [--show-headers] [--success|--failed|--status CODE]
       lazyreq import '<curl command>'";

impl Config {
    pub fn new(args: &[String]) -> Result<Config, String> {
        if args.iter().any(|a| a == "--version" || a == "-V") {
            return Ok(Config {
                filename: String::new(),
                target: String::new(),
                mode: Mode::Version,
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
        let mut opts = HistoryOpts {
            last: None,
            verbose: false,
            show_headers: false,
            filter: HistoryFilter::All,
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

        if history {
            if !matches!(mode, Mode::Run) {
                return Err(format!(
                    "--history cannot be combined with --curl or --list\n{}",
                    USAGE
                ));
            }
            mode = Mode::History(opts);
        } else if opts.last.is_some()
            || opts.verbose
            || opts.show_headers
            || opts.filter != HistoryFilter::All
        {
            return Err(format!(
                "--last, --status, --success, --failed, -v and --show-headers only work with --history\n{}",
                USAGE
            ));
        }

        if filename.is_empty() {
            return Err(USAGE.to_string());
        }

        if !filename.ends_with(".lreq") {
            return Err(format!("`{}` is not a .lreq file\n{}", filename, USAGE));
        }

        if target.is_empty() && !matches!(mode, Mode::List | Mode::History(_)) {
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
    fn non_lreq_files_are_rejected() {
        assert!(parse(&["api.yaml", "login"]).is_err());
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
