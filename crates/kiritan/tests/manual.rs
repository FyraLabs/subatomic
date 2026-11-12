use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
}

fn examples() -> PathBuf {
    workspace().join("random-rpm-examples")
}

fn kiritan(dir: &tempfile::TempDir) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin(assert_cmd::pkg_name!()).unwrap();
    cmd.arg("-c").arg(dir.path().join("cache")).arg("-vv");
    cmd
}

fn dnf_names(repo: &Path) -> Vec<String> {
    let mut dnf = std::process::Command::new("dnf");
    dnf.args(["--refresh", "rq", "*", "--repo=random", "--repofrompath"])
        .arg(format!("random,{}", repo.display()))
        .args(["--qf", "%{name}\\n"]);
    let output = dnf.output().unwrap();
    assert!(output.status.success(), "dnf failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().lines().map(ToOwned::to_owned).collect()
}

fn has_repodata_suffix(out: &Path, suffix: &str) -> bool {
    std::fs::read_dir(out.join("repodata"))
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().ends_with(suffix))
}

#[test]
fn invalid_input_fails() {
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = kiritan(&dir);
    cmd.arg(dir.path().join("missing")).arg("auto");
    cmd.assert().failure();
}

#[test]
fn manual_add_remove_with_comps() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let comps = dir.path().join("comps.xml");
    std::fs::write(&comps, "<comps/>").unwrap();
    let release = examples().join("terra-release-44-4.noarch.rpm");
    let extras = examples().join("terra-release-extras-44-4.noarch.rpm");
    let broken = dir.path().join("broken.rpm");
    std::fs::write(&broken, b"not an rpm").unwrap();

    let mut cmd = kiritan(&dir);
    cmd.arg("-o").arg(&out).arg("--zstd-multi=-1").arg("manual");
    cmd.arg("--add").arg(&release).arg("--add").arg(&extras);
    cmd.arg("--add").arg(&broken).arg("--comps").arg(&comps);
    cmd.assert().success();

    assert_eq!(dnf_names(&out), ["terra-release", "terra-release-extras"]);

    let repomd = std::fs::read_to_string(out.join("repodata/repomd.xml")).unwrap();
    assert!(repomd.contains(r#"type="group""#));

    let mut cmd = kiritan(&dir);
    cmd.arg("-o").arg(&out).arg("manual");
    cmd.arg("--add").arg(&release).arg("--add").arg(&extras);
    cmd.arg("--remove").arg("terra-release-extras-44-4.noarch.rpm");
    cmd.arg("--remove").arg("does-not-exist.rpm");
    cmd.assert().success();

    assert_eq!(dnf_names(&out), ["terra-release"]);
}

#[test]
fn md_add_and_delete_custom_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let rpm = examples().join("terra-release-44-4.noarch.rpm");
    let custom = dir.path().join("custom.xml");
    std::fs::write(&custom, "<custom/>").unwrap();

    let mut cmd = kiritan(&dir);
    cmd.arg("-o").arg(&out).arg("manual").arg("--add").arg(&rpm);
    cmd.assert().success();

    let mut cmd = kiritan(&dir);
    cmd.arg("-o").arg(&out).arg("md").arg("missing");
    cmd.assert().failure();

    let invalid = dir.path().join(std::ffi::OsString::from_vec(b"bad-\xff.xml".to_vec()));
    std::fs::write(&invalid, "<custom/>").unwrap();
    let mut cmd = kiritan(&dir);
    cmd.arg("-o").arg(&out).arg("md").arg("mykey").arg(&invalid);
    cmd.assert().failure();

    let mut cmd = kiritan(&dir);
    cmd.arg("-o").arg(&out).arg("md").arg("mykey").arg(&custom);
    cmd.assert().success();

    // `md` only updates the cache, so run again to update repomd
    // TODO: reconsider this behaviour
    let mut cmd = kiritan(&dir);
    cmd.arg("-o").arg(&out).arg("manual");
    cmd.assert().success();

    let repomd = std::fs::read_to_string(out.join("repodata/repomd.xml")).unwrap();
    assert!(repomd.contains(r#"type="mykey""#));
    assert!(has_repodata_suffix(&out, "-custom.xml.zst"));

    let mut cmd = kiritan(&dir);
    cmd.arg("-o").arg(&out).arg("md").arg("--delete").arg("mykey");
    cmd.assert().success();

    let mut cmd = kiritan(&dir);
    cmd.arg("-o").arg(&out).arg("manual");
    cmd.assert().success();

    let repomd = std::fs::read_to_string(out.join("repodata/repomd.xml")).unwrap();
    assert!(!repomd.contains(r#"type="mykey""#));
    assert!(!has_repodata_suffix(&out, "-custom.xml.zst"));
}

#[test]
fn auto_default_output_skips_invalid_rpms() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    std::fs::create_dir_all(&input).unwrap();
    std::fs::copy(
        examples().join("terra-release-extras-44-4.noarch.rpm"),
        input.join("terra-release-extras-44-4.noarch.rpm"),
    )
    .unwrap();
    std::fs::write(input.join("broken.rpm"), b"not an rpm").unwrap();
    let invalid_utf8 = std::ffi::OsString::from_vec(b"bad-\xff.rpm".to_vec());
    std::fs::write(input.join(invalid_utf8), b"not an rpm").unwrap();

    let mut cmd = kiritan(&dir);
    cmd.arg(&input).arg("auto");
    cmd.assert().success();

    assert!(input.join("repodata/repomd.xml").is_file());
    assert_eq!(dnf_names(&input), ["terra-release-extras"]);
}
