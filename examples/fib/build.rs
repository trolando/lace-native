fn main() {
    lace_ws_build::process("tasks.def")
        // Uses LACE_DIR env var by default; or uncomment:
        // .lace_dir("/path/to/lace")
        .compile();
}
