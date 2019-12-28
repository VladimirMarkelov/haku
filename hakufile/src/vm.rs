use std::convert::From;
use std::fmt;
use std::process::Command;
use std::io;
use std::iter::FromIterator;
use std::usize;

use log::{debug, info, trace, warn};

use crate::parse::{HakuFile,DisabledRecipe};
use crate::errors::HakuError;
use crate::ops::{Op, Seq, FLAG_PASS, FLAG_QUIET, is_flag_on};
use crate::func::{run_func};
use crate::var::{VarMgr,VarValue,ExecResult};

const DEFAULT_SECTION: &str = "_default";

pub struct RunOpts {
    pub(crate) feats: Vec<String>,
    dry_run: bool
}

impl RunOpts {
    pub fn new() -> Self {
        RunOpts {
            dry_run: false,
            feats: Vec::new(),
        }
    }

    pub fn with_dry_run(self, dry_run: bool) -> Self {
        RunOpts {
            dry_run,
            ..self
        }
    }

    pub fn with_features(self, feats: Vec<String>) -> Self {
        RunOpts {
            feats,
            ..self
        }
    }
}
#[derive(Clone)]
pub struct RecipeDesc {
    pub name: String,
    pub desc: String,
    pub depends: Vec<String>,
    pub system: bool,
    pub loc: RecipeLoc,
    pub flags: u32,
    pub vars: Vec<String>,
}

impl fmt::Display for RecipeDesc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut r = write!(f, "{}", self.name)?;
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
            r = write!(f, "]")?;
        }
        if !self.desc.is_empty() {
            r = write!(f, " #{}", self.desc)?;
        }
        Ok(r)
    }
}

#[derive(Clone,Debug)]
enum Condition {
    If(bool), // whether `if` condition is true
    While(Vec<Op>),
    ForInt(String, i64, i64, i64), // variable, current, final, step
    ForList(String, Vec<String>),  // variable, list of values
}
#[derive(Clone,Debug)]
struct CondItem {
    line: usize,
    cond: Condition,
}

pub struct Engine {
    files: Vec<HakuFile>, // Hakufile in order of includes
    included: Vec<String>, // which files were included to catch recursion
    recipes: Vec<RecipeDesc>,
    verbosity: usize,
    logfile: String,
    varmgr: VarMgr,

    cond_stack: Vec<CondItem>,
    real_line: usize,
}

#[derive(Debug,Clone)]
pub struct RecipeLoc {
    pub file: usize,
    pub line: usize,
}

#[derive(Debug)]
struct RecipeItem {
    name: String,
    loc: RecipeLoc,
    vars: Vec<String>,
    flags: u32,
}

impl Engine {
    pub fn new(verbosity: usize, logfile: &str) -> Self {
        let eng = Engine {
            files: Vec::new(),
            included: Vec::new(),
            recipes: Vec::new(),
            verbosity,
            logfile: logfile.to_string(),
            varmgr: VarMgr::new(),
            cond_stack: Vec::new(),
            real_line: usize::MAX,
        };
        eng.init_logging();
        eng
    }

    fn init_logging(&self) {
        if self.verbosity == 0 {
            return;
        }
        let logfile = if self.logfile.is_empty() {
            "haku.log"
        } else {
            &self.logfile
        };

        let mut base_config = fern::Dispatch::new();
        base_config = match self.verbosity {
            0 => base_config
                    .level(log::LevelFilter::Info)
                    .level_for("overly-verbose-target", log::LevelFilter::Warn),
            1 => base_config
                    .level(log::LevelFilter::Debug)
                    .level_for("overly-verbose-target", log::LevelFilter::Info),
            2 => base_config.level(log::LevelFilter::Debug),
            _ => base_config.level(log::LevelFilter::Trace),
        };
        let flog = match fern::log_file(logfile) {
            Ok(fl) => fl,
            Err(e) => {
                eprintln!("Failed to initialize log: {:?}", e);
                return;
            }
        };
        let file_config = fern::Dispatch::new()
            .format(|out, message, record| {
                let lvl = format!("{}", record.level());
                out.finish(format_args!(
                        "{}{} {}",
                        lvl.chars().next().unwrap(),
                        chrono::Local::now().format("[%y-%m-%d %H:%M:%S%.3f]"),
                        message
                ))
            })
            .chain(flog);

        if self.verbosity < 2 {
            if let Err(e) = base_config.chain(file_config).apply() {
                eprintln!("Failed to initialize log: {:?}", e);
            }
            return;
        }
        let stdout_config = fern::Dispatch::new()
            .format(|out, message, record| {
                let lvl = format!("{}", record.level());
                out.finish(format_args!(
                    "{}[{}] {}",
                        lvl.chars().next().unwrap(),
                    chrono::Local::now().format("%H:%M:%S%.3f"),
                    message
                ))
            })
            .chain(io::stdout());

        if let Err(e) = base_config
            .chain(file_config)
            .chain(stdout_config)
            .apply() {
                eprintln!("Failed to initialize log: {:?}", e);
        }
    }

    pub fn load_file(&mut self, filepath: &str, opts: &RunOpts) -> Result<(), HakuError> {
        debug!("Loading file: {}", filepath);
        for s in &self.included {
            if s == filepath {
                return Err(HakuError::IncludeRecursionError(filepath.to_string()));
            }
        }
        let hk = HakuFile::load_file(filepath, opts)?;
        self.files.push(hk);
        self.included.push(filepath.to_string());
        self.run_header(self.files.len()-1, opts)?;
        self.detect_recipes();
        Ok(())
    }

    fn run_header(&mut self, idx: usize, opts: &RunOpts) -> Result<(), HakuError> {
        debug!("RUN HEADER: {}: {}", idx, self.files[idx].ops.len());
        let mut to_include: Vec<String> = Vec::new();
        let mut to_include_flags: Vec<u32> = Vec::new();
        for op in &self.files[idx].ops {
            self.real_line = op.line;
            match &op.op {
                Op::Feature(_, _) => { /* Since dead code is removed, Feature can be just skipped */ },
                Op::Recipe(_,_,_,_) => break,
                Op::Comment(_) | Op::DocComment(_) => { /* just continue */ },
                Op::Include(flags, path) => {
                    debug!("        !!INCLUDE - {}", path);
                    to_include.push(path.to_string());
                    to_include_flags.push(*flags);
                },
                _ => { /*run = true */ },
            }
        }
        debug!("TO INCLUDE: {}", to_include.len());
        for (i, path) in to_include.iter().enumerate() {
            let f = to_include_flags[i];
            let res = self.load_file(path, opts);
            debug!("ERROR: {:?}", res);
            if res.is_err() && !is_flag_on(f, FLAG_PASS) {
                return res;
            }
            eprintln!("Skipping included file: {:?}", res);
        }
        Ok(())
    }

    fn is_system_recipe(name: &str) -> bool {
        name == "_default"
            || name == "_before"
            || name == "_after"
    }

    fn detect_recipes(&mut self) {
        for (file_idx, hk) in self.files.iter().enumerate() {
            let mut desc = String::new();
            // let mut pass = true;
            for (line_idx, op) in hk.ops.iter().enumerate() {
                match op.op {
                    Op::Feature(_, _) => {}, //pass &= b,
                    Op::DocComment(ref s) => desc = s.clone(),
                    Op::Recipe(ref nm, flags, ref vars, ref deps) => {
                        let mut recipe = RecipeDesc{
                            name: nm.clone(),
                            desc: desc.clone(),
                            loc: RecipeLoc{
                                line: line_idx,
                                file: file_idx,
                            },
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
                        // pass = true;
                    },
                    Op::Comment(_) => {
                        // do not change anything
                    },
                    _ => {
                        desc.clear();
                        // pass = true;
                    },
                }
            }
        }
        self.recipes.sort_by(|a, b| a.name.partial_cmp(&b.name).unwrap());
    }

    pub fn file_name(&self, file_idx: usize) -> Result<&str, HakuError> {
        if file_idx >= self.files.len() {
            return Err(HakuError::FileNotLoaded(file_idx));
        }
        return Ok(&self.included[file_idx]);
    }

    pub fn recipes(&self) -> &[RecipeDesc] {
        return &self.recipes
    }

    pub fn disabled_recipes(&self) -> Vec<DisabledRecipe> {
        let mut v = Vec::new();
        for fidx in 0..self.files.len() {
            for ds in self.files[fidx].disabled.iter() {
                v.push(ds.clone());
            }
        }
        v
    }

    fn find_recipe(&self, name: &str) -> Result<RecipeDesc, HakuError> {
        for sec in &self.recipes {
            if sec.name == name {
                return Ok(sec.clone());
            }
        }
        Err(HakuError::RecipeNotFoundError(name.to_string()))
    }

    pub fn set_free_args(&mut self, args: &[String]) {
        self.varmgr.free = Vec::from_iter(args.iter().cloned());
    }

    pub fn run_recipe(&mut self, name: &str, opts: RunOpts) -> Result<(), HakuError> {
        debug!("Running SECTION '{}'", name);
        let sec_res = if name.is_empty() {
            match self.find_recipe(DEFAULT_SECTION) {
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

    fn exec_file_init(&mut self, file: usize) -> Result<(), HakuError> {
        // let mut pass = true;
        let cnt = self.files[file].ops.len();
        let mut i = 0;
        while i < cnt {
            let op = self.files[file].ops[i].clone();
            self.real_line = op.line;
            match op.op {
                // Op::Feature(b, _) => pass &= b,
                Op::Recipe(_, _, _, _) | Op::Return => return Ok(()),
                Op::Include(_, _) => { i += 1; },
                Op::DocComment(_) | Op::Comment(_) => { i += 1; },
                Op::Shell(flags, cmd) => { self.exec_cmd_shell(flags, &cmd, i)?; i += 1; },
                Op::EitherAssign(chk, name, ops) => { self.exec_either_assign(chk, &name, &ops)?; i += 1; },
                Op::DefAssign(name, ops) => { self.exec_assign_or(&name, &ops)?; i += 1; },
                Op::Assign(name, ops) => { self.exec_assign(&name, &ops)?; i += 1; },
                Op::Func(name, ops) => { self.exec_func(&name, &ops)?; i += 1; }, // top level - func value is dropped
                Op::StmtClose => { let next = self.exec_end()?; if next == 0 {i += 1;} else { i = next; }},
                Op::For(name, seq) => {
                    let ok = self.exec_for(&name, seq, i)?;
                    if ok {
                        i += 1;
                    } else {
                        i = self.find_end(file, i+1, "for")?;
                    }
                },
                Op::While(ops) => {
                    // must have exact 1 op
                    let ok = self.exec_while(&ops, i)?;
                    if ok {
                        i += 1;
                    } else {
                        i = self.find_end(file, i+1, "while")?;
                    }
                },
                Op::Break => { i = self.exec_break(file)?; },
                Op::Continue => { i = self.exec_continue(file)?; },
                Op::If(ops) => { i = self.exec_if(&ops, file, i)?; },
                Op::Else => { i = self.exec_else(file, i)?; },
                Op::ElseIf(ops) => { i = self.exec_elseif(&ops, file, i)?; }, // must have exact 1 op
                _ => {
                    debug!("UNIMPLEMENTED: {:?}", op);
                    i += 1;
                },
            }
        }
        Ok(())
    }

    fn exec_init(&mut self) -> Result<(), HakuError> {
        let cnt  = self.files.len();
        for i in 0..cnt {
            self.exec_file_init(cnt - i - 1)?;
        }
        Ok(())
    }

    fn push_recipe(&mut self, loc: RecipeLoc, found: Option<&[RecipeItem]>, parent: Option<&[String]>) -> Result<Vec<RecipeItem>, HakuError> {
        let op = self.files[loc.file].ops[loc.line].clone();
        let mut sec_item: RecipeItem = RecipeItem{
            name: String::new(),
            loc: RecipeLoc{file: 0, line: 0},
            vars: Vec::new(),
            flags: 0,
        };
        debug!("Checking recipe: {:?}", op);
        let mut vc: Vec<RecipeItem> = Vec::new();
        let mut parents: Vec<String> = match parent {
            None => Vec::new(),
            Some(p) => p.iter().map(|a| a.to_string()).collect(),
        };
        match op.op {
            Op::Recipe(name, flags, vars, deps) => {
                if vc.iter().any(|s| s.name == name)
                    || deps.iter().any(|d| d == &name) {
                    return Err(HakuError::RecipeRecursionError(name, HakuError::err_line(op.line)));
                }
                for dep in deps {
                    if let Some(ps) = parent {
                        if ps.iter().any(|p| p == &dep) {
                            return Err(HakuError::RecipeRecursionError(dep, HakuError::err_line(op.line)));
                        }
                    }
                    if let Some(fnd) = found {
                        if fnd.iter().any(|f| f.name == dep) {
                            return Err(HakuError::RecipeRecursionError(dep, HakuError::err_line(op.line)));
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
            },
            _ => unreachable!(),
        }
        vc.push(sec_item);
        Ok(vc)
    }

    fn exec_recipe(&mut self, loc: RecipeLoc) -> Result<(), HakuError> {
        debug!("Start recipe [{}:{}]", loc.file, loc.line);
        let sec = self.push_recipe(loc, None, None)?;
        debug!("recipe call stack: {:?}", sec);
        let mut idx = 0;
        while idx < sec.len() {
            let op = &sec[idx];
            debug!("Starting recipe: {}", op.name);
            self.enter_recipe(op);
            self.exec_from(op.loc.file, op.loc.line+1, op.flags)?;
            self.leave_recipe();
            idx += 1;
        }
        Ok(())
    }

    fn exec_from(&mut self, file: usize, line: usize, sec_flags: u32) -> Result<(), HakuError> {
        let mut idx = line;
        let l = self.files[file].ops.len();
        while idx < l {
            let op = (self.files[file].ops[idx]).clone();
            self.real_line = op.line;
            match op.op {
                Op::Return | Op::Recipe(_,_,_,_) => return Ok(()),
                Op::Include(_,_) => return Err(HakuError::IncludeInRecipeError(HakuError::err_line(self.real_line))),
                Op::Shell(flags, cmd) => {
                    let cmd_flags = sec_flags ^ flags;
                    self.exec_cmd_shell(cmd_flags, &cmd, idx)?; idx += 1;
                },
                Op::EitherAssign(chk, name, ops) => { self.exec_either_assign(chk, &name, &ops)?; idx += 1; },
                Op::DefAssign(name, ops) => { self.exec_assign_or(&name, &ops)?; idx += 1; },
                Op::Assign(name, ops) => { self.exec_assign(&name, &ops)?; idx += 1; },
                Op::Func(name, ops) => { self.exec_func(&name, &ops)?; idx += 1; }, // top level - func value is dropped
                Op::StmtClose => { let next = self.exec_end()?; if next == 0 { idx += 1} else { idx = next; }},
                Op::For(name, seq) => {
                    let ok = self.exec_for(&name, seq, idx)?;
                    if ok {
                        idx += 1;
                    } else {
                        idx = self.find_end(file, idx+1, "for")?;
                    }
                },
                Op::While(ops) => {
                    // must have exact 1 op
                    let ok = self.exec_while(&ops, idx)?;
                    if ok {
                        idx += 1;
                    } else {
                        idx = self.find_end(file, idx+1, "while")?;
                    }
                },
                Op::Break => { idx = self.exec_break(file)?; },
                Op::Continue => { idx = self.exec_continue(file)?; },
                Op::If(ops) => { idx = self.exec_if(&ops, file, idx)?; }, // must have exact 1 op
                Op::Else => { idx = self.exec_else(file, idx)?; },
                Op::ElseIf(ops) => { idx = self.exec_elseif(&ops, file, idx)?; }, // must have exact 1 op
                _ => { idx += 1;/* just skip */ },
            }
        }
        Ok(())
    }

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
                        return Ok(idx+1);
                    }
                },
                Op::If(_) | Op::While(_) | Op::For(_,_) => nesting += 1,
                _ => {},
            }
            idx += 1;
        }
        Err(HakuError::NoMatchingEndError(tp.to_string(), HakuError::err_line(self.real_line)))
    }

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
                        return Ok((true, idx+1));
                    }
                },
                Op::If(_) | Op::While(_) | Op::For(_,_) => nesting += 1,
                Op::ElseIf(_) | Op::Else => {
                    if nesting == 1 {
                        return Ok((false, idx));
                    }
                },
                _ => {},
            }
            idx += 1;
        }
        Err(HakuError::NoMatchingEndError(tp.to_string(), HakuError::err_line(self.real_line)))
    }

    fn exec_cmd(&mut self, cmdline: &str) -> Result<ExecResult, HakuError> {
        let cmdline = self.varmgr.interpolate(&cmdline, true);
        let mut eres = ExecResult {
            code: 0,
            stdout: String::new(),
        };
        let out = match Command::new("powershell")
            .arg("-c")
            .arg(&cmdline).output() {
            Ok(o) => o,
            Err(e) => return Err(HakuError::ExecFailureError(cmdline, e.to_string(), HakuError::err_line(self.real_line))),
        };

        if !out.status.success() {
            if let Ok(s) = String::from_utf8(out.stderr) {
                eprint!("{}", s);
            }
            return Err(HakuError::ExecFailureError(cmdline.to_string(), format!("exit code {}", out.status.code().unwrap_or(0)), HakuError::err_line(self.real_line)));
        }

        if let Ok(s) = String::from_utf8(out.stdout) {
            eres.stdout = s.trim_end().to_string();
        } else {
            eres.stdout = String::from("[Non-UTF-8 Output]");
        }
        Ok(eres)
    }

    fn exec_cmd_shell(&mut self, flags: u32, cmdline: &str, line: usize) -> Result<(), HakuError> {
        let no_fail = is_flag_on(flags, FLAG_PASS);
        let cmdline = self.varmgr.interpolate(&cmdline, true);
        debug!("ExecShell[{}]: {}", no_fail, cmdline);
        if !is_flag_on(flags, FLAG_QUIET) {
            println!("{}", cmdline);
        }

        let result = Command::new("powershell")
            .arg("-c")
            .arg(&cmdline)
            .status();
        let st = match result {
            Ok(exit_status) => exit_status,
            Err(e) => {
                if is_flag_on(flags, FLAG_PASS) {
                    return Ok(());
                }
                return Err(HakuError::ExecFailureError(
                    cmdline,
                    e.to_string(),
                    HakuError::err_line(line)));
            },
        };

        if !st.success() && !is_flag_on(flags, FLAG_PASS) {
            let code = match st.code() {
                None => "(unknown exit code)".to_string(),
                Some(c) => format!("(exit code: {}", c),
            };
            return Err( HakuError::ExecFailureError(
                    cmdline.to_string(),
                    code,
                    HakuError::err_line(line),
            ));
        }

        Ok(())
    }

    fn exec_either_assign(&mut self, chk: bool, name: &str, ops: &[Op]) -> Result<(), HakuError> {
        if chk && self.varmgr.var(name).is_true() {
            return Ok(());
        }
        for op in ops.iter() {
            let v = self.exec_op(op)?;
            if v.is_true() {
                debug!("{:?} -> true", op);
                self.varmgr.set_var(name, v);
                return Ok(());
            }
        }
        Ok(())
    }

    fn exec_assign_generic(&mut self, chk: bool, name: &str, ops: &[Op]) -> Result<(), HakuError> {
        if chk && self.varmgr.var(name).is_true() {
            return Ok(());
        }
        let cnt = ops.len(); // 1=simple assing, >1=logical
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

    fn exec_func(&mut self, name: &str, ops: &[Op]) -> Result<VarValue, HakuError> {
        debug!("Exec func {}", name);
        let mut args: Vec<VarValue> = Vec::new();
        for op in ops.iter() {
            let v = self.exec_op(op)?;
            args.push(v);
        }
        let r = run_func(name, &args);
        debug!("func {} with {} args returned {:?}", name, ops.len(), r);
        match r {
            Ok(r) => Ok(r),
            Err(s) => Err(HakuError::FunctionError(format!("{}: {}", s, HakuError::err_line(self.real_line)))),
        }
    }

    fn exec_if(&mut self, ops: &[Op], file: usize, idx: usize) -> Result<usize, HakuError> {
        debug!("Exec if");
        assert!(ops.len() == 1);
        let v = self.exec_op(&ops[0])?;
        if v.is_true() {
            debug!("   if == true");
            self.cond_stack.push(CondItem{line: idx, cond: Condition::If(true)});
            Ok(idx+1)
        } else {
            debug!("   if == false -> look for else/end");
            let (is_end, else_idx) = self.find_else(file, idx+1, "if")?;
            if !is_end {
                self.cond_stack.push(CondItem{line: idx, cond: Condition::If(false)});
            }
            Ok(else_idx)
        }
    }

    fn exec_else(&mut self, file: usize, idx: usize) -> Result<usize, HakuError> {
        debug!("Exec else");
        if self.cond_stack.is_empty() {
            return Err(HakuError::StrayElseError(HakuError::err_line(self.real_line)));
        }
        let op = self.cond_stack[self.cond_stack.len() - 1].clone();
        match op.cond {
            Condition::If(c) => {
                if c {
                    Ok(idx+1)
                } else {
                    Ok(self.find_end(file, idx+1, "else")?)
                }
            },
            _ => Err(HakuError::StrayElseError(HakuError::err_line(self.real_line))),
        }
    }

    fn exec_elseif(&mut self, ops: &[Op], file: usize, idx: usize) -> Result<usize, HakuError> {
        debug!("Exec elseif");
        if self.cond_stack.is_empty() {
            return Err(HakuError::StrayElseIfError(HakuError::err_line(self.real_line)));
        }
        assert!(ops.len() == 1);

        let op = self.cond_stack[self.cond_stack.len() - 1].clone();
        match op.cond {
            Condition::If(c) => {
                if !c {
                    return Ok(self.find_end(file, idx+1, "else")?);
                }
                let v = self.exec_op(&ops[0])?;
                if v.is_true() {
                    let mut cnd = match self.cond_stack.pop() {
                        Some(cc) => cc,
                        None => return Err(HakuError::InternalError(HakuError::err_line(self.real_line))),
                    };
                    cnd.cond = Condition::If(true);
                    self.cond_stack.push(cnd);
                    Ok(idx + 1)
                } else {
                    let (_, else_idx) = self.find_else(file, idx+1, "elseif")?;
                    Ok(else_idx)
                }
            },
            _ => Err(HakuError::StrayElseIfError(HakuError::err_line(self.real_line))),
        }
    }

    fn exec_while(&mut self, ops: &[Op], idx: usize) -> Result<bool, HakuError> {
        debug!("Exec while {:?}", ops);
        assert!(ops.len() == 1);
        let v = self.exec_op(&ops[0])?;
        if v.is_true() {
            debug!("   while == true");
            let lst: Vec<Op> = ops.iter().cloned().collect();
            self.cond_stack.push(CondItem{line: idx, cond: Condition::While(lst)});
        } else {
            debug!("   while == false -> look for end");
        }
        Ok(v.is_true())
    }

    fn exec_end(&mut self) -> Result<usize, HakuError> {
        debug!("Exec end");
        if let Some(op) = self.cond_stack.pop() {
            debug!("END OP >> {:?}", op);
            match op.cond {
                Condition::If(_) => return Ok(0), // just continue
                Condition::While(ref ops) => {
                    // should be 1 operation
                    assert!(ops.len() == 1);
                    let val = self.exec_op(&ops[0])?;
                    if val.is_true() {
                        let ln = op.line + 1;
                        self.cond_stack.push(op);
                        return Ok(ln);
                    } else {
                        return Ok(0);
                    }
                },
                Condition::ForList(var, mut vals) => {
                    debug!("END FOR LIST: {} = {:?}", var, vals);
                    if vals.is_empty() {
                        return Ok(0);
                    }
                    let val = vals[0].clone();
                    vals.remove(0);
                    self.varmgr.set_var(&var, VarValue::Str(val));
                    self.cond_stack.push(CondItem{line: op.line, cond: Condition::ForList(var, vals)});
                    return Ok(op.line+1);
                },
                Condition::ForInt(var, mut curr, end, step) => {
                    curr += step;
                    debug!("END FOR INT: {} of {}", curr, end);
                    if (step > 0 && curr >= end) || (step < 0 && curr <= end) {
                        return Ok(0);
                    }
                    self.varmgr.set_var(&var, VarValue::Int(curr));
                    self.cond_stack.push(CondItem{line: op.line, cond: Condition::ForInt(var, curr, end, step)});
                    return Ok(op.line+1);
                },
            }
        } else {
            return Err(HakuError::StrayEndError(HakuError::err_line(self.real_line)));
        }
    }

    fn exec_break(&mut self, file: usize) -> Result<usize, HakuError> {
        debug!("Exec break");
        while let Some(cnd) = self.cond_stack.pop() {
            match cnd.cond {
                Condition::If(_) => continue,
                _ => { return Ok(self.find_end(file, cnd.line+1, "break")?); },
            }
        }
        Err(HakuError::NoMatchingForWhileError(HakuError::err_line(self.real_line)))
    }

    fn exec_continue(&mut self, file: usize) -> Result<usize, HakuError> {
        debug!("Exec continue");
        let mut next: usize = usize::MAX;
        while let Some(cnd) = self.cond_stack.pop() {
            match cnd.cond {
                Condition::If(_) => continue,
                _ => {
                    next = self.find_end(file, cnd.line+1, "continue")?;
                    self.cond_stack.push(cnd.clone());
                    break;
                },
            }
        }
        if next == usize::MAX {
            Err(HakuError::NoMatchingForWhileError(HakuError::err_line(self.real_line)))
        } else {
            // step back to point to END statement
            Ok(next-1)
        }
    }

    fn exec_for(&mut self, name: &str, seq: Seq, idx: usize) -> Result<bool, HakuError> {
        debug!("Exec for");
        match seq {
            Seq::Int(start, end, step) => {
                debug!("  FOR: from {} to {} step {}", start, end, step);
                if (step > 0 && end <= start) || (step < 0 && end >= start) {
                    // look for an END
                    return Ok(false);
                }
                if step == 0 {
                    return Err(HakuError::ForeverForError(HakuError::err_line(idx)));
                }
                self.varmgr.set_var(name, VarValue::Int(start));
                self.cond_stack.push(CondItem{line: idx, cond: Condition::ForInt(name.to_string(), start, end, step),});
                return Ok(true);
            },
            Seq::Str(s) => {
                let s = self.varmgr.interpolate(&s, false);
                let mut v: Vec<String> = if s.find('\n').is_some() {
                    s.trim_end().split('\n').map(|s| s.trim_end().to_string()).collect()
                } else {
                    s.split_ascii_whitespace().map(|s| s.to_string()).collect()
                };
                debug!("   FOR whitespace: {:?}", v);
                if v.is_empty() {
                    return Ok(false);
                }
                self.varmgr.set_var(name, VarValue::Str(v[0].clone()));
                v.remove(0);
                self.cond_stack.push(CondItem{line: idx, cond: Condition::ForList(name.to_string(), v)});
                return Ok(true);
            },
            Seq::Idents(ids) => {
                debug!("  FOR idents: {:?}", ids);
                if ids.is_empty() {
                    return Ok(false);
                }
                self.varmgr.set_var(name, VarValue::Str(ids[0].clone()));
                let v: Vec<String> = ids.iter().skip(1).map(|s| s.to_string()).collect();
                self.cond_stack.push(CondItem{line: idx, cond: Condition::ForList(name.to_string(), v)});
                return Ok(true);
            },
            Seq::Exec(s) => {
                match self.exec_cmd(&s) {
                    Ok(res) => {
                        if res.code == 0 {
                            let mut v: Vec<String> = res.stdout.lines().map(|s| s.trim_end().to_string()).collect();
                            debug!("   FOR lines: {:?}", v);
                            if v.is_empty() {
                                return Ok(false);
                            }
                            self.varmgr.set_var(name, VarValue::Str(v[0].clone()));
                            v.remove(0);
                            self.cond_stack.push(CondItem{line: idx, cond: Condition::ForList(name.to_string(), v)});
                            return Ok(true);
                        } else {
                            debug!("   FOR lines: FAILURE");
                        };
                    },
                    Err(_) => {
                            debug!("   FOR lines: FAILURE[2]");
                    },
                }
            },
        }
        Ok(false)
    }

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

    fn exec_op(&mut self, op: &Op) -> Result<VarValue, HakuError> {
        match op {
            Op::Int(i) => Ok(VarValue::Int(*i)),
            Op::Str(s) => {
                let s = self.varmgr.interpolate(&s, false);
                Ok(VarValue::Str(s))
            },
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
            },
            Op::AndExpr(ops) => self.exec_and_expr(ops),
            Op::Func(name, ops) => self.exec_func(name, ops),
            Op::Compare(cmp_op, ops) => self.exec_compare(cmp_op, ops),
            _ => unreachable!(),
        }
    }

    fn enter_recipe(&mut self, recipe: &RecipeItem) {
        debug!("enter section. Vars {:?}, Free {:?}", recipe.vars, self.varmgr.free);
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
                    idx+=1;
                }
                self.varmgr.set_recipe_var(nm, VarValue::List(out));
                return;
            } else {
                self.varmgr.set_recipe_var(v, VarValue::Str(self.varmgr.free[idx].clone()));
                idx+=1;
                if idx >= self.varmgr.free.len() {
                    return;
                }
            }
        }
    }

    fn leave_recipe(&mut self) {
        self.varmgr.recipe_vars.clear();
        self.cond_stack.clear();
    }
}
