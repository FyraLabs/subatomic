mod common;

#[test]
fn auto_pass_dnf() {
    let dir = tempfile::tempdir().unwrap();
    common::run_once(&dir, |_| {}, |_| {});
    let mut dnf = std::process::Command::new("dnf");
    dnf.args(["rq", "*", "--repo=random", "--repofrompath"])
        .arg(format!("random,{}", dir.path().to_str().unwrap()));
    let dnf = assert_cmd::Command::from_std(dnf).unwrap().exit_ok().unwrap();
    // count number of rpms
    assert_eq!(String::from_utf8(dnf.stdout).unwrap().lines().count(), 7);
}
