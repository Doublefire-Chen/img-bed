//! Image types img-bed accepts, detected from a file's content (magic bytes),
//! never from its name or the client's `Content-Type`.
//!
//! Accepted: JPEG, PNG, GIF, WebP, AVIF and SVG. Everything else, including
//! HEIC and files that merely claim to be images, is rejected.

/// A detected, accepted image type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageType {
    Jpeg,
    Png,
    Gif,
    Webp,
    Avif,
    Svg,
}

impl ImageType {
    pub const ALL: [ImageType; 6] = [
        Self::Jpeg,
        Self::Png,
        Self::Gif,
        Self::Webp,
        Self::Avif,
        Self::Svg,
    ];

    /// Detects the type from the file's leading bytes.
    pub fn detect(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(Self::Jpeg)
        } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some(Self::Gif)
        } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            Some(Self::Webp)
        } else if is_avif(bytes) {
            Some(Self::Avif)
        } else if is_svg(bytes) {
            Some(Self::Svg)
        } else {
            None
        }
    }

    /// The MIME type served as `Content-Type`.
    pub fn mime(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
            Self::Avif => "image/avif",
            Self::Svg => "image/svg+xml",
        }
    }

    pub fn from_mime(mime: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.mime() == mime)
    }

    /// The file extension used by the Path Template's `{ext}`.
    pub fn ext(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Gif => "gif",
            Self::Webp => "webp",
            Self::Avif => "avif",
            Self::Svg => "svg",
        }
    }

    /// Width and height in pixels, when the file states them.
    pub fn dimensions(self, bytes: &[u8]) -> Option<(u32, u32)> {
        match self {
            Self::Svg => svg_dimensions(bytes),
            _ => {
                let size = imagesize::blob_size(bytes).ok()?;
                Some((
                    u32::try_from(size.width).ok()?,
                    u32::try_from(size.height).ok()?,
                ))
            }
        }
    }
}

/// AVIF is an ISO-BMFF file whose `ftyp` box names the `avif` (still) or
/// `avis` (sequence) brand. HEIC uses the same container with other brands
/// (`heic`, `heix`, `mif1` without `avif`, ...) and is rejected.
fn is_avif(bytes: &[u8]) -> bool {
    if bytes.len() < 16 || &bytes[4..8] != b"ftyp" {
        return false;
    }
    let size = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    if size < 16 || size > bytes.len() {
        return false;
    }
    let major = &bytes[8..12];
    // bytes[12..16] is the minor version; compatible brands follow.
    let compatible = bytes[16..size].chunks_exact(4);
    std::iter::once(major)
        .chain(compatible)
        .any(|brand| brand == b"avif" || brand == b"avis")
}

/// How much of a file is examined to find an SVG root element.
const SVG_SNIFF_LEN: usize = 4096;

/// An SVG is a text file whose first element is `<svg`, optionally preceded
/// by a byte-order mark, whitespace, an XML declaration, comments,
/// processing instructions and a doctype.
fn is_svg(bytes: &[u8]) -> bool {
    svg_root(bytes).is_some()
}

/// The text of the `<svg ...>` start tag (attributes included), if the file
/// is an SVG.
fn svg_root(bytes: &[u8]) -> Option<&str> {
    let head = &bytes[..bytes.len().min(SVG_SNIFF_LEN)];
    let head = match std::str::from_utf8(head) {
        Ok(s) => s,
        // The sniff window may cut a multi-byte character in half.
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&head[..e.valid_up_to()]).ok()?,
        Err(_) => return None,
    };
    let mut rest = head.strip_prefix('\u{feff}').unwrap_or(head);
    loop {
        rest = rest.trim_start_matches([' ', '\t', '\r', '\n']);
        if let Some(after) = rest.strip_prefix("<?") {
            rest = &after[after.find("?>")? + 2..];
        } else if let Some(after) = rest.strip_prefix("<!--") {
            rest = &after[after.find("-->")? + 3..];
        } else if let Some(after) = rest.strip_prefix("<!DOCTYPE") {
            rest = skip_doctype(after)?;
        } else {
            break;
        }
    }
    let after = rest.strip_prefix("<svg")?;
    match after.chars().next()? {
        ' ' | '\t' | '\r' | '\n' | '>' | '/' => {}
        _ => return None,
    }
    let end = after.find('>')?;
    Some(&after[..end])
}

/// Skips a doctype body (after `<!DOCTYPE`), including an internal subset in
/// square brackets.
fn skip_doctype(s: &str) -> Option<&str> {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.checked_sub(1)?,
            '>' if depth == 0 => return Some(&s[i + 1..]),
            _ => {}
        }
    }
    None
}

/// Reads `width`/`height` (unitless or `px`) from the root element, falling
/// back to the `viewBox` size. `None` when neither is usable (e.g. `100%`).
fn svg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let tag = svg_root(bytes)?;
    let width = attribute(tag, "width").and_then(svg_length);
    let height = attribute(tag, "height").and_then(svg_length);
    if let (Some(w), Some(h)) = (width, height) {
        return Some((w, h));
    }
    let view_box: Vec<f64> = attribute(tag, "viewBox")?
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    match view_box[..] {
        [_, _, w, h] => Some((to_pixels(w)?, to_pixels(h)?)),
        _ => None,
    }
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    while let Some(pos) = rest.find(name) {
        let before_ok = rest[..pos]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace);
        let after = rest[pos + name.len()..].trim_start();
        if before_ok && let Some(value) = after.strip_prefix('=') {
            let value = value.trim_start();
            let quote = value.chars().next()?;
            if quote == '"' || quote == '\'' {
                let value = &value[1..];
                return Some(&value[..value.find(quote)?]);
            }
        }
        rest = &rest[pos + name.len()..];
    }
    None
}

fn svg_length(value: &str) -> Option<u32> {
    let value = value.trim();
    let number = value.strip_suffix("px").unwrap_or(value);
    to_pixels(number.trim().parse().ok()?)
}

fn to_pixels(v: f64) -> Option<u32> {
    (v.is_finite() && v > 0.0 && v <= f64::from(u32::MAX)).then(|| v.round() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_raster_types_by_magic_bytes() {
        assert_eq!(
            ImageType::detect(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(ImageType::Jpeg)
        );
        assert_eq!(
            ImageType::detect(b"\x89PNG\r\n\x1a\n...."),
            Some(ImageType::Png)
        );
        assert_eq!(ImageType::detect(b"GIF89a.."), Some(ImageType::Gif));
        assert_eq!(
            ImageType::detect(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(ImageType::Webp)
        );
        assert_eq!(ImageType::detect(b""), None);
        assert_eq!(ImageType::detect(b"hello"), None);
    }

    fn ftyp(major: &[u8; 4], compatible: &[&[u8; 4]]) -> Vec<u8> {
        let size = 16 + 4 * compatible.len();
        let mut v = (size as u32).to_be_bytes().to_vec();
        v.extend_from_slice(b"ftyp");
        v.extend_from_slice(major);
        v.extend_from_slice(&[0, 0, 0, 0]);
        for b in compatible {
            v.extend_from_slice(*b);
        }
        v.extend_from_slice(b"\0\0\0\x08meta");
        v
    }

    #[test]
    fn avif_is_accepted_and_heic_is_not() {
        assert_eq!(
            ImageType::detect(&ftyp(b"avif", &[b"mif1", b"miaf"])),
            Some(ImageType::Avif)
        );
        assert_eq!(
            ImageType::detect(&ftyp(b"mif1", &[b"avif"])),
            Some(ImageType::Avif)
        );
        assert_eq!(ImageType::detect(&ftyp(b"heic", &[b"mif1", b"heic"])), None);
        assert_eq!(ImageType::detect(&ftyp(b"mif1", &[b"heic"])), None);
        assert_eq!(ImageType::detect(&ftyp(b"isom", &[b"mp41"])), None);
    }

    #[test]
    fn svg_needs_an_svg_root_element() {
        let ok: &[&str] = &[
            "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
            "\u{feff}  \n<svg>",
            "<?xml version=\"1.0\"?>\n<!-- made by hand -->\n<svg width=\"1\">",
            "<?xml version=\"1.0\"?><!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x.dtd\" [<!ENTITY a \"b\">]><svg\n>",
        ];
        for s in ok {
            assert_eq!(ImageType::detect(s.as_bytes()), Some(ImageType::Svg), "{s}");
        }
        let bad: &[&str] = &[
            "<html><svg></svg></html>",
            "<svgfoo>",
            "text <svg>",
            "<!-- unterminated <svg>",
            "<?xml version=\"1.0\"?><root/>",
        ];
        for s in bad {
            assert_eq!(ImageType::detect(s.as_bytes()), None, "{s}");
        }
    }

    #[test]
    fn svg_dimensions_from_attributes_or_view_box() {
        let dims = |s: &str| ImageType::Svg.dimensions(s.as_bytes());
        assert_eq!(dims(r#"<svg width="120" height="80px">"#), Some((120, 80)));
        assert_eq!(dims(r#"<svg viewBox="0 0 24 16">"#), Some((24, 16)));
        assert_eq!(
            dims(r#"<svg width="100%" height="100%" viewBox="0,0,10,5">"#),
            Some((10, 5))
        );
        assert_eq!(dims(r#"<svg stroke-width="3">"#), None);
        assert_eq!(dims(r#"<svg width="100%">"#), None);
    }

    #[test]
    fn extensions_and_mime_types() {
        assert_eq!(ImageType::Jpeg.ext(), "jpg");
        assert_eq!(ImageType::Svg.mime(), "image/svg+xml");
        for t in ImageType::ALL {
            assert_eq!(ImageType::from_mime(t.mime()), Some(t));
        }
    }
}
