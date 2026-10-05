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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    fn run(pkgname: &str, xml: &str, filesize: Option<usize>) -> quick_xml::Result<Vec<u8>> {
        let mut out = Vec::new();
        transform(pkgname, BufReader::new(xml.as_bytes()), filesize, &mut out)?;
        Ok(out)
    }

    fn run_str(pkgname: &str, xml: &str) -> String {
        let out = run(pkgname, xml, None).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn transform_single_component_injects_pkgname() {
        let xml = r#"<component type="desktop"><id>foo.desktop</id></component>"#;
        let out = run_str("mypkg", xml);
        assert!(out.contains("<pkgname>mypkg</pkgname>"), "out={out}");
        // pkgname should be immediately after <component ...>
        assert!(out.contains("<component type=\"desktop\"><pkgname>mypkg</pkgname>"));
        // original content preserved after injection
        assert!(out.contains("<id>foo.desktop</id>"));
    }

    #[test]
    fn transform_multiple_components_each_gets_pkgname() {
        let xml = r#"<components><component><id>a</id></component><component><id>b</id></component></components>"#;
        let out = run_str("pkg", xml);
        assert_eq!(out.matches("<pkgname>pkg</pkgname>").count(), 2, "out={out}");
    }

    #[test]
    fn transform_no_component_no_injection() {
        let xml = r#"<root><foo>bar</foo></root>"#;
        let out = run_str("pkg", xml);
        assert!(!out.contains("pkgname"), "out={out}");
        assert!(out.contains("<foo>bar</foo>"));
    }

    #[test]
    fn transform_strips_decl_pi_doctype() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE foo><component><id>x</id></component><?pi target?><root/>"#;
        let out = run_str("pkg", xml);
        assert!(!out.contains("<?xml"), "decl not stripped: {out}");
        assert!(!out.contains("<!DOCTYPE"), "doctype not stripped: {out}");
        assert!(!out.contains("<?pi"), "pi not stripped: {out}");
        assert!(out.contains("<pkgname>pkg</pkgname>"));
    }

    #[test]
    fn transform_invalid_xml_returns_error() {
        // mismatched end tag triggers IllFormed error in quick-xml
        let xml = r#"<a><b></a>"#;
        let res = run("pkg", xml, None);
        assert!(res.is_err(), "expected error for invalid xml");

        // unclosed tag syntax error
        let res2 = run("pkg", "<", None);
        assert!(res2.is_err());

        // unclosed entity reference
        let res3 = run("pkg", "&", None);
        assert!(res3.is_err());
    }

    #[test]
    fn transform_with_filesize_hint_same_output() {
        let xml = r#"<component><id>x</id></component>"#;
        let out_none = run("pkg", xml, None).unwrap();
        let out_sized = run("pkg", xml, Some(xml.len())).unwrap();
        let out_large = run("pkg", xml, Some(1_000_000)).unwrap();
        assert_eq!(out_none, out_sized);
        assert_eq!(out_none, out_large);
    }

    #[test]
    fn transform_preserves_attributes_and_nested() {
        let xml = r#"<component type="consoleapp" version="1.0"><name>Foo</name><summary>Bar</summary></component>"#;
        let out = run_str("mypkg", xml);
        assert!(out.contains(r#"type="consoleapp""#));
        assert!(out.contains("<name>Foo</name>"));
        assert!(out.contains("<summary>Bar</summary>"));
    }

    #[test]
    fn transform_empty_input_produces_empty() {
        let out = run_str("pkg", "");
        assert_eq!(out, "");
    }

    #[test]
    fn transform_escapes_pkgname_special_chars() {
        // BytesText should escape xml special chars in text content
        let out = run_str("pkg&<special>", r#"<component><id>x</id></component>"#);
        assert!(out.contains("<pkgname>pkg&amp;&lt;special&gt;</pkgname>"), "out={out}");
    }
}
