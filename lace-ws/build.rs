use std::env;
use std::fs;
use std::path::PathBuf;

fn has_feature(name: &str) -> bool {
    env::var(format!("CARGO_FEATURE_{}", name.to_uppercase())).is_ok()
}

fn target_os() -> String {
    env::var("CARGO_CFG_TARGET_OS").unwrap_or_default()
}

fn target_env() -> String {
    env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default()
}

fn configure_c_compiler(build: &mut cc::Build) {
    build.std("c11");

    if target_env() == "msvc" {
        // Keep this in sync with Lace's CMake MSVC flags.
        // /Zc:__STDC__ makes MSVC define __STDC__ for C11/C17 code.
        // /Zc:preprocessor enables the standard-conforming preprocessor.
        // /experimental:c11atomics enables <stdatomic.h> in /std:c11 mode.
        build.flag("/Zc:__STDC__");
        build.flag("/Zc:preprocessor");
        build.flag("/experimental:c11atomics");
    }
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Use LACE_DIR if set (for development), otherwise use vendored sources
    let lace_src = match env::var("LACE_DIR") {
        Ok(dir) => {
            let dir = PathBuf::from(dir);
            if dir.join("src").join("lace.h").exists() {
                dir.join("src")
            } else {
                dir
            }
        }
        Err(_) => manifest_dir.join("vendor"),
    };

    println!("cargo:rerun-if-changed={}", lace_src.display());
    println!("cargo:rerun-if-changed={}", manifest_dir.join("csrc").display());

    // Read feature flags
    let backoff = if has_feature("backoff") { "1" } else { "0" };
    let hwloc = if has_feature("hwloc") { "1" } else { "0" };
    let stats_tasks = if has_feature("stats") { "1" } else { "0" };
    let stats_steals = if has_feature("stats") { "1" } else { "0" };
    let stats_splits = if has_feature("stats") { "1" } else { "0" };
    let has_sched = if target_os() == "linux" { "1" } else { "0" };

    // Generate lace_config.h — the single source of truth for Lace configuration.
    // lace-ws-build reads this via DEP_LACE_CONFIG_INCLUDE so that the generated
    // task wrappers use the exact same settings as the runtime.
    let config = format!(
        "#define LACE_PIE_TIMES 0\n\
         #define LACE_COUNT_TASKS {stats_tasks}\n\
         #define LACE_COUNT_STEALS {stats_steals}\n\
         #define LACE_COUNT_SPLITS {stats_splits}\n\
         #define LACE_BACKOFF {backoff}\n\
         #define LACE_USE_HWLOC {hwloc}\n\
         #define HAVE_SCHED_GETAFFINITY {has_sched}\n"
    );
    fs::write(out_dir.join("lace_config.h"), &config)
        .expect("Failed to write lace_config.h");

    // Export include paths for lace-ws-build (via links = "lace" → DEP_LACE_*)
    println!("cargo:include={}", lace_src.display());
    println!("cargo:config_include={}", out_dir.display());

    // Compile Lace
    let mut build = cc::Build::new();
    build
        .file(lace_src.join("lace.c"))
        .file(manifest_dir.join("csrc").join("lace_helpers.c"))
        .include(&lace_src)
        .include(&out_dir);

    configure_c_compiler(&mut build);

    if target_os() == "linux" {
        build.define("_GNU_SOURCE", None);
    }

    build.compile("lace");

    // Link hwloc if enabled
    if has_feature("hwloc") {
        println!("cargo:rustc-link-lib=hwloc");
    }
}
