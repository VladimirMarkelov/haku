mod config;
use std::collections::HashSet;
use std::path::Path;
use std::process::exit;

use config::{parse_args, Config};

use hakufile::errors::HakuError;
use hakufile::vm::{Engine, RunOpts};

fn nice_vec_print(lst: &[String]) {
    for (idx, s) in lst.iter().enumerate() {
        if idx > 0 {
            print!(",");
        }
        print!("{}", s);
    }
}

fn display_recipes(eng: Engine, conf: &Config) {
    if conf.show_features {
        let feats = eng.user_features();
        if !feats.is_empty() {
            print!("Features: ");
            nice_vec_print(&feats);
            println!();
        }
    }
    if !conf.list {
        return;
    }

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
            continue;
        }
        print!("    {}", s.name);
        if !s.vars.is_empty() {
            print!(" (");
            nice_vec_print(&s.vars);
            print!(")");
        }
        if !s.depends.is_empty() {
            print!(": ");
            nice_vec_print(&s.depends);
        }
        if !s.desc.is_empty() {
            print!(" #{}", s.desc);
        }
        println!();
    }

    if disabled.is_empty() || !conf.show_all {
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
            return (*name).to_string();
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

    let filename = if conf.filename.is_empty() { detect_taskfile() } else { conf.filename.clone() };

    let opts = RunOpts::new()
        .with_dry_run(conf.dry_run)
        .with_features(conf.features.clone())
        .with_verbosity(conf.verbose)
        .with_time(conf.show_time);
    let mut eng = Engine::new(opts);
    eng.set_free_args(&conf.args);
    if let Err(e) = eng.load_from_file(&filename) {
        eprintln!("{}", e);
        exit(1);
    }

    if !conf.show_recipe.is_empty() {
        match eng.recipe_content(&conf.show_recipe) {
            Err(e) => {
                eprintln!("{:?}", e);
                exit(1);
            }
            Ok(rcp) => {
                if !rcp.filename.is_empty() {
                    println!("{}", rcp.filename);
                }
                if rcp.enabled {
                    println!("Active recipe: {}", conf.show_recipe);
                } else {
                    println!("Disabled recipe: {}", conf.show_recipe);
                }
                for line in rcp.content {
                    println!("  {}", line);
                }
            }
        }
        exit(0);
    }

    if conf.list || conf.show_features {
        display_recipes(eng, &conf);
        exit(0);
    }

    if let Err(e) = eng.run_recipe(&conf.recipe) {
        match e {
            HakuError::DefaultRecipeError => {
                println!("Default recipe is not found. Consider creating recipe '_default'");
            }
            _ => {
                eprintln!("{}", e);
                exit(1);
            }
        }
    };
    Ok(())
}
