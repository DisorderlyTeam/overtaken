use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const GILRS_CORE_VERSION_DIR: &str = "gilrs-core-0.6.7";
const GILRS_CORE_REPO: &str = "https://gitlab.com/gilrs-project/gilrs.git";
const GILRS_CORE_REF: &str = "984c4337f090431c0ab8f59287de9b4266d14ab2";

fn main() {
    let root = workspace_root();
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("ensure-vendor");

    match cmd {
        "ensure-vendor" => ensure_vendor(&root),
        "vendor" => vendor(&root),
        _ => panic!("unknown command '{cmd}'. expected: ensure-vendor | vendor"),
    }
}

fn ensure_vendor(root: &Path) {
    let dst = root.join("vendor").join("gilrs-core");
    if dst.is_dir() {
        return;
    }
    vendor(root)
}

fn vendor(root: &Path) {
    let vendor_dir = root.join("vendor");
    let dst = vendor_dir.join("gilrs-core");
    let repo = vendor_dir.join(".gilrs-core-repo");
    let patch = root.join("patches").join("gilrs-core-linux-ffrumble.patch");

    if !patch.is_file() {
        panic!("missing patch file: {}", patch.display());
    }

    fs::create_dir_all(&vendor_dir).unwrap();
    if dst.exists() {
        fs::remove_dir_all(&dst).unwrap();
    }
    if repo.exists() {
        fs::remove_dir_all(&repo).unwrap();
    }

    if let Some(src) = find_cached_gilrs_core_dir().unwrap() {
        eprintln!("vendor_tool: copying from cargo cache: {}", src.display());
        copy_dir_recursive(&src, &dst).unwrap();
    } else {
        eprintln!("vendor_tool: cargo cache miss; cloning from {GILRS_CORE_REPO}");
        run(Command::new("git").args([
            "clone",
            "--filter=blob:none",
            "--depth",
            "1",
            "--no-checkout",
            GILRS_CORE_REPO,
            repo.to_string_lossy().as_ref(),
        ]));

        run(Command::new("git").current_dir(&repo).args([
            "fetch",
            "--depth",
            "1",
            "origin",
            GILRS_CORE_REF,
        ]));
        run(Command::new("git")
            .current_dir(&repo)
            .args(["sparse-checkout", "init", "--no-cone"]));
        run(Command::new("git").current_dir(&repo).args([
            "sparse-checkout",
            "set",
            "--no-cone",
            "/gilrs-core/",
        ]));
        run(Command::new("git")
            .current_dir(&repo)
            .args(["checkout", "--detach", "FETCH_HEAD"]));

        copy_dir_recursive(&repo.join("gilrs-core"), &dst).unwrap();
        fs::remove_dir_all(&repo).unwrap();
    }

    eprintln!("vendor_tool: applying patch {}", patch.display());
    run(Command::new("git").current_dir(root).args([
        "apply",
        "--whitespace=nowarn",
        patch.to_string_lossy().as_ref(),
        "--directory=vendor",
        "-v",
    ]));
}

fn cargo_home() -> PathBuf {
    if let Ok(val) = env::var("CARGO_HOME") {
        return PathBuf::from(val);
    }
    if cfg!(windows) {
        // ~/.cargo doesn't exist on Windows.
        if let Ok(profile) = env::var("USERPROFILE") {
            return PathBuf::from(profile).join(".cargo");
        }
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(".cargo");
    }
    PathBuf::from(".cargo")
}

fn find_cached_gilrs_core_dir() -> io::Result<Option<PathBuf>> {
    let src_root = cargo_home().join("registry").join("src");
    if !src_root.is_dir() {
        return Ok(None);
    }
    for entry in fs::read_dir(&src_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let candidate = entry.path().join(GILRS_CORE_VERSION_DIR);
        if candidate.is_dir() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to)?;
        } else if ty.is_symlink() {
            // Best effort: follow the link contents by copying the target file.
            let target = fs::read_link(&from)?;
            let target_abs = if target.is_absolute() {
                target
            } else {
                from.parent().unwrap_or(src).join(target)
            };
            if target_abs.is_dir() {
                copy_dir_recursive(&target_abs, &to)?;
            } else {
                fs::copy(&target_abs, &to)?;
            }
        }
    }
    Ok(())
}

fn run(cmd: &mut Command) {
    cmd.stdin(Stdio::null());
    let status = cmd.status().unwrap();
    if !status.success() {
        panic!("command failed (exit {status}): {}", display_cmd(cmd))
    }
}

fn display_cmd(cmd: &Command) -> String {
    let mut out = String::new();
    out.push_str(cmd.get_program().to_string_lossy().as_ref());
    for arg in cmd.get_args() {
        out.push(' ');
        out.push_str(&shell_escape(arg));
    }
    out
}

fn shell_escape(s: &OsStr) -> String {
    let s = s.to_string_lossy();
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-:".contains(c))
    {
        return s.into_owned();
    }
    format!("{s:?}")
}

fn workspace_root() -> PathBuf {
    let exe = env::current_exe().unwrap();
    let mut dir = exe.parent().unwrap().to_path_buf();
    // We run under target/{debug,release}/ so walk up until we find Cargo.toml with [workspace].
    for _ in 0..8 {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.is_file() {
            if let Ok(text) = fs::read_to_string(&cargo_toml) {
                if text.contains("[workspace]") {
                    return cargo_toml.parent().unwrap().to_path_buf();
                }
            }
        }
        if !dir.pop() {
            break;
        }
    }
    panic!("failed to locate workspace root (Cargo.toml with [workspace])",)
}
