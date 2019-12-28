use std::path::{Path, PathBuf};
use std::ffi::OsStr;
use std::env;

use target::{arch, os, os_family, endian, pointer_width};
use log::{debug, info, trace, warn};
use dirs;

use crate::var::{VarValue};

// arch: aarch64, arm, asmjs, hexagon, mips, mips64, msp430, powerpc, powerpc64, s390x
//       sparc, sparc64, wasm32, x86, x86_64, xcore
// os: android, bitrig, dragonfly, emscripten, freebsd, haiku, ios, linux, macos,
//     netbsd, openbsd, solaris, windows
// os_family: unix, windows
// pointer_width: 32, 64
// endian: big, little
//
// file'n'dir functions
//   +test: is_file, is_dir, exists
//   +parts: stem, ext, dir, base
//   +change: with_ext, with_stem, with_filename, add_ext
//   +user: home, config_dir, doc_dir, desktop_dir, temp
//   +misc: join?
//   temp: make_temp_dir, make_temp_file

type FuncResult = Result<VarValue, String>;
enum CheckType {
    IsFile,
    IsDir,
    Exists,
}
enum PathPart {
    Stem,
    Name,
    Dir,
    Ext,
}
enum SysPath {
    Temp,
    Home,
    Docs,
    Config,
}

pub(crate) fn run_func(name: &str, args: &[VarValue]) -> FuncResult {
    let lowstr = name.to_lowercase();
    match lowstr.as_str() {
        "os" => Ok(VarValue::Str(os().to_string())),
        "family" => Ok(VarValue::Str(os_family().to_string())),
        "bit" => Ok(VarValue::Str(pointer_width().to_string())),
        "arch" => Ok(VarValue::Str(arch().to_string())),
        "endian" => Ok(VarValue::Str(endian().to_string())),
        "is_file" | "is-file" | "isfile" => all_are(args, CheckType::IsFile),
        "is_dir" | "is-dir" | "isdir" => all_are(args, CheckType::IsDir),
        "exists" => all_are(args, CheckType::Exists),
        "stem" => extract_part(args, PathPart::Stem),
        "ext" => extract_part(args, PathPart::Ext),
        "dir" => extract_part(args, PathPart::Dir),
        "filename" => extract_part(args, PathPart::Name),
        "add_ext" | "add-ext" => add_ext(args),
        "with_ext" | "with-ext" => replace_ext(args),
        "with_filename" | "with-filename"
            | "with_name" | "with-name" => replace_name(args),
        "with_stem" | "with-stem" => replace_stem(args),
        "join" => join_path(args),
        "temp" | "temp_dir" | "temp-dir" => system_path(SysPath::Temp),
        "home" | "home_dir" | "home-dir"
            | "user_dir" | "user-dir" => system_path(SysPath::Home),
        "config" | "config_dir" | "config-dir" => system_path(SysPath::Config),
        "documents" | "docs_dir" | "docs-dir" => system_path(SysPath::Docs),
        _ => Err(format!("function {} not found", name)),
    }
}

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

fn extract_part(args: &[VarValue], tp: PathPart) -> FuncResult {
    if args.is_empty() {
        return Ok(VarValue::Int(0));
    }
    let s = args[0].to_string();
    let p = Path::new(&s);
    let empty = OsStr::new("");
    let empty_path = Path::new("");
    match tp {
        PathPart::Stem => Ok(VarValue::Str(p.file_stem().unwrap_or(&empty).to_string_lossy().to_string())),
        PathPart::Ext => Ok(VarValue::Str(p.extension().unwrap_or(&empty).to_string_lossy().to_string())),
        PathPart::Dir => Ok(VarValue::Str(p.parent().unwrap_or(&empty_path).to_string_lossy().to_string())),
        PathPart::Name => Ok(VarValue::Str(p.file_name().unwrap_or(&empty).to_string_lossy().to_string())),
    }
}

fn replace_ext(args: &[VarValue]) -> FuncResult {
    if args.is_empty() {
        return Err("path undefined".to_string());
    }
    if args.len() == 1 {
        return Ok(args[0].clone());
    }
    let mut p = Path::new(&args[0].to_string()).to_owned();
    let ext = if args.len() == 1 {
        String::new()
    } else {
        args[1].to_string()
    };
    p.set_extension(ext);
    Ok(VarValue::Str(p.to_string_lossy().to_string()))
}

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
    Ok(VarValue::Str(p+&e))
}

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
    let fname = if ext.is_empty() {
        new_stem
    } else {
        new_stem + "." + &ext
    };
    Ok(VarValue::Str(dir.join(fname).to_string_lossy().to_string()))
}

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

#[cfg(test)]
mod path_test {
    use super::*;

    #[test]
    fn extract() {
        let v = vec![VarValue::Str("c:\\tmp\\file.abc".to_string())];
        let r = extract_part(&v, PathPart::Ext);
        assert_eq!(r, Ok(VarValue::Str("abc".to_string())));
        let r = extract_part(&v, PathPart::Stem);
        assert_eq!(r, Ok(VarValue::Str("file".to_string())));
        let r = extract_part(&v, PathPart::Name);
        assert_eq!(r, Ok(VarValue::Str("file.abc".to_string())));
        let r = extract_part(&v, PathPart::Dir);
        assert_eq!(r, Ok(VarValue::Str("c:\\tmp".to_string())));
    }

    #[test]
    fn change_ext() {
        let v = vec![VarValue::Str("c:\\tmp\\file.abc".to_string()), VarValue::Str(String::new())];
        let r = replace_ext(&v);
        assert_eq!(r, Ok(VarValue::Str("c:\\tmp\\file".to_string())));
        let v = vec![VarValue::Str("c:\\tmp\\file.abc".to_string()), VarValue::Str("def".to_string())];
        let r = replace_ext(&v);
        assert_eq!(r, Ok(VarValue::Str("c:\\tmp\\file.def".to_string())));
        let v = vec![VarValue::Str("c:\\tmp\\file.abc".to_string())];
        let r = replace_ext(&v);
        assert_eq!(r, Ok(VarValue::Str("c:\\tmp\\file.abc".to_string())));
    }

    #[test]
    fn append_ext() {
        let v = vec![VarValue::Str("c:\\tmp\\file.abc".to_string()), VarValue::Str(String::new())];
        let r = add_ext(&v);
        assert_eq!(r, Ok(VarValue::Str("c:\\tmp\\file.abc".to_string())));
        let v = vec![VarValue::Str("c:\\tmp\\file.abc".to_string()), VarValue::Str("def".to_string())];
        let r = add_ext(&v);
        assert_eq!(r, Ok(VarValue::Str("c:\\tmp\\file.abc.def".to_string())));
        let v = vec![VarValue::Str("c:\\tmp\\file.abc".to_string())];
        let r = add_ext(&v);
        assert_eq!(r, Ok(VarValue::Str("c:\\tmp\\file.abc".to_string())));
    }

    #[test]
    fn change_name() {
        let v = vec![VarValue::Str("c:\\tmp\\file.abc".to_string()), VarValue::Str(String::new())];
        let r = replace_name(&v);
        assert_eq!(r, Ok(VarValue::Str("c:\\tmp\\".to_string())));
        let v = vec![VarValue::Str("c:\\tmp\\file.abc".to_string()), VarValue::Str("some.def".to_string())];
        let r = replace_name(&v);
        assert_eq!(r, Ok(VarValue::Str("c:\\tmp\\some.def".to_string())));
        let v = vec![VarValue::Str("c:\\tmp\\file.abc".to_string())];
        let r = replace_name(&v);
        assert!(r.is_err());
    }

    #[test]
    fn change_stem() {
        let v = vec![VarValue::Str("c:\\tmp\\file.abc".to_string()), VarValue::Str(String::new())];
        let r = replace_stem(&v);
        assert!(r.is_err());
        let v = vec![VarValue::Str("c:\\tmp\\file.abc".to_string()), VarValue::Str("some.def".to_string())];
        let r = replace_stem(&v);
        assert_eq!(r, Ok(VarValue::Str("c:\\tmp\\some.def.abc".to_string())));
        let v = vec![VarValue::Str("c:\\tmp\\file.abc".to_string()), VarValue::Str("some".to_string())];
        let r = replace_stem(&v);
        assert_eq!(r, Ok(VarValue::Str("c:\\tmp\\some.abc".to_string())));
        let v = vec![VarValue::Str("c:\\tmp\\file.abc".to_string())];
        let r = replace_stem(&v);
        assert!(r.is_err());
    }
}
