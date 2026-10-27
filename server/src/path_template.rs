//! The Path Template engine: turns a template such as `{random:6}.{ext}`
//! plus the facts of an upload into a candidate Path.
//!
//! Pure: it touches neither the database nor the disk. Whether a candidate
//! collides with an existing Path or hits a Reserved Prefix is decided by the
//! caller, which simply renders again (see `store`).
//!
//! Variables:
//!
//! | Variable     | Meaning                                                       |
//! |--------------|---------------------------------------------------------------|
//! | `{random:N}` | N random base62 characters (`0-9A-Za-z`), 1 ≤ N ≤ 64          |
//! | `{hash:N}`   | the first N hex characters of the SHA-256, 1 ≤ N ≤ 64         |
//! | `{Y}`        | upload year, 4 digits (UTC)                                   |
//! | `{m}`        | upload month, 2 digits                                        |
//! | `{d}`        | upload day, 2 digits                                          |
//! | `{name}`     | the original filename without extension, cleaned ([`clean_name`]) |
//! | `{ext}`      | extension of the detected type (`jpg`, `png`, ...)            |
//!
//! A template must contain `{random:N}` or `{hash:N}`, so two uploads of
//! `image.png` cannot render the same Path.

use time::OffsetDateTime;

use crate::image_type::ImageType;
use crate::path;
use crate::random::RandomSource;

/// The characters of `{random:N}`.
pub const BASE62: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// The longest `{random:N}` / `{hash:N}`.
pub const MAX_LEN: usize = 64;

/// Longest `{name}` output, in characters.
pub const MAX_NAME_CHARS: usize = 64;

/// A ready-made Path Template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub template: &'static str,
}

pub const PRESETS: [Preset; 4] = [
    Preset {
        id: "short",
        name: "Short",
        template: "{random:6}",
    },
    Preset {
        id: "short_ext",
        name: "Short with extension",
        template: "{random:6}.{ext}",
    },
    Preset {
        id: "dated",
        name: "Dated",
        template: "{Y}/{m}/{d}/{random:13}.{ext}",
    },
    Preset {
        id: "filename",
        name: "Filename",
        template: "{name}-{random:4}.{ext}",
    },
];

/// The default Path Template: the "Short with extension" Preset.
pub const DEFAULT_TEMPLATE: &str = "{random:6}.{ext}";

/// What a template knows about an upload.
#[derive(Debug, Clone, Copy)]
pub struct UploadFacts<'a> {
    pub image_type: ImageType,
    pub original_filename: &'a str,
    /// Lower-case hex SHA-256 of the file.
    pub sha256: &'a str,
    pub uploaded_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TemplateError {
    #[error("the template is empty")]
    Empty,
    #[error("the template must contain {{random:N}} or {{hash:N}}")]
    NoUniquePart,
    #[error("unknown variable {{{0}}}")]
    UnknownVariable(String),
    #[error("{{{0}}} needs a length from 1 to 64, e.g. {{{0}:6}}")]
    BadLength(String),
    #[error("unmatched brace in the template")]
    UnmatchedBrace,
    #[error("the character {0:?} is not allowed in a template")]
    BadCharacter(char),
    #[error("the template must not start with '/', end with '/' or contain '//'")]
    BadSlash,
    #[error("the template must not contain '.' or '..' segments")]
    DotSegment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Literal(String),
    Random(usize),
    Hash(usize),
    Year,
    Month,
    Day,
    Name,
    Ext,
}

/// A parsed, valid Path Template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    source: String,
    parts: Vec<Part>,
}

impl Default for Template {
    fn default() -> Self {
        Self::parse(DEFAULT_TEMPLATE).expect("the default template is valid")
    }
}

impl Template {
    /// Parses and validates a template.
    pub fn parse(source: &str) -> Result<Self, TemplateError> {
        if source.is_empty() {
            return Err(TemplateError::Empty);
        }
        if path::has_bad_slash(source) {
            return Err(TemplateError::BadSlash);
        }
        if path::has_dot_segment(source) {
            return Err(TemplateError::DotSegment);
        }
        let mut parts = Vec::new();
        let mut literal = String::new();
        let mut chars = source.chars();
        while let Some(c) = chars.next() {
            match c {
                '{' => {
                    let mut var = String::new();
                    loop {
                        match chars.next() {
                            Some('}') => break,
                            Some('{') | None => return Err(TemplateError::UnmatchedBrace),
                            Some(c) => var.push(c),
                        }
                    }
                    if !literal.is_empty() {
                        parts.push(Part::Literal(std::mem::take(&mut literal)));
                    }
                    parts.push(parse_variable(&var)?);
                }
                '}' => return Err(TemplateError::UnmatchedBrace),
                // Literals are ASCII; only `{name}` brings in other scripts.
                c if c.is_ascii() && path::is_path_char(c) => literal.push(c),
                c => return Err(TemplateError::BadCharacter(c)),
            }
        }
        if !literal.is_empty() {
            parts.push(Part::Literal(literal));
        }
        if !parts
            .iter()
            .any(|p| matches!(p, Part::Random(_) | Part::Hash(_)))
        {
            return Err(TemplateError::NoUniquePart);
        }
        Ok(Self {
            source: source.to_owned(),
            parts,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.source
    }

    /// Renders one candidate Path (without a leading `/`). Each call draws
    /// fresh randomness for `{random:N}`.
    pub fn render(&self, facts: &UploadFacts<'_>, random: &dyn RandomSource) -> String {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Literal(s) => out.push_str(s),
                Part::Random(n) => out.push_str(&random_base62(*n, random)),
                Part::Hash(n) => out.push_str(&facts.sha256[..(*n).min(facts.sha256.len())]),
                Part::Year => out.push_str(&format!("{:04}", facts.uploaded_at.year())),
                Part::Month => out.push_str(&format!("{:02}", u8::from(facts.uploaded_at.month()))),
                Part::Day => out.push_str(&format!("{:02}", facts.uploaded_at.day())),
                Part::Name => out.push_str(&clean_name(facts.original_filename)),
                Part::Ext => out.push_str(facts.image_type.ext()),
            }
        }
        out
    }
}

fn parse_variable(var: &str) -> Result<Part, TemplateError> {
    let (name, arg) = match var.split_once(':') {
        Some((name, arg)) => (name, Some(arg)),
        None => (var, None),
    };
    let length = |name: &str| -> Result<usize, TemplateError> {
        arg.and_then(|a| a.parse::<usize>().ok())
            .filter(|n| (1..=MAX_LEN).contains(n))
            .ok_or_else(|| TemplateError::BadLength(name.to_owned()))
    };
    let part = match name {
        "random" => Part::Random(length(name)?),
        "hash" => Part::Hash(length(name)?),
        "Y" => Part::Year,
        "m" => Part::Month,
        "d" => Part::Day,
        "name" => Part::Name,
        "ext" => Part::Ext,
        _ => return Err(TemplateError::UnknownVariable(var.to_owned())),
    };
    if arg.is_some() && !matches!(part, Part::Random(_) | Part::Hash(_)) {
        return Err(TemplateError::UnknownVariable(var.to_owned()));
    }
    Ok(part)
}

/// `n` uniformly distributed base62 characters.
fn random_base62(n: usize, random: &dyn RandomSource) -> String {
    let mut out = String::with_capacity(n);
    let mut buf = [0u8; 1];
    while out.len() < n {
        random.fill_bytes(&mut buf);
        // 248 = 4 * 62; rejecting larger bytes keeps the distribution uniform.
        if buf[0] < 248 {
            out.push(BASE62[usize::from(buf[0] % 62)] as char);
        }
    }
    out
}

/// The `{name}` variable: the original filename without its extension, with
/// spaces and unsafe characters turned into `-`. Letters and digits of any
/// script are kept, as are `-` and `_`; runs of `-` collapse and leading or
/// trailing `-` are dropped. Falls back to `image` when nothing is left.
pub fn clean_name(original_filename: &str) -> String {
    // Browsers send a bare name, but some tools send a client-side path.
    let base = original_filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(original_filename);
    let stem = match base.rfind('.') {
        Some(i) if i > 0 => &base[..i],
        _ => base,
    };
    let mut out = String::new();
    for c in stem.chars() {
        let c = if c.is_alphanumeric() || c == '_' {
            c
        } else {
            '-'
        };
        if c == '-' && (out.is_empty() || out.ends_with('-')) {
            continue;
        }
        out.push(c);
    }
    let out: String = out.chars().take(MAX_NAME_CHARS).collect();
    let out = out.trim_end_matches('-');
    if out.is_empty() {
        "image".to_owned()
    } else {
        out.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashSet, VecDeque};
    use std::sync::Mutex;

    use super::*;
    use crate::random::SeededRandom;

    fn facts(image_type: ImageType, name: &str) -> UploadFacts<'static> {
        UploadFacts {
            image_type,
            original_filename: Box::leak(name.to_owned().into_boxed_str()),
            sha256: "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
            // 2026-03-07T23:59:59Z
            uploaded_at: OffsetDateTime::from_unix_timestamp(1_772_927_999).unwrap(),
        }
    }

    fn render(template: &str, f: &UploadFacts<'_>) -> String {
        Template::parse(template)
            .unwrap()
            .render(f, &SeededRandom::new(7))
    }

    /// Hands out exactly the given bytes.
    struct Bytes(Mutex<VecDeque<u8>>);
    impl RandomSource for Bytes {
        fn fill_bytes(&self, buf: &mut [u8]) {
            for b in buf {
                *b = self.0.lock().unwrap().pop_front().expect("out of bytes");
            }
        }
    }

    #[test]
    fn random_is_base62_of_the_requested_length() {
        let f = facts(ImageType::Png, "a.png");
        for n in [1, 6, 13, 64] {
            let out = render(&format!("{{random:{n}}}"), &f);
            assert_eq!(out.len(), n);
            assert!(out.bytes().all(|b| BASE62.contains(&b)), "{out}");
        }
        let t = Template::parse("{random:6}").unwrap();
        let rng = SeededRandom::new(1);
        let draws: HashSet<_> = (0..100).map(|_| t.render(&f, &rng)).collect();
        assert_eq!(draws.len(), 100, "each render draws fresh randomness");
    }

    #[test]
    fn random_maps_bytes_to_base62_and_skips_biased_bytes() {
        let rng = Bytes(Mutex::new(VecDeque::from([0, 61, 62, 255, 248, 10])));
        let t = Template::parse("{random:4}").unwrap();
        assert_eq!(t.render(&facts(ImageType::Png, "a"), &rng), "0z0A");
    }

    #[test]
    fn hash_takes_a_prefix_of_the_sha256() {
        let f = facts(ImageType::Png, "a.png");
        assert_eq!(render("{hash:8}", &f), "9f86d081");
        assert_eq!(render("{hash:64}", &f), f.sha256);
    }

    #[test]
    fn date_variables_use_the_upload_time_in_utc() {
        let f = facts(ImageType::Png, "a.png");
        assert_eq!(render("{Y}/{m}/{d}/{hash:4}", &f), "2026/03/07/9f86");
    }

    #[test]
    fn ext_comes_from_the_detected_type() {
        // The original name says .jpg, but the content is PNG.
        assert_eq!(
            render("{hash:2}.{ext}", &facts(ImageType::Png, "photo.jpg")),
            "9f.png"
        );
        let exts: Vec<_> = ImageType::ALL
            .iter()
            .map(|t| render("{ext}{hash:1}", &facts(*t, "x")))
            .collect();
        assert_eq!(exts, ["jpg9", "png9", "gif9", "webp9", "avif9", "svg9"]);
    }

    #[test]
    fn name_is_the_cleaned_original_filename() {
        assert_eq!(
            render("{name}-{hash:2}", &facts(ImageType::Png, "My Cat.PNG")),
            "My-Cat-9f"
        );
    }

    #[test]
    fn filename_cleaning() {
        assert_eq!(clean_name("holiday photo.jpg"), "holiday-photo");
        assert_eq!(clean_name("a  b\t c.png"), "a-b-c");
        assert_eq!(clean_name("we?ird#na&me%.gif"), "we-ird-na-me");
        assert_eq!(clean_name("../../etc/passwd"), "passwd");
        assert_eq!(clean_name("C:\\Users\\me\\shot 1.png"), "shot-1");
        assert_eq!(clean_name("截图 2026.png"), "截图-2026");
        assert_eq!(clean_name("Ünïcødé_name.webp"), "Ünïcødé_name");
        assert_eq!(clean_name("archive.tar.gz"), "archive-tar");
        assert_eq!(clean_name(".hidden"), "hidden");
        assert_eq!(clean_name("--x--.png"), "x");
        assert_eq!(clean_name("???.png"), "image");
        assert_eq!(clean_name(""), "image");
        assert_eq!(clean_name(&"a".repeat(200)).len(), MAX_NAME_CHARS);
    }

    #[test]
    fn presets_parse_and_render() {
        let f = facts(ImageType::Jpeg, "Sunset at sea.jpeg");
        let rng = SeededRandom::new(3);
        type Check = fn(&str) -> bool;
        let shapes: Vec<(&str, Check)> = vec![
            ("short", |p| p.len() == 6 && !p.contains('.')),
            ("short_ext", |p| p.len() == 10 && p.ends_with(".jpg")),
            ("dated", |p| {
                p.starts_with("2026/03/07/") && p.len() == 11 + 13 + 4 && p.ends_with(".jpg")
            }),
            ("filename", |p| {
                p.starts_with("Sunset-at-sea-") && p.len() == 14 + 4 + 4 && p.ends_with(".jpg")
            }),
        ];
        assert_eq!(PRESETS.len(), shapes.len());
        for (preset, (id, ok)) in PRESETS.iter().zip(shapes) {
            assert_eq!(preset.id, id);
            let path = Template::parse(preset.template).unwrap().render(&f, &rng);
            assert!(ok(&path), "{id}: {path}");
        }
    }

    #[test]
    fn default_is_short_with_extension() {
        assert_eq!(Template::default().as_str(), "{random:6}.{ext}");
        let preset = PRESETS.iter().find(|p| p.id == "short_ext").unwrap();
        assert_eq!(preset.template, DEFAULT_TEMPLATE);
    }

    #[test]
    fn invalid_templates_are_rejected() {
        use TemplateError::*;
        let cases = [
            ("", Empty),
            ("{name}.{ext}", NoUniquePart),
            ("static.png", NoUniquePart),
            ("{random}", BadLength("random".into())),
            ("{random:0}", BadLength("random".into())),
            ("{random:65}", BadLength("random".into())),
            ("{hash:x}", BadLength("hash".into())),
            ("{foo}-{random:6}", UnknownVariable("foo".into())),
            ("{ext:3}-{random:6}", UnknownVariable("ext:3".into())),
            ("{random:6", UnmatchedBrace),
            ("{random:6}}", UnmatchedBrace),
            ("a b{random:6}", BadCharacter(' ')),
            ("{random:6}?x", BadCharacter('?')),
            ("/{random:6}", BadSlash),
            ("{random:6}/", BadSlash),
            ("a//{random:6}", BadSlash),
            ("../{random:6}", DotSegment),
            ("{random:6}/.", DotSegment),
        ];
        for (template, error) in cases {
            assert_eq!(Template::parse(template), Err(error), "{template:?}");
        }
    }
}
