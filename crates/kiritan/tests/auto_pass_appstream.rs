#![feature(exit_status_error)]

use std::collections::BTreeSet;
use std::io::Write;
mod common;

#[test]
fn auto_pass_appstream() {
    let workspace = env!("CARGO_WORKSPACE_DIR");
    let dir = tempfile::tempdir().unwrap();
    common::run_once(&dir, |cmd| _ = cmd.arg("--appstream"), |_| {});

    // The file name is content-addressed, so look it up by suffix instead of hardcoding it.
    let appstream = std::fs::read_dir(dir.path().join("repodata"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.to_string_lossy().ends_with("-appstream.xml.zst"))
        .expect("kiritan did not generate appstream metadata");

    let xml = zstd::decode_all(std::fs::File::open(appstream).unwrap()).unwrap();
    let mut xml_file = tempfile::Builder::new().suffix(".xml").tempfile().unwrap();
    xml_file.write_all(&xml).unwrap();

    let yaml_path = dir.path().join("appstream.yaml");
    let mut appstreamcli = std::process::Command::new("appstreamcli");
    appstreamcli.arg("convert").arg(xml_file.path()).arg(&yaml_path);
    let output = appstreamcli.output().expect("failed to run appstreamcli");
    assert!(
        output.status.success(),
        "appstreamcli failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let yaml = std::fs::read_to_string(yaml_path).unwrap();
    let packages: BTreeSet<_> =
        yaml.lines().filter_map(|line| line.strip_prefix("Package: ")).collect();

    assert_eq!(packages, BTreeSet::from(["moonshot", "sbctl"]));
}
