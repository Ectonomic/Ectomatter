//! Steam's own text format (KeyValues, as config.vdf and localconfig.vdf
//! hold Steam's settings): each key in order, a value words or a group of
//! keys, read and written back as Steam writes them, so the files Ectomatter
//! changes for a game come out as Steam would leave them.

/// A value: words, or a group of keys in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Words(String),
    Keys(Vec<(String, Node)>),
}

impl Node {
    /// The group's value at a key (the first, as Steam reads it; its keys are
    /// the same whatever their case, as one Steam writes "Valve" and another "valve").
    pub fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Keys(k) => k.iter().find(|(n, _)| n.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Node::Words(_) => None,
        }
    }

    pub fn words(&self) -> Option<&str> {
        match self {
            Node::Words(w) => Some(w),
            Node::Keys(_) => None,
        }
    }

    /// The value down a path of keys.
    pub fn at(&self, path: &[&str]) -> Option<&Node> {
        path.iter().try_fold(self, |n, k| n.get(k))
    }

    /// The group down a path of keys, each made where it's missing (a key
    /// holding words is made a group).
    pub fn group_at(&mut self, path: &[&str]) -> &mut Vec<(String, Node)> {
        let mut n = self;
        for k in path {
            let keys = n.keys_mut();
            let i = match keys.iter().position(|(name, _)| name.eq_ignore_ascii_case(k)) {
                Some(i) => i,
                None => {
                    keys.push((k.to_string(), Node::Keys(Vec::new())));
                    keys.len() - 1
                }
            };
            n = &mut keys[i].1;
        }
        n.keys_mut()
    }

    fn keys_mut(&mut self) -> &mut Vec<(String, Node)> {
        if !matches!(self, Node::Keys(_)) {
            *self = Node::Keys(Vec::new());
        }
        match self {
            Node::Keys(k) => k,
            Node::Words(_) => unreachable!(),
        }
    }
}

/// Words set at a key of a group, in place where the key is, else last.
pub fn set(keys: &mut Vec<(String, Node)>, key: &str, value: Node) {
    match keys.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(key)) {
        Some(slot) => slot.1 = value,
        None => keys.push((key.to_string(), value)),
    }
}

/// A key taken out of a group; whether it was there.
pub fn remove(keys: &mut Vec<(String, Node)>, key: &str) -> bool {
    let before = keys.len();
    keys.retain(|(n, _)| !n.eq_ignore_ascii_case(key));
    keys.len() != before
}

/// A file read: its keys at the top, in order.
pub fn parse(text: &str) -> Result<Node, String> {
    let mut toks: Vec<Tok> = Vec::new();
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' => {
                let mut s = String::new();
                loop {
                    match it.next() {
                        Some('\\') => match it.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(c) => s.push(c),
                            None => return Err("a file ends inside words".into()),
                        },
                        Some('"') => break,
                        Some(c) => s.push(c),
                        None => return Err("a file ends inside words".into()),
                    }
                }
                toks.push(Tok::Words(s));
            }
            '{' => toks.push(Tok::Open),
            '}' => toks.push(Tok::Close),
            '/' if it.peek() == Some(&'/') => {
                for c in it.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {}
            // (a word without quotes, as some of Steam's files have)
            c => {
                let mut s = c.to_string();
                while let Some(&n) = it.peek() {
                    if n.is_whitespace() || n == '{' || n == '}' || n == '"' {
                        break;
                    }
                    s.push(n);
                    it.next();
                }
                toks.push(Tok::Words(s));
            }
        }
    }
    let mut at = 0;
    let keys = group(&toks, &mut at)?;
    if at != toks.len() {
        return Err("a file closes a group it never opened".into());
    }
    Ok(Node::Keys(keys))
}

enum Tok {
    Words(String),
    Open,
    Close,
}

fn group(toks: &[Tok], at: &mut usize) -> Result<Vec<(String, Node)>, String> {
    let mut out = Vec::new();
    while *at < toks.len() {
        match &toks[*at] {
            Tok::Close => return Ok(out),
            Tok::Open => return Err("a group with no name".into()),
            Tok::Words(key) => {
                *at += 1;
                match toks.get(*at) {
                    Some(Tok::Words(v)) => {
                        out.push((key.clone(), Node::Words(v.clone())));
                        *at += 1;
                    }
                    Some(Tok::Open) => {
                        *at += 1;
                        let inner = group(toks, at)?;
                        match toks.get(*at) {
                            Some(Tok::Close) => *at += 1,
                            _ => return Err(format!("the group {key} isn't closed")),
                        }
                        out.push((key.clone(), Node::Keys(inner)));
                    }
                    _ => return Err(format!("the key {key} has no value")),
                }
            }
        }
    }
    Ok(out)
}

/// A file written as Steam writes it: a tab more for each group in, a key and
/// its words two tabs apart.
pub fn write(root: &Node) -> String {
    let mut out = String::new();
    if let Node::Keys(keys) = root {
        lines(keys, 0, &mut out);
    }
    out
}

fn lines(keys: &[(String, Node)], depth: usize, out: &mut String) {
    let tabs = "\t".repeat(depth);
    for (k, v) in keys {
        match v {
            Node::Words(w) => out.push_str(&format!("{tabs}\"{}\"\t\t\"{}\"\n", quoted(k), quoted(w))),
            Node::Keys(inner) => {
                out.push_str(&format!("{tabs}\"{}\"\n{tabs}{{\n", quoted(k)));
                lines(inner, depth + 1, out);
                out.push_str(&format!("{tabs}}}\n"));
            }
        }
    }
}

fn quoted(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "\"InstallConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n\t\t\t\t\"CompatToolMapping\"\n\t\t\t\t{\n\t\t\t\t\t\"0\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"name\"\t\t\"Proton-GE Latest\"\n\t\t\t\t\t\t\"config\"\t\t\"\"\n\t\t\t\t\t\t\"priority\"\t\t\"75\"\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t\t\"LaunchOptions\"\t\t\"PROTON_DLSS_UPGRADE=\\\"1\\\" game-helper -b -- %command%\"\n\t\t\t}\n\t\t}\n\t}\n}\n";

    /// A file read and written back as it was, escaped quotes and all.
    #[test]
    fn steams_files_come_back_as_they_were() {
        let n = parse(CONFIG).unwrap();
        assert_eq!(write(&n), CONFIG);
        let lo = n.at(&["InstallConfigStore", "Software", "Valve", "Steam", "LaunchOptions"]).and_then(Node::words);
        assert_eq!(lo, Some("PROTON_DLSS_UPGRADE=\"1\" game-helper -b -- %command%"));
    }

    /// A game's Compatibility setting added where Steam keeps them, its
    /// launch options taken out, everything else where it was.
    #[test]
    fn a_games_settings_change_in_place() {
        let mut n = parse(CONFIG).unwrap();
        let map = n.group_at(&["InstallConfigStore", "Software", "Valve", "Steam", "CompatToolMapping"]);
        set(map, "1808500", Node::Keys(vec![("name".into(), Node::Words("ectomatter".into())), ("config".into(), Node::Words("".into())), ("priority".into(), Node::Words("250".into()))]));
        assert!(remove(n.group_at(&["InstallConfigStore", "Software", "Valve", "Steam"]), "LaunchOptions"));
        let out = write(&n);
        assert!(out.contains("\t\t\t\t\t\"1808500\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"name\"\t\t\"ectomatter\"\n"), "{out}");
        assert!(!out.contains("LaunchOptions"));
        assert_eq!(parse(&out).unwrap().at(&["InstallConfigStore", "Software", "Valve", "Steam", "CompatToolMapping", "0", "name"]).and_then(Node::words), Some("Proton-GE Latest"));
    }

    /// A key found whatever its case, as Steam reads its files; set in place, as spelled.
    #[test]
    fn keys_are_found_whatever_their_case() {
        let mut n = parse("\"InstallConfigStore\" { \"software\" { \"valve\" { \"Steam\" { } } } }").unwrap();
        assert!(n.at(&["InstallConfigStore", "Software", "Valve", "Steam"]).is_some());
        set(n.group_at(&["InstallConfigStore", "Software", "Valve", "Steam"]), "a", Node::Words("1".into()));
        assert!(write(&n).contains("\"software\"") && !write(&n).contains("\"Software\""), "no second tree");
    }

    #[test]
    fn a_broken_file_is_said_so() {
        assert!(parse("\"a\" { \"b\" \"c\"").is_err());
        assert!(parse("\"a\" }").is_err());
    }
}
