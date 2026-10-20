//! User names for UIDs, from `/etc/passwd`.

use std::collections::HashMap;

#[derive(Default)]
pub struct Users {
    names: HashMap<u32, String>,
}

impl Users {
    pub fn load() -> Self {
        Self::parse(&std::fs::read_to_string("/etc/passwd").unwrap_or_default())
    }

    fn parse(text: &str) -> Self {
        let names = text
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| {
                let mut fields = line.split(':');
                let name = fields.next()?;
                let uid = fields.nth(1)?.parse().ok()?;
                Some((uid, name.to_string()))
            })
            .collect();
        Self { names }
    }

    /// The user name, or the UID as text for users that aren't in the file
    /// (directory services, containers).
    pub fn name(&self, uid: u32) -> String {
        self.names.get(&uid).cloned().unwrap_or_else(|| uid.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_uids_to_names() {
        let users = Users::parse("root:x:0:0::/root:/bin/bash\n# comment\nsizzlebop:x:1000:1000::/home/sizzlebop:/bin/fish\nbroken\n");
        assert_eq!(users.name(0), "root");
        assert_eq!(users.name(1000), "sizzlebop");
        assert_eq!(users.name(4242), "4242");
    }
}
