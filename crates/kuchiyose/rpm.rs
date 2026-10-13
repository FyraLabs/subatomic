#[non_exhaustive]
#[derive(Clone, Debug, Default)]
pub struct ParsePathOutput<'a> {
    pub name: &'a [u8],
    pub epoch: u64,
    pub ver: &'a [u8],
    pub rel: &'a [u8],
    pub arch: &'a [u8],
}

/// Attempt to obtain nevra from a filename.
#[must_use]
pub fn parse_filename(filename: &crate::kiri::Kiri) -> Option<ParsePathOutput<'_>> {
    let filename = filename.as_bytes();
    let (nevr, arch) = filename.strip_suffix(b".rpm")?.rsplit_once(|&b| b == b'.')?;
    let (nev, rel) = nevr.rsplit_once(|&b| b == b'-')?;
    let (name, ev) = nev.rsplit_once(|&b| b == b'-')?;
    let (epoch, ver) = ev
        .rsplit_once(|&b| b == b':')
        .and_then(|(ep, ver)| Some((atoi::atoi(ep)?, ver)))
        .unwrap_or((0, ev));
    Some(ParsePathOutput { name, epoch, ver, rel, arch })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_filename_basic() {
        let out = parse_filename(crate::Kiri::from_str("bash-5.2.15-1.fc39.x86_64.rpm").unwrap())
            .unwrap();
        assert_eq!(out.name, b"bash");
        assert_eq!(out.epoch, 0);
        assert_eq!(out.ver, b"5.2.15");
        assert_eq!(out.rel, b"1.fc39");
        assert_eq!(out.arch, b"x86_64");
    }

    #[test]
    fn parse_filename_with_epoch() {
        // epoch is embedded as "epoch:version" before the split
        let out = parse_filename(crate::Kiri::from_str("pkgname-2:1.0-3.el9.noarch.rpm").unwrap())
            .unwrap();
        assert_eq!(out.name, b"pkgname");
        assert_eq!(out.epoch, 2);
        assert_eq!(out.ver, b"1.0");
        assert_eq!(out.rel, b"3.el9");
        assert_eq!(out.arch, b"noarch");
    }

    #[test]
    fn parse_filename_dashes_in_name() {
        // rsplit_once('-') from the right means multi-dash names still work
        // as long as version/release themselves contain no dashes
        let out =
            parse_filename(crate::Kiri::from_str("python3-pip-23.0-1.fc39.noarch.rpm").unwrap())
                .unwrap();
        assert_eq!(out.name, b"python3-pip");
        assert_eq!(out.epoch, 0);
        assert_eq!(out.ver, b"23.0");
        assert_eq!(out.rel, b"1.fc39");
        assert_eq!(out.arch, b"noarch");
    }

    #[test]
    fn parse_filename_src_rpm() {
        let out =
            parse_filename(crate::Kiri::from_str("kernel-6.5.0-1.fc39.src.rpm").unwrap()).unwrap();
        assert_eq!(out.name, b"kernel");
        assert_eq!(out.epoch, 0);
        assert_eq!(out.ver, b"6.5.0");
        assert_eq!(out.rel, b"1.fc39");
        assert_eq!(out.arch, b"src");
    }

    #[test]
    fn parse_filename_missing_rpm_suffix_returns_none() {
        assert!(
            parse_filename(crate::Kiri::from_str("bash-5.2.15-1.fc39.x86_64").unwrap()).is_none()
        );
    }

    #[test]
    fn parse_filename_no_arch_returns_none() {
        assert!(parse_filename(crate::Kiri::from_str("bash.rpm").unwrap()).is_none());
    }

    #[test]
    fn parse_filename_too_few_dashes_returns_none() {
        assert!(
            parse_filename(crate::Kiri::from_str("bash-5.2.15.fc39.x86_64.rpm").unwrap()).is_none()
        );
    }

    #[test]
    fn parse_filename_epoch_non_numeric_treated_as_ver() {
        let out =
            parse_filename(crate::Kiri::from_str("pkg-abc:1.0-1.noarch.rpm").unwrap()).unwrap();
        assert_eq!(out.name, b"pkg");
        assert_eq!(out.epoch, 0);
        assert_eq!(out.ver, b"abc:1.0");
        assert_eq!(out.rel, b"1");
        assert_eq!(out.arch, b"noarch");
    }

    #[test]
    fn parse_filename_epoch_zero_explicit() {
        let out = parse_filename(crate::Kiri::from_str("pkg-0:1.0-1.noarch.rpm").unwrap()).unwrap();
        assert_eq!(out.epoch, 0);
        assert_eq!(out.ver, b"1.0");
        assert_eq!(out.rel, b"1");
    }

    #[test]
    fn parse_filename_same_name_arch_different_rel() {
        let a = parse_filename(crate::Kiri::from_str("terra-release-44-4.noarch.rpm").unwrap())
            .unwrap();
        let b = parse_filename(crate::Kiri::from_str("terra-release-44-5.noarch.rpm").unwrap())
            .unwrap();
        assert_eq!(a.name, b.name);
        assert_eq!(a.arch, b.arch);
        assert_eq!(a.ver, b.ver);
        assert_ne!(a.rel, b.rel);
        assert_eq!(a.name, b"terra-release");
    }
}
