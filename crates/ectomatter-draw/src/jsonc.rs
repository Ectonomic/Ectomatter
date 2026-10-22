//! A core's files: JSON with comments (`//` to the end of a line, and
//! `/* */`), as the core protocol has them. Read, and changed in place: a
//! value set or taken out, everything else kept as the core's author wrote
//! it, comments, order and lines alike.

use serde_json::Value;

/// The text with its comments blanked out: each character of them a space
/// (a line's end kept), so every place in the text stays where it was.
pub fn strip(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = b.to_vec();
    let (mut i, mut string) = (0, false);
    while i < b.len() {
        if string {
            match b[i] {
                b'\\' => i += 1,
                b'"' => string = false,
                _ => {}
            }
            i += 1;
            continue;
        }
        match (b[i], b.get(i + 1)) {
            (b'"', _) => {
                string = true;
                i += 1;
            }
            (b'/', Some(b'/')) => {
                while i < b.len() && b[i] != b'\n' {
                    out[i] = b' ';
                    i += 1;
                }
            }
            (b'/', Some(b'*')) => {
                let end = text[i + 2..].find("*/").map_or(b.len(), |e| i + 2 + e + 2);
                for c in &mut out[i..end] {
                    if *c != b'\n' {
                        *c = b' ';
                    }
                }
                i = end;
            }
            _ => i += 1,
        }
    }
    // (only ASCII was blanked, so the text is as valid as it was)
    String::from_utf8(out).unwrap_or_default()
}

/// The text read, comments and all, each key that was once spelled the British way under
/// its name now (see [american]).
pub fn parse(text: &str) -> Result<Value, String> {
    serde_json::from_str(&strip(&american(text))).map_err(|e| e.to_string())
}

/// Keys Ectomatter's files once spelled the British way, and their names now (Grant: "Never
/// Colour - Use Color", 2026-10-14). A file kept before reads as if written with the names
/// now, and is saved with them.
const RENAMED: [(&str, &str); 2] = [("colour", "color"), ("centred", "centered")];

/// The text with each key in [RENAMED] under its name now, wherever it is a key (a string
/// with a colon after it, outside a comment), and everything else as it was.
pub fn american(text: &str) -> String {
    let clean = strip(text);
    let mut found: Vec<(usize, &str, &str)> = Vec::new();
    for (old, new) in RENAMED {
        let quoted = format!("\"{old}\"");
        let mut from = 0;
        while let Some(i) = clean[from..].find(&quoted).map(|i| from + i) {
            from = i + quoted.len();
            if clean[from..].trim_start().starts_with(':') {
                found.push((i + 1, old, new));
            }
        }
    }
    // (from the end, so each place found is still where it was)
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    let mut out = text.to_string();
    for (at, old, new) in found {
        out.replace_range(at..at + old.len(), new);
    }
    out
}

/// Where a member of an object is in the text: its key's start, and its value's start and end.
#[derive(Clone, Copy, Debug)]
struct Member {
    key: usize,
    start: usize,
    end: usize,
}

/// Where an object is in the text, and its members, by name.
struct Object {
    open: usize,
    close: usize,
    members: Vec<(String, Member)>,
}

/// Walks the comment-blanked text.
struct Scan<'a> {
    b: &'a [u8],
    i: usize,
}

impl Scan<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn string(&mut self) -> Result<String, String> {
        let start = self.i;
        self.i += 1;
        while self.i < self.b.len() && self.b[self.i] != b'"' {
            if self.b[self.i] == b'\\' {
                self.i += 1;
            }
            self.i += 1;
        }
        self.i += 1;
        serde_json::from_slice::<String>(&self.b[start..self.i.min(self.b.len())]).map_err(|e| e.to_string())
    }

    /// Past one value, wherever it starts.
    fn value(&mut self) -> Result<(), String> {
        self.ws();
        match self.b.get(self.i) {
            Some(b'"') => self.string().map(|_| ()),
            Some(b'{') | Some(b'[') => {
                let mut depth = 0;
                while self.i < self.b.len() {
                    match self.b[self.i] {
                        b'"' => {
                            self.string()?;
                            continue;
                        }
                        b'{' | b'[' => depth += 1,
                        b'}' | b']' => {
                            depth -= 1;
                            if depth == 0 {
                                self.i += 1;
                                return Ok(());
                            }
                        }
                        _ => {}
                    }
                    self.i += 1;
                }
                Err("an object or a list with no end".into())
            }
            Some(_) => {
                while self.i < self.b.len() && !matches!(self.b[self.i], b',' | b'}' | b']') && !self.b[self.i].is_ascii_whitespace() {
                    self.i += 1;
                }
                Ok(())
            }
            None => Err("no value".into()),
        }
    }

    /// The object starting here, and its members.
    fn object(&mut self) -> Result<Object, String> {
        self.ws();
        if self.b.get(self.i) != Some(&b'{') {
            return Err("not an object".into());
        }
        let open = self.i;
        self.i += 1;
        let mut members = Vec::new();
        loop {
            self.ws();
            match self.b.get(self.i) {
                Some(b'}') => return Ok(Object { open, close: self.i, members }),
                Some(b',') => self.i += 1,
                Some(b'"') => {
                    let key = self.i;
                    let name = self.string()?;
                    self.ws();
                    if self.b.get(self.i) != Some(&b':') {
                        return Err(format!("no colon after {name}"));
                    }
                    self.i += 1;
                    self.ws();
                    let start = self.i;
                    self.value()?;
                    members.push((name, Member { key, start, end: self.i }));
                }
                _ => return Err("an object with no end".into()),
            }
        }
    }
}

/// The object at `path` (from the text's own), as the comment-blanked text has it.
fn object_at(clean: &str, path: &[&str]) -> Result<Object, String> {
    let mut scan = Scan { b: clean.as_bytes(), i: 0 };
    let mut obj = scan.object()?;
    for name in path {
        let m = obj.members.iter().find(|(n, _)| n == name).map(|(_, m)| *m).ok_or_else(|| format!("no {name}"))?;
        scan.i = m.start;
        obj = scan.object()?;
    }
    Ok(obj)
}

/// A value written on one line, as a person would: `{ "kind": "pudding", "top": 6 }`,
/// a whole number without a point.
pub fn inline(v: &Value) -> String {
    match v {
        Value::Number(n) if n.is_f64() && n.as_f64().is_some_and(|f| f.fract() == 0.0 && f.abs() < 1e15) => format!("{}", n.as_f64().unwrap_or(0.0) as i64),
        Value::Object(o) if o.is_empty() => "{}".into(),
        Value::Object(o) => format!("{{ {} }}", o.iter().map(|(k, v)| format!("{}: {}", Value::String(k.clone()), inline(v))).collect::<Vec<_>>().join(", ")),
        Value::Array(a) => format!("[{}]", a.iter().map(inline).collect::<Vec<_>>().join(", ")),
        _ => v.to_string(),
    }
}

fn line_start(t: &str, at: usize) -> usize {
    t[..at].rfind('\n').map_or(0, |i| i + 1)
}

fn line_end(t: &str, at: usize) -> usize {
    t[at..].find('\n').map_or(t.len(), |i| at + i)
}

/// The text with the value at `path` set to `v`: in its place if it is
/// there, or else added to its object (and the objects on the way made),
/// after the object's last member, on a line of its own where the object
/// has its members on lines of their own.
pub fn set(text: &str, path: &[&str], v: &Value) -> Result<String, String> {
    let text = &american(text);
    let clean = strip(text);
    // the deepest object on the way that is there, the rest made in it
    let mut at = path.len().checked_sub(1).ok_or("no path")?;
    while at > 0 && object_at(&clean, &path[..at]).is_err() {
        at -= 1;
    }
    let obj = object_at(&clean, &path[..at])?;
    let mut v = v.clone();
    for name in path[at + 1..].iter().rev() {
        let mut made = serde_json::Map::new();
        made.insert(name.to_string(), v);
        v = Value::Object(made);
    }
    let (name, value) = (path[at], inline(&v));
    if let Some((_, m)) = obj.members.iter().find(|(n, _)| n == name) {
        // a comment after it on its line stays in its column
        let rest = &text[m.end..line_end(text, m.end)];
        let comma = if rest.starts_with(',') { "," } else { "" };
        let gap = rest[comma.len()..].len() - rest[comma.len()..].trim_start_matches(' ').len();
        if gap > 0 && rest[comma.len() + gap..].starts_with("//") {
            let moved = text[m.start..m.end].chars().count() as isize - value.chars().count() as isize;
            let gap_now = (gap as isize + moved).max(gap.min(2) as isize) as usize;
            return Ok(format!("{}{}{}{}{}", &text[..m.start], value, comma, " ".repeat(gap_now), &text[m.end + comma.len() + gap..]));
        }
        return Ok(format!("{}{}{}", &text[..m.start], value, &text[m.end..]));
    }
    let member = format!("{}: {}", Value::String(name.to_string()), value);
    let Some((_, last)) = obj.members.last() else {
        // an empty object: the member in it
        return Ok(format!("{}{{ {} }}{}", &text[..obj.open], member, &text[obj.close + 1..]));
    };
    // on the object's own line where it has one, or its end shares the last member's
    if !clean[last.end..obj.close].contains('\n') {
        return Ok(format!("{}, {}{}", &text[..last.end], member, &text[last.end..]));
    }
    // after the last member's line (its comment kept on it), as indented as it
    let ls = line_start(text, last.key);
    let indent: String = text[ls..last.key].chars().take_while(|c| c.is_whitespace()).collect();
    let le = line_end(text, last.end);
    Ok(format!("{},{}\n{}{}{}", &text[..last.end], &text[last.end..le], indent, member, &text[le..]))
}

/// The text with the member at `path` taken out: its line, comment and all,
/// where it has a line of its own; and the comma that went with it. An
/// object it leaves with nothing in it is `{}`.
pub fn remove(text: &str, path: &[&str]) -> Result<String, String> {
    let (name, parent) = path.split_last().ok_or("no path")?;
    let text = &american(text);
    let clean = strip(text);
    let obj = object_at(&clean, parent)?;
    let Some(i) = obj.members.iter().position(|(n, _)| n == name) else { return Ok(text.to_string()) };
    if obj.members.len() == 1 {
        return Ok(format!("{}{{}}{}", &text[..obj.open], &text[obj.close + 1..]));
    }
    let m = obj.members[i].1;
    let next = obj.members.get(i + 1).map(|(_, m)| *m);
    let prev = i.checked_sub(1).map(|j| obj.members[j].1);
    let own_line = clean[line_start(&clean, m.key)..m.key].trim().is_empty() && clean[m.end..line_end(&clean, m.end)].trim_start().trim_start_matches(',').trim().is_empty();
    if own_line {
        let (ls, le) = (line_start(text, m.key), line_end(text, m.end));
        let mut out = format!("{}{}", &text[..ls], &text[(le + 1).min(text.len())..]);
        // the last one: the comma after the one before it goes
        if let (None, Some(p)) = (next, prev) {
            if let Some(c) = clean[p.end..ls].find(|c: char| !c.is_whitespace()).map(|c| p.end + c).filter(|&c| clean.as_bytes()[c] == b',') {
                out.remove(c);
            }
        }
        return Ok(out);
    }
    // on a line with others: from it to the next member's key, or from the one before's end
    Ok(match (next, prev) {
        (Some(n), _) => format!("{}{}", &text[..m.key], &text[n.key..]),
        (None, Some(p)) => format!("{}{}", &text[..p.end], &text[m.end..]),
        (None, None) => unreachable!("an object with one member is emptied above"),
    })
}

/// The text with the object at `at` made to hold what `to` does, as a
/// person would edit it: each value that differs set in its place, each new
/// one added, each one `to` does not have taken out, and the rest of the
/// text left as it was. Numbers are the same where their values are (14 and 14.0).
pub fn update(text: &str, at: &[&str], to: &Value) -> Result<String, String> {
    let text = american(text);
    let now = parse(&text)?;
    let was = at.iter().fold(&now, |v, name| &v[*name]);
    let mut out = text;
    let mut path: Vec<String> = at.iter().map(|s| s.to_string()).collect();
    update_in(&mut out, &mut path, was, to)?;
    Ok(out)
}

/// Whether two values are the same, numbers by their values (14 and 14.0).
pub fn same(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

fn refs(p: &[String]) -> Vec<&str> {
    p.iter().map(String::as_str).collect()
}

fn update_in(out: &mut String, path: &mut Vec<String>, was: &Value, to: &Value) -> Result<(), String> {
    let (Some(w), Some(t)) = (was.as_object(), to.as_object()) else {
        if !same(was, to) {
            *out = set(out, &refs(path), to)?;
        }
        return Ok(());
    };
    for (k, v) in t {
        path.push(k.clone());
        match w.get(k) {
            Some(old) if old.is_object() && v.is_object() => update_in(out, path, old, v)?,
            Some(old) if same(old, v) => {}
            _ => *out = set(out, &refs(path), v)?,
        }
        path.pop();
    }
    for k in w.keys().filter(|k| !t.contains_key(*k)) {
        path.push(k.clone());
        *out = remove(out, &refs(path))?;
        path.pop();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FILE: &str = r#"// a core's file
{
  "name": "Z", // its name
  "url": "http://example.com/a//b", /* not a comment inside a string */
  "light": {
    "scale": 1.38,       // the drawing's units to the millimeter
    "caps": {
      "kind": "pudding",  // its kind
      "top": 14           //   and its height
    },
    "display": { "lit": 45, "dot": 0.84 }
  }
}
"#;

    /// Comments are left out of what is read, strings with slashes in them kept whole.
    #[test]
    fn comments_are_read_past() {
        let v = parse(FILE).unwrap();
        assert_eq!(v["url"], json!("http://example.com/a//b"));
        assert_eq!(v["light"]["caps"]["top"], json!(14));
        assert_eq!(strip(FILE).len(), FILE.len(), "every place where it was");
    }

    /// A value set in place keeps its line's comment, in its column, and
    /// everything else; a whole number is written without a point.
    #[test]
    fn a_value_set_keeps_the_file_as_written() {
        let t = set(FILE, &["light", "caps", "top"], &json!(12.5)).unwrap();
        assert!(t.contains(r#""top": 12.5         //   and its height"#), "{t}");
        assert_eq!(t.replace("12.5         //", "14           //"), FILE, "nothing else changed");
        let t = set(FILE, &["light", "scale"], &json!(2.0)).unwrap();
        assert!(t.contains("\"scale\": 2,          // the drawing's"), "{t}");
        let t = set(FILE, &["light", "scale"], &json!(1234567.125)).unwrap();
        assert!(t.contains("\"scale\": 1234567.125,  // the drawing's"), "at least as far from it as before: {t}");
        let t = set(FILE, &["light", "display", "dot"], &json!(0.9)).unwrap();
        assert!(t.contains(r#""display": { "lit": 45, "dot": 0.9 }"#));
    }

    /// A value not there is added after its object's last member, on a line
    /// of its own as indented as it (the comma before that one's comment), or
    /// on the object's own line where it has one; the objects on the way made.
    #[test]
    fn a_value_added_goes_where_a_person_would_put_it() {
        let t = set(FILE, &["light", "caps", "milky"], &json!(1)).unwrap();
        assert!(t.contains("\"top\": 14,           //   and its height\n      \"milky\": 1\n    }"), "{t}");
        assert_eq!(parse(&t).unwrap()["light"]["caps"]["milky"], json!(1));
        let t = set(FILE, &["light", "display", "glass"], &json!(1)).unwrap();
        assert!(t.contains(r#""display": { "lit": 45, "dot": 0.84, "glass": 1 }"#), "{t}");
        let t = set(FILE, &["light", "keys", "space", "caps", "kind"], &json!("shine-through")).unwrap();
        assert!(t.contains(r#""keys": { "space": { "caps": { "kind": "shine-through" } } }"#), "{t}");
        assert_eq!(parse(&t).unwrap()["light"]["keys"]["space"]["caps"]["kind"], json!("shine-through"));
    }

    /// An object brought to what another holds changes only what differs,
    /// keeping comments, lines and numbers written another way (14.0 for 14).
    #[test]
    fn an_object_updated_changes_only_what_differs() {
        let mut to = parse(FILE).unwrap()["light"].clone();
        assert_eq!(update(FILE, &["light"], &to).unwrap(), FILE, "nothing differs: nothing changes");
        to["caps"]["top"] = json!(14.0);
        assert_eq!(update(FILE, &["light"], &to).unwrap(), FILE, "14.0 is 14");
        to["caps"]["top"] = json!(12);
        to["display"].as_object_mut().unwrap().remove("dot");
        to["keys"] = json!({ "gpc": { "caps": { "top": 3 } } });
        let t = update(FILE, &["light"], &to).unwrap();
        assert_eq!(parse(&t).unwrap()["light"], to);
        assert!(t.contains(r#""top": 12           //   and its height"#) && t.contains(r#""display": { "lit": 45 },"#), "{t}");
        assert!(!t.contains("12.0"));
        assert!(t.contains("\"scale\": 1.38,       // the drawing's units to the millimeter"), "{t}");
        to.as_object_mut().unwrap().remove("keys");
        to["display"]["dot"] = json!(0.84);
        assert_eq!(update(&t, &["light"], &to).unwrap(), FILE.replace("\"top\": 14 ", "\"top\": 12 "));
        let one = "{ \"a\": { \"b\": 1,\n  \"c\": 2 } }";
        assert_eq!(set(one, &["a", "d"], &json!(3)).unwrap(), "{ \"a\": { \"b\": 1,\n  \"c\": 2, \"d\": 3 } }");
    }

    /// A member taken out goes with its line and its comment, and its comma.
    #[test]
    fn a_value_taken_out_leaves_the_rest() {
        let t = remove(FILE, &["light", "caps", "top"]).unwrap();
        assert!(t.contains("\"kind\": \"pudding\"  // its kind\n    },"), "{t}");
        assert!(parse(&t).unwrap()["light"]["caps"].get("top").is_none());
        let t = remove(FILE, &["light", "display", "lit"]).unwrap();
        assert!(t.contains(r#""display": { "dot": 0.84 }"#), "{t}");
        let t = remove(FILE, &["light", "display", "dot"]).unwrap();
        assert!(t.contains(r#""display": { "lit": 45 }"#), "{t}");
        assert_eq!(remove(FILE, &["light", "nothing"]).unwrap(), FILE);
        let t = remove(FILE, &["light", "display"]).unwrap();
        assert!(t.contains("    }\n  }\n}") && parse(&t).is_ok(), "{t}");
        let t = remove(&remove(FILE, &["light", "caps", "top"]).unwrap(), &["light", "caps", "kind"]).unwrap();
        assert!(t.contains(r#""caps": {},"#) && parse(&t).is_ok(), "{t}");
    }

    #[test]
    fn a_file_kept_with_british_keys_reads_and_saves_with_the_american_ones() {
        let t = "{\n  \"caps\": { \"colour\": \"#CCCCCC\" },  // \"colour\": as a comment says it\n  \"name\": \"colour\",\n  \"centred\": true\n}\n";
        let v = parse(t).unwrap();
        assert_eq!((&v["caps"]["color"], v["caps"].get("colour")), (&serde_json::json!("#CCCCCC"), None));
        assert_eq!(v["centered"], serde_json::json!(true));
        assert_eq!(v["name"], serde_json::json!("colour"), "a value is left as it is");
        // saved: under the name now, in its place, the comment as it was written
        let saved = set(t, &["caps", "color"], &serde_json::json!("#E6E6E6")).unwrap();
        assert_eq!(saved, "{\n  \"caps\": { \"color\": \"#E6E6E6\" },  // \"colour\": as a comment says it\n  \"name\": \"colour\",\n  \"centered\": true\n}\n");
        assert_eq!(update(t, &[], &v).unwrap(), american(t), "nothing changed: only the names");
        assert_eq!(remove(t, &["centered"]).unwrap(), "{\n  \"caps\": { \"color\": \"#CCCCCC\" },  // \"colour\": as a comment says it\n  \"name\": \"colour\"\n}\n");
    }
}
