use std::convert::From;
use std::fmt;
use std::iter::FromIterator;
use std::process::Command;
use std::usize;

use crate::errors::HakuError;
use crate::func::run_func;
use crate::ops::{is_flag_on, Op, Seq, FLAG_PASS, FLAG_QUIET};
use crate::parse::{DisabledRecipe, HakuFile};
use crate::var::{ExecResult, VarMgr, VarValue};

/// Name of a recipe that is executed if no recipe is set by a caller
const DEFAULT_RECIPE: &str = "_default";

#[macro_export]
macro_rules! output {
    ($v:expr, $lvl:literal, $fmt:literal) => {
        if $v >= $lvl {
            println!($fmt);
        }
    };
    ($v:expr, $lvl:literal, $fmt:literal, $vals:expr) => {
        if $v >= $lvl {
            println!($fmt, $vals);
        }
    };
    ($v:expr, $lvl:literal, $fmt:literal, $($vals:expr),+) => {
        if $v >= $lvl {
            println!($fmt, $($vals), +);
        }
    };
}

/// Runtime engine options
#[derive(Clone)]
pub struct RunOpts {
    /// the list of user-defined features passed from a caller
    pub(crate) feats: Vec<String>,
    /// verbosity level - affects amount of info print while executing a recipe
    verbosity: usize,
    /// `true` - do not run any shell commands (except ones in assignments and for's)
    dry_run: bool,
}

impl RunOpts {
    pub fn new() -> Self {
        RunOpts { dry_run: false, feats: Vec::new(), verbosity: 0 }
    }

    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn with_features(mut self, feats: Vec<String>) -> Self {
        self.feats = feats;
        self
    }

    pub fn with_verbosity(mut self, verbosity: usize) -> Self {
        self.verbosity = verbosity;
        self
    }
}

/// Recipe detailed information
#[derive(Clone, Debug)]
pub struct RecipeDesc {
    /// recipe's name
    pub name: String,
    /// recipe's description from its doc comments
    pub desc: String,
    /// a list of recipes this one depends on
    pub depends: Vec<String>,
    /// is it a system recipe? (system recipes are not show by default)
    pub system: bool,
    /// the recipe location (file and line)
    pub loc: RecipeLoc,
    /// recipe-wide flags (i.e., echo off, skip errors)
    pub flags: u32,
    /// recipe local variables (they override any global variables with the same names)
    pub vars: Vec<String>,
}

impl fmt::Display for RecipeDesc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if !self.vars.is_empty() {
            write!(f, " (")?;
            for v in self.vars.iter() {
                write!(f, "{},", v)?;
            }
            write!(f, ") ")?;
        }
        if !self.depends.is_empty() {
            write!(f, "[")?;
            for dep in self.depends.iter() {
                write!(f, "{},", dep)?;
            }
            write!(f, "]")?;
        }
        if !self.desc.is_empty() {
            write!(f, " #{}", self.desc)?;
        }
        Ok(())
    }
}

/// Description of a condition that is not finished yet
#[derive(Clone, Debug)]
enum Condition {
    /// value of the last if/elseif condition. The value determines what to do when the engine
    /// meets `elseif` statement. If `true`, the `if` finishes (as if `end` was met).
    /// Otherwise, `elseif` condition is evaluated and its value replaces old `If` value.
    If(bool),
    /// the engine is in a while loop. Value is the condition to check (never changes,
    /// assigned when `while` statement met first time
    While(Vec<Op>),
    /// the engine is in a for loop that runs through a list of integers, arguments:
    ///
    /// * loop variable name (its value changed every cycle)
    /// * current counter value (changed every cycle)
    /// * the final value - for stops when the current value reaches or exceeds the final one
    /// * step - every cycle the current value is changes by the step
    ForInt(String, i64, i64, i64),
    /// the engine is in a for loop that runs through a list of strings (i.e., result of
    /// executing an external command or list of values separated with whitespace):
    ///
    /// * loop variable name (its value changed every cycle)
    /// * list of values (changed every cycle - the used value is removed from the list)
    ForList(String, Vec<String>),
}

/// Describes a condition(loop) the engine is in
#[derive(Clone, Debug)]
struct CondItem {
    /// the line number of the first line of the if/for/while
    line: usize,
    /// detailed info about condition state
    cond: Condition,
}

/// Engine that runs the recipes
pub struct Engine {
    /// List of all loaded Hakufiles (in order of `include`s)
    files: Vec<HakuFile>,
    /// Already loaded file paths so far. Used when loading a file to detect include recursion
    included: Vec<String>,
    /// The list of available recipes
    recipes: Vec<RecipeDesc>,
    /// Variable manager
    varmgr: VarMgr,
    /// What shell is used to execute any external command. By default:
    ///
    /// * Windows = `["cmd", "/c"]`
    /// * Others = `["sh", "-cu"]`
    shell: Vec<String>,
    /// Runtime options passed by a caller
    opts: RunOpts,

    /// stack of nested if/for/while loops
    cond_stack: Vec<CondItem>,
    /// real line of the currently executed line (to generate more helpful error message)
    real_line: usize,
    /// file index of currently executing line
    file_idx: usize,
}

/// Describes a recipe location
#[derive(Debug, Clone)]
pub struct RecipeLoc {
    // the number of file (in `engine.files` list)
    pub file: usize,
    // the line number in operation list
    pub line: usize,
    // the line number in the script
    pub script_line: usize,
}

/// Describes a recipe
#[derive(Debug)]
struct RecipeItem {
    /// recipe's name
    name: String,
    /// recipe's location
    loc: RecipeLoc,
    /// recipe's local variables (overrides existing global variables with the same names)
    vars: Vec<String>,
    /// global recipe flags (i.e., echo off)
    flags: u32,
}

/// Recipe content
pub struct RecipeContent {
    /// File name where the recipe is located
    pub filename: String,
    /// recipe content with its declaration
    pub content: Vec<String>,
    /// recipe is enable/disabled
    pub enabled: bool,
}

impl Engine {
    pub fn new(opts: RunOpts) -> Self {
        #[cfg(windows)]
        let shell = vec!["powershell".to_string(), "-c".to_string()];
        #[cfg(not(windows))]
        let shell = vec!["sh".to_string(), "-cu".to_string()];

        Engine {
            files: Vec::new(),
            included: Vec::new(),
            recipes: Vec::new(),
            varmgr: VarMgr::new(opts.verbosity),
            cond_stack: Vec::new(),
            real_line: usize::MAX,
            file_idx: usize::MAX,
            opts,
            shell,
        }
    }

    /// Returns filename & its line by their indices
    fn line_desc(&self, fidx: usize, lidx: usize) -> (String, String) {
        if fidx == usize::MAX || fidx >= self.files.len() {
            return (String::new(), String::new());
        }
        let fname = if self.files.len() == 1 { String::new() } else { self.included[fidx].clone() };
        if lidx == usize::MAX || lidx >= self.files[fidx].orig_lines.len() {
            return (fname, String::new());
        }
        (fname, self.files[fidx].orig_lines[lidx].clone())
    }

    /// Generates extra info for an error
    fn error_extra(&self) -> String {
        let (filename, line) = self.line_desc(self.file_idx, self.real_line);
        HakuError::error_extra(&filename, &line, self.real_line)
    }

    /// Loads and parse a script from a file (all `include` statements are processed at
    /// this step as well), and builds a list of available and disables recipes
    pub fn load_from_file(&mut self, filepath: &str) -> Result<(), HakuError> {
        output!(self.opts.verbosity, 2, "Loading file: {}", filepath);
        for s in &self.included {
            if s == filepath {
                return Err(HakuError::IncludeRecursionError(filepath.to_string()));
            }
        }
        let hk = HakuFile::load_from_file(filepath, &self.opts)?;
        self.files.push(hk);
        self.included.push(filepath.to_string());
        self.run_header(self.files.len() - 1)?;
        self.detect_recipes();
        Ok(())
    }

    /// Loads and parse a script from memory (but all `include` statements try to load
    /// included scripts from local files), and builds a list of available and disables recipes
    pub fn load_from_str(&mut self, src: &str) -> Result<(), HakuError> {
        output!(self.opts.verbosity, 2, "Executing string: {}", src);
        let hk = HakuFile::load_from_str(src, &self.opts)?;
        self.files.push(hk);
        self.run_header(self.files.len() - 1)?;
        self.detect_recipes();
        Ok(())
    }

    /// Looks for all `import` statements between the first line and the first recipe(or the end
    /// of the script if it does not contain any recipe) and recursively loads imported scripts
    fn run_header(&mut self, idx: usize) -> Result<(), HakuError> {
        output!(self.opts.verbosity, 3, "RUN HEADER: {}: {}", idx, self.files[idx].ops.len());
        let mut to_include: Vec<String> = Vec::new();
        let mut to_include_flags: Vec<u32> = Vec::new();
        for op in &self.files[idx].ops {
            self.real_line = op.line;
            self.file_idx = idx;
            match &op.op {
                Op::Feature(_, _) => { /* Since dead code is removed, it can be skipped */ }
                Op::Recipe(_, _, _, _) => break,
                Op::Comment(_) | Op::DocComment(_) => { /* just continue */ }
                Op::Include(flags, path) => {
                    let inc_path = self.varmgr.interpolate(&path, true);
                    output!(self.opts.verbosity, 3, "        !!INCLUDE - {}", inc_path);
                    to_include.push(inc_path);
                    to_include_flags.push(*flags);
                }
                _ => { /*run = true */ }
            }
        }
        output!(self.opts.verbosity, 3, "TO INCLUDE: {}", to_include.len());
        for (i, path) in to_include.iter().enumerate() {
            let f = to_include_flags[i];
            let res = self.load_from_file(path);
            if res.is_err() {
                output!(self.opts.verbosity, 2, "ERROR: {:?}", res);
            }
            if res.is_err() && !is_flag_on(f, FLAG_PASS) {
                return res;
            }
            eprintln!("Skipping included file: {:?}", res);
        }
        Ok(())
    }

    /// Returns `true` if the name of a recipe is a system one. System recipes should not
    /// be displayed by a caller
    fn is_system_recipe(name: &str) -> bool {
        name == "_default" || name == "_before" || name == "_after"
    }

    /// Build a list of available and disabled recipes. If there are a few recipes have the
    /// same name they are sorted by the following rules:
    ///
    /// 1. An active recipe goes before disabled one
    /// 2. If both recipes are disabled(or enabled) only the first loaded one goes first. It
    ///    makes it possible to override recipes which already exist in imported scripts
    fn detect_recipes(&mut self) {
        for (file_idx, hk) in self.files.iter().enumerate() {
            let mut desc = String::new();
            for (line_idx, op) in hk.ops.iter().enumerate() {
                match op.op {
                    Op::Feature(_, _) => {}
                    Op::DocComment(ref s) => desc = self.varmgr.interpolate(s, true),
                    Op::Recipe(ref nm, flags, ref vars, ref deps) => {
                        let mut recipe = RecipeDesc {
                            name: nm.clone(),
                            desc: desc.clone(),
                            loc: RecipeLoc { line: line_idx, file: file_idx, script_line: op.line },
                            depends: Vec::new(),
                            system: Engine::is_system_recipe(&nm),
                            vars: vars.clone(),
                            flags,
                        };
                        if !deps.is_empty() {
                            for d in deps.iter() {
                                recipe.depends.push(d.to_string());
                            }
                        }

                        self.recipes.push(recipe);
                        desc.clear();
                    }
                    Op::Comment(_) => { /* do not change anything */ }
                    _ => {
                        desc.clear();
                    }
                }
            }
        }
        self.recipes.sort_by(|a, b| a.name.partial_cmp(&b.name).unwrap());
    }

    /// Returns full path to a script by its number (the number must be less than
    /// `engine.files` length
    pub fn file_name(&self, file_idx: usize) -> Result<&str, HakuError> {
        if file_idx >= self.files.len() {
            return Err(HakuError::FileNotLoaded(file_idx));
        }
        Ok(&self.included[file_idx])
    }

    /// Returns info about all loaded available recipes
    pub fn recipes(&self) -> &[RecipeDesc] {
        &self.recipes
    }

    /// Returns info about all loaded disabled recipes
    pub fn disabled_recipes(&self) -> Vec<DisabledRecipe> {
        let mut v = Vec::new();
        for file in self.files.iter() {
            for ds in file.disabled.iter() {
                v.push(ds.clone());
            }
        }
        v
    }

    /// Finds the first recipe after the line `line` in file `idx`. If there is no
    /// recipes after that line, it returns usize::MAX
    fn next_recipe(&self, file: usize, line: usize) -> usize {
        let mut min = usize::MAX;
        for r in self.recipes.iter() {
            if r.loc.file != file || r.loc.script_line <= line {
                continue;
            }
            if min > r.loc.script_line {
                min = r.loc.script_line;
            }
        }
        for d in self.files[file].disabled.iter() {
            if d.line > line && d.line < min {
                min = d.line;
            }
        }
        if min == usize::MAX {
            min = self.files[file].orig_lines.len();
        }
        min
    }

    /// Returns the content of a recipe that would be executed
    pub fn recipe_content(&self, name: &str) -> Result<RecipeContent, HakuError> {
        if let Ok(desc) = self.find_recipe(name) {
            let fidx = desc.loc.file;
            let sidx = desc.loc.script_line;
            let mut eidx = self.next_recipe(fidx, sidx);
            let mut content = Vec::new();

            // ignore all doc comments and feature lists related to the next section
            while eidx > sidx
                && (self.files[fidx].orig_lines[eidx - 1].trim_start().starts_with("#[")
                    || self.files[fidx].orig_lines[eidx - 1].trim_start().starts_with("##"))
            {
                eidx -= 1;
            }

            for lidx in sidx..eidx {
                content.push(self.files[fidx].orig_lines[lidx].clone());
            }

            return Ok(RecipeContent {
                filename: self.file_name(fidx).unwrap_or("").to_string(),
                content,
                enabled: true,
            });
        }

        // no active recipe found with this name. Look for a disabled one
        for (fidx, f) in self.files.iter().enumerate() {
            let mut sidx = usize::MAX;
            for r in f.disabled.iter() {
                if r.name == name {
                    sidx = r.line;
                    break;
                }
            }

            if sidx != usize::MAX {
                let eidx = self.next_recipe(fidx, sidx);
                let mut content = Vec::new();
                for lidx in sidx..eidx {
                    content.push(f.orig_lines[lidx].clone());
                }

                return Ok(RecipeContent {
                    filename: self.file_name(fidx).unwrap_or("").to_string(),
                    content,
                    enabled: false,
                });
            }
        }

        Err(HakuError::RecipeNotFoundError(name.to_string()))
    }

    /// Returns a list of unique user-defined features found in loaded scripts
    pub fn user_features(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for file in self.files.iter() {
            for feat in file.user_feats.iter() {
                let mut unique = true;
                for ex in v.iter() {
                    if ex == feat {
                        unique = false;
                        break;
                    }
                }
                if unique {
                    v.push(feat.clone());
                }
            }
        }
        v
    }

    /// Finds a recipe location by its name (see `detect_recipes` for details about
    /// name conflicts): file and line numbers
    fn find_recipe(&self, name: &str) -> Result<RecipeDesc, HakuError> {
        for sec in &self.recipes {
            if sec.name == name {
                return Ok(sec.clone());
            }
        }
        Err(HakuError::RecipeNotFoundError(name.to_string()))
    }

    /// Sets the values to initialize recipe variables (used by a caller).
    /// Free args are assigned to recipe variables by their ordinal numbers (not by name).
    pub fn set_free_args(&mut self, args: &[String]) {
        self.varmgr.free = Vec::from_iter(args.iter().cloned());
    }

    /// Execute a recipe. If `name` is empty DEFAULT_RECIPE is executed. If `name` is not
    /// empty the recipe with this names must exist and be active.
    /// In all cases, the engine runs all the lines until the first recipe in all imported
    /// scripts.
    pub fn run_recipe(&mut self, name: &str) -> Result<(), HakuError> {
        output!(self.opts.verbosity, 1, "Running SECTION '{}'", name);
        let sec_res = if name.is_empty() {
            match self.find_recipe(DEFAULT_RECIPE) {
                Ok(loc) => Some(loc),
                _ => None,
            }
        } else {
            Some(self.find_recipe(name)?)
        };

        self.exec_init()?;
        if let Some(sec) = sec_res {
            // default recipe can be missing
            return self.exec_recipe(sec.loc);
        }
        Ok(())
    }

    /// If a script line is a function and it is a system one(that changes the engine
    /// internal state), this function executes the line and returns `true`. Otherwise,
    /// this function does nothing and returns `false`.
    fn system_call(&mut self, name: &str, ops: &[Op]) -> Result<bool, HakuError> {
        let lowname = name.to_lowercase();
        match lowname.as_str() {
            "shell" => {
                let v: Vec<String> = ops
                    .iter()
                    .map(|a| match self.exec_op(a) {
                        Err(_) => String::new(),
                        Ok(res) => res.to_string(),
                    })
                    .filter(|a| !a.is_empty())
                    .collect();
                if v.is_empty() {
                    return Err(HakuError::EmptyShellArgError(self.error_extra()));
                }
                output!(self.opts.verbosity, 1, "Setting new shell: {:?}", v);
                self.shell = v;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Executes a script from the first line until the first recipe or end of the script.
    fn exec_file_init(&mut self, file: usize) -> Result<(), HakuError> {
        let cnt = self.files[file].ops.len();
        let mut i = 0;
        while i < cnt {
            let op = self.files[file].ops[i].clone();
            self.real_line = op.line;
            self.file_idx = file;
            match op.op {
                Op::Recipe(_, _, _, _) | Op::Return => return Ok(()),
                Op::Include(_, _) => {
                    i += 1;
                }
                Op::Error(msg) => return Err(HakuError::UserError(format!("{} at line {}", msg, op.line))),
                Op::DocComment(_) | Op::Comment(_) => {
                    i += 1;
                }
                Op::Shell(flags, cmd) => {
                    self.exec_cmd_shell(flags, &cmd)?;
                    i += 1;
                }
                Op::EitherAssign(chk, name, ops) => {
                    self.exec_either_assign(chk, &name, &ops)?;
                    i += 1;
                }
                Op::DefAssign(name, ops) => {
                    self.exec_assign_or(&name, &ops)?;
                    i += 1;
                }
                Op::Assign(name, ops) => {
                    self.exec_assign(&name, &ops)?;
                    i += 1;
                }
                Op::Func(name, ops) => {
                    let is_processed = self.system_call(&name, &ops)?;
                    if !is_processed {
                        self.exec_func(&name, &ops)?;
                    }
                    i += 1;
                } // top level - func value is dropped
                Op::StmtClose => {
                    let next = self.exec_end()?;
                    if next == 0 {
                        i += 1;
                    } else {
                        i = next;
                    }
                }
                Op::For(name, seq) => {
                    let ok = self.exec_for(&name, seq, i)?;
                    if ok {
                        i += 1;
                    } else {
                        i = self.find_end(file, i + 1, "for")?;
                    }
                }
                Op::While(ops) => {
                    // must have exact 1 op
                    let ok = self.exec_while(&ops, i)?;
                    if ok {
                        i += 1;
                    } else {
                        i = self.find_end(file, i + 1, "while")?;
                    }
                }
                Op::Break => {
                    i = self.exec_break(file)?;
                }
                Op::Continue => {
                    i = self.exec_continue(file)?;
                }
                Op::If(ops) => {
                    i = self.exec_if(&ops, file, i)?;
                }
                Op::Else => {
                    i = self.exec_else(file, i)?;
                }
                Op::ElseIf(ops) => {
                    i = self.exec_elseif(&ops, file, i)?;
                } // must have exact 1 op
                _ => {
                    eprintln!("UNIMPLEMENTED: {:?}", op);
                    i += 1;
                }
            }
        }
        Ok(())
    }

    /// Executes headers of all loaded scripts. It is called before running a given recipe.
    /// The order of script execution is a reversed order of script loading. That makes it
    /// possible to override e.g. variable values in the user script.
    fn exec_init(&mut self) -> Result<(), HakuError> {
        let cnt = self.files.len();
        for i in 0..cnt {
            self.exec_file_init(cnt - i - 1)?;
        }
        Ok(())
    }

    /// Adds a recipe to the list of recipes to execute before running a given one.
    /// Used only internally.
    fn push_recipe(
        &mut self,
        loc: RecipeLoc,
        found: Option<&[RecipeItem]>,
        parent: Option<&[String]>,
    ) -> Result<Vec<RecipeItem>, HakuError> {
        let op = self.files[loc.file].ops[loc.line].clone();
        let mut sec_item: RecipeItem = RecipeItem {
            name: String::new(),
            loc: RecipeLoc { file: 0, line: 0, script_line: 0 },
            vars: Vec::new(),
            flags: 0,
        };
        output!(self.opts.verbosity, 2, "Checking recipe: {:?}", op);
        let mut vc: Vec<RecipeItem> = Vec::new();
        let mut parents: Vec<String> = match parent {
            None => Vec::new(),
            Some(p) => p.iter().map(|a| a.to_string()).collect(),
        };
        match op.op {
            Op::Recipe(name, flags, vars, deps) => {
                if vc.iter().any(|s| s.name == name) || deps.iter().any(|d| d == &name) {
                    return Err(HakuError::RecipeRecursionError(name, self.error_extra()));
                }
                for dep in deps {
                    if let Some(ps) = parent {
                        if ps.iter().any(|p| p == &dep) {
                            return Err(HakuError::RecipeRecursionError(dep, self.error_extra()));
                        }
                    }
                    if let Some(fnd) = found {
                        if fnd.iter().any(|f| f.name == dep) {
                            return Err(HakuError::RecipeRecursionError(dep, self.error_extra()));
                        }
                    }
                    let next_s = self.find_recipe(&dep)?;
                    parents.push(name.clone());
                    let mut slist = self.push_recipe(next_s.loc, Some(&vc), Some(&parents))?;
                    vc.append(&mut slist);
                }
                sec_item.name = name;
                sec_item.loc = loc;
                sec_item.vars = vars;
                sec_item.flags = flags;
            }
            _ => unreachable!(),
        }
        vc.push(sec_item);
        Ok(vc)
    }

    /// Runs a given recipe. First, it runs all recipe dependencies recursively. Second,
    /// it runs the body of the given recipe.
    fn exec_recipe(&mut self, loc: RecipeLoc) -> Result<(), HakuError> {
        output!(self.opts.verbosity, 2, "Start recipe [{}:{}]", loc.file, loc.line);
        self.real_line = loc.script_line;
        self.file_idx = loc.file;
        let sec = self.push_recipe(loc, None, None)?;
        output!(self.opts.verbosity, 2, "recipe call stack: {:?}", sec);
        let mut idx = 0;
        while idx < sec.len() {
            let op = &sec[idx];
            output!(self.opts.verbosity, 1, "Starting recipe: {}", op.name);
            self.enter_recipe(op);
            self.exec_from(op.loc.file, op.loc.line + 1, op.flags)?;
            self.leave_recipe();
            idx += 1;
        }
        Ok(())
    }

    /// Executes a script from a given file and the line in it. Used by run recipe function:
    /// it looks for a recipe location and then executes from that position.
    fn exec_from(&mut self, file: usize, line: usize, sec_flags: u32) -> Result<(), HakuError> {
        let mut idx = line;
        let l = self.files[file].ops.len();
        while idx < l {
            let op = (self.files[file].ops[idx]).clone();
            self.real_line = op.line;
            self.file_idx = file;
            match op.op {
                Op::Return | Op::Recipe(_, _, _, _) => return Ok(()),
                Op::Include(_, _) => return Err(HakuError::IncludeInRecipeError(self.error_extra())),
                Op::Error(msg) => return Err(HakuError::UserError(format!("{} at line {}", msg, op.line))),
                Op::Shell(flags, cmd) => {
                    let cmd_flags = sec_flags ^ flags;
                    self.exec_cmd_shell(cmd_flags, &cmd)?;
                    idx += 1;
                }
                Op::EitherAssign(chk, name, ops) => {
                    self.exec_either_assign(chk, &name, &ops)?;
                    idx += 1;
                }
                Op::DefAssign(name, ops) => {
                    self.exec_assign_or(&name, &ops)?;
                    idx += 1;
                }
                Op::Assign(name, ops) => {
                    self.exec_assign(&name, &ops)?;
                    idx += 1;
                }
                Op::Func(name, ops) => {
                    let is_processed = self.system_call(&name, &ops)?;
                    if !is_processed {
                        self.exec_func(&name, &ops)?;
                    }
                    idx += 1;
                } // top level - func value is dropped
                Op::StmtClose => {
                    let next = self.exec_end()?;
                    if next == 0 {
                        idx += 1
                    } else {
                        idx = next;
                    }
                }
                Op::For(name, seq) => {
                    let ok = self.exec_for(&name, seq, idx)?;
                    if ok {
                        idx += 1;
                    } else {
                        idx = self.find_end(file, idx + 1, "for")?;
                    }
                }
                Op::While(ops) => {
                    // must have exact 1 op
                    let ok = self.exec_while(&ops, idx)?;
                    if ok {
                        idx += 1;
                    } else {
                        idx = self.find_end(file, idx + 1, "while")?;
                    }
                }
                Op::Break => {
                    idx = self.exec_break(file)?;
                }
                Op::Continue => {
                    idx = self.exec_continue(file)?;
                }
                Op::If(ops) => {
                    idx = self.exec_if(&ops, file, idx)?;
                } // must have exact 1 op
                Op::Else => {
                    idx = self.exec_else(file, idx)?;
                }
                Op::ElseIf(ops) => {
                    idx = self.exec_elseif(&ops, file, idx)?;
                } // must have exact 1 op
                _ => {
                    idx += 1; /* just skip */
                }
            }
        }
        Ok(())
    }

    /// Looks for `end` statement for the current if/for/while considering nested if/for/while
    /// statements. Returns an error if corresponding `end` is not found.
    /// Used by engine when the current value of if/elseif/while/for is false.
    fn find_end(&self, file: usize, line: usize, tp: &str) -> Result<usize, HakuError> {
        let mut idx = line;
        let l = self.files[file].ops.len();
        let mut nesting = 1;
        while idx < l {
            let op = (self.files[file].ops[idx]).clone();
            match op.op {
                Op::StmtClose => {
                    nesting -= 1;
                    if nesting == 0 {
                        return Ok(idx + 1);
                    }
                }
                Op::If(_) | Op::While(_) | Op::For(_, _) => nesting += 1,
                _ => {}
            }
            idx += 1;
        }
        Err(HakuError::NoMatchingEndError(tp.to_string(), self.error_extra()))
    }

    /// Looks for `end`, `else` or `elseif` for the current `if` statement in case of `if`
    /// condition is false. Returns and error if the statement is not found.
    fn find_else(&self, file: usize, line: usize, tp: &str) -> Result<(bool, usize), HakuError> {
        let mut idx = line;
        let l = self.files[file].ops.len();
        let mut nesting = 1;
        while idx < l {
            let op = (self.files[file].ops[idx]).clone();
            match op.op {
                Op::StmtClose => {
                    nesting -= 1;
                    if nesting == 0 {
                        return Ok((true, idx + 1));
                    }
                }
                Op::If(_) | Op::While(_) | Op::For(_, _) => nesting += 1,
                Op::ElseIf(_) | Op::Else => {
                    if nesting == 1 {
                        return Ok((false, idx));
                    }
                }
                _ => {}
            }
            idx += 1;
        }
        Err(HakuError::NoMatchingEndError(tp.to_string(), self.error_extra()))
    }

    /// Executes external command and collects its output. The command and its output are not
    /// displayed. But if the stderr is not empty it is printed out. Before execution the
    /// engine substitutes used variables in command line.
    /// The output is expected to be valid UTF-8.
    ///
    /// Internal function to use by `for` or assignment statement.
    fn exec_cmd(&mut self, cmdline: &str) -> Result<ExecResult, HakuError> {
        let cmdline = self.varmgr.interpolate(&cmdline, true);
        let mut eres = ExecResult { code: 0, stdout: String::new() };
        let mut cmd = Command::new(&self.shell[0]);
        for arg in self.shell[1..].iter() {
            cmd.arg(arg);
        }
        cmd.arg(&cmdline);
        let out = match cmd.output() {
            Ok(o) => o,
            Err(e) => return Err(HakuError::ExecFailureError(cmdline, e.to_string(), self.error_extra())),
        };

        if !out.status.success() {
            if let Ok(s) = String::from_utf8(out.stderr) {
                eprint!("{}", s);
            }
            return Err(HakuError::ExecFailureError(
                cmdline.to_string(),
                format!("exit code {}", out.status.code().unwrap_or(0)),
                self.error_extra(),
            ));
        }

        if let Ok(s) = String::from_utf8(out.stdout) {
            eres.stdout = s.trim_end().to_string();
        } else {
            eres.stdout = String::from("[Non-UTF-8 Output]");
        }
        Ok(eres)
    }

    /// Executes external command and collects its standard and error output, and exit code.
    /// Before execution the engine substitutes used variables in command line.
    ///
    /// Used by script lines that are standalone shell calls, like `rm "${filename}"`
    fn exec_cmd_shell(&mut self, flags: u32, cmdline: &str) -> Result<(), HakuError> {
        let no_fail = is_flag_on(flags, FLAG_PASS);
        let cmdline = self.varmgr.interpolate(&cmdline, true);
        output!(self.opts.verbosity, 2, "ExecShell[{}]: {}", no_fail, cmdline);
        if !is_flag_on(flags, FLAG_QUIET) {
            println!("{}", cmdline);
        }

        let mut cmd = Command::new(&self.shell[0]);
        for arg in self.shell[1..].iter() {
            cmd.arg(arg);
        }
        cmd.arg(&cmdline);
        let result = cmd.status();
        let st = match result {
            Ok(exit_status) => exit_status,
            Err(e) => {
                if is_flag_on(flags, FLAG_PASS) {
                    return Ok(());
                }
                return Err(HakuError::ExecFailureError(cmdline, e.to_string(), self.error_extra()));
            }
        };

        if !st.success() && !is_flag_on(flags, FLAG_PASS) {
            let code = match st.code() {
                None => "(unknown exit code)".to_string(),
                Some(c) => format!("(exit code: {}", c),
            };
            return Err(HakuError::ExecFailureError(cmdline.to_string(), code, self.error_extra()));
        }

        Ok(())
    }

    /// Evaluates `ops` one by one and assigns the first non-falsy result to variable `name`.
    /// When `chk` is `true` it evaluates and assigns the new value only if the variable is
    /// falsy one(0, empty string, or shell command with non-zero exit code)
    fn exec_either_assign(&mut self, chk: bool, name: &str, ops: &[Op]) -> Result<(), HakuError> {
        if chk && self.varmgr.var(name).is_true() {
            return Ok(());
        }
        for op in ops.iter() {
            let v = self.exec_op(op)?;
            if v.is_true() {
                self.varmgr.set_var(name, v);
                return Ok(());
            }
        }
        Ok(())
    }

    /// Evaluates the whole expression `ops` and assigns a new value to variable `name`.
    /// When `chk` is `true` it evaluates and assigns the new value only if the variable is
    /// falsy one(0, empty string, or shell command with non-zero exit code).
    /// If evaluator detect logical expression (e.g., `$a > 10 && $msg =="text"), the
    /// final value is 0 or 1 depending on the result is false or true.
    fn exec_assign_generic(&mut self, chk: bool, name: &str, ops: &[Op]) -> Result<(), HakuError> {
        if chk && self.varmgr.var(name).is_true() {
            return Ok(());
        }
        let cnt = ops.len(); // 1=simple assign, >1=logical
        let mut val = false;
        for op in ops.iter() {
            let v = self.exec_op(op)?;
            if cnt == 1 {
                // simple assign
                self.varmgr.set_var(name, v);
                return Ok(());
            }
            if v.is_true() {
                val = true;
                break;
            }
        }
        if val {
            let v = VarValue::Int(1);
            self.varmgr.set_var(name, v);
        } else {
            let v = VarValue::Int(0);
            self.varmgr.set_var(name, v);
        }
        Ok(())
    }

    fn exec_assign_or(&mut self, name: &str, ops: &[Op]) -> Result<(), HakuError> {
        self.exec_assign_generic(true, name, ops)
    }

    fn exec_assign(&mut self, name: &str, ops: &[Op]) -> Result<(), HakuError> {
        self.exec_assign_generic(false, name, ops)
    }

    /// Evaluates `ops` one by one: return 1 if all items are evaluated as `true`,
    /// and returns 0 immediately when the first falsy value is met.
    fn exec_and_expr(&mut self, ops: &[Op]) -> Result<VarValue, HakuError> {
        let cnt = ops.len();
        let mut val = true;

        for op in ops.iter() {
            let v = self.exec_op(op)?;
            if cnt == 1 {
                // single value
                return Ok(v);
            }
            if !v.is_true() {
                val = false;
                break;
            }
        }
        if val {
            Ok(VarValue::Int(1))
        } else {
            Ok(VarValue::Int(0))
        }
    }

    /// Executes a built-in function. First, it tries to execute as a system function(that
    /// modifies internal engine state). If this way fails, executes the function in a common way.
    fn exec_func(&mut self, name: &str, ops: &[Op]) -> Result<VarValue, HakuError> {
        output!(self.opts.verbosity, 2, "Exec func {}, args: {:?}", name, ops);
        let mut args: Vec<VarValue> = Vec::new();
        for op in ops.iter() {
            let v = self.exec_op(op)?;
            args.push(v);
        }
        let r = run_func(name, &args);
        output!(self.opts.verbosity, 3, "func {} with {} args returned {:?}", name, ops.len(), r);
        match r {
            Ok(r) => Ok(r),
            Err(s) => Err(HakuError::FunctionError(format!("{}: {}", s, self.error_extra()))),
        }
    }

    /// Evaluates a condition `ops`. If it is true, starts executing `if` body. Otherwise,
    /// looks for corresponding `elseif`/`else`/`end` which comes first.
    fn exec_if(&mut self, ops: &[Op], file: usize, idx: usize) -> Result<usize, HakuError> {
        output!(self.opts.verbosity, 3, "Exec if");
        assert!(ops.len() == 1);
        let v = self.exec_op(&ops[0])?;
        if v.is_true() {
            output!(self.opts.verbosity, 3, "   if == true");
            self.cond_stack.push(CondItem { line: idx, cond: Condition::If(true) });
            Ok(idx + 1)
        } else {
            output!(self.opts.verbosity, 3, "   if == false -> look for else/end");
            let (is_end, else_idx) = self.find_else(file, idx + 1, "if")?;
            if !is_end {
                self.cond_stack.push(CondItem { line: idx, cond: Condition::If(false) });
            }
            Ok(else_idx)
        }
    }

    /// If the corresponding `if` or `elseif` condition is true, the function finishes `if`
    /// execution by looking for its `end`. Otherwise starts executing `else` body.
    fn exec_else(&mut self, file: usize, idx: usize) -> Result<usize, HakuError> {
        output!(self.opts.verbosity, 3, "Exec else");
        if self.cond_stack.is_empty() {
            return Err(HakuError::StrayElseError(self.error_extra()));
        }
        let op = self.cond_stack[self.cond_stack.len() - 1].clone();
        match op.cond {
            Condition::If(c) => {
                if c {
                    Ok(idx + 1)
                } else {
                    Ok(self.find_end(file, idx + 1, "else")?)
                }
            }
            _ => Err(HakuError::StrayElseError(self.error_extra())),
        }
    }

    /// If the corresponding `if` or previous `elseif` condition is true, the function
    /// finishes `if` execution by looking for its `end`. Otherwise, it evaluates `elseif`
    /// condition. If it is `true`, it starts executing `elseif` body. If `false`, looks
    /// for the next `elseif`/`else`/`end` which comes first.
    fn exec_elseif(&mut self, ops: &[Op], file: usize, idx: usize) -> Result<usize, HakuError> {
        output!(self.opts.verbosity, 3, "Exec elseif");
        if self.cond_stack.is_empty() {
            return Err(HakuError::StrayElseIfError(self.error_extra()));
        }
        assert!(ops.len() == 1);

        let op = self.cond_stack[self.cond_stack.len() - 1].clone();
        match op.cond {
            Condition::If(c) => {
                if !c {
                    return Ok(self.find_end(file, idx + 1, "else")?);
                }
                let v = self.exec_op(&ops[0])?;
                if v.is_true() {
                    let mut cnd = match self.cond_stack.pop() {
                        Some(cc) => cc,
                        None => return Err(HakuError::InternalError(self.error_extra())),
                    };
                    cnd.cond = Condition::If(true);
                    self.cond_stack.push(cnd);
                    Ok(idx + 1)
                } else {
                    let (_, else_idx) = self.find_else(file, idx + 1, "elseif")?;
                    Ok(else_idx)
                }
            }
            _ => Err(HakuError::StrayElseIfError(self.error_extra())),
        }
    }

    /// Evaluates a condition `ops`. If it is true, starts executing `while` body. Otherwise,
    /// looks for corresponding `end`.
    fn exec_while(&mut self, ops: &[Op], idx: usize) -> Result<bool, HakuError> {
        output!(self.opts.verbosity, 3, "Exec while {:?}", ops);
        assert!(ops.len() == 1);
        let v = self.exec_op(&ops[0])?;
        if v.is_true() {
            let lst: Vec<Op> = ops.to_vec();
            self.cond_stack.push(CondItem { line: idx, cond: Condition::While(lst) });
        }
        Ok(v.is_true())
    }

    /// Processed `end` statement. For `if` it just continues execution. For `while` and `for`
    /// it checks the current loop condition: if `true`, it start the next loop cycle;
    /// if `false`, it continues execution for the next line.
    fn exec_end(&mut self) -> Result<usize, HakuError> {
        output!(self.opts.verbosity, 3, "Exec end");
        if let Some(op) = self.cond_stack.pop() {
            output!(self.opts.verbosity, 3, "END OP >> {:?}", op);
            match op.cond {
                Condition::If(_) => Ok(0), // just continue
                Condition::While(ref ops) => {
                    // should be 1 operation
                    assert!(ops.len() == 1);
                    let val = self.exec_op(&ops[0])?;
                    if val.is_true() {
                        let ln = op.line + 1;
                        self.cond_stack.push(op);
                        Ok(ln)
                    } else {
                        Ok(0)
                    }
                }
                Condition::ForList(var, mut vals) => {
                    output!(self.opts.verbosity, 3, "END FOR LIST: {} = {:?}", var, vals);
                    if vals.is_empty() {
                        return Ok(0);
                    }
                    let val = vals[0].clone();
                    vals.remove(0);
                    self.varmgr.set_var(&var, VarValue::Str(val));
                    self.cond_stack.push(CondItem { line: op.line, cond: Condition::ForList(var, vals) });
                    Ok(op.line + 1)
                }
                Condition::ForInt(var, mut curr, end, step) => {
                    curr += step;
                    output!(self.opts.verbosity, 3, "END FOR INT: {} of {}", curr, end);
                    if (step > 0 && curr >= end) || (step < 0 && curr <= end) {
                        return Ok(0);
                    }
                    self.varmgr.set_var(&var, VarValue::Int(curr));
                    self.cond_stack.push(CondItem { line: op.line, cond: Condition::ForInt(var, curr, end, step) });
                    Ok(op.line + 1)
                }
            }
        } else {
            Err(HakuError::StrayEndError(self.error_extra()))
        }
    }

    /// Breaks the current `for` or `while`.
    fn exec_break(&mut self, file: usize) -> Result<usize, HakuError> {
        output!(self.opts.verbosity, 3, "Exec break");
        while let Some(cnd) = self.cond_stack.pop() {
            match cnd.cond {
                Condition::If(_) => continue,
                _ => {
                    return Ok(self.find_end(file, cnd.line + 1, "break")?);
                }
            }
        }
        Err(HakuError::NoMatchingForWhileError(self.error_extra()))
    }

    /// Restarts the current `for` or `while` cycle from its first line.
    fn exec_continue(&mut self, file: usize) -> Result<usize, HakuError> {
        output!(self.opts.verbosity, 3, "Exec continue");
        let mut next: usize = usize::MAX;
        while let Some(cnd) = self.cond_stack.pop() {
            match cnd.cond {
                Condition::If(_) => continue,
                _ => {
                    next = self.find_end(file, cnd.line + 1, "continue")?;
                    self.cond_stack.push(cnd.clone());
                    break;
                }
            }
        }
        if next == usize::MAX {
            Err(HakuError::NoMatchingForWhileError(self.error_extra()))
        } else {
            // step back to point to END statement
            Ok(next - 1)
        }
    }

    /// Initialize `for` loop. Calculates its execution range or list of values and
    /// starts executing from the first one(if the initial conditions are valid). Otherwise,
    /// skips the loop by looking for the corresponding `end` statement.
    fn exec_for(&mut self, name: &str, seq: Seq, idx: usize) -> Result<bool, HakuError> {
        output!(self.opts.verbosity, 3, "Exec for");
        match seq {
            Seq::Int(start, end, step) => {
                output!(self.opts.verbosity, 3, "  FOR: from {} to {} step {}", start, end, step);
                if (step > 0 && end <= start) || (step < 0 && end >= start) {
                    // look for an END
                    return Ok(false);
                }
                if step == 0 {
                    return Err(HakuError::ForeverForError(self.error_extra()));
                }
                self.varmgr.set_var(name, VarValue::Int(start));
                self.cond_stack
                    .push(CondItem { line: idx, cond: Condition::ForInt(name.to_string(), start, end, step) });
                return Ok(true);
            }
            Seq::Str(s) => {
                let s = self.varmgr.interpolate(&s, false);
                let mut v: Vec<String> = if s.find('\n').is_some() {
                    s.trim_end().split('\n').map(|s| s.trim_end().to_string()).collect()
                } else {
                    s.split_ascii_whitespace().map(|s| s.to_string()).collect()
                };
                output!(self.opts.verbosity, 3, "   FOR whitespace: {:?}", v);
                if v.is_empty() {
                    return Ok(false);
                }
                self.varmgr.set_var(name, VarValue::Str(v[0].clone()));
                v.remove(0);
                self.cond_stack.push(CondItem { line: idx, cond: Condition::ForList(name.to_string(), v) });
                return Ok(true);
            }
            Seq::Idents(ids) => {
                output!(self.opts.verbosity, 3, "  FOR idents: {:?}", ids);
                if ids.is_empty() {
                    return Ok(false);
                }
                self.varmgr.set_var(name, VarValue::Str(ids[0].clone()));
                let v: Vec<String> = ids.iter().skip(1).map(|s| s.to_string()).collect();
                self.cond_stack.push(CondItem { line: idx, cond: Condition::ForList(name.to_string(), v) });
                return Ok(true);
            }
            Seq::Exec(s) => match self.exec_cmd(&s) {
                Ok(res) => {
                    if res.code == 0 {
                        let mut v: Vec<String> = res.stdout.lines().map(|s| s.trim_end().to_string()).collect();
                        output!(self.opts.verbosity, 3, "   FOR lines: {:?}", v);
                        if v.is_empty() {
                            return Ok(false);
                        }
                        self.varmgr.set_var(name, VarValue::Str(v[0].clone()));
                        v.remove(0);
                        self.cond_stack.push(CondItem { line: idx, cond: Condition::ForList(name.to_string(), v) });
                        return Ok(true);
                    } else {
                        output!(self.opts.verbosity, 3, "   FOR lines: FAILURE");
                    };
                }
                Err(_) => {
                    output!(self.opts.verbosity, 3, "   FOR lines: FAILURE[2]");
                }
            },
        }
        Ok(false)
    }

    /// Compares two variables. Returns 1 if condition is true, and 0 otherwise.
    fn exec_compare(&mut self, cmp_op: &str, args: &[Op]) -> Result<VarValue, HakuError> {
        // compare always get 2 arguments
        assert!(args.len() == 2);
        let v1 = self.exec_op(&args[0])?;
        let v2 = self.exec_op(&args[1])?;
        if v1.cmp(&v2, cmp_op) {
            Ok(VarValue::Int(1))
        } else {
            Ok(VarValue::Int(0))
        }
    }

    /// Generic function: executes any expression value(variable, shell exec, function).
    fn exec_op(&mut self, op: &Op) -> Result<VarValue, HakuError> {
        match op {
            Op::Int(i) => Ok(VarValue::Int(*i)),
            Op::Str(s) => {
                let s = self.varmgr.interpolate(&s, false);
                Ok(VarValue::Str(s))
            }
            Op::Var(name) => Ok(self.varmgr.var(name)),
            Op::Exec(s) => match self.exec_cmd(s) {
                Err(_) => Ok(VarValue::Undefined),
                Ok(er) => Ok(VarValue::Exec(er)),
            },
            Op::Not(ops) => {
                // now Not must contain only 1 op - it should be by *.pest rules
                for o in ops.iter() {
                    let v = self.exec_op(o)?;
                    if v.is_true() {
                        return Ok(VarValue::Int(0));
                    } else {
                        return Ok(VarValue::Int(1));
                    }
                }
                unreachable!()
            }
            Op::AndExpr(ops) => self.exec_and_expr(ops),
            Op::Func(name, ops) => self.exec_func(name, ops),
            Op::Compare(cmp_op, ops) => self.exec_compare(cmp_op, ops),
            _ => unreachable!(),
        }
    }

    /// Executed before staring the next recipe. It does all preparations, like recipe
    /// local variable initialization.
    fn enter_recipe(&mut self, recipe: &RecipeItem) {
        output!(self.opts.verbosity, 2, "enter recipe. Vars {:?}, Free {:?}", recipe.vars, self.varmgr.free);
        if recipe.vars.is_empty() || self.varmgr.free.is_empty() {
            return;
        }

        // init recipe vars
        let mut idx = 0usize;
        for v in recipe.vars.iter() {
            if v.starts_with('+') {
                let nm = v.trim_start_matches('+');
                let mut out = Vec::new();
                while idx < self.varmgr.free.len() {
                    out.push(self.varmgr.free[idx].clone());
                    idx += 1;
                }
                self.varmgr.set_recipe_var(nm, VarValue::List(out));
                return;
            } else {
                self.varmgr.set_recipe_var(v, VarValue::Str(self.varmgr.free[idx].clone()));
                idx += 1;
                if idx >= self.varmgr.free.len() {
                    return;
                }
            }
        }
    }

    /// When the last line of a recipe is done, it cleans up temporary resources allocated
    /// for the recipe (e.g. deletes all recipe local variables)
    fn leave_recipe(&mut self) {
        self.varmgr.recipe_vars.clear();
        self.cond_stack.clear();
    }
}

#[cfg(test)]
mod vm_test {
    use super::*;
    use std::mem;

    struct Prs {
        expr: &'static str,
        tp: Op,
    }

    #[test]
    fn load() {
        let opts = RunOpts::new();
        let mut vm = Engine::new(opts);
        let res = vm.load_from_str("recipe: deps");
        assert!(res.is_ok());
        assert_eq!(vm.files.len(), 1);
        assert_eq!(vm.recipes.len(), 1);
        assert_eq!(vm.files[0].ops.len(), 1);
        assert_eq!(vm.files[0].disabled.len(), 0);
        assert_eq!(
            mem::discriminant(&vm.files[0].ops[0].op),
            mem::discriminant(&Op::Recipe(String::new(), 0, Vec::new(), Vec::new()))
        );
    }

    #[test]
    fn ops() {
        let parses: Vec<Prs> = vec![
            Prs { expr: "run('cmd')", tp: Op::Func(String::new(), Vec::new()) },
            Prs { expr: "run('cmd', `abs`, inner(10,2,3))", tp: Op::Func(String::new(), Vec::new()) },
            Prs { expr: "END", tp: Op::StmtClose },
            Prs { expr: "Return", tp: Op::Return },
            Prs { expr: "ELse", tp: Op::Else },
            Prs { expr: "brEAk", tp: Op::Break },
            Prs { expr: "continuE", tp: Op::Continue },
            Prs { expr: "a = `ls` || `dir` && 12 == r#zcv#", tp: Op::Assign(String::new(), Vec::new()) },
            Prs { expr: "a = `ls` || !`dir` && 12 || $ui != 'zcv'", tp: Op::Assign(String::new(), Vec::new()) },
            Prs { expr: "a ?= `ls` || `dir` || r#default#", tp: Op::DefAssign(String::new(), Vec::new()) },
            Prs { expr: "a = `ls` ? `dir` ? 'default'", tp: Op::EitherAssign(false, String::new(), Vec::new()) },
            Prs { expr: "a ?= `ls` ? `dir` ? 'default'", tp: Op::EitherAssign(false, String::new(), Vec::new()) },
            Prs { expr: "if $a > `dir | wc -l` || $b == 'test${zef}':", tp: Op::If(Vec::new()) },
            Prs { expr: "if $a > `dir | wc -l` || $b == 'test${zef}' ; do", tp: Op::If(Vec::new()) },
            Prs { expr: "if $a > `dir | wc -l` || $b == 'test${zef}' then", tp: Op::If(Vec::new()) },
            Prs { expr: "while `ping ${ip}`:", tp: Op::While(Vec::new()) },
            Prs { expr: "while `ping ${ip}` && $b == 90 do", tp: Op::While(Vec::new()) },
            Prs { expr: "for a in 1..2:", tp: Op::For(String::new(), Seq::Int(0, 0, 0)) },
            Prs { expr: "for a in 1..2..8 do", tp: Op::For(String::new(), Seq::Int(0, 0, 0)) },
            Prs { expr: "for a in 'a b c d' then", tp: Op::For(String::new(), Seq::Int(0, 0, 0)) },
            Prs { expr: "for a in a b c d :", tp: Op::For(String::new(), Seq::Int(0, 0, 0)) },
            Prs { expr: "for a in `dir *.*`", tp: Op::For(String::new(), Seq::Int(0, 0, 0)) },
        ];
        for p in parses {
            let opts = RunOpts::new();
            let mut vm = Engine::new(opts);
            let res = vm.load_from_str(p.expr);
            assert!(res.is_ok());
            println!("{}", p.expr);
            assert_eq!(mem::discriminant(&vm.files[0].ops[0].op), mem::discriminant(&p.tp));
        }
    }
}
