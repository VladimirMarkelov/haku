use pest::iterators::Pairs;
use target::{arch, os, os_family, endian, pointer_width};

use crate::parse::{Rule};
use crate::vm::{RunOpts};

// arch: aarch64, arm, asmjs, hexagon, mips, mips64, msp430, powerpc, powerpc64, s390x
//       sparc, sparc64, wasm32, x86, x86_64, xcore
// os: android, bitrig, dragonfly, emscripten, freebsd, haiku, ios, linux, macos,
//     netbsd, openbsd, solaris, windows
// os_family: unix, windows
// pointer_width: 32, 64
// endian: big, little

fn check_feature_val(val: &str, p: Pairs<Rule>, neg: bool) -> bool {
    let mut found = false;
    for fv in p {
        let val_low = fv.as_str().to_lowercase();
        if val == val_low.as_str() {
            found = true;
            break;
        }
    }
    if neg {
        found = !found;
    }

   found
}

fn check_feature_list(vals: &[String], p: Pairs<Rule>, neg: bool) -> bool {
    if vals.is_empty() {
        return false;
    }
    let mut found = false;
    for fv in p {
        let val_low = fv.as_str().to_lowercase();
        for val in vals {
            let val = val.to_lowercase();
            if val == val_low {
                found = true;
                break;
            }
        }
        if found {
            break;
        }
    }
    if neg {
        found = !found;
    }

    found
}

pub fn process_feature(p: Pairs<Rule>, opts: &RunOpts) -> Result<bool, String> {
    let mut ok = true;
    for ss in p {
        let mut inverse = false;
        let mut f_name: String = String::new();
        for sss in ss.into_inner() {
            match sss.as_rule() {
                Rule::not_op => { inverse = true; },
                Rule::feature_name => { f_name = sss.as_str().to_lowercase(); },
                Rule::feature_val => {
                    let pass = match f_name.as_str() {
                        "os" => check_feature_val(os(), sss.into_inner(), inverse),
                        "bit" => check_feature_val(pointer_width(), sss.into_inner(), inverse),
                        "family" => check_feature_val(os_family(), sss.into_inner(), inverse),
                        "arch" => check_feature_val(arch(), sss.into_inner(), inverse),
                        "endian" => check_feature_val(endian(), sss.into_inner(), inverse),
                        "feature" | "feat" => check_feature_list(&opts.feats, sss.into_inner(), inverse),
                        _ => return Err(f_name),
                    };
                    ok &= pass;
                    if !ok {
                        return Ok(ok)
                    }
                },
                _ => { unreachable!() },
            }
        }
    }
    Ok(ok)
}
