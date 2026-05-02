//! Build-time code generator for [lace-native](https://crates.io/crates/lace-native)
//! task definitions.
//!
//! This crate is used as a build dependency. It reads a `tasks.def` file
//! and generates:
//! - A C file with Lace `TASK()` macro invocations and non-inline FFI wrappers
//! - A Rust file with safe wrappers, borrow-checked guards, and `extern "C"` trampolines
//!
//! # Usage
//!
//! In your `build.rs`:
//! ```rust,ignore
//! fn main() {
//!     lace_native_build::process("tasks.def").compile();
//! }
//! ```
//!
//! The generated Rust bindings are included in your source via:
//! ```rust,ignore
//! include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));
//! ```
//!
//! # tasks.def format
//!
//! ```text
//! c {
//!     #include "lace.h"       // C headers, copied verbatim
//! }
//!
//! rust {
//!     use crate::MyType;      // Rust imports, copied verbatim
//! }
//!
//! types {
//!     BDD = uint64_t          // Custom value types: RustType = CType
//! }
//!
//! task fib(n: i32) -> i32                        // free function
//! task process(data: &MyData, len: usize) -> u64 // reference param
//!
//! impl MyTree {
//!     task search(&self, key: u64) -> bool       // shared method
//!     task insert(&mut self, key: u64, val: u64) // exclusive method
//! }
//! ```

use std::env;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};

// ═══════════════════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════════════════

/// Start processing a `tasks.def` file. Returns a [`Builder`] for configuration.
///
/// The path is relative to `CARGO_MANIFEST_DIR` (your crate root).
pub fn process(def_path: impl AsRef<Path>) -> Builder {
    Builder {
        def_path: def_path.as_ref().to_path_buf(),
        lace_dir: None,
        extra_c_includes: Vec::new(),
        extra_c_files: Vec::new(),
    }
}

/// Configuration builder for Lace code generation and C compilation.
///
/// Created by [`process()`]. Call [`compile()`](Builder::compile) to
/// run the code generator and compile the resulting C wrappers.
pub struct Builder {
    def_path: PathBuf,
    lace_dir: Option<PathBuf>,
    extra_c_includes: Vec<PathBuf>,
    extra_c_files: Vec<PathBuf>,
}

impl Builder {
    /// Override the Lace source directory (containing `lace.h`).
    ///
    /// If not set, headers are found automatically via the `lace-native`
    /// crate dependency (`DEP_LACE_INCLUDE`). You can also set the
    /// `LACE_DIR` environment variable as a fallback.
    pub fn lace_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.lace_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Add an extra C include directory for compilation.
    ///
    /// Use this when your task signatures reference types defined in
    /// headers outside the Lace source tree.
    pub fn include(mut self, dir: impl AsRef<Path>) -> Self {
        self.extra_c_includes.push(dir.as_ref().to_path_buf());
        self
    }

    /// Add an extra C source file to compile alongside the generated wrappers.
    pub fn c_file(mut self, path: impl AsRef<Path>) -> Self {
        self.extra_c_files.push(path.as_ref().to_path_buf());
        self
    }

    /// Parse `tasks.def`, generate C and Rust code, and compile the C wrappers.
    ///
    /// This is the main entry point. It:
    /// 1. Reads and parses the `tasks.def` file
    /// 2. Generates `lace_tasks.c` (C wrappers) and `lace_tasks.rs` (Rust bindings)
    /// 3. Compiles the C code using the `cc` crate
    ///
    /// The generated Rust file should be included in your source with:
    /// ```rust,ignore
    /// include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));
    /// ```
    pub fn compile(self) {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

        let def_path = if self.def_path.is_relative() {
            manifest_dir.join(&self.def_path)
        } else {
            self.def_path.clone()
        };

        println!("cargo:rerun-if-changed={}", def_path.display());

        let content = fs::read_to_string(&def_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", def_path.display(), e));
        let def = parse_def_file(&content);

        let c_code = generate_c(&def);
        let c_path = out_dir.join("lace_tasks.c");
        fs::write(&c_path, &c_code).expect("Failed to write lace_tasks.c");

        let rust_code = generate_rust(&def);
        let rust_path = out_dir.join("lace_tasks.rs");
        fs::write(&rust_path, &rust_code).expect("Failed to write lace_tasks.rs");

        // Find lace.h: DEP_LACE_INCLUDE (from lace-native) > LACE_DIR > .lace_dir()
        let lace_include = env::var("DEP_LACE_INCLUDE")
            .map(PathBuf::from)
            .ok()
            .or_else(|| {
                self.lace_dir.as_ref().map(|d| {
                    if d.join("src").join("lace.h").exists() {
                        d.join("src")
                    } else {
                        d.clone()
                    }
                })
            })
            .or_else(|| {
                env::var("LACE_DIR").ok().map(|d| {
                    let d = PathBuf::from(d);
                    if d.join("src").join("lace.h").exists() {
                        d.join("src")
                    } else {
                        d
                    }
                })
            })
            .expect(
                "Cannot find lace.h. Either:\n\
                 - depend on lace-native (recommended, provides headers automatically), or\n\
                 - set LACE_DIR, or\n\
                 - use .lace_dir() in your build.rs",
            );

        // Find lace_config.h: must come from lace-native via DEP_LACE_CONFIG_INCLUDE.
        // This ensures the wrappers use the exact same Lace configuration
        // (features, task size, etc.) as the runtime compiled by lace-native.
        let config_include = env::var("DEP_LACE_CONFIG_INCLUDE")
            .map(PathBuf::from)
            .expect(
                "DEP_LACE_CONFIG_INCLUDE not set. This means lace-native is not a \n\
                 dependency of your crate. Add lace-native to [dependencies] so that \n\
                 the Lace configuration (features, task layout) is consistent \n\
                 between the runtime and the generated task wrappers.",
            );

        let mut build = cc::Build::new();
        build
            .file(&c_path)
            .include(&lace_include)
            .include(&config_include);

        configure_c_compiler(&mut build);

        // _GNU_SOURCE needed on Linux for sched_getaffinity etc.
        if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
            build.define("_GNU_SOURCE", None);
        }

        for inc in &self.extra_c_includes {
            build.include(inc);
        }
        for src in &self.extra_c_files {
            build.file(src);
        }

        build.compile("lace_tasks");
    }
}

fn configure_c_compiler(build: &mut cc::Build) {
    build.std("c11");

    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        // Keep this in sync with Lace's CMake MSVC flags.
        // /Zc:__STDC__ makes MSVC define __STDC__ for C11/C17 code.
        // /Zc:preprocessor enables the standard-conforming preprocessor.
        // /experimental:c11atomics enables <stdatomic.h> in /std:c11 mode.
        build.flag("/Zc:__STDC__");
        build.flag("/Zc:preprocessor");
        build.flag("/experimental:c11atomics");
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Data model
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
struct Param {
    name: String,
    rust_type: String,
}

#[derive(Debug, Clone)]
enum SelfKind {
    Ref,
    MutRef,
}

#[derive(Debug, Clone)]
struct TaskDef {
    name: String,
    params: Vec<Param>,
    ret_type: Option<String>,
    impl_type: Option<String>,
    self_kind: Option<SelfKind>,
}

struct DefFile {
    c_preamble: String,
    rust_preamble: String,
    type_map: Vec<(String, String)>,
    tasks: Vec<TaskDef>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Parser
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug)]
enum State {
    TopLevel,
    InCBlock,
    InRustBlock,
    InTypesBlock,
    InImplBlock(String),
}

/// Report a parse error in tasks.def with location and context.
fn def_error(line_no: usize, line: &str, msg: &str) -> ! {
    panic!(
        "\n\
         error: tasks.def:{}: {}\n\
         |\n\
         |  {}\n\
         |\n",
        line_no + 1,
        msg,
        line
    )
}

fn parse_def_file(content: &str) -> DefFile {
    let mut state = State::TopLevel;
    let mut def = DefFile {
        c_preamble: String::new(),
        rust_preamble: String::new(),
        type_map: Vec::new(),
        tasks: Vec::new(),
    };
    for (ln, raw) in content.lines().enumerate() {
        let line = raw.trim();
        match &state {
            State::TopLevel | State::InTypesBlock | State::InImplBlock(_)
                if line.is_empty() || line.starts_with('#') =>
            {
                continue;
            }
            State::TopLevel => {
                if line == "c {" {
                    state = State::InCBlock;
                } else if line == "rust {" {
                    state = State::InRustBlock;
                } else if line == "types {" {
                    state = State::InTypesBlock;
                } else if line.starts_with("impl ") && line.ends_with('{') {
                    let tn = line
                        .strip_prefix("impl ")
                        .unwrap()
                        .strip_suffix('{')
                        .unwrap()
                        .trim()
                        .to_string();
                    state = State::InImplBlock(tn);
                } else if line.starts_with("task ") {
                    def.tasks.push(parse_task_line(line, None, ln));
                } else {
                    def_error(
                        ln,
                        line,
                        "unexpected line (expected 'task', 'impl', 'c {', 'rust {', or 'types {')",
                    );
                }
            }
            State::InCBlock => {
                if line == "}" {
                    state = State::TopLevel;
                } else {
                    def.c_preamble.push_str(raw);
                    def.c_preamble.push('\n');
                }
            }
            State::InRustBlock => {
                if line == "}" {
                    state = State::TopLevel;
                } else {
                    def.rust_preamble.push_str(raw);
                    def.rust_preamble.push('\n');
                }
            }
            State::InTypesBlock => {
                if line == "}" {
                    state = State::TopLevel;
                } else {
                    let p: Vec<&str> = line.splitn(2, '=').collect();
                    if p.len() != 2 {
                        def_error(
                            ln,
                            line,
                            "invalid type mapping (expected 'RustType = CType')",
                        );
                    }
                    def.type_map.push((p[0].trim().into(), p[1].trim().into()));
                }
            }
            State::InImplBlock(tn) => {
                if line == "}" {
                    state = State::TopLevel;
                } else if line.starts_with("task ") {
                    def.tasks.push(parse_task_line(line, Some(tn.clone()), ln));
                } else {
                    def_error(ln, line, "expected 'task' declaration inside impl block");
                }
            }
        }
    }

    if def.tasks.is_empty() {
        panic!(
            "\nerror: tasks.def contains no task declarations.\n\
             Add at least one task, e.g.:\n\n  task my_task(n: i32) -> i32\n"
        );
    }

    def
}

fn parse_task_line(line: &str, impl_type: Option<String>, ln: usize) -> TaskDef {
    let rest = line.strip_prefix("task ").unwrap().trim();
    let (sig, ret_type) = if let Some(i) = rest.find("->") {
        (rest[..i].trim(), Some(rest[i + 2..].trim().to_string()))
    } else {
        (rest, None)
    };
    let open = sig.find('(').unwrap_or_else(|| {
        def_error(
            ln,
            line,
            "missing '(' — expected: task name(param: Type, ...) -> RetType",
        )
    });
    let close = sig.rfind(')').unwrap_or_else(|| {
        def_error(
            ln,
            line,
            "missing ')' — expected: task name(param: Type, ...) -> RetType",
        )
    });
    let name = sig[..open].trim().to_string();
    let pstr = sig[open + 1..close].trim();
    let mut params = Vec::new();
    let mut self_kind = None;
    if !pstr.is_empty() {
        for ps in split_params(pstr) {
            let ps = ps.trim();
            if ps == "&self" {
                self_kind = Some(SelfKind::Ref);
            } else if ps == "&mut self" {
                self_kind = Some(SelfKind::MutRef);
            } else {
                let c = ps.find(':').unwrap_or_else(|| {
                    def_error(
                        ln,
                        line,
                        &format!("missing ':' in parameter '{}' — expected: name: Type", ps),
                    )
                });
                params.push(Param {
                    name: ps[..c].trim().into(),
                    rust_type: ps[c + 1..].trim().into(),
                });
            }
        }
    }
    if self_kind.is_some() && impl_type.is_none() {
        def_error(ln, line, "&self can only be used inside an impl block");
    }
    TaskDef {
        name,
        params,
        ret_type,
        impl_type,
        self_kind,
    }
}

fn split_params(s: &str) -> Vec<String> {
    let mut r = Vec::new();
    let mut cur = String::new();
    let mut d = 0i32;
    for ch in s.chars() {
        match ch {
            '<' | '(' => {
                d += 1;
                cur.push(ch);
            }
            '>' | ')' => {
                d -= 1;
                cur.push(ch);
            }
            ',' if d == 0 => {
                r.push(cur.clone());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        r.push(cur);
    }
    r
}

// ═══════════════════════════════════════════════════════════════════════════════
// Type mapping
// ═══════════════════════════════════════════════════════════════════════════════

fn rust_type_to_c(rt: &str, m: &[(String, String)]) -> String {
    for (a, b) in m {
        if rt == a {
            return b.clone();
        }
    }
    match rt {
        "i8"=>"int8_t","i16"=>"int16_t","i32"=>"int32_t","i64"=>"int64_t",
        "u8"=>"uint8_t","u16"=>"uint16_t","u32"=>"uint32_t","u64"=>"uint64_t",
        "usize"=>"size_t","isize"=>"ptrdiff_t","bool"=>"_Bool",
        "f32"=>"float","f64"=>"double",
        _ if rt.starts_with("*mut ") || rt.starts_with("*const ") => "void*",
        _ => panic!(
            "\nerror: unknown Rust type '{}' in tasks.def.\n\
             Add a mapping in the types {{ }} block, e.g.:\n\n  types {{\n      {} = <c_type>\n  }}\n\n\
             Built-in types: i8..i64, u8..u64, usize, isize, bool, f32, f64,\n\
             *const T, *mut T, &T, &mut T\n",
            rt, rt
        ),
    }.into()
}

fn is_ref(t: &str) -> bool {
    t.starts_with('&')
}
fn is_mut_ref(t: &str) -> bool {
    t.starts_with("&mut ")
}
fn ref_inner(t: &str) -> &str {
    if t.starts_with("&mut ") {
        t.strip_prefix("&mut ").unwrap().trim()
    } else {
        t.strip_prefix('&').unwrap().trim()
    }
}
fn to_pascal(s: &str) -> String {
    s.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().chain(c).collect(),
            }
        })
        .collect()
}
fn c_prefix(t: &TaskDef) -> String {
    match &t.impl_type {
        Some(tp) => format!("{}_{}", tp, t.name),
        None => t.name.clone(),
    }
}
fn has_lifetime(t: &TaskDef) -> bool {
    t.params.iter().any(|p| is_ref(&p.rust_type))
}

fn is_raw_ptr(t: &str) -> bool {
    t.starts_with("*const ") || t.starts_with("*mut ")
}

fn has_raw_ptr_param(task: &TaskDef) -> bool {
    task.params.iter().any(|p| is_raw_ptr(&p.rust_type))
}

fn allow_raw_ptr_lint_if_needed(o: &mut String, task: &TaskDef, indent: &str) {
    if has_raw_ptr_param(task) {
        writeln!(o, "{}#[allow(clippy::not_unsafe_ptr_arg_deref)]", indent).unwrap();
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// C code generation — matches actual Lace API from lace.h
// ═══════════════════════════════════════════════════════════════════════════════

fn generate_c(def: &DefFile) -> String {
    let mut o = String::new();
    writeln!(o, "// AUTO-GENERATED by lace-native-build — do not edit\n").unwrap();
    writeln!(
        o,
        "#include <stdint.h>\n#include <stddef.h>\n#include <stdbool.h>\n"
    )
    .unwrap();
    if !def.c_preamble.is_empty() {
        o.push_str(&def.c_preamble);
        writeln!(o).unwrap();
    }

    for task in &def.tasks {
        let pf = c_prefix(task);
        let cr = task
            .ret_type
            .as_ref()
            .map(|t| rust_type_to_c(t, &def.type_map))
            .unwrap_or("void".into());
        let has_ret = task.ret_type.is_some();

        // TASK macro args
        let mut targs = vec![cr.clone(), pf.clone()];
        if task.self_kind.is_some() {
            targs.push("void*".into());
            targs.push("self_".into());
        }
        for p in &task.params {
            let ct = if is_ref(&p.rust_type) {
                "void*".into()
            } else {
                rust_type_to_c(&p.rust_type, &def.type_map)
            };
            targs.push(ct);
            targs.push(p.name.clone());
        }

        // Worker-param lists (for SPAWN, SYNC, DROP)
        let mut pw = vec!["lace_worker *w".to_string()];
        // All-param lists without worker (for NAME, NEWFRAME, TOGETHER)
        let mut pnw: Vec<String> = Vec::new();
        let mut args: Vec<String> = Vec::new();

        if task.self_kind.is_some() {
            pw.push("void *self_".into());
            pnw.push("void *self_".into());
            args.push("self_".into());
        }
        for p in &task.params {
            let ct = if is_ref(&p.rust_type) {
                "void*".into()
            } else {
                rust_type_to_c(&p.rust_type, &def.type_map)
            };
            pw.push(format!("{} {}", ct, p.name));
            pnw.push(format!("{} {}", ct, p.name));
            args.push(p.name.clone());
        }

        let s_pw = pw.join(", ");
        let s_pnw = if pnw.is_empty() {
            "void".to_string()
        } else {
            pnw.join(", ")
        };
        let s_args = args.join(", ");
        let s_wargs = if args.is_empty() {
            "w".into()
        } else {
            format!("w, {}", s_args)
        };
        let ret = if has_ret { "return " } else { "" };

        writeln!(o, "// ── {} ──\n", pf).unwrap();
        writeln!(o, "extern {} {}_CALL({});\n", cr, pf, s_pw).unwrap();

        let np = task.params.len() + if task.self_kind.is_some() { 1 } else { 0 };
        if np == 0 {
            writeln!(o, "TASK({}, {});\n", cr, pf).unwrap();
        } else {
            writeln!(o, "TASK({});\n", targs.join(", ")).unwrap();
        }

        // SPAWN, SYNC, DROP — take lace_worker*
        writeln!(o, "void {pf}_SPAWN_w({s_pw}) {{ {pf}_SPAWN({s_wargs}); }}").unwrap();
        if has_ret {
            writeln!(
                o,
                "{cr} {pf}_SYNC_w(lace_worker *w) {{ return {pf}_SYNC(w); }}"
            )
            .unwrap();
        } else {
            writeln!(o, "void {pf}_SYNC_w(lace_worker *w) {{ {pf}_SYNC(w); }}").unwrap();
        }
        writeln!(o, "void {pf}_DROP_w(lace_worker *w) {{ {pf}_DROP(w); }}").unwrap();

        // NAME (auto-dispatch), NEWFRAME, TOGETHER — NO worker param
        writeln!(o, "{cr} {pf}_RUN_w({s_pnw}) {{ {ret}{pf}({s_args}); }}").unwrap();
        if has_ret {
            writeln!(
                o,
                "{cr} {pf}_NEWFRAME_w({s_pnw}) {{ return {pf}_NEWFRAME({s_args}); }}"
            )
            .unwrap();
        } else {
            writeln!(
                o,
                "void {pf}_NEWFRAME_w({s_pnw}) {{ {pf}_NEWFRAME({s_args}); }}"
            )
            .unwrap();
        }
        writeln!(
            o,
            "void {pf}_TOGETHER_w({s_pnw}) {{ {pf}_TOGETHER({s_args}); }}"
        )
        .unwrap();
        writeln!(o).unwrap();
    }
    o
}

// ═══════════════════════════════════════════════════════════════════════════════
// Rust code generation
// ═══════════════════════════════════════════════════════════════════════════════

fn generate_rust(def: &DefFile) -> String {
    let mut o = String::new();
    writeln!(o, "// AUTO-GENERATED by lace-native-build — do not edit\n").unwrap();
    writeln!(o, "#[allow(unused_imports)]").unwrap();
    writeln!(o, "use std::marker::PhantomData;").unwrap();
    writeln!(o, "#[allow(unused_imports)]").unwrap();
    writeln!(o, "use std::ffi::c_void;").unwrap();
    writeln!(o, "#[allow(unused_imports)]").unwrap();
    writeln!(o, "use lace_native::{{Worker, LaceWorker}};").unwrap();
    writeln!(o).unwrap();

    if !def.rust_preamble.is_empty() {
        o.push_str(&def.rust_preamble);
        writeln!(o).unwrap();
    }

    // Free function tasks
    for task in def.tasks.iter().filter(|t| t.impl_type.is_none()) {
        gen_rust_free(&mut o, task);
    }

    // Method tasks grouped by impl type
    let mut groups: std::collections::BTreeMap<String, Vec<&TaskDef>> =
        std::collections::BTreeMap::new();
    for task in &def.tasks {
        if let Some(ref it) = task.impl_type {
            groups.entry(it.clone()).or_default().push(task);
        }
    }
    for (tn, tasks) in &groups {
        writeln!(o, "// ── Methods on {} ──\n", tn).unwrap();
        writeln!(o, "impl {} {{", tn).unwrap();
        for task in tasks {
            gen_rust_method_in_impl(&mut o, task);
        }
        writeln!(o, "}}\n").unwrap();
        for task in tasks {
            gen_rust_method_outside(&mut o, task);
        }
    }
    o
}

// ── FFI extern block ──

fn gen_ffi(o: &mut String, task: &TaskDef) {
    let pf = c_prefix(task);
    let rr = task.ret_type.as_deref().unwrap_or("()");
    let has_ret = task.ret_type.is_some();

    // Worker-taking params (SPAWN, SYNC, DROP)
    let mut fw = vec!["w: *mut LaceWorker".to_string()];
    // Non-worker params (RUN, NEWFRAME, TOGETHER)
    let mut fnw: Vec<String> = Vec::new();

    if task.self_kind.is_some() {
        fw.push("self_: *mut c_void".into());
        fnw.push("self_: *mut c_void".into());
    }
    for p in &task.params {
        let ft = if is_ref(&p.rust_type) {
            "*mut c_void".into()
        } else {
            p.rust_type.clone()
        };
        fw.push(format!("{}: {}", p.name, ft));
        fnw.push(format!("{}: {}", p.name, ft));
    }

    let sw = fw.join(", ");
    let snw = fnw.join(", ");
    let ret = if has_ret {
        format!(" -> {}", rr)
    } else {
        String::new()
    };

    writeln!(o, "extern \"C\" {{").unwrap();
    writeln!(o, "    fn {}_SPAWN_w({});", pf, sw).unwrap();
    writeln!(o, "    fn {}_SYNC_w(w: *mut LaceWorker){};", pf, ret).unwrap();
    writeln!(o, "    fn {}_DROP_w(w: *mut LaceWorker);", pf).unwrap();
    writeln!(o, "    fn {}_RUN_w({}){};", pf, snw, ret).unwrap();
    writeln!(o, "    fn {}_NEWFRAME_w({}){};", pf, snw, ret).unwrap();
    writeln!(o, "    fn {}_TOGETHER_w({});", pf, snw).unwrap();
    writeln!(o, "}}\n").unwrap();
}

// ── Trampoline ──

fn gen_trampoline(o: &mut String, task: &TaskDef) {
    let pf = c_prefix(task);
    let rr = task.ret_type.as_deref().unwrap_or("()");
    let has_ret = task.ret_type.is_some();
    let is_method = task.self_kind.is_some();
    let is_mut_s = matches!(task.self_kind, Some(SelfKind::MutRef));

    let mut params = vec!["w: *mut LaceWorker".to_string()];
    if is_method {
        let it = task.impl_type.as_deref().unwrap();
        params.push(if is_mut_s {
            format!("self_: *mut {}", it)
        } else {
            format!("self_: *const {}", it)
        });
    }
    for p in &task.params {
        if is_mut_ref(&p.rust_type) {
            params.push(format!("{}: *mut {}", p.name, ref_inner(&p.rust_type)));
        } else if is_ref(&p.rust_type) {
            params.push(format!("{}: *const {}", p.name, ref_inner(&p.rust_type)));
        } else {
            params.push(format!("{}: {}", p.name, p.rust_type));
        }
    }

    let ret_arr = if has_ret {
        format!(" -> {}", rr)
    } else {
        String::new()
    };

    writeln!(o, "#[allow(non_snake_case)]").unwrap();
    writeln!(o, "#[no_mangle]").unwrap();
    writeln!(
        o,
        "extern \"C\" fn {}_CALL({}){} {{",
        pf,
        params.join(", "),
        ret_arr
    )
    .unwrap();
    writeln!(o, "    let w = unsafe {{ Worker::from_raw(w) }};").unwrap();

    if is_method {
        if is_mut_s {
            writeln!(o, "    let this = unsafe {{ &mut *self_ }};").unwrap();
        } else {
            writeln!(o, "    let this = unsafe {{ &*self_ }};").unwrap();
        }
    }

    let mut call_args = vec!["&w".to_string()];
    for p in &task.params {
        if is_mut_ref(&p.rust_type) {
            writeln!(o, "    let {} = unsafe {{ &mut *{} }};", p.name, p.name).unwrap();
        } else if is_ref(&p.rust_type) {
            writeln!(o, "    let {} = unsafe {{ &*{} }};", p.name, p.name).unwrap();
        }
        call_args.push(p.name.clone());
    }

    if is_method {
        writeln!(o, "    this.{}({})", task.name, call_args.join(", ")).unwrap();
    } else {
        writeln!(o, "    crate::{}({})", task.name, call_args.join(", ")).unwrap();
    }
    writeln!(o, "}}\n").unwrap();
}

// ── Guard type ──

fn gen_guard(o: &mut String, task: &TaskDef, gn: &str, is_method: bool) {
    let has_lt = has_lifetime(task) || is_method;
    let is_mut_s = matches!(task.self_kind, Some(SelfKind::MutRef));
    let rr = task.ret_type.as_deref().unwrap_or("()");
    let has_ret = task.ret_type.is_some();
    let pf = c_prefix(task);

    writeln!(o, "#[allow(dead_code)]").unwrap();
    if has_lt {
        writeln!(o, "pub struct {}<'__lace> {{", gn).unwrap();
    } else {
        writeln!(o, "pub struct {} {{", gn).unwrap();
    }

    if is_method {
        let it = task.impl_type.as_deref().unwrap();
        if is_mut_s {
            writeln!(o, "    _self: &'__lace mut {},", it).unwrap();
        } else {
            writeln!(o, "    _self: &'__lace {},", it).unwrap();
        }
    }
    let mut has_borrow_fields = false;
    for p in &task.params {
        if is_ref(&p.rust_type) {
            let inner = ref_inner(&p.rust_type);
            if is_mut_ref(&p.rust_type) {
                writeln!(o, "    _{}: &'__lace mut {},", p.name, inner).unwrap();
            } else {
                writeln!(o, "    _{}: &'__lace {},", p.name, inner).unwrap();
            }
            has_borrow_fields = true;
        }
    }
    if !is_method && !has_borrow_fields {
        if has_lt {
            writeln!(o, "    _p: PhantomData<&'__lace ()>,").unwrap();
        } else {
            writeln!(o, "    _p: (),").unwrap();
        }
    }
    writeln!(o, "}}\n").unwrap();

    // Impl sync/drop on guard
    if has_lt {
        writeln!(o, "impl<'__lace> {}<'__lace> {{", gn).unwrap();
    } else {
        writeln!(o, "impl {} {{", gn).unwrap();
    }
    if has_ret {
        writeln!(
            o,
            "    pub fn sync(self, w: &Worker) -> {} {{ unsafe {{ {}_SYNC_w(w.as_ptr()) }} }}",
            rr, pf
        )
        .unwrap();
    } else {
        writeln!(
            o,
            "    pub fn sync(self, w: &Worker) {{ unsafe {{ {}_SYNC_w(w.as_ptr()) }} }}",
            pf
        )
        .unwrap();
    }
    writeln!(
        o,
        "    pub fn drop(self, w: &Worker) {{ unsafe {{ {}_DROP_w(w.as_ptr()) }} }}",
        pf
    )
    .unwrap();
    writeln!(o, "}}\n").unwrap();
}

// ── Helpers for parameter formatting ──

fn safe_params(task: &TaskDef) -> String {
    task.params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.rust_type))
        .collect::<Vec<_>>()
        .join(", ")
}

fn ffi_cast_args(task: &TaskDef) -> String {
    task.params
        .iter()
        .map(|p| {
            if is_mut_ref(&p.rust_type) {
                format!("{} as *mut _ as *mut c_void", p.name)
            } else if is_ref(&p.rust_type) {
                format!("{} as *const _ as *mut c_void", p.name)
            } else {
                p.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn guard_fields(task: &TaskDef, is_method: bool) -> String {
    let mut f = Vec::new();
    if is_method {
        f.push("_self: self".into());
    }
    for p in &task.params {
        if is_ref(&p.rust_type) {
            f.push(format!("_{}: {}", p.name, p.name));
        }
    }
    if f.is_empty() {
        if has_lifetime(task) {
            "_p: PhantomData".into()
        } else {
            "_p: ()".into()
        }
    } else {
        f.join(", ")
    }
}

// ── Free function Rust generation ──

fn gen_rust_free(o: &mut String, task: &TaskDef) {
    let pf = c_prefix(task);
    let rr = task.ret_type.as_deref().unwrap_or("()");
    let has_ret = task.ret_type.is_some();
    let gn = format!("{}Guard", to_pascal(&pf));
    let has_lt = has_lifetime(task);
    let params = safe_params(task);
    let args = ffi_cast_args(task);
    let gf = guard_fields(task, false);

    writeln!(o, "// ── {} ──\n", pf).unwrap();

    gen_ffi(o, task);
    gen_trampoline(o, task);
    gen_guard(o, task, &gn, false);

    // spawn
    allow_raw_ptr_lint_if_needed(o, task, "");
    writeln!(o, "#[must_use]").unwrap();
    if has_lt {
        writeln!(
            o,
            "pub fn {}_spawn<'__lace>(w: &Worker, {}) -> {}<'__lace> {{",
            task.name, params, gn
        )
        .unwrap();
    } else {
        writeln!(
            o,
            "pub fn {}_spawn(w: &Worker, {}) -> {} {{",
            task.name, params, gn
        )
        .unwrap();
    }
    writeln!(o, "    unsafe {{ {}_SPAWN_w(w.as_ptr(), {}) }};", pf, args).unwrap();
    writeln!(o, "    {} {{ {} }}", gn, gf).unwrap();
    writeln!(o, "}}\n").unwrap();

    // sync (standalone, C-style)
    if has_ret {
        writeln!(
            o,
            "pub fn {}_sync(w: &Worker) -> {} {{ unsafe {{ {}_SYNC_w(w.as_ptr()) }} }}\n",
            task.name, rr, pf
        )
        .unwrap();
    } else {
        writeln!(
            o,
            "pub fn {}_sync(w: &Worker) {{ unsafe {{ {}_SYNC_w(w.as_ptr()) }} }}\n",
            task.name, pf
        )
        .unwrap();
    }

    // drop (standalone)
    writeln!(
        o,
        "pub fn {}_drop(w: &Worker) {{ unsafe {{ {}_DROP_w(w.as_ptr()) }} }}\n",
        task.name, pf
    )
    .unwrap();

    // run (auto-dispatch, no worker — works from any thread)
    allow_raw_ptr_lint_if_needed(o, task, "");
    if has_ret {
        writeln!(
            o,
            "pub fn {}_run({}) -> {} {{ unsafe {{ {}_RUN_w({}) }} }}\n",
            task.name, params, rr, pf, args
        )
        .unwrap();
    } else {
        writeln!(
            o,
            "pub fn {}_run({}) {{ unsafe {{ {}_RUN_w({}) }} }}\n",
            task.name, params, pf, args
        )
        .unwrap();
    }

    // newframe (no worker)
    allow_raw_ptr_lint_if_needed(o, task, "");
    if has_ret {
        writeln!(
            o,
            "pub fn {}_newframe({}) -> {} {{ unsafe {{ {}_NEWFRAME_w({}) }} }}\n",
            task.name, params, rr, pf, args
        )
        .unwrap();
    } else {
        writeln!(
            o,
            "pub fn {}_newframe({}) {{ unsafe {{ {}_NEWFRAME_w({}) }} }}\n",
            task.name, params, pf, args
        )
        .unwrap();
    }

    // together (no worker)
    allow_raw_ptr_lint_if_needed(o, task, "");
    writeln!(
        o,
        "pub fn {}_together({}) {{ unsafe {{ {}_TOGETHER_w({}) }} }}\n",
        task.name, params, pf, args
    )
    .unwrap();
}

// ── Method Rust generation (inside impl block) ──

fn gen_rust_method_in_impl(o: &mut String, task: &TaskDef) {
    let pf = c_prefix(task);
    let rr = task.ret_type.as_deref().unwrap_or("()");
    let has_ret = task.ret_type.is_some();
    let gn = format!("{}Guard", to_pascal(&pf));
    let is_mut_s = matches!(task.self_kind, Some(SelfKind::MutRef));
    let sp = if is_mut_s { "&mut self" } else { "&self" };
    let sc = if is_mut_s {
        "self as *mut _ as *mut c_void"
    } else {
        "self as *const _ as *mut c_void"
    };

    let ep = safe_params(task);
    let ea = ffi_cast_args(task);
    let gf = guard_fields(task, true);

    let all_a = if ea.is_empty() {
        sc.to_string()
    } else {
        format!("{}, {}", sc, ea)
    };

    // spawn
    allow_raw_ptr_lint_if_needed(o, task, "    ");
    writeln!(o, "    #[must_use]").unwrap();
    if ep.is_empty() {
        writeln!(
            o,
            "    pub fn {}_spawn({}, w: &Worker) -> {}<'_> {{",
            task.name, sp, gn
        )
        .unwrap();
    } else {
        writeln!(
            o,
            "    pub fn {}_spawn({}, w: &Worker, {}) -> {}<'_> {{",
            task.name, sp, ep, gn
        )
        .unwrap();
    }
    writeln!(
        o,
        "        unsafe {{ {}_SPAWN_w(w.as_ptr(), {}) }};",
        pf, all_a
    )
    .unwrap();
    writeln!(o, "        {} {{ {} }}", gn, gf).unwrap();
    writeln!(o, "    }}\n").unwrap();

    // sync (standalone)
    if has_ret {
        writeln!(
            o,
            "    pub fn {}_sync(w: &Worker) -> {} {{ unsafe {{ {}_SYNC_w(w.as_ptr()) }} }}\n",
            task.name, rr, pf
        )
        .unwrap();
    } else {
        writeln!(
            o,
            "    pub fn {}_sync(w: &Worker) {{ unsafe {{ {}_SYNC_w(w.as_ptr()) }} }}\n",
            task.name, pf
        )
        .unwrap();
    }

    // drop (standalone)
    writeln!(
        o,
        "    pub fn {}_drop(w: &Worker) {{ unsafe {{ {}_DROP_w(w.as_ptr()) }} }}\n",
        task.name, pf
    )
    .unwrap();

    // run (no worker)
    allow_raw_ptr_lint_if_needed(o, task, "    ");
    if ep.is_empty() {
        if has_ret {
            writeln!(
                o,
                "    pub fn {}_run({}) -> {} {{ unsafe {{ {}_RUN_w({}) }} }}\n",
                task.name, sp, rr, pf, sc
            )
            .unwrap();
        } else {
            writeln!(
                o,
                "    pub fn {}_run({}) {{ unsafe {{ {}_RUN_w({}) }} }}\n",
                task.name, sp, pf, sc
            )
            .unwrap();
        }
    } else {
        if has_ret {
            writeln!(
                o,
                "    pub fn {}_run({}, {}) -> {} {{ unsafe {{ {}_RUN_w({}) }} }}\n",
                task.name, sp, ep, rr, pf, all_a
            )
            .unwrap();
        } else {
            writeln!(
                o,
                "    pub fn {}_run({}, {}) {{ unsafe {{ {}_RUN_w({}) }} }}\n",
                task.name, sp, ep, pf, all_a
            )
            .unwrap();
        }
    }

    // newframe (no worker)
    allow_raw_ptr_lint_if_needed(o, task, "    ");
    if ep.is_empty() {
        if has_ret {
            writeln!(
                o,
                "    pub fn {}_newframe({}) -> {} {{ unsafe {{ {}_NEWFRAME_w({}) }} }}\n",
                task.name, sp, rr, pf, sc
            )
            .unwrap();
        } else {
            writeln!(
                o,
                "    pub fn {}_newframe({}) {{ unsafe {{ {}_NEWFRAME_w({}) }} }}\n",
                task.name, sp, pf, sc
            )
            .unwrap();
        }
    } else {
        if has_ret {
            writeln!(
                o,
                "    pub fn {}_newframe({}, {}) -> {} {{ unsafe {{ {}_NEWFRAME_w({}) }} }}\n",
                task.name, sp, ep, rr, pf, all_a
            )
            .unwrap();
        } else {
            writeln!(
                o,
                "    pub fn {}_newframe({}, {}) {{ unsafe {{ {}_NEWFRAME_w({}) }} }}\n",
                task.name, sp, ep, pf, all_a
            )
            .unwrap();
        }
    }

    // together (no worker)
    allow_raw_ptr_lint_if_needed(o, task, "    ");
    if ep.is_empty() {
        writeln!(
            o,
            "    pub fn {}_together({}) {{ unsafe {{ {}_TOGETHER_w({}) }} }}\n",
            task.name, sp, pf, sc
        )
        .unwrap();
    } else {
        writeln!(
            o,
            "    pub fn {}_together({}, {}) {{ unsafe {{ {}_TOGETHER_w({}) }} }}\n",
            task.name, sp, ep, pf, all_a
        )
        .unwrap();
    }
}

// ── Method extras (outside impl block) ──

fn gen_rust_method_outside(o: &mut String, task: &TaskDef) {
    let pf = c_prefix(task);
    let gn = format!("{}Guard", to_pascal(&pf));
    gen_ffi(o, task);
    gen_trampoline(o, task);
    gen_guard(o, task, &gn, true);
}
