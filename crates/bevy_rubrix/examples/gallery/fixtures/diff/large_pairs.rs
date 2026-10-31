//! `gen_drives.py` mirrors these edits as `LARGE_RS_EDITS` and
//! `edit_suffix`.

pub fn with_review_edits(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (at, line) in text.lines().enumerate() {
        match at {
            2000..2006 => continue,
            5000 => {
                out.push_str("// Added by the pair's edit.\n");
                out.push_str("const PAIRED: bool = true;\n");
                out.push_str(line);
            }
            8000 => out.push_str(&format!("{line} // changed")),
            _ => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

pub fn hundred_k_pair(text: &str) -> (String, String) {
    let old = text.repeat(10);
    let mut new = String::with_capacity(old.len() + old.len() / 400);
    for (at, line) in old.split_inclusive('\n').enumerate() {
        if at % 100 == 0 {
            new.push_str(line.strip_suffix('\n').unwrap_or(line));
            new.push_str(" // edit\n");
        } else {
            new.push_str(line);
        }
    }
    (old, new)
}
