use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use jaw::*;

/*

fn has_prog(prog: &str, arg: &str) -> bool {
    Command::new(prog)
        .arg(arg)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn python_cmd() -> Option<&'static str> {
    if has_prog("python3", "--version") {
        Some("python3")
    } else if has_prog("python", "--version") {
        Some("python")
    } else {
        None
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn regenerate_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    let jaw_path = root.join("assets").join("basic.jaw");
    let src = fs::read_to_string(&jaw_path)?;
    let stem = jaw_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("module");

    // Generate Python
    {
        let pm = PartialModule::from_string(stem, &src)?.compile();
        let out_path = root.join("generated/python/basic.py");
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let f = fs::File::create(&out_path)?;
        let mut w = std::io::BufWriter::new(f);
        emit_for(KnownGenerators::PYTHON, pm, &mut w)?;
        w.flush()?;
    }

    // Generate C++ header
    {
        let pm = PartialModule::from_string(stem, &src)?.compile();
        let out_path = root.join("generated/cpp/src/basic.hpp");
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let f = fs::File::create(&out_path)?;
        let mut w = std::io::BufWriter::new(f);
        emit_for(KnownGenerators::CPP, pm, &mut w)?;
        w.flush()?;
    }

    Ok(())
}

fn cmake_configure_build(build_dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let root = repo_root();
    let src_dir = root.join("generated/cpp");
    fs::create_dir_all(build_dir)?;

    // Configure
    let status = Command::new("cmake")
        .arg("-S")
        .arg(&src_dir)
        .arg("-B")
        .arg(build_dir)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .status()?;
    if !status.success() {
        return Err("cmake configure failed".into());
    }

    // Build
    let status = Command::new("cmake")
        .arg("--build")
        .arg(build_dir)
        .arg("--config")
        .arg("Release")
        .status()?;
    if !status.success() {
        return Err("cmake build failed".into());
    }

    // Locate binary (handle single- and multi-config generators)
    let exe_name = if cfg!(target_os = "windows") {
        "jaw_cpp.exe"
    } else {
        "jaw_cpp"
    };
    let candidates = [
        build_dir.join(exe_name),
        build_dir.join("Release").join(exe_name),
        build_dir.join("Debug").join(exe_name),
    ];
    let exe = candidates
        .into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| "built driver not found".to_string())?;
    Ok(exe)
}

fn cargo_build_rust_driver() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let root = repo_root();
    let rust_dir = root.join("generated/rust");

    // Build debug driver
    let status = Command::new("cargo")
        .arg("build")
        .current_dir(&rust_dir)
        .status()?;
    if !status.success() {
        return Err("cargo build failed".into());
    }

    // Locate binary
    let exe_name = if cfg!(target_os = "windows") {
        "jaw_rust.exe"
    } else {
        "jaw_rust"
    };
    let candidates = [
        rust_dir.join("target").join("debug").join(exe_name),
        rust_dir.join("target").join("release").join(exe_name),
    ];
    let exe = candidates
        .into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| "built rust driver not found".to_string())?;
    Ok(exe)
}

fn run_cpp_dump(exe: &Path, out_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new(exe).arg("--dump").arg(out_path).status()?;
    if !status.success() {
        return Err("C++ driver --dump failed".into());
    }
    Ok(())
}

fn run_cpp_read(exe: &Path, in_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new(exe).arg("--read").arg(in_path).status()?;
    if !status.success() {
        return Err("C++ driver --read failed".into());
    }
    Ok(())
}

fn run_python_dump(py: &str, out_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    let driver = root.join("generated/python/driver.py");
    let status = Command::new(py)
        .arg(&driver)
        .arg("--dump")
        .arg(out_path)
        .status()?;
    if !status.success() {
        return Err("Python driver --dump failed".into());
    }
    Ok(())
}

fn run_python_read(py: &str, in_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    let driver = root.join("generated/python/driver.py");
    let status = Command::new(py)
        .arg(&driver)
        .arg("--read")
        .arg(in_path)
        .status()?;
    if !status.success() {
        return Err("Python driver --read failed".into());
    }
    Ok(())
}

fn run_rust_dump(exe: &Path, out_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new(exe).arg("--dump").arg(out_path).status()?;
    if !status.success() {
        return Err("Rust driver --dump failed".into());
    }
    Ok(())
}

fn run_rust_read(exe: &Path, in_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new(exe).arg("--read").arg(in_path).status()?;
    if !status.success() {
        return Err("Rust driver --read failed".into());
    }
    Ok(())
}

#[test]
fn cross_language_roundtrip_and_compatibility() -> Result<(), Box<dyn std::error::Error>> {
    // Pre-flight checks
    if !has_prog("cmake", "--version") {
        eprintln!("skipping: cmake not found");
        return Ok(());
    }
    let Some(py) = python_cmd() else {
        eprintln!("skipping: python not found");
        return Ok(());
    };

    // 1) Regenerate bindings to ensure up-to-date code
    regenerate_bindings()?;

    // 2) Build C++ driver in a temp build dir under target/
    let target_dir = repo_root().join("target").join("itests").join("cpp_build");
    // Add some uniqueness to avoid clashes in parallel runs
    let unique = format!(
        "run_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let build_dir = target_dir.join(unique);
    let cpp_exe = cmake_configure_build(&build_dir)?;

    // 2b) Build Rust driver (debug)
    let rust_exe = cargo_build_rust_driver()?;

    // 3) Create temp dir for dumps
    let dumps_dir = build_dir.join("dumps");
    fs::create_dir_all(&dumps_dir)?;
    let cpp_dump = dumps_dir.join("from_cpp.bin");
    let py_dump = dumps_dir.join("from_py.bin");
    let rs_dump = dumps_dir.join("from_rs.bin");

    // 4) Generate dump via C++ and verify via Python + Rust
    run_cpp_dump(&cpp_exe, &cpp_dump)?;
    run_python_read(py, &cpp_dump)?;
    run_rust_read(&rust_exe, &cpp_dump)?;

    // 5) Generate dump via Python and verify via C++ + Rust
    run_python_dump(py, &py_dump)?;
    run_cpp_read(&cpp_exe, &py_dump)?;
    run_rust_read(&rust_exe, &py_dump)?;

    // 6) Generate dump via Rust and verify via Python + C++
    run_rust_dump(&rust_exe, &rs_dump)?;
    run_python_read(py, &rs_dump)?;
    run_cpp_read(&cpp_exe, &rs_dump)?;

    // Do not assert dumps are byte-for-byte identical: padding bytes will be garbage

    Ok(())
}
 */
