//! The end of the stderr of a failed `podvm` call, for the error of the
//! kind.

/// The bytes of the end of stderr that an error keeps.
const KEEP: usize = 1024;

/// The last lines of the output that fit in [`KEEP`] bytes, each line
/// whole: podvm gives the cause and the fix on its last line (the free
/// space of a full pool and the need of the image, COORD-124). A last line
/// that is longer keeps its end, cut at a character.
pub(crate) fn tail(b: &[u8]) -> String {
    let s = String::from_utf8_lossy(b);
    let s = s.trim();
    let (mut kept, mut end) = (None, s.len());
    loop {
        let begin = s[..end].rfind('\n').map_or(0, |i| i + 1);
        if s.len() - begin > KEEP {
            break;
        }
        kept = Some(begin);
        if begin == 0 {
            break;
        }
        end = begin - 1;
    }
    let cut = kept.unwrap_or_else(|| (s.len() - KEEP..s.len()).find(|&i| s.is_char_boundary(i)).unwrap_or(s.len()));
    s[cut..].to_string()
}
