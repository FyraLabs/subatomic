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
