pub struct Config {
    pub filename: String,
    pub target: String,
    pub export_curl: bool,
}

impl Config {
    pub fn new(args: &[String]) -> Config {
        if args.len() < 3 {
            panic!("not enough arguments");
        }

        let mut export_curl = false;
        let mut filename = String::new();
        let mut target = String::new();

        let mut i = 1;
        while i < args.len() {
            if args[i] == "--curl" {
                export_curl = true;
            } else if filename.is_empty() {
                filename = args[i].clone();
            } else if target.is_empty() {
                target = args[i].clone();
            }
            i += 1;
        }

        if filename.is_empty() || target.is_empty() {
            panic!("invalid arguments");
        }

        if !filename.ends_with(".lreq") {
            panic!("invalid filename provided");
        }

        Config { filename, target, export_curl }
    }
}
