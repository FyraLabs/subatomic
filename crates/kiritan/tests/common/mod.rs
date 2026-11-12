#![feature(exit_status_error)]
pub fn run_once(
    dir: &tempfile::TempDir,
    pre: impl FnOnce(&mut assert_cmd::Command),
    post: impl FnOnce(&mut assert_cmd::Command),
) {
    // SAFETY: safe when single-threaded.
    unsafe {
        std::env::set_var("RUST_LOG", "trace");
    }
    let workspace = env!("CARGO_WORKSPACE_DIR");
    let mut cmd = assert_cmd::Command::cargo_bin(assert_cmd::pkg_name!()).unwrap();
    pre(&mut cmd);
    cmd.arg("-c").arg(dir.path().join("cache"));
    cmd.args(["--compact", "-o"]).arg(dir.path());
    cmd.arg(format!("{workspace}/random-rpm-examples")).arg("auto");
    post(&mut cmd);
    cmd.unwrap().exit_ok().unwrap();
}
