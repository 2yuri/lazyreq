pub struct Config {
    pub filename: String,
    pub target: String,
}

impl Config {
    pub fn new(args: &[String]) -> Config {
        if args.len() < 3 {
            panic!("not enough arguments");
        }

        if args[1] == "" || args[2] == "" {
            panic!("invalid arguments");
        }
        
        let filename = args[1].clone();

        if !filename.ends_with(".lreq") {
            panic!("invalid filename provided");
        }

        let target = args[2].clone();

        Config { filename, target }
    }
}
