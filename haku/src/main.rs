mod config;
use std::path::{Path};

use std::process::exit;
use std::collections::HashSet;

use config::{parse_args};

use hakufile::errors::HakuError;
use hakufile::vm::{Engine, RunOpts};

fn display_recipes(eng: Engine) {
    let recipes = eng.recipes();
    let disabled = eng.disabled_recipes();
    if recipes.is_empty() && disabled.is_empty() {
        println!("No recipes found");
        return;
    }

    if !recipes.is_empty() {
        println!("Available:");
    }
    let mut sec_names = HashSet::new();
    for s in recipes {
        if sec_names.contains(&s.name) {
            continue;
        }
        sec_names.insert(s.name.clone());
        if s.system {
            continue
        }
        print!("    {}", s.name);
        if !s.vars.is_empty() {
            print!(" ({:?})", s.vars)
        }
        if !s.depends.is_empty() {
            print!(": {:?}", s.depends);
        }
        if !s.desc.is_empty() {
            print!(" #{}", s.desc);
        }
        println!();
    }

    if disabled.is_empty() {
        return;
    }
    if !recipes.is_empty() {
        println!()
    }

    println!("Disabled:");
    for s in disabled {
        print!("    {}", s.name);
        if !s.feat.is_empty() {
            print!(" {}", s.feat);
        }
        if !s.desc.is_empty() {
            print!(" #{}", s.desc);
        }
        println!();
    }
}

fn detect_taskfile() -> String {
    #[cfg(windows)]
    let names = vec!["Taskfile", "Hakufile"];
    #[cfg(not(windows))]
    let names = vec!["Taskfile", "taskfile", "Hakufile", "hakufile"];

    for name in names.iter() {
        let p = Path::new(name);
        if p.is_file() {
            return name.to_string();
        }
    }

    eprintln!("No task file in this directory ({:?})", names);
    exit(1);
}

fn main() -> Result<(), HakuError> {
    let conf = parse_args()?;

    if conf.version {
        let version = env!("CARGO_PKG_VERSION");
        println!("Haku Version {}", version);
        exit(0);
    }

    let filename = if conf.filename.is_empty() {
        detect_taskfile()
    } else {
        conf.filename.clone()
    };

    let opts = RunOpts::new()
        .with_dry_run(conf.dry_run)
        .with_features(conf.features.clone())
        .with_verbosity(conf.verbose);
    let mut eng = Engine::new(conf.verbose, &conf.logfile);
    eng.set_free_args(&conf.args);
    if let Err(e) = eng.load_file(&filename, &opts) {
        eprintln!("{}", e);
        exit(1);
    }

    if conf.list {
        display_recipes(eng);
        exit(0);
    }

    let res = eng.run_recipe(&conf.recipe, opts);
    match res {
        Err(e) => {
            eprintln!("{}", e);
            exit(1);
        },
        _ => {},
    };
    Ok(())
}
