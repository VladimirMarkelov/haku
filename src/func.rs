use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use chrono;
use dirs;
use rand::prelude::*;
use regex::Regex;
use target::{arch, endian, os, os_family, pointer_width};
use glob::glob;
use unicode_width::UnicodeWidthStr;

use crate::var::VarValue;
use crate::vm::Engine;

// arch: aarch64, arm, asmjs, hexagon, mips, mips64, msp430, powerpc, powerpc64, s390x
//       sparc, sparc64, wasm32, x86, x86_64, xcore
// os: android, bitrig, dragonfly, emscripten, freebsd, haiku, ios, linux, macos,
//     netbsd, openbsd, solaris, windows
// os_family: unix, windows
// pointer_width: 32, 64
// endian: big, little

/// default alphabet to generate random strings
const LETTERS: &str = "0123456789abcdefghijklmnopqrstuvwxyz";

pub(crate) type FuncResult = Result<VarValue, String>;
/// File path check: object is file, object is directory, object exists
enum CheckType {
    IsFile,
    IsDir,
    Exists,
}
/// Which part of file path to return/replace
enum PathPart {
    /// filename stem: `stem("/abc/file1.txt")` -> `"file1"`
    Stem,
    /// filename: `name("/abc/file1.txt")` -> `"file1.txt"`
    Name,
    /// file parent directory: `dir("/abc/file1.txt")` -> `"/abc"`
    Dir,
    /// file extension(without leading dot): `ext("/abc/file1.txt")` -> `"txt"`
    Ext,
}
/// System directories
enum SysPath {
    /// directory for temporary files
    Temp,
    /// user home directory
    Home,
    /// user documents directory
    Docs,
    /// directory for application configuration files
    Config,
}
/// What end use to trim/pad
enum Where {
    /// both ends
    All,
    /// left end
    Left,
    /// right end
    Right,
}
/// Character case
enum StrCase {
    /// upcase
    Up,
    /// lowcase
    Low,
}

pub(crate) fn run_func(name: &str, eng: &mut Engine, args: &[VarValue]) -> FuncResult {
    let lowstr = name.to_lowercase();
    match lowstr.as_str() {
        "os" => Ok(VarValue::from(os())),
        "family" | "platform" => Ok(VarValue::from(os_family())),
        "bit" => Ok(VarValue::from(pointer_width())),
        "arch" => Ok(VarValue::from(arch())),
        "endian" => Ok(VarValue::from(endian())),
        "is_file" | "is-file" | "isfile" => all_are(args, CheckType::IsFile),
        "is_dir" | "is-dir" | "isdir" => all_are(args, CheckType::IsDir),
        "exists" => all_are(args, CheckType::Exists),
        "stem" => extract_part(args, PathPart::Stem),
        "ext" => extract_part(args, PathPart::Ext),
        "dir" => extract_part(args, PathPart::Dir),
        "filename" => extract_part(args, PathPart::Name),
        "add_ext" | "add-ext" => add_ext(args),
        "with_ext" | "with-ext" => replace_ext(args),
        "with_filename" | "with-filename" | "with_name" | "with-name" => replace_name(args),
        "with_stem" | "with-stem" => replace_stem(args),
        "join" => join_path(args),
        "temp" | "temp_dir" | "temp-dir" => system_path(SysPath::Temp),
        "home" | "home_dir" | "home-dir" | "user_dir" | "user-dir" => system_path(SysPath::Home),
        "config" | "config_dir" | "config-dir" => system_path(SysPath::Config),
        "documents" | "docs_dir" | "docs-dir" => system_path(SysPath::Docs),
        "print" => print_all(args, false),
        "println" => print_all(args, true),
        "time" | "format-time" | "format_time" | "time-format" | "time_format" => format_time(args),
        "trim" => trim_string(args, Where::All),
        "trim_left" | "trim-left" | "trim_start" | "trim-start" => trim_string(args, Where::Left),
        "trim_right" | "trim-right" | "trim_end" | "trim-end" => trim_string(args, Where::Right),
        "starts-with" | "starts_with" => starts_with(args),
        "ends-with" | "ends_with" => ends_with(args),
        "lowcase" => change_case(args, StrCase::Low),
        "upcase" => change_case(args, StrCase::Up),
        "contains" => contains(args),
        "replace" => replace(args),
        "match" => match_regex(args),
        "pad-center" | "pad_center" => pad(args, Where::All),
        "pad-left" | "pad_left" => pad(args, Where::Left),
        "pad-right" | "pad_right" => pad(args, Where::Right),
        "field" | "fields" => fields(args),
        "field-sep" | "fields-sep" | "field_sep" | "fields_sep" => fields_with_sep(args),
        "rand-str" | "rand_str" => rand_string(args),
        "inc" => increment(args),
        "dec" => decrement(args),
        "shell" => change_shell(eng, args),
        "invoke-dir" | "invoke_dir" | "invokedir" => {
            if eng.cwd_history.is_empty() {
                return Ok(VarValue::from(eng.cwd.clone().to_string_lossy().to_string()));
            }
            Ok(VarValue::from(eng.cwd_history[0].clone().to_string_lossy().to_string()))
        }
        "set-env" | "set_env" | "setenv" => set_env_var(eng, args),
        "del-env" | "del_env" | "delenv" => del_env_var(eng, args),
        "clear-env" | "clear_env" | "clearenv" => eng.clear_env_vars(),
        "glob" => globfiles(args),
        _ => Err(format!("function {} not found", name)),
    }
}

fn change_shell(eng: &mut Engine, args: &[VarValue]) -> FuncResult {
    let v: Vec<String> = args.iter().map(|v| v.to_string()).filter(|a| !a.is_empty()).collect();
    eng.set_shell(v)
}

fn set_env_var(eng: &mut Engine, args: &[VarValue]) -> FuncResult {
    let v: Vec<String> = args.iter().map(|v| v.to_string()).filter(|a| !a.is_empty()).collect();
    let name = if v.is_empty() { String::new() } else { v[0].clone() };
    let val = if v.len() > 1 { v[1].clone() } else { String::new() };
    eng.set_env_var(name, val)
}

fn del_env_var(eng: &mut Engine, args: &[VarValue]) -> FuncResult {
    let v: Vec<String> = args.iter().map(|v| v.to_string()).filter(|a| !a.is_empty()).collect();
    let name = if v.is_empty() { String::new() } else { v[0].clone() };
    eng.del_env_var(name)
}

/// Checks if all paths are the same: files, directories, existing filesystem objects
fn all_are(args: &[VarValue], tp: CheckType) -> FuncResult {
    if args.is_empty() {
        return Ok(VarValue::Int(0));
    }
    for arg in args {
        let s = arg.to_string();
        let p = Path::new(&s);
        let ok = match tp {
            CheckType::IsFile => p.is_file(),
            CheckType::IsDir => p.is_dir(),
            CheckType::Exists => p.exists(),
        };
        if !ok {
            return Ok(VarValue::Int(0));
        }
    }
    Ok(VarValue::Int(1))
}

/// Extract a part of a filesystem path: stem, name, parent directory, extension
fn extract_part(args: &[VarValue], tp: PathPart) -> FuncResult {
    if args.is_empty() {
        return Ok(VarValue::Int(0));
    }
    let s = args[0].to_string();
    let p = Path::new(&s);
    let empty = OsStr::new("");
    let empty_path = Path::new("");
    match tp {
        PathPart::Stem => Ok(VarValue::from(p.file_stem().unwrap_or(&empty).to_string_lossy().to_string())),
        PathPart::Ext => Ok(VarValue::from(p.extension().unwrap_or(&empty).to_string_lossy().to_string())),
        PathPart::Dir => Ok(VarValue::from(p.parent().unwrap_or(&empty_path).to_string_lossy().to_string())),
        PathPart::Name => Ok(VarValue::from(p.file_name().unwrap_or(&empty).to_string_lossy().to_string())),
    }
}

/// Replaces path extension:
///
/// - First value is the path
/// - Second values is the new extension (without leading dot)
///
/// replace_ext(&["/abc/file.txt", "msg") -> "/abc/file.msg"
fn replace_ext(args: &[VarValue]) -> FuncResult {
    if args.is_empty() {
        return Err("path undefined".to_string());
    }
    if args.len() == 1 {
        return Ok(args[0].clone());
    }
    let mut p = Path::new(&args[0].to_string()).to_owned();
    let ext = if args.len() == 1 { String::new() } else { args[1].to_string() };
    p.set_extension(ext);
    Ok(VarValue::Str(p.to_string_lossy().to_string()))
}

/// Adds an extension to a path:
///
/// - First value is the path
/// - Second values is the new extension (without leading dot)
///
/// add_ext(&["/abc/file.txt", "msg") -> "/abc/file.txt.msg"
fn add_ext(args: &[VarValue]) -> FuncResult {
    if args.is_empty() {
        return Err("path undefined".to_string());
    }
    if args.len() == 1 {
        return Ok(args[0].clone());
    }
    let p = args[0].to_string();
    let mut e = args[1].to_string();
    if e.is_empty() {
        return Ok(VarValue::Str(p));
    }
    if !e.starts_with('.') {
        e = format!(".{}", e);
    }
    Ok(VarValue::Str(p + &e))
}

/// Replaces the last name in a path:
///
/// - First value is the path
/// - Second values is the new name
///
/// add_ext(&["/abc/file.txt", "out.msg") -> "/abc/out.msg"
fn replace_name(args: &[VarValue]) -> FuncResult {
    if args.is_empty() {
        return Err("path undefined".to_string());
    }
    if args.len() == 1 {
        return Err("new name undefined".to_string());
    }
    let mut p = Path::new(&args[0].to_string()).to_owned();
    let new_name = Path::new(&args[1].to_string()).to_owned();
    p.set_file_name(new_name);
    Ok(VarValue::Str(p.to_string_lossy().to_string()))
}

/// Replaces the name stem in a path:
///
/// - First value is the path
/// - Second values is the new stem
///
/// add_ext(&["/abc/file.txt", "out") -> "/abc/out.txt"
fn replace_stem(args: &[VarValue]) -> FuncResult {
    if args.is_empty() {
        return Err("path undefined".to_string());
    }
    if args.len() == 1 {
        return Err("new stem undefined".to_string());
    }
    let arg_str = args[0].to_string();
    let p = Path::new(&arg_str);
    let new_stem = args[1].to_string();
    if new_stem.is_empty() {
        return Err("new stem undefined".to_string());
    }
    let empty = OsStr::new("");
    let empty_path = Path::new("");
    let ext = p.extension().unwrap_or(&empty).to_string_lossy().to_string();
    let dir = p.parent().unwrap_or(&empty_path);
    let fname = if ext.is_empty() { new_stem } else { new_stem + "." + &ext };
    Ok(VarValue::Str(dir.join(fname).to_string_lossy().to_string()))
}

/// Joins all paths into a single one using system path separator
fn join_path(args: &[VarValue]) -> FuncResult {
    if args.is_empty() {
        return Ok(VarValue::Str(String::new()));
    }
    if args.len() == 1 {
        return Ok(args[0].clone());
    }
    let mut path = PathBuf::from(args[0].to_string());
    for a in &args[1..] {
        let astr = a.to_string();
        let p = Path::new(&astr);
        path = path.join(p);
    }
    Ok(VarValue::Str(path.to_string_lossy().to_string()))
}

/// Returns a path to a system directory
fn system_path(pathtype: SysPath) -> FuncResult {
    match pathtype {
        SysPath::Temp => Ok(VarValue::Str(env::temp_dir().to_string_lossy().to_string())),
        SysPath::Home => match dirs::home_dir() {
            None => Err("user home directory undefined".to_string()),
            Some(p) => Ok(VarValue::Str(p.to_string_lossy().to_string())),
        },
        SysPath::Config => match dirs::config_dir() {
            None => Err("user configuration directory indefined".to_string()),
            Some(p) => Ok(VarValue::Str(p.to_string_lossy().to_string())),
        },
        SysPath::Docs => match dirs::document_dir() {
            None => Err("user document directory indefined".to_string()),
            Some(p) => Ok(VarValue::Str(p.to_string_lossy().to_string())),
        },
    }
}

/// Prints all arguments separating them with a space. If `add_new_line` is true,
/// outputs `\n` at the end.
fn print_all(args: &[VarValue], add_new_line: bool) -> FuncResult {
    for v in args.iter() {
        print!("{}", v.to_string());
    }
    if add_new_line {
        println!();
    }
    Ok(VarValue::Int(1))
}

/// Formats current time using format specification. If the specification is empty
/// the format `"%Y%m%d-%H%M%S"` is used.
fn format_time(args: &[VarValue]) -> FuncResult {
    let now = chrono::Local::now();
    let format = if args.is_empty() { "%Y%m%d-%H%M%S".to_string() } else { args[0].to_flat_string() };
    let r = match format.to_lowercase().as_str() {
        "2822" | "rfc2822" => now.to_rfc2822(),
        "3339" | "rfc3339" => now.to_rfc3339(),
        _ => now.format(&format).to_string(),
    };
    Ok(VarValue::Str(r))
}

/// Trims characters from a string. Function with one argument trims all whitespaces.
/// Otherwise, it trims the first character of the second string from the first one.
fn trim_string(args: &[VarValue], dir: Where) -> FuncResult {
    if args.is_empty() {
        return Ok(VarValue::Str(String::new()));
    }

    let s = args[0].to_string();
    if args.len() == 1 {
        let st = match dir {
            Where::All => s.trim(),
            Where::Left => s.trim_start(),
            Where::Right => s.trim_end(),
        };
        return Ok(VarValue::from(st));
    }

    let what = args[1].to_string().chars().next();
    let c = match what {
        None => return Ok(VarValue::Str(s)),
        Some(cc) => cc,
    };
    let st = match dir {
        Where::All => s.trim_matches(c),
        Where::Left => s.trim_start_matches(c),
        Where::Right => s.trim_end_matches(c),
    };
    Ok(VarValue::from(st))
}

/// Checks if the string starts with a substring. The function accepts unlimited number
/// of argument. It returns `true` if a string (the first argument) starts with any
/// substring(the rest arguments).
fn starts_with(args: &[VarValue]) -> FuncResult {
    if args.len() < 2 {
        return Ok(VarValue::Int(1));
    }

    let s = args[0].to_string();
    for a in args[1..].iter() {
        let what = a.to_string();
        if s.starts_with(&what) {
            return Ok(VarValue::Int(1));
        }
    }
    Ok(VarValue::Int(0))
}

/// Checks if the string ends with a substring. The function accepts unlimited number
/// of argument. It returns `true` if a string (the first argument) ends with any
/// substrings(the rest arguments).
fn ends_with(args: &[VarValue]) -> FuncResult {
    if args.len() < 2 {
        return Ok(VarValue::Int(1));
    }

    let s = args[0].to_string();
    for a in args[1..].iter() {
        let what = a.to_string();
        if s.ends_with(&what) {
            return Ok(VarValue::Int(1));
        }
    }
    Ok(VarValue::Int(0))
}

/// Returns a string with changed character case
fn change_case(args: &[VarValue], case: StrCase) -> FuncResult {
    if args.is_empty() {
        return Ok(VarValue::Str(String::new()));
    }

    let s = args[0].to_string();
    let res = match case {
        StrCase::Up => s.to_uppercase(),
        StrCase::Low => s.to_lowercase(),
    };
    Ok(VarValue::Str(res))
}

/// Checks if the string contains with a substring. The function accepts unlimited number
/// of argument. It returns `true` if a string (the first argument) contains any
/// substrings(the rest arguments).
fn contains(args: &[VarValue]) -> FuncResult {
    if args.len() < 2 {
        return Ok(VarValue::Int(1));
    }

    let s = args[0].to_string();
    for a in args[1..].iter() {
        let what = a.to_string();
        if s.find(&what).is_some() {
            return Ok(VarValue::Int(1));
        }
    }
    Ok(VarValue::Int(0))
}

/// Replaces a substring with another substring. The source string is the first argument. The
/// second argument is the substring to look for. The third argument is the value to replace
/// with. If the third argument is missing, the function just removes the substring.
fn replace(args: &[VarValue]) -> FuncResult {
    if args.len() < 2 {
        return Err("requires at least two arguments".to_string());
    }

    let s = args[0].to_string();
    let what = args[1].to_string();
    let with = if args.len() > 2 { args[2].to_string() } else { String::new() };
    Ok(VarValue::Str(s.replace(&what, &with)))
}

/// Checks if the string matches a regular expression. The function accepts unlimited number
/// of argument. It returns `true` if a string (the first argument) matches any
/// regular expression(the rest arguments).
fn match_regex(args: &[VarValue]) -> FuncResult {
    if args.len() < 2 {
        return Ok(VarValue::from(1));
    }

    let s = args[0].to_string();
    for a in args[1..].iter() {
        let rx = a.to_string();
        match Regex::new(&rx) {
            Err(e) => return Err(e.to_string()),
            Ok(r) => {
                if r.is_match(&s) {
                    return Ok(VarValue::Int(1));
                }
            }
        }
    }
    Ok(VarValue::Int(0))
}

/// Pads a string with another string until its length equals a given one. The result string
/// never exceeds the given length. So, if padding string length is greater than one character,
/// the result may be shorter than expected one.
///
/// NOTE: string length is calculated in UTF8 characters, not in bytes.
fn pad(args: &[VarValue], loc: Where) -> FuncResult {
    if args.len() < 3 {
        return Err("requires three arguments".to_string());
    }

    let patt = args[1].to_string();
    let patt_width = patt.width() as usize;
    if patt_width == 0 {
        return Err("pad string cannot be empty".to_string());
    }
    let l = args[2].to_int() as usize;
    let s = args[0].to_string();
    let orig_width = s.width() as usize;

    if orig_width + patt_width >= l {
        return Ok(VarValue::from(s));
    }

    let cnt = (l - orig_width) / patt_width;

    let res = match loc {
        Where::All => {
            let right = cnt / 2;
            let left = cnt - right;
            patt.repeat(left) + &s + &patt.repeat(right)
        }
        Where::Left => patt.repeat(cnt) + &s,
        Where::Right => s + &patt.repeat(cnt),
    };
    Ok(VarValue::from(res))
}

/// Treats a string(the first argument) as a string with values delimited with whitespaces, and
/// returns the fields by their indices(the rest arguments) as an array of strings. Field index
/// starts with 0.  If field index is equal to or greater than the number of values, the empty
/// string returned.
///
/// NOTE: if only one index is given, the function returns single value - a string. For more
/// indices it returns a list of strings.
fn fields(args: &[VarValue]) -> FuncResult {
    if args.len() < 2 {
        return Err("requires at least two arguments".to_string());
    }

    let s = args[0].to_string();
    let mut vals = Vec::new();
    let flds: Vec<&str> = s.split_whitespace().collect();
    for a in args[1..].iter() {
        let idx = a.to_int() as usize;
        if idx >= flds.len() {
            vals.push(String::new());
        } else {
            vals.push(flds[idx].to_string());
        }
    }

    if args.len() == 2 {
        Ok(VarValue::from(vals.pop().unwrap())) // unwrap is OK - vals is never empty here
    } else {
        Ok(VarValue::List(vals))
    }
}

/// Treats a string(the first argument) as a string with values delimited with `sep`(the second
/// argument), and returns the fields by their indices(the rest arguments) as an array of strings.
/// Field index starts with 0.  If field index is equal to or greater than the number of values,
/// the empty string returned.
///
/// NOTE: if only one index is given, the function returns single value - a string. For more
/// indices it returns a list of strings.
fn fields_with_sep(args: &[VarValue]) -> FuncResult {
    if args.len() < 3 {
        return Err("requires at least three arguments".to_string());
    }

    let s = args[0].to_string();
    let sep = args[1].to_string();
    if sep.is_empty() {
        return Err("separator cannot be emtpy".to_string());
    }

    let mut vals = Vec::new();
    let flds: Vec<&str> = s.split(&sep).collect();
    for a in args[2..].iter() {
        let idx = a.to_int() as usize;
        if idx >= flds.len() {
            vals.push(String::new());
        } else {
            vals.push(flds[idx].to_string());
        }
    }

    if args.len() == 3 {
        Ok(VarValue::from(vals.pop().unwrap())) // unwrap is OK - vals is never empty here
    } else {
        Ok(VarValue::List(vals))
    }
}

/// Generates a random string of a given length. First argument is the length of the string.
/// The second argument is alphabet to generate a string(it must be longer than 10 characters).
/// If the seconds argument is missing `LETTERS` is used(digits and lowcase Latin characters).
fn rand_string(args: &[VarValue]) -> FuncResult {
    if args.is_empty() {
        return Err("requires at least one argument".to_string());
    }

    let l = args[0].to_int();
    if l <= 0 {
        return Err("length must be greater than 0".to_string());
    }
    let ls = l as usize;
    let chr: Vec<char> = if args.len() == 1 {
        LETTERS.chars().collect()
    } else {
        let arg2 = args[1].to_string();
        arg2.as_str().chars().collect()
    };
    let mx = chr.len();
    if mx < 10 {
        let r: String = chr.into_iter().collect();
        return Err(format!("alphabet '{}' is too short: must have at least 10 characters", r));
    }

    let mut rng = thread_rng();
    let mut c: Vec<char> = Vec::with_capacity(ls);
    for _idx in 0..ls {
        let cidx = rng.gen_range(0, mx) as usize;
        c.push(chr[cidx]);
    }

    let s: String = c.into_iter().collect();
    Ok(VarValue::from(s))
}

/// Returns incremented value. If only one argument is provided, it is incremented it by one.
/// Otherwise it returns sum of all arguments.
/// NOTE: all values are converted into integers.
fn increment(args: &[VarValue]) -> FuncResult {
    if args.is_empty() {
        return Ok(VarValue::Int(1));
    }

    let mut val = args[0].to_int();
    if args.len() == 1 {
        return Ok(VarValue::Int(val + 1));
    }
    for a in args[1..].iter() {
        let inc = a.to_int();
        val += inc;
    }
    Ok(VarValue::Int(val))
}

/// Returns decremented value. If only one argument is provided, it is decrements it by one.
/// Otherwise it subtracts all values(except the first one) from the first one.
/// NOTE: all values are converted into integers.
fn decrement(args: &[VarValue]) -> FuncResult {
    if args.is_empty() {
        return Ok(VarValue::Int(-1));
    }

    let mut val = args[0].to_int();
    if args.len() == 1 {
        return Ok(VarValue::Int(val - 1));
    }
    for a in args[1..].iter() {
        let inc = a.to_int();
        val -= inc;
    }
    Ok(VarValue::Int(val))
}

fn globfiles(args: &[VarValue]) -> FuncResult {
    let patt = if args.is_empty() {
        "*".to_owned()
    } else {
        args[0].to_string()
    };
    let globtype = if args.len() > 1 {
        args[1].to_int()
    } else {
        0
    };

    let mut v: Vec<String> = Vec::new();
    let entries = match glob(&patt) {
        Ok(p) => p,
        Err(e) => return Err(e.to_string()),
    };

    for entry in entries {
        if let Ok(p) = entry {
            if globtype == 1 && !p.is_file() {
                continue;
            }
            if globtype == 2 && !p.is_dir() {
                continue;
            }
            let s = p.to_string_lossy();
            v.push(s.to_string());
        }
    }

    Ok(VarValue::List(v))
}

#[cfg(test)]
mod path_test {
    use super::*;

    #[test]
    fn extract() {
        #[cfg(windows)]
        let v = vec![VarValue::from("c:\\tmp\\file.abc")];
        #[cfg(unix)]
        let v = vec![VarValue::from("/tmp/file.abc")];
        let r = extract_part(&v, PathPart::Ext);
        assert_eq!(r, Ok(VarValue::from("abc")));
        let r = extract_part(&v, PathPart::Stem);
        assert_eq!(r, Ok(VarValue::from("file")));
        let r = extract_part(&v, PathPart::Name);
        assert_eq!(r, Ok(VarValue::from("file.abc")));
        let r = extract_part(&v, PathPart::Dir);
        #[cfg(windows)]
        assert_eq!(r, Ok(VarValue::from("c:\\tmp")));
        #[cfg(unix)]
        assert_eq!(r, Ok(VarValue::from("/tmp")));
    }

    #[test]
    fn change_ext() {
        let v = vec![VarValue::from("file.abc"), VarValue::Str(String::new())];
        let r = replace_ext(&v);
        assert_eq!(r, Ok(VarValue::from("file")));
        let v = vec![VarValue::from("file.abc"), VarValue::from("def")];
        let r = replace_ext(&v);
        assert_eq!(r, Ok(VarValue::from("file.def")));
        let v = vec![VarValue::from("file.abc")];
        let r = replace_ext(&v);
        assert_eq!(r, Ok(VarValue::from("file.abc")));
    }

    #[test]
    fn append_ext() {
        let v = vec![VarValue::from("file.abc"), VarValue::Str(String::new())];
        let r = add_ext(&v);
        assert_eq!(r, Ok(VarValue::from("file.abc")));
        let v = vec![VarValue::from("file.abc"), VarValue::from("def")];
        let r = add_ext(&v);
        assert_eq!(r, Ok(VarValue::from("file.abc.def")));
        let v = vec![VarValue::from("file.abc")];
        let r = add_ext(&v);
        assert_eq!(r, Ok(VarValue::from("file.abc")));
    }

    #[test]
    fn change_name() {
        let v = vec![VarValue::from("file.abc"), VarValue::Str(String::new())];
        let r = replace_name(&v);
        assert_eq!(r, Ok(VarValue::from("")));
        let v = vec![VarValue::from("file.abc"), VarValue::from("some.def")];
        let r = replace_name(&v);
        assert_eq!(r, Ok(VarValue::from("some.def")));
        let v = vec![VarValue::from("file.abc")];
        let r = replace_name(&v);
        assert!(r.is_err());
    }

    #[test]
    fn change_stem() {
        let v = vec![VarValue::from("file.abc"), VarValue::Str(String::new())];
        let r = replace_stem(&v);
        assert!(r.is_err());
        let v = vec![VarValue::from("file.abc"), VarValue::from("some.def")];
        let r = replace_stem(&v);
        assert_eq!(r, Ok(VarValue::from("some.def.abc")));
        let v = vec![VarValue::from("file.abc"), VarValue::from("some")];
        let r = replace_stem(&v);
        assert_eq!(r, Ok(VarValue::from("some.abc")));
        let v = vec![VarValue::from("file.abc")];
        let r = replace_stem(&v);
        assert!(r.is_err());
    }

    #[test]
    fn trims() {
        let v = vec![VarValue::from(" \n abc\t   ")];
        let r = trim_string(&v, Where::All);
        assert_eq!(r, Ok(VarValue::from("abc")));
        let r = trim_string(&v, Where::Left);
        assert_eq!(r, Ok(VarValue::from("abc\t   ")));
        let r = trim_string(&v, Where::Right);
        assert_eq!(r, Ok(VarValue::from(" \n abc")));

        let v = vec![VarValue::from("++abc==="), VarValue::from("+")];
        let r = trim_string(&v, Where::All);
        assert_eq!(r, Ok(VarValue::from("abc===")));
        let v = vec![VarValue::from("++abc==="), VarValue::from("=")];
        let r = trim_string(&v, Where::All);
        assert_eq!(r, Ok(VarValue::from("++abc")));

        let v = vec![VarValue::from("++abc==="), VarValue::from("+")];
        let r = trim_string(&v, Where::Left);
        assert_eq!(r, Ok(VarValue::from("abc===")));
        let r = trim_string(&v, Where::Right);
        assert_eq!(r, Ok(VarValue::from("++abc===")));
    }

    #[test]
    fn end_start() {
        let v = vec![VarValue::from("testabc")];
        let r = starts_with(&v);
        assert_eq!(r, Ok(VarValue::Int(1)));
        let r = ends_with(&v);
        assert_eq!(r, Ok(VarValue::Int(1)));
        let v = vec![VarValue::from("testabc"), VarValue::from("test")];
        let r = starts_with(&v);
        assert_eq!(r, Ok(VarValue::Int(1)));
        let r = ends_with(&v);
        assert_eq!(r, Ok(VarValue::Int(0)));
        let v = vec![VarValue::from("testabc"), VarValue::from("abc")];
        let r = starts_with(&v);
        assert_eq!(r, Ok(VarValue::Int(0)));
        let r = ends_with(&v);
        assert_eq!(r, Ok(VarValue::Int(1)));
        let v = vec![VarValue::from("testabc"), VarValue::from("xxx")];
        let r = starts_with(&v);
        assert_eq!(r, Ok(VarValue::Int(0)));
        let r = ends_with(&v);
        assert_eq!(r, Ok(VarValue::Int(0)));
        let v = vec![VarValue::from("testabc"), VarValue::from("")];
        let r = starts_with(&v);
        assert_eq!(r, Ok(VarValue::Int(1)));
        let r = ends_with(&v);
        assert_eq!(r, Ok(VarValue::Int(1)));
        let v = vec![VarValue::from("testabc"), VarValue::from("test"), VarValue::from("abc")];
        let r = starts_with(&v);
        assert_eq!(r, Ok(VarValue::Int(1)));
        let r = ends_with(&v);
        assert_eq!(r, Ok(VarValue::Int(1)));
    }

    #[test]
    fn up_low() {
        let v = vec![VarValue::from("aBc DeF")];
        let r = change_case(&v, StrCase::Low);
        assert_eq!(r, Ok(VarValue::from("abc def")));
        let r = change_case(&v, StrCase::Up);
        assert_eq!(r, Ok(VarValue::from("ABC DEF")));
    }

    #[test]
    fn contain() {
        let v = vec![VarValue::from("aBc DeF")];
        let r = contains(&v);
        assert_eq!(r, Ok(VarValue::Int(1)));
        let v = vec![VarValue::from("aBc DeF"), VarValue::from("Bc")];
        let r = contains(&v);
        assert_eq!(r, Ok(VarValue::Int(1)));
        let v = vec![VarValue::from("aBc DeF"), VarValue::from("bc")];
        let r = contains(&v);
        assert_eq!(r, Ok(VarValue::Int(0)));
        let v = vec![VarValue::from("aBc DeF"), VarValue::from("bc"), VarValue::from("eF")];
        let r = contains(&v);
        assert_eq!(r, Ok(VarValue::Int(1)));
    }

    #[test]
    fn replaces() {
        let v = vec![VarValue::from("aBc DeF")];
        let r = replace(&v);
        assert!(r.is_err());
        let v = vec![VarValue::from("abc def"), VarValue::from("bc")];
        let r = replace(&v);
        assert_eq!(r, Ok(VarValue::from("a def")));
        let v = vec![VarValue::from("abc def"), VarValue::from("Bc")];
        let r = replace(&v);
        assert_eq!(r, Ok(VarValue::from("abc def")));
        let v = vec![VarValue::from("abc def"), VarValue::from("bc"), VarValue::from("eFG")];
        let r = replace(&v);
        assert_eq!(r, Ok(VarValue::from("aeFG def")));
    }

    #[test]
    fn matches() {
        let v = vec![VarValue::from("aBc DeF")];
        let r = match_regex(&v);
        assert_eq!(r, Ok(VarValue::Int(1)));
        let v = vec![VarValue::from("abc def"), VarValue::from("bc")];
        let r = match_regex(&v);
        assert_eq!(r, Ok(VarValue::from(1)));
        let v = vec![VarValue::from("abc def"), VarValue::from("b.*e")];
        let r = match_regex(&v);
        assert_eq!(r, Ok(VarValue::from(1)));
        let v = vec![VarValue::from("abc def"), VarValue::from("b.*g")];
        let r = match_regex(&v);
        assert_eq!(r, Ok(VarValue::from(0)));
        let v = vec![VarValue::from("abc def"), VarValue::from("b.*g"), VarValue::from("d[mge]+")];
        let r = match_regex(&v);
        assert_eq!(r, Ok(VarValue::from(1)));
    }

    #[test]
    fn pads() {
        let v = vec![VarValue::from("abc")];
        let r = pad(&v, Where::All);
        assert!(r.is_err());
        let v = vec![VarValue::from("abc"), VarValue::from("+=")];
        let r = pad(&v, Where::All);
        assert!(r.is_err());
        let v = vec![VarValue::from("abc"), VarValue::from("")];
        let r = pad(&v, Where::All);
        assert!(r.is_err());

        let v = vec![VarValue::from("abc"), VarValue::from("+="), VarValue::from("aa")];
        let r = pad(&v, Where::All);
        assert_eq!(r, Ok(VarValue::from("abc")));
        let v = vec![VarValue::from("abc"), VarValue::from("+="), VarValue::from(0)];
        let r = pad(&v, Where::All);
        assert_eq!(r, Ok(VarValue::from("abc")));
        let v = vec![VarValue::from("abc"), VarValue::from("+="), VarValue::from(2)];
        let r = pad(&v, Where::All);
        assert_eq!(r, Ok(VarValue::from("abc")));
        let v = vec![VarValue::from("abc"), VarValue::from("+="), VarValue::from(10)];
        let r = pad(&v, Where::All);
        assert_eq!(r, Ok(VarValue::from("+=+=abc+=")));
        let r = pad(&v, Where::Left);
        assert_eq!(r, Ok(VarValue::from("+=+=+=abc")));
        let r = pad(&v, Where::Right);
        assert_eq!(r, Ok(VarValue::from("abc+=+=+=")));

        let v = vec![VarValue::from("abc"), VarValue::from("+="), VarValue::from(11)];
        let r = pad(&v, Where::All);
        assert_eq!(r, Ok(VarValue::from("+=+=abc+=+=")));
    }

    #[test]
    fn field() {
        let v = vec![VarValue::from("abc def\tghi")];
        let r = fields(&v);
        assert!(r.is_err());
        let v = vec![VarValue::from("abc def\tghi"), VarValue::from("s")];
        let r = fields(&v);
        assert_eq!(r, Ok(VarValue::from("abc")));
        let v = vec![VarValue::from("abc def\tghi"), VarValue::from(8)];
        let r = fields(&v);
        assert_eq!(r, Ok(VarValue::from("")));
        let v = vec![VarValue::from("abc def\tghi"), VarValue::from(1), VarValue::from(2), VarValue::from(1)];
        let r = fields(&v);
        assert_eq!(r, Ok(VarValue::List(vec!["def".to_string(), "ghi".to_string(), "def".to_string()])));

        // with separator
        let v = vec![VarValue::from("abc daf\tahi")];
        let r = fields_with_sep(&v);
        assert!(r.is_err());
        let v = vec![VarValue::from("abc daf\tahi"), VarValue::from("a")];
        let r = fields_with_sep(&v);
        assert!(r.is_err());

        let v = vec![VarValue::from("abc daf\tahi"), VarValue::from("a"), VarValue::from(8)];
        let r = fields_with_sep(&v);
        assert_eq!(r, Ok(VarValue::from("")));
        let v = vec![VarValue::from("abc daf\tahi"), VarValue::from("a"), VarValue::from(2), VarValue::from("1")];
        let r = fields_with_sep(&v);
        assert_eq!(r, Ok(VarValue::List(vec!["f\t".to_string(), "bc d".to_string()])));
        let v = vec![VarValue::from("abc daf\tahi"), VarValue::from("a"), VarValue::from(1), VarValue::from("asd")];
        let r = fields_with_sep(&v);
        assert_eq!(r, Ok(VarValue::List(vec!["bc d".to_string(), String::new()])));
    }

    #[test]
    fn rand() {
        let v = vec![VarValue::from(10)];
        let s1 = rand_string(&v);
        let s2 = rand_string(&v);
        assert_eq!(s1.clone().unwrap().to_string().len(), 10);
        assert_eq!(s2.clone().unwrap().to_string().len(), 10);
        assert_ne!(s1, s2);
        let v = vec![VarValue::from(10), VarValue::from("0123456789")];
        let s1 = rand_string(&v);
        for chr in s1.unwrap().to_string().chars() {
            assert!(chr >= '0' && chr <= '9');
        }
    }

    #[test]
    fn inc() {
        let v: Vec<VarValue> = Vec::new();
        let r = increment(&v);
        assert_eq!(r, Ok(VarValue::Int(1)));
        let v = vec![VarValue::from("abc")];
        let r = increment(&v);
        assert_eq!(r, Ok(VarValue::Int(1)));
        let r = increment(&v);
        assert_eq!(r, Ok(VarValue::Int(1)));
        let v = vec![VarValue::from(10)];
        let r = increment(&v);
        assert_eq!(r, Ok(VarValue::Int(11)));
        let v = vec![VarValue::from(10), VarValue::from(-3), VarValue::from(77)];
        let r = increment(&v);
        assert_eq!(r, Ok(VarValue::Int(84)));
    }

    #[test]
    fn dec() {
        let v: Vec<VarValue> = Vec::new();
        let r = decrement(&v);
        assert_eq!(r, Ok(VarValue::Int(-1)));
        let v = vec![VarValue::from("abc")];
        let r = decrement(&v);
        assert_eq!(r, Ok(VarValue::Int(-1)));
        let v = vec![VarValue::from(10)];
        let r = decrement(&v);
        assert_eq!(r, Ok(VarValue::Int(9)));
        let v = vec![VarValue::from(10), VarValue::from(-3), VarValue::from(77)];
        let r = decrement(&v);
        assert_eq!(r, Ok(VarValue::Int(-64)));
    }
}
