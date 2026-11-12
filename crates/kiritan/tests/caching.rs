#![feature(exit_status_error)]

use std::io::Write;
mod common;

#[test]
fn run_twice_repomd_same() {
    let dir = tempfile::tempdir().unwrap();
    common::run_once(&dir, |_| {}, |_| {});
    let repomd = std::fs::read(dir.path().join("repodata/repomd.xml")).unwrap();
    common::run_once(&dir, |_| {}, |_| {});
    let repomd2 = std::fs::read(dir.path().join("repodata/repomd.xml")).unwrap();
    assert_eq!(repomd, repomd2, "repomd.xml should not change when ran twice");
}

#[test]
fn rm_stale_cache() {
    let dir = tempfile::tempdir().unwrap();
    common::run_once(&dir, |_| {}, |_| {});
    let mut f = std::fs::File::create(dir.path().join("cache/repo/data.mdb")).unwrap();
    f.write_all(b"0").unwrap();
    drop(f);
    common::run_once(&dir, |_| {}, |cmd| _ = cmd.arg("--no-cache"));
}
