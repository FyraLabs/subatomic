use crate::error::ApiError;

/// Validate repo name: used for `{name}` and `{md}` path params
pub fn repo_name(name: &str) -> Result<(), ApiError> {
    if !(1..=64).contains(&name.len()) {
        return Err(ApiError::BadRequest("repo name length must be 1..64".into()));
    }
    if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.') {
        return Err(ApiError::BadRequest(
            "repo name may only contain alphanumeric, '-', '_' and '.'".into(),
        ));
    }
    if name.starts_with('.')
        || name.contains("..")
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
    {
        return Err(ApiError::BadRequest("repo name contains illegal sequence".into()));
    }
    Ok(())
}

/// Validate rpm filename: no path separators, must end with .rpm and parseable
pub fn rpm_filename(name: &str) -> Result<(), ApiError> {
    if name.is_empty() || name.len() > 255 {
        return Err(ApiError::BadRequest("filename empty or too long".into()));
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') || name.contains("..") {
        return Err(ApiError::BadRequest("filename contains illegal path".into()));
    }
    if !name.ends_with(".rpm") {
        return Err(ApiError::BadRequest("filename must end with .rpm".into()));
    }
    // also ensure parseable as rpm filename
    if libsubatomic::pkg::parse_filename(name.as_bytes()).is_none() {
        return Err(ApiError::BadRequest("invalid rpm filename format".into()));
    }
    Ok(())
}

/// Validate generic md filename (for custom datatypes): no traversal, reasonable length
pub fn md_filename(name: &str) -> Result<(), ApiError> {
    if name.is_empty() || name.len() > 255 {
        return Err(ApiError::BadRequest("md filename empty or too long".into()));
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') || name.contains("..") {
        return Err(ApiError::BadRequest("md filename contains illegal path".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_name_ok() {
        for good in ["my-repo", "rpmfission", "a.b_c-1", "A1"] {
            assert!(repo_name(good).is_ok(), "{good}");
        }
    }
    #[test]
    fn repo_name_rejects_traversal() {
        for bad in ["..", "../etc", "a/b", "/etc", "a\\b", ".hidden", "a..b", ""] {
            assert!(repo_name(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn rpm_filename_ok() {
        assert!(rpm_filename("bash-5.2.15-1.fc39.x86_64.rpm").is_ok());
        assert!(rpm_filename("terra-release-44-4.noarch.rpm").is_ok());
    }
    #[test]
    fn rpm_filename_rejects_traversal() {
        for bad in
            ["../../escape-1-1.x86_64.rpm", "/tmp/pwn.rpm", "a/b.rpm", "noext", "a..b.rpm", ""]
        {
            assert!(rpm_filename(bad).is_err(), "{bad}");
        }
    }
}
