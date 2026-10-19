//! Builds the Live2D avatar bridge (avatar/avatar.cpp) when Live2D's Cubism SDK
//! for Native is available: set CATOS_CUBISM_SDK to the unpacked
//! CubismSdkForNative-* folder. Without it the menu builds as before, with no avatar.
//!
//! The SDK can't go in this repo (Cubism Core is proprietary), so build.sh
//! passes it in from cache/.

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo::rustc-check-cfg=cfg(avatar)");
    println!("cargo::rerun-if-env-changed=CATOS_CUBISM_SDK");
    println!("cargo::rerun-if-changed=avatar");
    let Some(sdk) = std::env::var_os("CATOS_CUBISM_SDK").map(PathBuf::from) else {
        return;
    };
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() != "linux" {
        println!("cargo::warning=the Live2D avatar is only built for Linux; building without it");
        return;
    }
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "x86_64" => "x86_64",
        "aarch64" => "arm64",
        a => panic!("no Cubism Core for {a}"),
    };
    // Linux ARM64 is in the SDK's experimental builds (fine for the VMs).
    let core_lib = [sdk.join("Core/lib/linux").join(arch), sdk.join("Core/lib/experimental/linux").join(arch)]
        .into_iter()
        .find(|p| p.is_dir())
        .unwrap_or_else(|| sdk.join("Core/lib/linux").join(arch));
    let framework = sdk.join("Framework/src");
    for p in [&core_lib, &framework] {
        assert!(p.is_dir(), "{} not found: is CATOS_CUBISM_SDK the unpacked SDK folder?", p.display());
    }

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .warnings(false)
        // Desktop GL; avatar/include/GL/glew.h stands in for GLEW, and glcompat.cpp
        // makes the renderer work on a core-profile context.
        .define("CSM_TARGET_LINUX_GL", None)
        .include("avatar/include")
        .include(&framework)
        .include(sdk.join("Core/include"))
        .file("avatar/avatar.cpp")
        .file("avatar/glcompat.cpp");
    add_sources(&mut build, &framework);
    build.compile("catos_avatar");

    // After the bridge, so the linker can resolve the framework's calls into Core.
    println!("cargo::rustc-link-search=native={}", core_lib.display());
    println!("cargo::rustc-link-lib=static=Live2DCubismCore");
    println!("cargo::rustc-link-lib=dylib=OpenGL");
    println!("cargo::rustc-cfg=avatar");
}

/// Every .cpp in the framework, except the renderers for other graphics APIs.
fn add_sources(build: &mut cc::Build, dir: &Path) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if path.is_dir() {
            if !matches!(name.as_ref(), "D3D9" | "D3D11" | "Metal" | "Vulkan") {
                add_sources(build, &path);
            }
        } else if name.ends_with(".cpp") {
            build.file(&path);
        }
    }
}
