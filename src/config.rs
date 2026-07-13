pub enum Mode {
    Run,
    ExportCurl,
    List,
    Import(String),
    Version,
}

pub struct Config {
    pub filename: String,
    pub target: String,
    pub mode: Mode,
}

const USAGE: &str = "usage: lazyreq <file.lreq> <request-id> [--curl]
       lazyreq <file.lreq> --list
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

        for arg in args.iter().skip(1) {
            if arg == "--curl" {
                mode = Mode::ExportCurl;
            } else if arg == "--list" {
                mode = Mode::List;
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
            return Err(USAGE.to_string());
        }

        if !filename.ends_with(".lreq") {
            return Err(format!("`{}` is not a .lreq file\n{}", filename, USAGE));
        }

        if target.is_empty() && !matches!(mode, Mode::List) {
            return Err(format!("missing request id\n{}", USAGE));
        }

        Ok(Config {
            filename,
            target,
            mode,
        })
    }
}
