use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=../../assets/lumina.ico");
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-env-changed=RC");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("lumina.res");
    let compiler = env::var_os("RC").unwrap_or_else(|| "rc.exe".into());
    let status = Command::new(compiler)
        .args(["/nologo", "/fo"]).arg(&output).arg("app.rc")
        .status().expect("Set RC to the Windows SDK rc.exe path or build from Developer PowerShell");
    assert!(status.success(), "Could not compile the Lumina icon resource");
    println!("cargo:rustc-link-arg-bin=lumina={}", output.display());
}