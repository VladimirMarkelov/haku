use pest::iterators::{Pair, Pairs};

use crate::parse::{Rule};
use crate::errors::HakuError;

#[derive(Debug,Clone)]
pub enum Seq {
    Int(i64, i64, i64), // 1..2..3
    Str(String), // "a b"
    Idents(Vec<String>), // a b c
    Exec(String), // `ls`
}

pub const FLAG_QUIET: u32 = 1;
pub const FLAG_PASS: u32 = 2;

pub fn is_flag_on(flags: u32, flag: u32) -> bool {
    flags & flag == flag
}

#[derive(Debug,Clone)]
pub enum Op {
    Comment(String), // comment text
    DocComment(String), // doc comment text
    Include(u32, String), // flags, filename
    Feature(bool, String), // passed?, text
    Func(String, Vec<Op>),
    StmtClose, //
    Assign(String, Vec<Op>),
    DefAssign(String, Vec<Op>),
    EitherAssign(bool, String, Vec<Op>), // bool = whether to check for empty before assign
    Compare(String, Vec<Op>), // "==", "<" etc, and list of two args
    If(Vec<Op>),
    ElseIf(Vec<Op>),
    AndExpr(Vec<Op>),
    Else, //
    Break, //
    Continue, //
    Return, //
    While(Vec<Op>),
    For(String, Seq), //
    Recipe(String, u32, Vec<String>, Vec<String>), // name, flags, vars, deps
    Shell(u32, String), // flags, shell command

    // arguments etc
    Int(i64),
    Str(String),
    Var(String),
    Exec(String),
    Not(Vec<Op>),
}

fn str_to_flags(s: &str) -> u32 {
    let mut flags: u32 = 0;
    if let Some(_) = s.find('@') {
        flags |= FLAG_QUIET;
    }
    if let Some(_) = s.find('-') {
        flags |= FLAG_PASS;
    }
    flags
}

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
                    for v in &vars[..vars.len()-1] {
                        if v.starts_with('+') {
                            return Err(HakuError::RecipeListArgError(pstr));
                        }
                    }
                }
            },
            Rule::sec_deps => {
                let inner = s.into_inner();
                for s_in in inner {
                    deps.push(s_in.as_str().to_string());
                }
            },
            _ => {
                // skip all other parts like sec_sep
            },
        }
    }

    Ok(Op::Recipe(name, flags, vars, deps))
}

pub fn build_include(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut flags: u32 = 0;
    let mut cmd = String::new();
    for s in p {
        match s.as_rule() {
            Rule::cmd_flags => flags = str_to_flags(s.as_str()),
            Rule::include_body => cmd = strip_quotes(s.as_str()).to_string(),
            _ => { },
        }
    }

    Ok(Op::Include(flags, cmd))
}

pub fn build_shell_cmd(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut flags: u32 = 0;
    let mut cmd = String::new();
    for s in p {
        match s.as_rule() {
            Rule::cmd_flags => flags = str_to_flags(s.as_str()),
            Rule::shell_cmd => cmd = s.as_str().to_string(),
            _ => { },
        }
    }

    Ok(Op::Shell(flags, cmd))
}

pub fn strip_quotes(s: &str) -> &str {
    if s.starts_with('"') {
        s.trim_matches('"')
    } else if s.starts_with('\'') {
        s.trim_matches('\'')
    } else if s.starts_with('`') {
        s.trim_matches('`')
    } else if s.starts_with("r#") {
        let s = s.trim_start_matches("r#");
        s.trim_end_matches('#')
    } else {
        s
    }
}

pub fn strip_var_deco(s: &str) -> &str {
    let s = s.trim_matches('$');
    let s = s.trim_start_matches('{');
    s.trim_end_matches('}')
}

fn build_seq(p: Pairs<Rule>) -> Result<Seq, HakuError> {
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
            },
            Rule::int_seq => {
                let mut start = String::new();
                let mut end = String::new();
                let mut step = "1".to_string();
                for int in pair.into_inner() {
                    match int.as_rule() {
                        Rule::int => if start.is_empty() {
                            start = int.as_str().to_owned();
                        } else if end.is_empty() {
                            end = int.as_str().to_owned();
                        } else {
                            step = int.as_str().to_owned();
                        },
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
            },
            _ => unimplemented!(),
        }
    }
    unimplemented!()
}

pub fn build_for(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut seq = Seq::Str(String::new());
    let mut var = String::new();
    for s in p {
        match s.as_rule() {
            Rule::ident => var = s.as_str().to_string(),
            Rule::seq => seq = build_seq(s.into_inner())?,
            _ => { },
        }
    }
    Ok(Op::For(var, seq))
}

fn build_arg_value(p: Pair<Rule>) -> Result<Op, HakuError> {
    match p.as_rule() {
        Rule::int => if let Ok(i) = p.as_str().parse::<i64>() { return Ok(Op::Int(i)); },
        Rule::exec => return Ok(Op::Exec(strip_quotes(p.as_str()).to_string())),
        Rule::string => {
            for in_p in p.into_inner() {
                match in_p.as_rule() {
                    Rule::rstr | Rule::squoted | Rule::dquoted => return Ok(Op::Str(strip_quotes(in_p.as_str()).to_string())),
                    _ => unimplemented!(),
                }
            }
        },
        Rule::var => return Ok(Op::Var(strip_var_deco(p.as_str()).to_string())),
        Rule::func => return build_func(p.into_inner()),
        Rule::dquoted | Rule::squoted | Rule::rstr => return Ok(Op::Str(strip_quotes(p.as_str()).to_string())),
        _ => {
            println!("{:?}", p);
            unimplemented!();
        },
    }
    unimplemented!()
}

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
            },
            _ => {
                let op = build_arg_value(pair);
                if neg {
                    let op = op?;
                    return Ok(Op::Not(vec![op]));
                } else {
                    return op;
                }
            },
        }
    }
    unimplemented!()
}

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

pub fn build_func(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut name = String::new();
    for pair in p {
        match pair.as_rule() {
            Rule::ident => name = pair.as_str().to_string(),
            Rule::arglist => {
                return Ok(Op::Func(name, build_arglist(pair.into_inner())?));
            },
            _ => {
                println!("{:?}", pair);
                unimplemented!();
            },
        }
    }
    Ok(Op::Func(name, Vec::new()))
}

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
            },
        }
    }
    if cmp.is_empty() {
        Ok(v.pop().unwrap_or_else(|| unreachable!()))
    } else {
        Ok(Op::Compare(cmp, v))
    }
}

fn build_and_expr(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut v = Vec::new();
    for pair in p {
        match pair.as_rule() {
            Rule::sexpr => {
                let op = build_s_expr(pair.into_inner())?;
                v.push(op);
            },
            Rule::and_op => {}, // do nothing
            _ => {
                println!("{:?}", pair);
                unimplemented!();
            },
        }
    }
    Ok(Op::AndExpr(v))
}

fn build_condition(p: Pairs<Rule>) -> Result<Vec<Op>, HakuError> {
    let mut v = Vec::new();
    for pair in p {
        match pair.as_rule() {
            Rule::andexpr => v.push(build_and_expr(pair.into_inner())?),
            Rule::or_op => {}, // do nothing
            _ => {
                println!("{:?}", pair);
                unimplemented!();
            },
        }
    }
    Ok(v)
}

fn build_expr(p: Pairs<Rule>) -> Result<Vec<Op>, HakuError> {
    let mut v: Vec<Op> = Vec::new();
    for pair in p {
        match pair.as_rule() {
            Rule::andexpr => v.push(build_and_expr(pair.into_inner())?),
            Rule::cond => {
                let mut cexpr = build_condition(pair.into_inner())?;
                v.append(&mut cexpr);
            },
            _ => {
                println!("{:?}", pair);
                unimplemented!();
            },
        }
    }
    Ok(v)
}

pub fn build_assign(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut name = String::new();
    for pair in p {
        match pair.as_rule() {
            Rule::ident => name = pair.as_str().to_string(),
            Rule::assign_expr => {
                return Ok(Op::Assign(name, build_expr(pair.into_inner())?));
            },
            _ => {}, // "="
        }
    }
    unreachable!();
}

pub fn build_def_assign(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut name = String::new();
    for pair in p {
        match pair.as_rule() {
            Rule::ident => name = pair.as_str().to_string(),
            Rule::assign_expr => {
                return Ok(Op::DefAssign(name, build_expr(pair.into_inner())?));
            },
            _ => {}, // "="
        }
    }
    unreachable!();
}

pub fn build_either_assign(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut name = String::new();
    let mut exprs = Vec::new();
    for pair in p {
        match pair.as_rule() {
            Rule::ident => name = pair.as_str().to_string(),
            Rule::either_arg => {
                let a = build_arg(pair.into_inner())?;
                exprs.push(a);
            },
            _ => {}, // "=" && "?"
        }
    }
    Ok(Op::EitherAssign(false, name, exprs))
}

pub fn build_either_def_assign(p: Pairs<Rule>) -> Result<Op, HakuError> {
    let mut name = String::new();
    let mut exprs = Vec::new();
    for pair in p {
        match pair.as_rule() {
            Rule::ident => name = pair.as_str().to_string(),
            Rule::either_arg => {
                let a = build_arg(pair.into_inner())?;
                exprs.push(a);
            },
            _ => {}, // "=" && "?"
        }
    }
    Ok(Op::EitherAssign(true, name, exprs))
}

pub fn build_if(p: Pairs<Rule>) -> Result<Op, HakuError> {
    for pair in p {
        match pair.as_rule() {
            Rule::cond => {
                return Ok(Op::If(build_condition(pair.into_inner())?));
            },
            _ => {}, // "if"
        }
    }
    unreachable!()
}

pub fn build_elseif(p: Pairs<Rule>) -> Result<Op, HakuError> {
    for pair in p {
        match pair.as_rule() {
            Rule::cond => {
                return Ok(Op::ElseIf(build_condition(pair.into_inner())?));
            },
            _ => {}, // "if"
        }
    }
    unreachable!()
}

pub fn build_while(p: Pairs<Rule>) -> Result<Op, HakuError> {
    for pair in p {
        match pair.as_rule() {
            Rule::cond => {
                return Ok(Op::While(build_condition(pair.into_inner())?));
            },
            _ => {}, // "if"
        }
    }
    unreachable!()
}
