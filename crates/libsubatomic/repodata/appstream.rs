//! Handle appstream xml serialization & deserialization.

use quick_xml::events::{BytesText, Event};

/// Transform package appstream xml to repodata appstream fragment.
///
/// Current implementation only adds the `<pkgname />` tag.
///
/// If `filesize` is given, allocate a buffer with that size. The caller is responsible for making
/// sure `filesize` is an acceptable size.
///
/// # Errors
/// Return errors when the given xml (`reader`) cannot be parsed.
///
/// # Panics
///
/// Panic when [`std::io::Error`] is raised by writing to `out`.
pub fn transform<R: std::io::BufRead>(
    pkgname: &str,
    reader: R,
    filesize: Option<usize>,
    out: &mut Vec<u8>,
) -> quick_xml::Result<()> {
    let mut reader = quick_xml::Reader::from_reader(reader);
    reader.config_mut().trim_text(true);
    let mut buf = filesize.map_or_else(Vec::new, Vec::with_capacity);
    let mut writer = quick_xml::Writer::new(out);
    let Err(e) = try {
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) if e.name().as_ref() == "component" => {
                    writer.write_event(Event::Start(e))?;
                    writer.create_element("pkgname").write_text_content(BytesText::new(pkgname))?;
                }
                Ok(Event::Eof) => return Ok(()),
                Ok(Event::Decl(_) | Event::PI(_) | Event::DocType(_)) => {}
                Ok(e) => writer.write_event(e)?,
                Err(e) => return Err(e),
            }
            buf.clear();
        }
    };
    panic!("unexpected io error during appstream::transform: {e}");
}
