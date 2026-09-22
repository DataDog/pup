use std::process::Command;

#[path = "build/surface.rs"]
mod surface;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(generated_tag, values(any()))");
    println!("cargo::rustc-check-cfg=cfg(generated_op, values(any()))");

    let surface_path = "src/generated/surface.txt";
    println!("cargo::rerun-if-changed={surface_path}");
    println!("cargo::rerun-if-changed=src/generated");

    for (key, value) in surface::generated_cfgs(surface_path) {
        println!("cargo::rustc-cfg={key}=\"{value}\"");
    }

    // The unoptimized command dispatcher exceeds MSVC's default 1 MiB stack.
    // Reserve 8 MiB for the executable; Windows commits stack pages on demand.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bin=pup=/STACK:8388608");
    }

    let version = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.split_whitespace().nth(1).map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=RUSTC_VERSION={version}");
    println!("cargo:rerun-if-env-changed=RUSTC_VERSION");
}
