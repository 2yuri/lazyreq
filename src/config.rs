pub enum Mode {
    Run,
    ExportCurl,
    List,
}

pub struct Config {
    pub filename: String,
    pub target: String,
    pub mode: Mode,
}

const USAGE: &str = "usage: lazyreq <file.lreq> <request-id> [--curl]
       lazyreq <file.lreq> --list";

impl Config {
    pub fn new(args: &[String]) -> Result<Config, String> {
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
            return Err(format!(
                "`{}` is not a .lreq file\n{}",
                filename, USAGE
            ));
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
