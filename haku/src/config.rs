use getopts::{Matches, Options};
use std::process::exit;
use std::env;
use std::iter::FromIterator;

use hakufile::errors::HakuError;

pub struct Config {
    pub dry_run: bool,
    pub list: bool,
    pub verbose: usize,
    pub version: bool,
    pub args: Vec<String>,
    pub filename: String,
    pub recipe: String,
    pub logfile: String,
    pub features: Vec<String>,
}

impl Config {
    pub fn new() -> Self {
        Config{
            dry_run: false,
            list: false,
            verbose: 0,
            version: false,
            args: Vec::new(),
            filename: String::new(),
            recipe: String::new(),
            logfile: String::new(),
            features: Vec::new(),
        }
    }
}

fn print_usage(program: &str, opts: &Options) {
    let brief = format!(
        "Usage: {} [options] recipe [arguments]",
        program
    );
    print!("{}", opts.usage(&brief));
}

pub fn parse_args() -> Result<Config, HakuError> {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();
    let mut conf = Config::new();

    let mut opts = Options::new();
    opts.optflag("h", "help", "Show this help");
    opts.optflagmulti("v", "verbose", "Display extra information");
    opts.optflag("", "version", "Display application version");
    opts.optflag(
        "", "dry-run",
        "Dry run: do not change todo list, only show which todos would be changed",
    );
    opts.optflag("l", "list", "list available commands");
    opts.optopt("f", "file", "Haku file path", "FILENAME");
    opts.optopt("", "log-file", "log file path", "FILEPATH");
    opts.optopt("", "feature", "use features", "Feature1,Feature2");

    let matches: Matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{}", e);
            print_usage(&program, &opts);
            exit(1);
        }
    };

    if matches.opt_present("h") {
        print_usage(&program, &opts);
        exit(0);
    }

    conf.list = matches.opt_present("l");
    conf.dry_run = matches.opt_present("dry-run");
    if matches.opt_present("v") {
        conf.verbose = matches.opt_count("v");
    }
    conf.version = matches.opt_present("version");
    if matches.free.len() != 0 {
        conf.recipe = matches.free[0].clone();
    }
    if matches.free.len() > 1 {
        conf.args = Vec::from_iter(matches.free[1..].iter().cloned());
    }
    if let Some(s) = matches.opt_str("f") {
        conf.filename = s;
    }
    if let Some(s) = matches.opt_str("log-file") {
        conf.logfile = s;
    }
    if let Some(s) = matches.opt_str("feature") {
        conf.features = s.split(',').map(|s| s.to_string()).collect();
    }

    Ok(conf)
}
