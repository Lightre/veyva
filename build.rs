use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/veyra.ico");
    println!("cargo:rerun-if-env-changed=WindowsSdkDir");
    println!("cargo:rerun-if-env-changed=VEYRA_RC");
    if !env::var("TARGET").unwrap().contains("windows-msvc") {
        return;
    }
    let rc = env::var_os("VEYRA_RC")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let root = env::var_os("WindowsSdkDir")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(
                        env::var_os("ProgramFiles(x86)")
                            .expect("Windows SDK directory unavailable"),
                    )
                    .join("Windows Kits/10")
                });
            let bin = root.join("bin");
            let mut candidates: Vec<_> = fs::read_dir(&bin)
                .expect("Install the Windows SDK resource compiler")
                .filter_map(Result::ok)
                .map(|e| e.path().join("x64/rc.exe"))
                .filter(|p| p.is_file())
                .collect();
            candidates.sort();
            candidates
                .pop()
                .or_else(|| {
                    let p = bin.join("x64/rc.exe");
                    p.is_file().then_some(p)
                })
                .expect("Windows SDK rc.exe not found; set VEYRA_RC to its full path")
        });
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("assets/veyra.ico");
    let resource = out.join("veyra.rc");
    let binary = out.join("veyra.res");
    fs::write(
        &resource,
        format!(
            "1 ICON \"{}\"\n",
            icon.display().to_string().replace('\\', "\\\\")
        ),
    )
    .unwrap();
    let status = Command::new(rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&binary)
        .arg(&resource)
        .status()
        .expect("Cannot run Windows resource compiler");
    assert!(status.success(), "Icon resource compilation failed");
    println!("cargo:rustc-link-arg={}", binary.display());
}
