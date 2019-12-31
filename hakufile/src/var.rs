use std::convert::From;
use std::env;
use std::usize;

use crate::{output};

#[derive(Clone,Debug,PartialEq)]
pub struct ExecResult {
    pub(crate) code: i32,
    pub(crate) stdout: String,
}

#[derive(Clone,Debug,PartialEq)]
pub enum VarValue {
    Undefined,
    Str(String),
    Int(i64),
    List(Vec<String>),
    Exec(ExecResult),
}

impl From<String> for VarValue {
    fn from(s: String) -> Self {
        VarValue::Str(s)
    }
}
impl From<&str> for VarValue {
    fn from(s: &str) -> Self {
        VarValue::Str(s.to_string())
    }
}
impl From<i64> for VarValue {
    fn from(i: i64) -> Self {
        VarValue::Int(i)
    }
}
impl From<i32> for VarValue {
    fn from(i: i32) -> Self {
        VarValue::Int(i as i64)
    }
}
impl From<u32> for VarValue {
    fn from(i: u32) -> Self {
        VarValue::Int(i as i64)
    }
}

impl ToString for VarValue {
    fn to_string(&self) -> String {
        match self {
            VarValue::Undefined => String::new(),
            VarValue::Str(s) => s.clone(),
            VarValue::Int(i) => format!("{}", i),
            VarValue::List(v) => {
                let mut s = String::new();
                for it in v.iter() {
                    if !s.is_empty() {
                        s += "\n";
                    }
                    s += it;
                }
                s
            },
            VarValue::Exec(ex) =>
                if ex.code == 0{
                    format!("{}", ex.stdout)
                } else {
                    String::new()
                },
        }
    }
}

impl VarValue {
    pub(crate) fn to_flat_string(&self) -> String {
        match self {
            VarValue::Undefined => String::new(),
            VarValue::Str(s) => s.clone(),
            VarValue::Int(i) => format!("{}", i),
            VarValue::List(v) => {
                let mut s = String::new();
                for it in v.iter() {
                    if !s.is_empty() {
                        s += " ";
                    }
                    s += it;
                }
                s
            },
            VarValue::Exec(ex) =>
                if ex.code == 0{
                    let mut s = String::new();
                    for l in ex.stdout.lines() {
                        if !s.is_empty() {
                            s += " ";
                        }
                        s += l.trim_end();
                    }
                    s
                } else {
                    String::new()
                },
        }
    }

    pub(crate) fn is_true(&self) -> bool {
        match self {
            VarValue::Undefined => false,
            VarValue::Int(i) => *i != 0,
            VarValue::Str(s) => !s.is_empty(),
            VarValue::List(v) => !v.is_empty() && !v[0].is_empty(),
            VarValue::Exec(er) => er.code == 0,
        }
    }
    fn cmp_eq(&self, val: &VarValue) -> bool {
        match self {
            VarValue::Undefined => match val {
                VarValue::Undefined => true,
                _ => false
            },
            VarValue::List(lst1) => match val {
                VarValue::List(lst2) => if lst1.len() != lst2.len() {
                    false
                } else {
                    for (idx, val) in lst1.iter().enumerate() {
                        if val != &lst2[idx] {
                            return false;
                        }
                    }
                    true
                },
                VarValue::Exec(ex_val) => ex_val.code == 0 && ex_val.stdout.trim() == &self.to_string(),
                VarValue::Str(s) => &self.to_flat_string() == s,
                VarValue::Int(i) => if lst1.len() != 1 {
                    false
                } else {
                    lst1[0] == format!("{}", *i)
                },
                _ => false,
            },
            VarValue::Exec(ex) => match val {
                VarValue::Exec(ex_val) => ex.code == ex_val.code,
                VarValue::Str(s) => &ex.stdout == s,
                VarValue::Int(i) => i64::from(ex.code) == *i,
                VarValue::List(_) => ex.code == 0 && ex.stdout == val.to_string(),
                _ => false,
            },
            VarValue::Str(s) => match val {
                VarValue::Exec(ex_val) => s == &ex_val.stdout,
                VarValue::Str(s_val) => s == s_val,
                VarValue::Int(i) => s == &format!("{}", *i),
                VarValue::List(_) => s == &val.to_flat_string(),
                _ => false,
            },
            VarValue::Int(i) => match val {
                VarValue::Exec(ex_val) => *i == i64::from(ex_val.code),
                VarValue::Str(s_val) => &format!("{}", *i) == s_val,
                VarValue::Int(i_val) => *i == *i_val,
                VarValue::List(lst) => if lst.len() != 1 {
                    false
                } else {
                    lst[0] == format!("{}", *i)
                },
                _ => false,
            },
        }
    }
    fn cmp_greater(&self, val: &VarValue) -> bool {
        match self {
            VarValue::Undefined => match val {
                _ => false
            },
            VarValue::Exec(ex) => match val {
                VarValue::Exec(ex_val) => ex.code > ex_val.code,
                VarValue::Str(s) => &ex.stdout > s,
                VarValue::Int(i) => i64::from(ex.code) > *i,
                VarValue::List(_) => ex.code == 0 && ex.stdout > val.to_string(),
                _ => true,
            },
            VarValue::Str(s) => match val {
                VarValue::Exec(ex_val) => s > &ex_val.stdout,
                VarValue::Str(s_val) => s >s_val,
                VarValue::Int(i) => s > &format!("{}", *i),
                VarValue::List(_) => s > &val.to_flat_string(),
                _ => true,
            },
            VarValue::Int(i) => match val {
                VarValue::Exec(ex_val) => *i > i64::from(ex_val.code),
                VarValue::Str(s_val) => &format!("{}", *i) > s_val,
                VarValue::Int(i_val) => *i > *i_val,
                VarValue::List(lst) => if lst.is_empty() {
                    true
                } else {
                    let vv = lst[0].parse::<i64>().unwrap_or(0i64);
                    *i > vv
                },
                _ => true,
            },
            VarValue::List(lst) => match val {
                VarValue::Exec(ex) => ex.code != 0 || self.to_string() > ex.stdout,
                VarValue::Str(s) => &self.to_flat_string() > s,
                VarValue::Int(i) => if lst.is_empty() {
                    false
                } else {
                    let vv = lst[0].parse::<i64>().unwrap_or(0i64);
                    vv > *i
                },
                VarValue::List(lst2) => if lst.len() > lst2.len() {
                    true
                } else {
                    for (idx, v) in lst.iter().enumerate() {
                        if v <= &lst2[idx] {
                            return false;
                        }
                    }
                    true
                },
                _ => true,
            },
        }
    }
    fn cmp_less(&self, val: &VarValue) -> bool {
        match self {
            VarValue::Undefined => match val {
                VarValue::Undefined => false,
                _ => true
            },
            VarValue::Exec(ex) => match val {
                VarValue::Exec(ex_val) => ex.code < ex_val.code,
                VarValue::Str(s) => &ex.stdout < s,
                VarValue::Int(i) => i64::from(ex.code) < *i,
                VarValue::List(_) => ex.code != 0 || ex.stdout < val.to_string(),
                _ => false,
            },
            VarValue::Str(s) => match val {
                VarValue::Exec(ex_val) => s < &ex_val.stdout,
                VarValue::Str(s_val) => s < s_val,
                VarValue::Int(i) => s < &format!("{}", i),
                VarValue::List(_) => s < &val.to_flat_string(),
                _ => false,
            },
            VarValue::Int(i) => match val {
                VarValue::Exec(ex_val) => i < &i64::from(ex_val.code),
                VarValue::Str(s_val) => &format!("{}", i) < s_val,
                VarValue::Int(i_val) => i < i_val,
                VarValue::List(lst) => if lst.is_empty() {
                    false
                } else {
                    let vv = lst[0].parse::<i64>().unwrap_or(0i64);
                    *i < vv
                },
                _ => false,
            },
            VarValue::List(lst) => match val {
                VarValue::Exec(ex) => ex.code == 0 && self.to_string() < ex.stdout,
                VarValue::Str(s) => &self.to_flat_string() < s,
                VarValue::Int(i) => if lst.is_empty() {
                    false
                } else {
                    let vv = lst[0].parse::<i64>().unwrap_or(0i64);
                    vv < *i
                },
                VarValue::List(lst2) => if lst.len() > lst2.len() {
                    false
                } else {
                    for (idx, v) in lst.iter().enumerate() {
                        if v >= &lst2[idx] {
                            return false;
                        }
                    }
                    true
                },
                _ => false,
            },
        }
    }
    fn cmp_neq(&self, val: &VarValue) -> bool {
        !self.cmp_eq(val)
    }
    fn cmp_eq_or_greater(&self, val: &VarValue) -> bool {
        !self.cmp_less(val)
    }
    fn cmp_eq_or_less(&self, val: &VarValue) -> bool {
        !self.cmp_greater(val)
    }
    pub(crate) fn cmp(&self, val: &VarValue, cmp_op: &str) -> bool {
        match cmp_op {
            "==" => self.cmp_eq(val),
            "!=" => self.cmp_neq(val),
            ">" => self.cmp_greater(val),
            "<" => self.cmp_less(val),
            ">=" => self.cmp_eq_or_greater(val),
            "<=" => self.cmp_eq_or_less(val),
            _ => unreachable!(),
        }
    }
}

pub struct Var {
    name: String,
    value: VarValue,
}
impl Default for Var {
    fn default() -> Self {
        Var{
            name: String::from(""),
            value: VarValue::Undefined,
        }
    }
}

pub(crate) struct VarMgr {
    pub(crate) free: Vec<String>,
    pub(crate) recipe_vars: Vec<Var>,
    vars: Vec<Var>,
    verbosity: usize,
}

impl VarMgr {
    pub(crate) fn new(verbosity: usize) -> Self {
        VarMgr {
            recipe_vars: Vec::new(),
            vars: Vec::new(),
            free: Vec::new(),
            verbosity,
        }
    }

    pub(crate) fn set_recipe_var(&mut self, name: &str, val: VarValue) {
        output!(self.verbosity, 2, "Setting recipe var {}", name);
        for v in self.recipe_vars.iter_mut() {
            if v.name == name {
                output!(self.verbosity, 2, "Changing recipe {} to {:?}", name, val);
                v.value = val;
                return;
            }
        }
        output!(self.verbosity, 2, "New recipe var {}: {:?}", name, val);
        self.recipe_vars.push(Var{name: name.to_string(), value: val});
    }

    pub(crate) fn set_var(&mut self, name: &str, val: VarValue) {
        output!(self.verbosity, 2, "Setting a var {}", name);
        for v in self.recipe_vars.iter_mut() {
            if v.name == name {
                output!(self.verbosity, 2, "Changing recipe {} to {:?}", name, val);
                v.value = val;
                return;
            }
        }
        for v in self.vars.iter_mut() {
            if v.name == name {
                output!(self.verbosity, 2, "Changing var {} to {:?}", name, val);
                v.value = val;
                return;
            }
        }
        output!(self.verbosity, 2, "New var {}: {:?}", name, val);
        self.vars.push(Var{name: name.to_string(), value: val});
    }

    pub(crate) fn var(&self, name: &str) -> VarValue {
        for v in self.recipe_vars.iter() {
            if v.name == name {
                output!(self.verbosity, 2, "Local recipe var {} found", name);
                return v.value.clone();
            }
        }
        for v in self.vars.iter() {
            if v.name == name {
                output!(self.verbosity, 2, "Global var {} found", name);
                return v.value.clone();
            }
        }

        if let Ok(s) = env::var(name) {
            output!(self.verbosity, 2, "Use environment variable {}", name);
            return VarValue::Str(s);
        }

        output!(self.verbosity, 2, "Variable {} not found", name);
        VarValue::Undefined
    }

    pub(crate) fn interpolate(&self, in_str: &str, flat: bool) -> String {
        let mut start_s: usize;
        let mut start_d: usize;
        let mut res = String::new();
        let mut s_ptr = in_str;

        while !s_ptr.is_empty() {
            start_d = match s_ptr.find('$') {
                None => usize::MAX,
                Some(pos) => pos,
            };
            start_s = match s_ptr.find('\\') {
                None => usize::MAX,
                Some(pos) => pos,
            };

            if start_s == usize::MAX && start_d == usize::MAX {
                return res + s_ptr;
            }

            if start_s == usize::MAX || start_d < start_s {
                res += &s_ptr[..start_d];
                s_ptr = &s_ptr[start_d..];
                // escaped '$'
                if s_ptr.starts_with("$$") {
                    res += "$";
                    s_ptr = &s_ptr["$$".len()..];
                    continue;
                }

                // stray '$' - skip it for now
                if !s_ptr.starts_with("${") {
                    res += "$";
                    s_ptr = &s_ptr["$".len()..];
                    continue;
                }

                // we have "${" - variable substitution starts
                s_ptr = &s_ptr["${".len()..];
                match s_ptr.find('}') {
                    None => return res + "${" + s_ptr,
                    Some(bp) => {
                        let var_name = &s_ptr[..bp];
                        if flat {
                            res += self.var(var_name).to_flat_string().as_str();
                        } else {
                            res += self.var(var_name).to_string().as_str();
                        }
                        s_ptr = &s_ptr[(bp+"}".len())..];
                    },
                }
                continue
            }

            res += &s_ptr[..start_s];
            s_ptr = &s_ptr[start_s..];
            // escaped '\\'
            if s_ptr.starts_with("\\\\") {
                res += "\\";
                s_ptr = &s_ptr["\\\\".len()..];
                continue;
            }
            // escaped \n
            if s_ptr.starts_with("\\n") {
                res += "\n";
                s_ptr = &s_ptr["\\n".len()..];
                continue;
            }
            // escaped \t
            if s_ptr.starts_with("\\t") {
                res += "\t";
                s_ptr = &s_ptr["\\t".len()..];
                continue;
            }
            // escaped $
            if s_ptr.starts_with("\\$") {
                res += "$";
                s_ptr = &s_ptr["\\$".len()..];
                continue;
            }
            // not escaped - take the next letter as is
            s_ptr = &s_ptr["\\".len()..];
        }
        res
    }
}

#[cfg(test)]
mod var_test {
    use super::*;

    #[test]
    fn var_mgr() {
        let mut v = VarMgr::new(0);
        v.set_var("abc", VarValue::Int(123));
        v.recipe_vars.push(Var{name: "def".to_string(), value: VarValue::Int(10)});
        let v1 = v.var("def");
        assert_eq!(v1, VarValue::Int(10));
        let v1 = v.var("abc");
        assert_eq!(v1, VarValue::Int(123));
        let v1 = v.var("abc2");
        assert_eq!(v1, VarValue::Undefined);
        v.recipe_vars.push(Var{name: "abc".to_string(), value: VarValue::Int(50)});
        let v1 = v.var("abc");
        assert_eq!(v1, VarValue::Int(50));
        v.recipe_vars.clear();
        let v1 = v.var("abc");
        assert_eq!(v1, VarValue::Int(123));
    }

    #[test]
    fn interpolate_no_matches() {
        let mut v = VarMgr::new(0);
        v.set_var("abc", VarValue::Str("123".to_string()));
        // no brackets
        let instr = "text $abc end";
        let outstr = v.interpolate(instr, false);
        assert_eq!(instr, &outstr);
        // escaped $
        let instr = "text $${abc} end";
        let outstr = v.interpolate(instr, false);
        assert_eq!("text ${abc} end", &outstr);
        // inclosed variable name
        let instr = "text ${abc end";
        let outstr = v.interpolate(instr, false);
        assert_eq!(instr, &outstr);
        // empty string
        let instr = "";
        let outstr = v.interpolate(instr, false);
        assert_eq!(instr, &outstr);
    }

    #[test]
    fn interpolate_one_match() {
        let mut v = VarMgr::new(0);
        v.set_var("abc", VarValue::from("123"));
        // escaped $
        let instr = "text $$${abc} end";
        let outstr = v.interpolate(instr, false);
        assert_eq!("text $123 end", &outstr);
        // no escaping
        let instr = "text ${abc} end";
        let outstr = v.interpolate(instr, false);
        assert_eq!("text 123 end", &outstr);
        // no escaping and no variable
        let instr = "text ${abc2} end";
        let outstr = v.interpolate(instr, false);
        assert_eq!("text  end", &outstr);
        // variable only
        let instr = "${abc}";
        let outstr = v.interpolate(instr, false);
        assert_eq!("123", &outstr);
        // non-existing variable only
        let instr = "${abcd}";
        let outstr = v.interpolate(instr, false);
        assert_eq!("", &outstr);
    }

    #[test]
    fn interpolate_few_matches() {
        let mut v = VarMgr::new(0);
        v.set_var("abc", VarValue::from("123"));
        v.set_var("def", VarValue::from("test"));
        // escaped $
        let instr = "text ${def}$$${abc} end";
        let outstr = v.interpolate(instr, false);
        assert_eq!("text test$123 end", &outstr);
        // no escaping
        let instr = "text ${abc} end ${def}";
        let outstr = v.interpolate(instr, false);
        assert_eq!("text 123 end test", &outstr);
        // no escaping and no variable
        let instr = "${def} text ${abc2} end";
        let outstr = v.interpolate(instr, false);
        assert_eq!("test text  end", &outstr);
        // variables only
        let instr = "${def}${abc}";
        let outstr = v.interpolate(instr, false);
        assert_eq!("test123", &outstr);
    }

    #[test]
    fn interpolate_mixed_matches() {
        let mut v = VarMgr::new(0);
        v.set_var("abc", VarValue::from("123"));
        v.set_var("def", VarValue::from("test"));
        // escaped $
        let instr = "text ${def}$${abc} end";
        let outstr = v.interpolate(instr, false);
        assert_eq!("text test${abc} end", &outstr);
        // no escaping
        let instr = "text $abc end ${def}";
        let outstr = v.interpolate(instr, false);
        assert_eq!("text $abc end test", &outstr);
    }

    #[test]
    fn unescaped() {
        let v = VarMgr::new(0);
        let ostr = v.interpolate("abcde 12345", false);
        assert_eq!(&ostr, "abcde 12345");
        let ostr = v.interpolate("", false);
        assert_eq!(&ostr, "");
        let ostr = v.interpolate("1234\\5678\\90", false);
        assert_eq!(&ostr, "1234567890");
        let ostr = v.interpolate("\\t1234\\\\5678\\n90\\t", false);
        assert_eq!(&ostr, "\t1234\\5678\n90\t");
    }

    #[test]
    fn mixed_interpolation() {
        let mut v = VarMgr::new(0);
        v.set_var("abc", VarValue::from("123"));
        v.set_var("def", VarValue::from("test"));
        // slash goes first
        let instr = "\\t${def} text ${ab\\nc} end";
        let outstr = v.interpolate(instr, false);
        assert_eq!("\ttest text  end", &outstr);
        // dollar goes first
        let instr = "${def}\\ttext ${abc} end";
        let outstr = v.interpolate(instr, false);
        assert_eq!("test\ttext 123 end", &outstr);
        // mixed escaping
        let instr = "\\${def} ${abc} end";
        let outstr = v.interpolate(instr, false);
        assert_eq!("${def} 123 end", &outstr);
        let instr = "$$\\$$${def} $$${abc}$$ end\\$";
        let outstr = v.interpolate(instr, false);
        assert_eq!("$$${def} $123$ end$", &outstr);
    }
}

