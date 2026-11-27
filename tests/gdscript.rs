use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn godot() -> Option<OsString> {
    let version = |path: &OsString| {
        Command::new(path)
            .arg("--version")
            .output()
            .ok()
            .is_some_and(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).starts_with("4.7.")
            })
    };
    if let Some(path) = std::env::var_os("GODOT_BIN") {
        assert!(
            version(&path),
            "GODOT_BIN must point to a Godot 4.7 executable"
        );
        return Some(path);
    }
    [
        "godot",
        "godot4",
        "/Applications/Godot.app/Contents/MacOS/Godot",
    ]
    .into_iter()
    .map(OsString::from)
    .find(version)
}

fn generate(source: &Path, output: &Path, extra: &[&str]) {
    let result = Command::new(env!("CARGO_BIN_EXE_jaw"))
        .args(["--kind", "gdscript"])
        .args(extra)
        .arg(output)
        .arg(source)
        .output()
        .expect("run jaw");
    assert!(
        result.status.success(),
        "jaw failed: {}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn godot_47_wire_compatibility() -> anyhow::Result<()> {
    let Some(godot) = godot() else {
        eprintln!("skipping GDScript runtime checks: Godot 4.7 is unavailable");
        return Ok(());
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let project: PathBuf = repo
        .join("target/itests/gdscript")
        .join(format!("run_{}_{unique}", std::process::id()));
    fs::create_dir_all(&project)?;
    fs::write(
        project.join("project.godot"),
        "[application]\nconfig/name=\"JAW GDScript tests\"\n",
    )?;
    fs::write(
        project.join("golden.hex"),
        include_str!("data/example.golden.hex"),
    )?;
    fs::write(project.join("test.gd"), include_str!("data/gdscript.gd"))?;
    let edges = project.join("edges.jaw");
    fs::write(&edges, include_str!("data/gdscript.jaw"))?;
    generate(
        &repo.join("assets/example.jaw"),
        &project.join("example.gd"),
        &[],
    );
    generate(&repo.join("assets/any.jaw"), &project.join("any.gd"), &[]);
    generate(
        &edges,
        &project.join("edges.gd"),
        &["--guard-array-size", "4"],
    );

    let output = Command::new(godot)
        .arg("--headless")
        .arg("--path")
        .arg(&project)
        .arg("--log-file")
        .arg(project.join("godot.log"))
        .args(["--script", "res://test.gd", "--quit-after", "5"])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout
                .lines()
                .any(|line| line == "JAW GDScript checks passed")
            && !stderr.contains("SCRIPT ERROR"),
        "Godot checks failed:\n{stdout}\n{stderr}"
    );
    Ok(())
}
