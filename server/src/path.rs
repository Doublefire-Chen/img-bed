//! The rules every Path follows, whether a Path Template rendered it or the
//! Owner typed it in: no leading or trailing `/`, no empty (`//`), `.` or
//! `..` segments, nothing under a Reserved Prefix.
//!
//! A Path the Owner adds by hand may contain the characters a rendered Path
//! can contain: letters and digits of any script (as `{name}` keeps them),
//! and `-`, `_`, `.`, `~` and `/`. Input may be percent-encoded (e.g. copied
//! from a browser's address bar); it is decoded first, just as serving
//! decodes the request path, and the decoded form is stored.

use percent_encoding::percent_decode_str;

/// Leading parts of the address space that can never be a Path, because the
/// admin area and the API live there. `admin` alone is reserved too: it
/// redirects to `admin/`.
pub const RESERVED_PREFIXES: [&str; 2] = ["admin", "api"];

/// The longest Path the Owner may add, in characters.
pub const MAX_PATH_CHARS: usize = 255;

/// Whether `path` (without a leading `/`) falls under a Reserved Prefix.
pub fn is_reserved(path: &str) -> bool {
    RESERVED_PREFIXES.iter().any(|prefix| {
        path.strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// Whether `s` starts or ends with `/` or contains an empty segment (`//`).
pub fn has_bad_slash(s: &str) -> bool {
    s.starts_with('/') || s.ends_with('/') || s.contains("//")
}

/// Whether `s` has a `.` or `..` segment.
pub fn has_dot_segment(s: &str) -> bool {
    s.split('/')
        .any(|segment| segment == "." || segment == "..")
}

/// Whether a Path may contain `c`.
pub fn is_path_char(c: char) -> bool {
    c.is_alphanumeric() || "-_.~/".contains(c)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("the Path is empty")]
    Empty,
    #[error("the Path is longer than {MAX_PATH_CHARS} characters")]
    TooLong,
    #[error("the Path is not valid percent-encoded UTF-8")]
    BadEncoding,
    #[error(
        "the character {0:?} is not allowed in a Path; use letters, digits, '-', '_', '.', '~' and '/'"
    )]
    BadCharacter(char),
    #[error("the Path must not start with '/', end with '/' or contain '//'")]
    BadSlash,
    #[error("the Path must not contain '.' or '..' segments")]
    DotSegment,
    #[error("/{0} starts with a Reserved Prefix (admin/ or api/)")]
    Reserved(String),
}

/// Checks a Path the Owner typed in and returns it in the stored (decoded)
/// form.
pub fn parse(input: &str) -> Result<String, PathError> {
    let path = percent_decode_str(input)
        .decode_utf8()
        .map_err(|_| PathError::BadEncoding)?
        .into_owned();
    if path.is_empty() {
        return Err(PathError::Empty);
    }
    if path.chars().count() > MAX_PATH_CHARS {
        return Err(PathError::TooLong);
    }
    if let Some(c) = path.chars().find(|c| !is_path_char(*c)) {
        return Err(PathError::BadCharacter(c));
    }
    if has_bad_slash(&path) {
        return Err(PathError::BadSlash);
    }
    if has_dot_segment(&path) {
        return Err(PathError::DotSegment);
    }
    if is_reserved(&path) {
        return Err(PathError::Reserved(path));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_prefixes() {
        for p in ["admin", "admin/", "admin/x.png", "api", "api/v1/info"] {
            assert!(is_reserved(p), "{p}");
        }
        for p in ["administrator.png", "apix", "x/admin/y", "Admin/x", "a.png"] {
            assert!(!is_reserved(p), "{p}");
        }
    }

    #[test]
    fn valid_paths_are_kept() {
        for p in [
            "cat.png",
            "a",
            "2026/03/07/k3J9xQ.jpg",
            "my-cat_v2~final.webp",
            "截图-2026.png",
            "Ünïcødé/name",
            "administrator.png",
            "apis/x",
            "x/admin/y",
            "...png",
            "a/.hidden",
        ] {
            assert_eq!(parse(p).as_deref(), Ok(p), "{p}");
        }
        assert!(parse(&"a".repeat(MAX_PATH_CHARS)).is_ok());
        assert!(parse(&"截".repeat(MAX_PATH_CHARS)).is_ok());
    }

    #[test]
    fn percent_encoded_input_is_decoded() {
        assert_eq!(parse("%E6%88%AA%E5%9B%BE.png").as_deref(), Ok("截图.png"));
        assert_eq!(parse("a%2Fb.png").as_deref(), Ok("a/b.png"));
        assert_eq!(
            parse("%61dmin/x"),
            Err(PathError::Reserved("admin/x".into()))
        );
        assert_eq!(parse("a%20b"), Err(PathError::BadCharacter(' ')));
        assert_eq!(parse("%ff.png"), Err(PathError::BadEncoding));
        assert_eq!(parse("100%"), Err(PathError::BadCharacter('%')));
    }

    #[test]
    fn invalid_paths_are_rejected() {
        use PathError::*;
        let long = "a".repeat(MAX_PATH_CHARS + 1);
        let cases = [
            ("", Empty),
            (long.as_str(), TooLong),
            ("a b.png", BadCharacter(' ')),
            ("a?b", BadCharacter('?')),
            ("a#b", BadCharacter('#')),
            ("a\\b", BadCharacter('\\')),
            ("a\nb", BadCharacter('\n')),
            ("/cat.png", BadSlash),
            ("cat/", BadSlash),
            ("a//b", BadSlash),
            (".", DotSegment),
            ("../cat.png", DotSegment),
            ("a/./b", DotSegment),
            ("admin", Reserved("admin".into())),
            ("admin/cat.png", Reserved("admin/cat.png".into())),
            ("api/v1/x", Reserved("api/v1/x".into())),
        ];
        for (input, error) in cases {
            assert_eq!(parse(input), Err(error), "{input:?}");
        }
    }
}
