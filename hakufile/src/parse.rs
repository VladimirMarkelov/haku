use std::fs::File;
use std::io::{BufReader, BufRead};

use pest::Parser;

use crate::feature::process_feature;
use crate::errors::HakuError;
use crate::ops::{Op, build_recipe, build_shell_cmd,
            build_for, build_func, build_assign, build_def_assign,
            build_either_assign, build_either_def_assign, build_if,
            build_elseif, build_while, build_include };
use crate::vm::{RunOpts};

#[derive(Parser)]
#[grammar = "haku.pest"]
pub struct TaskParser;

/// Disabled recipe description
#[derive(Clone)]
pub struct DisabledRecipe {
    /// recipe's name
    pub name: String,
    /// optional description from doc comment
    pub desc: String,
    /// list of features when the recipe is enabled
    pub feat: String,
}

/// A single operation description
#[derive(Clone,Debug)]
pub(crate) struct OpItem {
    /// Operation
    pub(crate) op: Op,
    /// the line number in the script
    pub(crate) line: usize,
}

/// A single script description
pub(crate) struct HakuFile {
    /// list of lines that can be executed (all comment and disabled code are removed)
    pub(crate) ops: Vec<OpItem>,
    /// list of disabled recipes (for list command)
    pub(crate) disabled: Vec<DisabledRecipe>,
}

/// What to skip while parsing the script
#[derive(Debug,PartialEq)]
enum Skip {
    /// Nothing - the next line should be executed
    None,
    /// skip a command block (entire IF/FOR/WHILE or just a single line)
    Command,
    /// skip the entire recipe - the recipe is marked as a disabled one
    Recipe,
}

impl HakuFile {
    pub(crate) fn new() -> Self {
        HakuFile {
            ops: Vec::new(),
            disabled: Vec::new(),
        }
    }

    /// Parses a single script line. Each line must contain only one rule(command/statement)
    fn process_line(&mut self, line: &str, idx: usize, opts: &RunOpts) -> Result<(), HakuError> {
        let res =  TaskParser::parse(Rule::expression, line);

        let pairs = match res {
            Err(e) => {
                let msg = format!("'{}': {}", line, e.to_string());
                return Err(HakuError::ParseError(msg, HakuError::err_line(idx)));
            },
            Ok(p) => p,
        };
        for pair in pairs {
            match pair.as_rule() {
                Rule::shell_stmt => {
                    self.ops.push(OpItem{op: build_shell_cmd(pair.into_inner())?, line: idx});
                },
                Rule::comment => {
                    let mut inner = pair.into_inner();
                    let s = inner.next().unwrap().as_str();
                    self.ops.push(OpItem{op: Op::Comment(s.to_owned()), line: idx});
                },
                Rule::doc_comment => {
                    let mut inner = pair.into_inner();
                    let s = inner.next().unwrap().as_str();
                    self.ops.push(OpItem{op: Op::DocComment(s.to_owned()), line: idx});
                },
                Rule::include_stmt => {
                    self.ops.push(OpItem{op: build_include(pair.into_inner())?, line: idx});
                },
                Rule::func => {
                    self.ops.push(OpItem{op: build_func(pair.into_inner())?, line: idx});
                },
                Rule::stmt_close => {
                    self.ops.push(OpItem{op: Op::StmtClose, line: idx});
                },
                Rule::break_stmt => {
                    self.ops.push(OpItem{op: Op::Break, line: idx});
                },
                Rule::cont_stmt => {
                    self.ops.push(OpItem{op: Op::Continue, line: idx});
                },
                Rule::either_def_assign => {
                    self.ops.push(OpItem{op: build_either_def_assign(pair.into_inner())?, line: idx});
                },
                Rule::either_assign => {
                    self.ops.push(OpItem{op: build_either_assign(pair.into_inner())?, line: idx});
                },
                Rule::def_assign => {
                    self.ops.push(OpItem{op: build_def_assign(pair.into_inner())?, line: idx});
                },
                Rule::assign => {
                    self.ops.push(OpItem{op: build_assign(pair.into_inner())?, line: idx});
                },
                Rule::while_stmt => {
                    self.ops.push(OpItem{op: build_while(pair.into_inner())?, line: idx});
                },
                Rule::for_stmt => {
                    self.ops.push(OpItem{op: build_for(pair.into_inner())?, line: idx});
                },
                Rule::if_stmt => {
                    self.ops.push(OpItem{op: build_if(pair.into_inner())?, line: idx});
                },
                Rule::elseif_stmt => {
                    self.ops.push(OpItem{op: build_elseif(pair.into_inner())?, line: idx});
                },
                Rule::else_stmt => {
                    self.ops.push(OpItem{op: Op::Else, line: idx});
                },
                Rule::return_stmt => {
                    self.ops.push(OpItem{op: Op::Return, line: idx});
                },
                Rule::recipe => {
                    self.ops.push(OpItem{op: build_recipe(pair.into_inner())?, line: idx});
                },
                Rule::feature_list => {
                    let txt = pair.as_str();
                    let pass = match process_feature(pair.into_inner(), opts) {
                        Ok(b) => b,
                        Err(s) => return Err(HakuError::InvalidFeatureName(s, HakuError::err_line(idx))),
                    };
                    self.ops.push(OpItem{op: Op::Feature(pass, txt.to_string()), line: idx});
                },
                _ => {
                    return Err(HakuError::ParseError(line.to_string(), HakuError::err_line(idx)));
                },
            }
            // one rule per line only
        }
        Ok(())
    }

    /// Loads and parses a script from a file. If thea script contains INCLUDE statements, all
    /// included files are loaded and parsed as well
    pub fn load_from_file(path: &str, opts: &RunOpts) -> Result<HakuFile, HakuError> {
        let mut hk = HakuFile::new();
        let input = match File::open(path) {
            Ok(f) => f,
            Err(e) => return Err(HakuError::FileOpenFailure(path.to_string(), e.to_string())),
        };
        let buffered = BufReader::new(input);
        let mut full_line = String::new();
        hk.ops.clear();
        let mut idx: usize = 0;
        for line in buffered.lines() {
            idx += 1;
            if let Ok(l) = line {
                let l = l.trim();
                full_line += l;
                if full_line.ends_with('\\') || full_line == "" {
                    continue;
                }
            } else {
                return Err(HakuError::FileReadFailure(path.to_string()));
            }

            if full_line != "" {
                hk.process_line(&full_line, idx, opts)?;
                full_line.clear();
            }
        }
        hk.remove_dead_code();
        Ok(hk)
    }

    /// Loads and parses a script from memory. If the script contains INCLUDE statements, all
    /// included files are loaded from files and parsed as well
    pub fn load_from_str(src: &str, opts: &RunOpts) -> Result<HakuFile, HakuError> {
        let mut hk = HakuFile::new();
        let mut full_line = String::new();
        hk.ops.clear();
        let mut idx: usize = 0;
        for l in src.lines() {
            idx += 1;
            let l = l.trim();
            full_line += l;
            if full_line.ends_with('\\') || full_line == "" {
                continue;
            }

            if full_line != "" {
                hk.process_line(&full_line, idx, opts)?;
                full_line.clear();
            }
        }
        hk.remove_dead_code();
        Ok(hk)
    }

    /// Removes all disabled blocks, but keep disabled recipe - to be able to list them
    pub fn remove_dead_code(&mut self) {
        let mut skip = Skip::None;
        let mut pass = true;
        let mut op_list: Vec<OpItem> = Vec::new();
        let mut f_list: Vec<OpItem> = Vec::new();
        let mut desc = String::new();
        let mut fstr = String::new();
        let mut nesting = 0;

        for o in self.ops.iter().cloned() {
            match o.op {
                Op::Comment(_) => continue,
                Op::DocComment(ref s) => {
                    desc = s.to_string();
                    f_list.push(o);
                },
                Op::Feature(b, ref s) => {
                    pass &=b;
                    f_list.push(o.clone());
                    fstr += s;
                },
                Op::Recipe(ref name, _, _, _) => {
                    if skip != Skip::None || pass {
                        skip = Skip::None;
                        op_list.append(&mut f_list);
                        op_list.push(o);
                    } else if !pass {
                        self.disabled.push(DisabledRecipe{
                            name: name.to_string(),
                            desc: desc.clone(),
                            feat: fstr.clone(),
                        });
                        skip = Skip::Recipe;
                    }
                    pass = true;
                    fstr.clear();
                    desc.clear();
                    f_list.clear();
                },
                Op::If(_) | Op::While(_) | Op::For(_, _) => {
                    if skip != Skip::None {
                        nesting += 1;
                    } else {
                        if pass {
                            op_list.push(o);
                        } else {
                            skip = Skip::Command;
                            nesting = 1;
                        }
                    }
                    pass = true;
                    fstr.clear();
                    desc.clear();
                    f_list.clear();
                },
                Op::StmtClose => {
                    if skip == Skip::None {
                        if pass {
                            op_list.push(o);
                        }
                    } else if skip == Skip::Command {
                        nesting -= 1;
                        if nesting == 0 {
                            skip =Skip::None;
                        }
                    }
                    pass = true;
                    fstr.clear();
                    desc.clear();
                    f_list.clear();
                },
                _ => {
                    if skip == Skip::None && pass {
                        op_list.push(o);
                    }
                    pass = true;
                    fstr.clear();
                    desc.clear();
                    f_list.clear();
                },
            }
        }
        self.ops = op_list;
    }
}
