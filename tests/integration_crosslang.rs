use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jaw::{codegen, compile, intermediate};

type DynError = Box<dyn std::error::Error>;

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

fn copy_file(src: impl AsRef<Path>, dest: impl AsRef<Path>) -> Result<(), DynError> {
    let dest = dest.as_ref();
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dest)?;
    Ok(())
}

fn load_world() -> Result<compile::World, DynError> {
    let jaw_path = repo_root().join("assets").join("example.jaw");
    let src = fs::read_to_string(&jaw_path)?;
    let stem = jaw_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("module")
        .to_string();
    let module = intermediate::Module::from_string(stem, src)?;
    Ok(compile::compile(module)?)
}

struct Bindings {
    python_dir: PathBuf,
    python_driver: PathBuf,
    cpp_src_dir: PathBuf,
    cpp_build_dir: PathBuf,
    rust_dir: PathBuf,
}

fn regenerate_bindings(world: &compile::World, out_root: &Path) -> Result<Bindings, DynError> {
    fs::create_dir_all(out_root)?;

    // Python
    let python_dir = out_root.join("python");
    fs::create_dir_all(&python_dir)?;
    codegen::emit_python(world, &Default::default(), python_dir.join("example.py"))?;
    copy_file(
        repo_root().join("assets/generated/python/driver.py"),
        python_dir.join("driver.py"),
    )?;

    // C++
    let cpp_src_dir = out_root.join("cpp");
    let cpp_src = cpp_src_dir.join("src");
    fs::create_dir_all(&cpp_src)?;
    codegen::emit_cpp(world, &Default::default(), cpp_src.join("example.hpp"))?;
    copy_file(
        repo_root().join("assets/generated/cpp/src/codec.hpp"),
        cpp_src.join("codec.hpp"),
    )?;
    copy_file(
        repo_root().join("assets/generated/cpp/src/main.cpp"),
        cpp_src.join("main.cpp"),
    )?;
    copy_file(
        repo_root().join("assets/generated/cpp/CMakeLists.txt"),
        cpp_src_dir.join("CMakeLists.txt"),
    )?;
    let cpp_build_dir = out_root.join("cpp_build");

    // Rust
    let rust_dir = out_root.join("rust");
    let rust_src = rust_dir.join("src");
    fs::create_dir_all(&rust_src)?;
    copy_file(
        repo_root().join("assets/generated/rust/Cargo.toml"),
        rust_dir.join("Cargo.toml"),
    )?;
    let lock = repo_root().join("assets/generated/rust/Cargo.lock");
    if lock.exists() {
        copy_file(&lock, rust_dir.join("Cargo.lock"))?;
    }
    copy_file(
        repo_root().join("assets/generated/rust/src/main.rs"),
        rust_src.join("main.rs"),
    )?;
    codegen::emit_rust(world, &Default::default(), rust_src.join("example.rs"))?;

    Ok(Bindings {
        python_dir: python_dir.clone(),
        python_driver: python_dir.join("driver.py"),
        cpp_src_dir,
        cpp_build_dir,
        rust_dir,
    })
}

fn cmake_configure_build(src_dir: &Path, build_dir: &Path) -> Result<PathBuf, DynError> {
    fs::create_dir_all(build_dir)?;

    let status = Command::new("cmake")
        .arg("-S")
        .arg(src_dir)
        .arg("-B")
        .arg(build_dir)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .status()?;
    if !status.success() {
        return Err("cmake configure failed".into());
    }

    let status = Command::new("cmake")
        .arg("--build")
        .arg(build_dir)
        .arg("--config")
        .arg("Release")
        .status()?;
    if !status.success() {
        return Err("cmake build failed".into());
    }

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

fn cargo_build_rust_driver(crate_dir: &Path) -> Result<PathBuf, DynError> {
    let status = Command::new("cargo")
        .arg("build")
        .current_dir(crate_dir)
        .status()?;
    if !status.success() {
        return Err("cargo build failed".into());
    }

    let exe_name = if cfg!(target_os = "windows") {
        "jaw_rust.exe"
    } else {
        "jaw_rust"
    };
    let candidates = [
        crate_dir.join("target").join("debug").join(exe_name),
        crate_dir.join("target").join("release").join(exe_name),
    ];
    let exe = candidates
        .into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| "built rust driver not found".to_string())?;
    Ok(exe)
}

fn run_cpp_dump(exe: &Path, out_path: &Path) -> Result<(), DynError> {
    let status = Command::new(exe).arg("--dump").arg(out_path).status()?;
    if !status.success() {
        return Err("C++ driver --dump failed".into());
    }
    Ok(())
}

fn run_cpp_read(exe: &Path, in_path: &Path) -> Result<(), DynError> {
    let status = Command::new(exe).arg("--read").arg(in_path).status()?;
    if !status.success() {
        return Err("C++ driver --read failed".into());
    }
    Ok(())
}

fn run_python_dump(
    py: &str,
    driver: &Path,
    workdir: &Path,
    out_path: &Path,
) -> Result<(), DynError> {
    let status = Command::new(py)
        .arg(driver)
        .arg("--dump")
        .arg(out_path)
        .current_dir(workdir)
        .status()?;
    if !status.success() {
        return Err("Python driver --dump failed".into());
    }
    Ok(())
}

fn run_python_read(
    py: &str,
    driver: &Path,
    workdir: &Path,
    in_path: &Path,
) -> Result<(), DynError> {
    let status = Command::new(py)
        .arg(driver)
        .arg("--read")
        .arg(in_path)
        .current_dir(workdir)
        .status()?;
    if !status.success() {
        return Err("Python driver --read failed".into());
    }
    Ok(())
}

fn run_rust_dump(exe: &Path, out_path: &Path) -> Result<(), DynError> {
    let status = Command::new(exe).arg("--dump").arg(out_path).status()?;
    if !status.success() {
        return Err("Rust driver --dump failed".into());
    }
    Ok(())
}

fn run_rust_read(exe: &Path, in_path: &Path) -> Result<(), DynError> {
    let status = Command::new(exe).arg("--read").arg(in_path).status()?;
    if !status.success() {
        return Err("Rust driver --read failed".into());
    }
    Ok(())
}

#[test]
fn cross_language_roundtrip_and_compatibility() -> Result<(), DynError> {
    if !has_prog("cmake", "--version") {
        eprintln!("skipping: cmake not found");
        return Ok(());
    }
    let Some(py) = python_cmd() else {
        eprintln!("skipping: python not found");
        return Ok(());
    };

    let world = load_world()?;

    let unique = format!(
        "run_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let workdir = repo_root()
        .join("target")
        .join("itests")
        .join("crosslang")
        .join(unique);

    let bindings = regenerate_bindings(&world, &workdir)?;

    let cpp_exe = cmake_configure_build(&bindings.cpp_src_dir, &bindings.cpp_build_dir)?;
    let rust_exe = cargo_build_rust_driver(&bindings.rust_dir)?;

    let dumps_dir = workdir.join("dumps");
    fs::create_dir_all(&dumps_dir)?;
    let cpp_dump = dumps_dir.join("from_cpp.bin");
    let py_dump = dumps_dir.join("from_py.bin");
    let rs_dump = dumps_dir.join("from_rs.bin");

    // 1) Dump from C++; validate via Python and Rust
    run_cpp_dump(&cpp_exe, &cpp_dump)?;
    run_python_read(py, &bindings.python_driver, &bindings.python_dir, &cpp_dump)?;
    run_rust_read(&rust_exe, &cpp_dump)?;

    // 2) Dump from Python; validate via C++ and Rust
    run_python_dump(py, &bindings.python_driver, &bindings.python_dir, &py_dump)?;
    run_cpp_read(&cpp_exe, &py_dump)?;
    run_rust_read(&rust_exe, &py_dump)?;

    // 3) Dump from Rust; validate via Python and C++
    run_rust_dump(&rust_exe, &rs_dump)?;
    run_python_read(py, &bindings.python_driver, &bindings.python_dir, &rs_dump)?;
    run_cpp_read(&cpp_exe, &rs_dump)?;

    Ok(())
}
