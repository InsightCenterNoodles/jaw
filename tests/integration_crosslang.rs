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

fn has_jsc(prog: &Path) -> bool {
    Command::new(prog)
        .args(["-e", "if (typeof BigInt !== 'function') throw new Error()"])
        .output()
        .map(|output| output.status.success())
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

#[derive(Clone)]
enum JavaScriptEngine {
    JavaScriptCore(PathBuf),
    Node,
}

fn javascript_engine() -> Option<JavaScriptEngine> {
    let system_jsc =
        PathBuf::from("/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc");
    if has_prog("node", "--version") {
        Some(JavaScriptEngine::Node)
    } else if has_jsc(Path::new("jsc")) {
        Some(JavaScriptEngine::JavaScriptCore(PathBuf::from("jsc")))
    } else if has_jsc(&system_jsc) {
        Some(JavaScriptEngine::JavaScriptCore(system_jsc))
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
    javascript_dir: PathBuf,
    python_dir: PathBuf,
    python_driver: PathBuf,
    cpp_src_dir: PathBuf,
    cpp_build_dir: PathBuf,
    rust_dir: PathBuf,
    swift_dir: PathBuf,
}

fn regenerate_bindings(world: &compile::World, out_root: &Path) -> Result<Bindings, DynError> {
    fs::create_dir_all(out_root)?;

    // JavaScript
    let javascript_dir = out_root.join("javascript");
    fs::create_dir_all(&javascript_dir)?;
    codegen::emit_javascript(
        world,
        &Default::default(),
        javascript_dir.join("example.mjs"),
    )?;
    copy_file(
        repo_root().join("assets/generated/javascript/driver.mjs"),
        javascript_dir.join("driver.mjs"),
    )?;

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
    copy_file(
        repo_root().join("assets/generated/rust/src/any.rs"),
        rust_src.join("any.rs"),
    )?;
    codegen::emit_rust(world, &Default::default(), rust_src.join("example.rs"))?;

    // Swift
    let swift_dir = out_root.join("swift");
    fs::create_dir_all(&swift_dir)?;
    codegen::emit_swift(world, &Default::default(), swift_dir.join("example.swift"))?;
    copy_file(
        repo_root().join("assets/generated/swift/main.swift"),
        swift_dir.join("main.swift"),
    )?;

    Ok(Bindings {
        javascript_dir,
        python_dir: python_dir.clone(),
        python_driver: python_dir.join("driver.py"),
        cpp_src_dir,
        cpp_build_dir,
        rust_dir,
        swift_dir,
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

fn build_swift_driver(source_dir: &Path) -> Result<PathBuf, DynError> {
    let executable = source_dir.join("jaw_swift");
    let status = Command::new("swiftc")
        .arg("-swift-version")
        .arg("6")
        .arg("-module-cache-path")
        .arg(source_dir.join("module-cache"))
        .arg(source_dir.join("example.swift"))
        .arg(source_dir.join("main.swift"))
        .arg("-o")
        .arg(&executable)
        .status()?;
    if !status.success() {
        return Err("swiftc build failed".into());
    }
    Ok(executable)
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

fn run_swift_roundtrip(
    exe: &Path,
    in_path: &Path,
    out_path: Option<&Path>,
) -> Result<(), DynError> {
    let mut command = Command::new(exe);
    command.arg(in_path);
    if let Some(out_path) = out_path {
        command.arg(out_path);
    }
    if !command.status()?.success() {
        return Err("Swift driver round-trip failed".into());
    }
    Ok(())
}

fn run_javascript_roundtrip(
    engine: &JavaScriptEngine,
    source_dir: &Path,
    in_path: &Path,
    out_path: Option<&Path>,
) -> Result<(), DynError> {
    let mut command = match engine {
        JavaScriptEngine::JavaScriptCore(path) => {
            let mut command = Command::new(path);
            command
                .arg("-m")
                .arg(source_dir.join("driver.mjs"))
                .arg("--");
            command
        }
        JavaScriptEngine::Node => {
            let mut command = Command::new("node");
            command.arg(source_dir.join("driver.mjs"));
            command
        }
    };
    command.arg(in_path);
    if out_path.is_some() {
        command.arg("--emit");
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "JavaScript driver round-trip failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    if let Some(out_path) = out_path {
        let hex = String::from_utf8(output.stdout)?;
        let hex = hex.trim();
        let (pairs, remainder) = hex.as_bytes().as_chunks::<2>();
        if !remainder.is_empty() {
            return Err("JavaScript driver emitted malformed hexadecimal output".into());
        }
        let bytes = pairs
            .iter()
            .map(|pair| {
                let pair = std::str::from_utf8(pair)?;
                Ok(u8::from_str_radix(pair, 16)?)
            })
            .collect::<Result<Vec<_>, DynError>>()?;
        fs::write(out_path, bytes)?;
    }
    Ok(())
}

fn golden_bytes() -> Result<Vec<u8>, DynError> {
    let text = include_str!("data/example.golden.hex");
    let digits: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if !digits.len().is_multiple_of(2) {
        return Err("golden hex fixture has an odd number of digits".into());
    }

    let (pairs, remainder) = digits.as_bytes().as_chunks::<2>();
    debug_assert!(remainder.is_empty());

    pairs
        .iter()
        .map(|pair| {
            let pair = std::str::from_utf8(pair)?;
            Ok(u8::from_str_radix(pair, 16)?)
        })
        .collect()
}

fn assert_golden(path: &Path, expected: &[u8]) -> Result<(), DynError> {
    let actual = fs::read(path)?;
    if actual != expected {
        return Err(format!(
            "{} does not match the canonical wire fixture ({} actual bytes, {} expected bytes)",
            path.display(),
            actual.len(),
            expected.len()
        )
        .into());
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
    let swift_exe = if cfg!(target_os = "macos") && has_prog("swiftc", "--version") {
        Some(build_swift_driver(&bindings.swift_dir)?)
    } else {
        eprintln!(
            "skipping Swift compatibility checks: swiftc is unavailable or host is not macOS"
        );
        None
    };
    let javascript_engine = javascript_engine();
    if javascript_engine.is_none() {
        eprintln!("skipping JavaScript compatibility checks: no JavaScript engine is available");
    }

    let dumps_dir = workdir.join("dumps");
    fs::create_dir_all(&dumps_dir)?;
    let cpp_dump = dumps_dir.join("from_cpp.bin");
    let py_dump = dumps_dir.join("from_py.bin");
    let rs_dump = dumps_dir.join("from_rs.bin");
    let swift_dump = dumps_dir.join("from_swift.bin");
    let javascript_dump = dumps_dir.join("from_javascript.bin");
    let golden = golden_bytes()?;

    // 1) Dump from C++; validate via Python and Rust
    run_cpp_dump(&cpp_exe, &cpp_dump)?;
    assert_golden(&cpp_dump, &golden)?;
    run_python_read(py, &bindings.python_driver, &bindings.python_dir, &cpp_dump)?;
    run_rust_read(&rust_exe, &cpp_dump)?;
    if let Some(exe) = &swift_exe {
        run_swift_roundtrip(exe, &cpp_dump, Some(&swift_dump))?;
        assert_golden(&swift_dump, &golden)?;
    }
    if let Some(engine) = &javascript_engine {
        run_javascript_roundtrip(
            engine,
            &bindings.javascript_dir,
            &cpp_dump,
            Some(&javascript_dump),
        )?;
        assert_golden(&javascript_dump, &golden)?;
    }

    // 2) Dump from Python; validate via C++ and Rust
    run_python_dump(py, &bindings.python_driver, &bindings.python_dir, &py_dump)?;
    assert_golden(&py_dump, &golden)?;
    run_cpp_read(&cpp_exe, &py_dump)?;
    run_rust_read(&rust_exe, &py_dump)?;
    if let Some(exe) = &swift_exe {
        run_swift_roundtrip(exe, &py_dump, None)?;
    }
    if let Some(engine) = &javascript_engine {
        run_javascript_roundtrip(engine, &bindings.javascript_dir, &py_dump, None)?;
    }

    // 3) Dump from Rust; validate via Python and C++
    run_rust_dump(&rust_exe, &rs_dump)?;
    assert_golden(&rs_dump, &golden)?;
    run_python_read(py, &bindings.python_driver, &bindings.python_dir, &rs_dump)?;
    run_cpp_read(&cpp_exe, &rs_dump)?;
    if let Some(exe) = &swift_exe {
        run_swift_roundtrip(exe, &rs_dump, None)?;

        // 4) Validate Swift's re-encoded stream with every existing backend.
        run_python_read(
            py,
            &bindings.python_driver,
            &bindings.python_dir,
            &swift_dump,
        )?;
        run_cpp_read(&cpp_exe, &swift_dump)?;
        run_rust_read(&rust_exe, &swift_dump)?;
    }
    if let Some(engine) = &javascript_engine {
        run_javascript_roundtrip(engine, &bindings.javascript_dir, &rs_dump, None)?;
        run_python_read(
            py,
            &bindings.python_driver,
            &bindings.python_dir,
            &javascript_dump,
        )?;
        run_cpp_read(&cpp_exe, &javascript_dump)?;
        run_rust_read(&rust_exe, &javascript_dump)?;
        if let Some(exe) = &swift_exe {
            run_swift_roundtrip(exe, &javascript_dump, None)?;
        }
    }

    Ok(())
}
