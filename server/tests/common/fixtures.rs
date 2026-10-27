//! Small files of each type, built in code. Only the headers that type
//! detection and dimension reading look at are realistic; `salt` is appended
//! so tests can make distinct files (distinct Images) of the same type.

/// A PNG of the given size.
pub fn png(width: u32, height: u32, salt: &str) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend_from_slice(&13u32.to_be_bytes());
    v.extend_from_slice(b"IHDR");
    v.extend_from_slice(&width.to_be_bytes());
    v.extend_from_slice(&height.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0]);
    v.extend_from_slice(&[0, 0, 0, 0]); // CRC (not checked)
    v.extend_from_slice(&0u32.to_be_bytes());
    v.extend_from_slice(b"IEND\xaeB`\x82");
    v.extend_from_slice(salt.as_bytes());
    v
}

/// A baseline JPEG header of the given size.
pub fn jpeg(width: u16, height: u16, salt: &str) -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8];
    // APP0 / JFIF
    v.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
    v.extend_from_slice(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
    // SOF0
    v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
    v.extend_from_slice(&height.to_be_bytes());
    v.extend_from_slice(&width.to_be_bytes());
    v.extend_from_slice(&[0x03, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    v.extend_from_slice(&[0xFF, 0xD9]);
    v.extend_from_slice(salt.as_bytes());
    v
}

pub fn gif(width: u16, height: u16, salt: &str) -> Vec<u8> {
    let mut v = b"GIF89a".to_vec();
    v.extend_from_slice(&width.to_le_bytes());
    v.extend_from_slice(&height.to_le_bytes());
    v.extend_from_slice(&[0, 0, 0, 0x3B]);
    v.extend_from_slice(salt.as_bytes());
    v
}

/// An extended-format (VP8X) WebP header.
pub fn webp(width: u32, height: u32, salt: &str) -> Vec<u8> {
    let mut chunk = b"VP8X".to_vec();
    chunk.extend_from_slice(&10u32.to_le_bytes());
    chunk.extend_from_slice(&[0, 0, 0, 0]);
    chunk.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
    chunk.extend_from_slice(&(height - 1).to_le_bytes()[..3]);
    chunk.extend_from_slice(salt.as_bytes());
    let mut v = b"RIFF".to_vec();
    v.extend_from_slice(&((chunk.len() + 4) as u32).to_le_bytes());
    v.extend_from_slice(b"WEBP");
    v.extend_from_slice(&chunk);
    v
}

/// An ISO-BMFF file with the given `ftyp` brands (AVIF, HEIC, ...).
pub fn bmff(major: &[u8; 4], compatible: &[&[u8; 4]], salt: &str) -> Vec<u8> {
    let size = 16 + 4 * compatible.len();
    let mut v = (size as u32).to_be_bytes().to_vec();
    v.extend_from_slice(b"ftyp");
    v.extend_from_slice(major);
    v.extend_from_slice(&[0, 0, 0, 0]);
    for brand in compatible {
        v.extend_from_slice(*brand);
    }
    v.extend_from_slice(b"\0\0\0\x08meta");
    v.extend_from_slice(salt.as_bytes());
    v
}

pub fn avif(salt: &str) -> Vec<u8> {
    bmff(b"avif", &[b"mif1", b"miaf"], salt)
}

pub fn heic(salt: &str) -> Vec<u8> {
    bmff(b"heic", &[b"mif1", b"heic"], salt)
}

pub fn svg(salt: &str) -> Vec<u8> {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- {salt} -->\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"30\">\
         <script>alert(document.cookie)</script><rect width=\"40\" height=\"30\"/></svg>\n"
    )
    .into_bytes()
}
