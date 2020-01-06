use getopts::{Matches, Options};
use std::env;
use std::iter::FromIterator;
use std::process::exit;

use hakufile::errors::HakuError;

pub struct Config {
    pub dry_run: bool,
    pub list: bool,
    pub verbose: usize,
    pub version: bool,
    pub args: Vec<String>,
    pub filename: String,
    pub recipe: String,
    pub features: Vec<String>,
    pub show_all: bool,
    pub show_features: bool,
    pub show_recipe: String,
}

impl Config {
    pub fn new() -> Self {
        Config {
            dry_run: false,
            list: false,
            verbose: 0,
            version: false,
            show_all: false,
            show_features: false,
            args: Vec::new(),
            filename: String::new(),
            recipe: String::new(),
            features: Vec::new(),
            show_recipe: String::new(),
        }
    }
}

fn print_usage(program: &str, opts: &Options) {
    let brief = format!("Usage: {} [options] recipe [arguments]", program);
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
    opts.optflag("", "dry-run", "Dry run: do not change todo list, only show which todos would be changed");
    opts.optflag("l", "list", "list available commands");
    opts.optopt("f", "file", "Haku file path", "FILENAME");
    opts.optopt("", "feature", "use features", "Feature1,Feature2");
    opts.optflag("a", "all", "list all recipes: available and disabled ones");
    opts.optflag("", "list-features", "list user-defined features used by a script");
    opts.optopt("", "show", "show recipe content", "RECIPE_NAME");

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
    conf.show_all = matches.opt_present("a");
    conf.show_features = matches.opt_present("list-features");
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
    if let Some(s) = matches.opt_str("feature") {
        conf.features = s.split(',').map(|s| s.to_string()).collect();
    }
    if let Some(s) = matches.opt_str("show") {
        conf.show_recipe = s;
    }

    Ok(conf)
}
