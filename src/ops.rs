use pest::iterators::{Pair, Pairs};

use crate::errors::HakuError;
use crate::parse::Rule;

/// Describes initial condition of a `for` statement
#[derive(Debug, Clone)]
pub enum Seq {
    /// integer arithmetic progression (initial value, final value, step)
    Int(i64, i64, i64),
    /// a string - a list of values separated with whitespaces
    Str(String),
    /// a list of identifiers
    Idents(Vec<String>),
    /// a result of external command execution
    Exec(String),
    /// a value of a variable
    Var(String),
}

/// external command and recipe flags. Flags are added as prefixes of a script lines.

/// Do not print the command before execution (`@`)
pub const FLAG_QUIET: u32 = 1;
/// Do not interrupt the execution if external command has failed(`-`)
pub const FLAG_PASS: u32 = 2;

/// Returns true if a value `flags` has a `flag` on
pub fn is_flag_on(flags: u32, flag: u32) -> bool {
    flags & flag == flag
}

/// Operations processed by the engine internally
#[derive(Debug, Clone)]
pub enum Op {
    /// Comment line (ignored) - comment text: starts with `#` or `//`
    Comment(String),
    /// Documentation comment - comment text: starts with `##`. Used as a recipe description
    /// when it is right before the recipe, ignored in other cases
    DocComment(String),
    /// Include another script:
    ///
    /// * flags - runtime flags, e.g. ignore file not found errors
    /// * path to the script
    Include(u32, String),
    /// Interrupt script with a error - error message
    Error(String),
    /// List of features which enable a following block of code
    ///
    /// * passed - whether all mentioned features are on (i.e., the block must be executed or
    /// ignored)
    /// * string representation of a condition to enable the following code block
    Feature(bool, String),
    /// Execute a function
    ///
    /// * function name
    /// * function arguments
    Func(String, Vec<Op>),
    /// END statement
    StmtClose,
    /// Simple variable assignment
    ///
    /// * variable name
    /// * expression
    Assign(String, Vec<Op>),
    /// Assign a new value to a variable only if it is undefined or falsy one
    ///
    /// * variable name
    /// * expression
    ///
    /// Example: `a ?= 10`
    DefAssign(String, Vec<Op>),
    /// Assign the first truthy value from the list of values
    ///
    /// * check - if it is true, the new value is calculated and assigned only if the current
    /// variable value is undefined or a falsy one
    /// * variable name
    /// * list of values
    ///
    /// Example: `a ?= $b ? $c`
    EitherAssign(bool, String, Vec<Op>),
    /// Comparison operation
    ///
    /// * operator to compare (==, !=, <, <=, >, >=)
    /// * list of values (should be 2 of them)
    Compare(String, Vec<Op>),
    /// IF statement - if's condition
    If(Vec<Op>),
    /// ELSEIF statement - elseif's condition
    ElseIf(Vec<Op>),
    /// A list of values joined with logical AND. The result of the expression is a logical value
    /// 0 or 1 (1 - if all values are truthy ones)
    AndExpr(Vec<Op>),
    /// ELSE statement
    Else,
    /// BREAK statement
    Break,
    /// CONTINUE statement
    Continue,
    /// RETURN statement
    Return,
    /// WHILE statement - the loop enter condition
    While(Vec<Op>),
    /// FOR statement - range of for values
    For(String, Seq),
    /// A recipe declaration
    ///
    /// * name
    /// * flags (e.g., "echo off" or "ignore shell errors")
    /// * list of local recipe variable names
    /// * list of recipes this one depends on (they are executed before this recipe)
    ///
    /// Example: `recipe-name loc_var1 +loc_var2: dependency1 dependency2
    Recipe(String, u32, Vec<String>, Vec<String>),
    /// Execute external command using the current shell
    ///
    /// * execution flags (e.g., "echo off" or "ignore shell errors")
    /// * command line to execute
    Shell(u32, String),

    // here goes a list of basic building blocks of any expression
    /// Integer value(i64)
    Int(i64),
    /// String value
    Str(String),
    /// Variable name
    Var(String),
    /// result of external execution with shell
    Exec(String),
    /// Logical negation of a value
    Not(Vec<Op>),
    /// change working directory: flags, directory
    Cd(u32, String),
    /// PAUSE statement
    Pause,
}

/// Converts a prefix of a script line to a runtime flags
fn str_to_flags(s: &str) -> u32 {
    let mut flags: u32 = 0;
    if s.find('@').is_some() {
        flags |= FLAG_QUIET;
    }
    if s.find('-').is_some() {
        flags |= FLAG_PASS;
    }
    flags
}

/// Parses a script line that describes a recipe
pub fn build_recipe(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut flags: u32 = 0;
    let mut name = String::new();
    let mut vars = Vec::new();
    let mut deps = Vec::new();

    let pstr = p.as_str().to_string();
    for s in p {
        match s.as_rule() {
            Rule::cmd_flags => flags = str_to_flags(s.as_str()),
            Rule::sec_name => name = s.as_str().to_string(),
            Rule::sec_args => {
                let inner = s.into_inner();
                for s_in in inner {
                    vars.push(s_in.as_str().to_string());
                }
                if !vars.is_empty() {
                    for v in &vars[..vars.len() - 1] {
                        if v.starts_with('+') {
                            return Err(HakuError::RecipeListArgError(pstr));
                        }
                    }
                }
            }
            Rule::sec_deps => {
                let inner = s.into_inner();
                for s_in in inner {
                    deps.push(s_in.as_str().to_string());
                }
            }
            _ => { /* skip all other parts like sec_sep */ }
        }
    }

    Ok(Op::Recipe(name, flags, vars, deps))
}

/// Parses a script line with cd statement
pub fn build_cd(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut flags: u32 = 0;
    let mut cmd = String::new();
    for s in p {
        match s.as_rule() {
            Rule::cmd_flags => flags = str_to_flags(s.as_str()),
            Rule::cd_body => cmd = strip_quotes(s.as_str()).to_string(),
            _ => {}
        }
    }

    Ok(Op::Cd(flags, cmd))
}

/// Parses a script line with include statement
pub fn build_include(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut flags: u32 = 0;
    let mut cmd = String::new();
    for s in p {
        match s.as_rule() {
            Rule::cmd_flags => flags = str_to_flags(s.as_str()),
            Rule::include_body => cmd = strip_quotes(s.as_str()).to_string(),
            _ => {}
        }
    }

    Ok(Op::Include(flags, cmd))
}

/// Parses a script line with error message
pub fn build_error(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut cmd = String::new();
    for s in p {
        if let Rule::error_body = s.as_rule() {
            cmd = strip_quotes(s.as_str()).to_string();
        }
    }

    Ok(Op::Error(cmd))
}

/// Parses a script line with external shell execution
pub fn build_shell_cmd(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut flags: u32 = 0;
    let mut cmd = String::new();
    for s in p {
        match s.as_rule() {
            Rule::cmd_flags => flags = str_to_flags(s.as_str()),
            Rule::shell_cmd => cmd = s.as_str().to_string(),
            _ => {}
        }
    }

    Ok(Op::Shell(flags, cmd))
}

/// Removes trailing and leading quotes from a string:
/// backticks, `'...'`, and `"..."`
pub fn strip_quotes(s: &str) -> &str {
    if s.starts_with('"') {
        s.trim_matches('"')
    } else if s.starts_with('\'') {
        s.trim_matches('\'')
    } else if s.starts_with('`') {
        s.trim_matches('`')
    } else {
        s
    }
}

/// Converts a variable "pointer" to a variable name:
///
/// * `$var` --> `var`
/// * `${var}` --> `var`
pub fn strip_var_deco(s: &str) -> &str {
    let s = s.trim_matches('$');
    let s = s.trim_start_matches('{');
    s.trim_end_matches('}')
}

/// Parses FOR intialization statement
///
/// * `1..10`
/// * `1..10..2`
/// * `"first item" "second item" "third item"
/// * `"val1 val2"`
/// * ident1 ident2
/// * `\`dir *.txt\``
/// * `${var-name}` or `$var-name`
fn build_seq(p: Pairs<Rule>) -> Result<Seq, HakuError> {
    let text = p.as_str().to_owned();
    for pair in p {
        match pair.as_rule() {
            Rule::squoted | Rule::dquoted => return Ok(Seq::Str(strip_quotes(pair.as_str()).to_string())),
            Rule::exec => return Ok(Seq::Exec(strip_quotes(pair.as_str()).to_string())),
            Rule::raw_seq => {
                let mut list = Vec::new();
                for ids in pair.into_inner() {
                    match ids.as_rule() {
                        Rule::ident => list.push(ids.as_str().to_owned()),
                        _ => unimplemented!(),
                    }
                }
                return Ok(Seq::Idents(list));
            }
            Rule::int_seq => {
                let mut start = String::new();
                let mut end = String::new();
                let mut step = "1".to_string();
                for int in pair.into_inner() {
                    match int.as_rule() {
                        Rule::int => {
                            if start.is_empty() {
                                start = int.as_str().to_owned();
                            } else if end.is_empty() {
                                end = int.as_str().to_owned();
                            } else {
                                step = int.as_str().to_owned();
                            }
                        }
                        _ => unimplemented!(),
                    }
                }
                let istart = if let Ok(i) = start.parse::<i64>() {
                    i
                } else {
                    return Err(HakuError::SeqIntError("start", start));
                };
                let iend = if let Ok(i) = end.parse::<i64>() {
                    i
                } else {
                    return Err(HakuError::SeqIntError("end", end));
                };
                let istep = if let Ok(i) = step.parse::<i64>() {
                    i
                } else {
                    return Err(HakuError::SeqIntError("step", end));
                };
                if istep == 0 || (istep > 0 && istart > iend) || (istep < 0 && istart < iend) {
                    return Err(HakuError::SeqError(istart, iend, istep));
                }
                return Ok(Seq::Int(istart, iend, istep));
            }
            Rule::str_seq => {
                let mut list = Vec::new();
                for ids in pair.into_inner() {
                    match ids.as_rule() {
                        Rule::string => list.push(strip_quotes(ids.as_str()).to_owned()),
                        _ => unimplemented!(),
                    }
                }
                return Ok(Seq::Idents(list));
            }
            Rule::var_seq => {
                let mut var_name = String::new();
                for ids in pair.into_inner() {
                    match ids.as_rule() {
                        Rule::ident => var_name = ids.as_str().to_owned(),
                        _ => unimplemented!(),
                    }
                }
                if var_name.is_empty() {
                    return Err(HakuError::SeqVarNameError(text));
                }
                return Ok(Seq::Var(var_name));
            }
            _ => unimplemented!(),
        }
    }
    unimplemented!()
}

/// Parsed FOR statement: `FOR var-name in FOR-SEQUENCE`
pub fn build_for(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut seq = Seq::Str(String::new());
    let mut var = String::new();
    for s in p {
        match s.as_rule() {
            Rule::ident => var = s.as_str().to_string(),
            Rule::seq => seq = build_seq(s.into_inner())?,
            _ => {}
        }
    }
    Ok(Op::For(var, seq))
}

/// Parses a single function or expression value
fn build_arg_value(p: Pair<Rule>) -> Result<Op, HakuError> {
    match p.as_rule() {
        Rule::int => {
            if let Ok(i) = p.as_str().parse::<i64>() {
                return Ok(Op::Int(i));
            }
        }
        Rule::exec => return Ok(Op::Exec(strip_quotes(p.as_str()).to_string())),
        Rule::string => {
            for in_p in p.into_inner() {
                match in_p.as_rule() {
                    Rule::squoted | Rule::dquoted => return Ok(Op::Str(strip_quotes(in_p.as_str()).to_string())),
                    _ => unimplemented!(),
                }
            }
        }
        Rule::var => return Ok(Op::Var(strip_var_deco(p.as_str()).to_string())),
        Rule::func => return build_func(p.into_inner()),
        Rule::dquoted | Rule::squoted => return Ok(Op::Str(strip_quotes(p.as_str()).to_string())),
        _ => {
            println!("{:?}", p);
            unimplemented!();
        }
    }
    unimplemented!()
}

/// Parses a single value or negated single value
fn build_arg(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut neg = false;
    for pair in p {
        match pair.as_rule() {
            Rule::not_op => neg = !neg,
            Rule::arg => {
                let val = pair.as_str().to_string();
                if let Some(pp) = pair.into_inner().next() {
                    let op = build_arg_value(pp);
                    if neg {
                        let op = op?;
                        return Ok(Op::Not(vec![op]));
                    } else {
                        return op;
                    }
                } else {
                    return Err(HakuError::ParseError(val, String::new()));
                }
            }
            _ => {
                let op = build_arg_value(pair);
                if neg {
                    let op = op?;
                    return Ok(Op::Not(vec![op]));
                } else {
                    return op;
                }
            }
        }
    }
    unimplemented!()
}

/// Parses a list of function or expression values
fn build_arglist(p: Pairs<Rule>) -> Result<Vec<Op>, HakuError> {
    let mut vec: Vec<Op> = Vec::new();
    for pair in p {
        match pair.as_rule() {
            Rule::arg => vec.push(build_arg(pair.into_inner())?),
            _ => unimplemented!(),
        }
    }
    Ok(vec)
}

/// Parses a function call
pub fn build_func(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut name = String::new();
    for pair in p {
        match pair.as_rule() {
            Rule::ident => name = pair.as_str().to_string(),
            Rule::arglist => {
                return Ok(Op::Func(name, build_arglist(pair.into_inner())?));
            }
            _ => {
                println!("{:?}", pair);
                unimplemented!();
            }
        }
    }
    Ok(Op::Func(name, Vec::new()))
}

/// Parses a basic expression: a single value or a comparison expression
fn build_s_expr(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut v = Vec::new();
    let mut cmp = String::new();
    for pair in p {
        match pair.as_rule() {
            Rule::arg => v.push(build_arg(pair.into_inner())?),
            Rule::cmp_op => cmp = pair.as_str().to_string(),
            _ => {
                println!("{:?}", pair);
                unimplemented!();
            }
        }
    }
    if cmp.is_empty() {
        Ok(v.pop().unwrap_or_else(|| unreachable!()))
    } else {
        Ok(Op::Compare(cmp, v))
    }
}

/// Parses AND expression: one or few basic expressions joined with AND(&&)
fn build_and_expr(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut v = Vec::new();
    for pair in p {
        match pair.as_rule() {
            Rule::sexpr => {
                let op = build_s_expr(pair.into_inner())?;
                v.push(op);
            }
            Rule::and_op => {} // do nothing
            _ => {
                println!("{:?}", pair);
                unimplemented!();
            }
        }
    }
    Ok(Op::AndExpr(v))
}

/// Parses OR expression: one or few AND expressions joined with OR(||)
fn build_condition(p: Pairs<Rule>) -> Result<Vec<Op>, HakuError> {
    let mut v = Vec::new();
    for pair in p {
        match pair.as_rule() {
            Rule::andexpr => v.push(build_and_expr(pair.into_inner())?),
            Rule::or_op => {} // do nothing
            _ => {
                println!("{:?}", pair);
                unimplemented!();
            }
        }
    }
    Ok(v)
}

/// Parses the entire expression
fn build_expr(p: Pairs<Rule>) -> Result<Vec<Op>, HakuError> {
    let mut v: Vec<Op> = Vec::new();
    for pair in p {
        match pair.as_rule() {
            Rule::andexpr => v.push(build_and_expr(pair.into_inner())?),
            Rule::cond => {
                let mut cexpr = build_condition(pair.into_inner())?;
                v.append(&mut cexpr);
            }
            _ => {
                println!("{:?}", pair);
                unimplemented!();
            }
        }
    }
    Ok(v)
}

/// Parses assignment statement: `a = $b`
pub fn build_assign(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut name = String::new();
    for pair in p {
        match pair.as_rule() {
            Rule::ident => name = pair.as_str().to_string(),
            Rule::assign_expr => {
                return Ok(Op::Assign(name, build_expr(pair.into_inner())?));
            }
            _ => {} // "="
        }
    }
    unreachable!();
}

/// Parses default assignment statement: `a ?= $b`
pub fn build_def_assign(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut name = String::new();
    for pair in p {
        match pair.as_rule() {
            Rule::ident => name = pair.as_str().to_string(),
            Rule::assign_expr => {
                return Ok(Op::DefAssign(name, build_expr(pair.into_inner())?));
            }
            _ => {} // "="
        }
    }
    unreachable!();
}

/// Parses assignment statement with variants: `a = $b ? $c`
pub fn build_either_assign(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut name = String::new();
    let mut exprs = Vec::new();
    for pair in p {
        match pair.as_rule() {
            Rule::ident => name = pair.as_str().to_string(),
            Rule::either_arg => {
                let a = build_arg(pair.into_inner())?;
                exprs.push(a);
            }
            _ => {} // "=" && "?"
        }
    }
    Ok(Op::EitherAssign(false, name, exprs))
}

/// Parses default assignment statement with variants: `a ?= $b ? $c`
pub fn build_either_def_assign(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut name = String::new();
    let mut exprs = Vec::new();
    for pair in p {
        match pair.as_rule() {
            Rule::ident => name = pair.as_str().to_string(),
            Rule::either_arg => {
                let a = build_arg(pair.into_inner())?;
                exprs.push(a);
            }
            _ => {} // "=" && "?"
        }
    }
    Ok(Op::EitherAssign(true, name, exprs))
}

/// Parses IF statement
pub fn build_if(p: Pairs<Rule>) -> Result<Op, HakuError> {
    for pair in p {
        if let Rule::cond = pair.as_rule() {
            return Ok(Op::If(build_condition(pair.into_inner())?));
        }
    }
    unreachable!()
}

/// Parses ELSEIF statement
pub fn build_elseif(p: Pairs<Rule>) -> Result<Op, HakuError> {
    for pair in p {
        if let Rule::cond = pair.as_rule() {
            return Ok(Op::ElseIf(build_condition(pair.into_inner())?));
        }
    }
    unreachable!()
}

/// Parses WHILE statement
pub fn build_while(p: Pairs<Rule>) -> Result<Op, HakuError> {
    for pair in p {
        if let Rule::cond = pair.as_rule() {
            return Ok(Op::While(build_condition(pair.into_inner())?));
        }
    }
    unreachable!()
}
