//! Builds the pinned whisper.cpp and the bridge that calls it.

use std::env;
use std::path::PathBuf;
use std::process::Command;

const PIN: &str = "48f628a84833905ee4a0658ee6d4a5c915ce1997";

fn enabled(flag: bool) -> &'static str {
    if flag { "ON" } else { "OFF" }
}

fn main() {
    println!("cargo:rerun-if-changed=bridge");
    println!("cargo:rerun-if-env-changed=WHISPER_CPP_DIR");
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let source = env::var_os("WHISPER_CPP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../vendor/whisper.cpp"));
    let revision = Command::new("git")
        .arg("-C")
        .arg(&source)
        .args(["rev-parse", "HEAD"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default();
    assert_eq!(
        revision, PIN,
        "expected whisper.cpp v1.8.7 at {source:?}; run core/scripts/fetch-whisper.sh"
    );

    let system = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let mac = system == "macos";
    let vulkan = env::var_os("CARGO_FEATURE_VULKAN").is_some();
    let mut build = cmake::Config::new(&source);
    build
        .profile("Release")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("WHISPER_BUILD_TESTS", "OFF")
        .define("WHISPER_BUILD_EXAMPLES", "OFF")
        .define("WHISPER_BUILD_SERVER", "OFF")
        .define("GGML_OPENMP", "OFF")
        .define("GGML_METAL", enabled(mac))
        .define("GGML_METAL_EMBED_LIBRARY", enabled(mac))
        .define("GGML_VULKAN", enabled(vulkan));
    if mac {
        build.define("CMAKE_OSX_DEPLOYMENT_TARGET", "14.0");
    }
    let out = build.build();

    cc::Build::new()
        .file("bridge/bridge.c")
        .include("bridge")
        .include(source.join("include"))
        .include(source.join("ggml/include"))
        .compile("soundveil_bridge");

    for folder in ["lib", "lib64"] {
        println!(
            "cargo:rustc-link-search=native={}",
            out.join(folder).display()
        );
    }
    let mut libraries = vec!["whisper", "ggml"];
    if mac {
        libraries.extend(["ggml-metal", "ggml-blas"]);
    }
    if vulkan {
        libraries.push("ggml-vulkan");
    }
    libraries.extend(["ggml-cpu", "ggml-base"]);
    for library in libraries {
        println!("cargo:rustc-link-lib=static={library}");
    }
    match system.as_str() {
        "macos" => {
            println!("cargo:rustc-link-lib=c++");
            for framework in ["Accelerate", "Metal", "MetalKit", "Foundation"] {
                println!("cargo:rustc-link-lib=framework={framework}");
            }
        }
        "linux" | "android" => println!("cargo:rustc-link-lib=stdc++"),
        // ggml reads the processor's name from the registry.
        "windows" => println!("cargo:rustc-link-lib=advapi32"),
        _ => {}
    }
    if vulkan {
        let name = if system == "windows" {
            "vulkan-1"
        } else {
            "vulkan"
        };
        if let Some(sdk) = env::var_os("VULKAN_SDK") {
            println!(
                "cargo:rustc-link-search=native={}",
                PathBuf::from(sdk).join("Lib").display()
            );
        }
        println!("cargo:rustc-link-lib={name}");
    }
}
