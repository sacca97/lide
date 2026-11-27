//! The hosts in the user's OpenSSH configuration.
//!
//! Only what the connect dialog needs: `Host` aliases, with wildcard and
//! negated patterns left out because they are not connectable names. `Include`
//! directives are not followed; a host in an included file will not be listed,
//! but typing its name in the dialog still works.

use std::path::PathBuf;

/// The hosts in `~/.ssh/config`, in file order, without duplicates.
pub fn hosts() -> Vec<String> {
    let Some(path) = config_path() else {
        return Vec::new();
    };
    match std::fs::read_to_string(path) {
        Ok(text) => parse_hosts(&text),
        Err(_) => Vec::new(),
    }
}

/// Where OpenSSH looks for its configuration.
pub fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh/config"))
}

/// Parse the `Host` lines of an OpenSSH config.
///
/// Both `Host foo bar` and `Host=foo` are accepted, comments are stripped, and
/// aliases may not be wildcards (`*`, `?`) or negations (`!foo`). Duplicates
/// are dropped: the dialog lists each host once.
pub fn parse_hosts(config: &str) -> Vec<String> {
    let mut hosts: Vec<String> = Vec::new();
    for line in config.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        // Whitespace and `=` both separate the keyword from its arguments.
        let mut tokens = line
            .split(|character: char| character.is_whitespace() || character == '=')
            .filter(|token| !token.is_empty());
        let Some(keyword) = tokens.next() else {
            continue;
        };
        if !keyword.eq_ignore_ascii_case("host") {
            continue;
        }
        for alias in tokens {
            if alias.contains('*') || alias.contains('?') || alias.starts_with('!') {
                continue;
            }
            if !hosts.iter().any(|host| host == alias) {
                hosts.push(alias.to_string());
            }
        }
    }
    hosts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_aliases_are_listed_once() {
        let config = "\
Host work
  HostName work.example.com

Host home pi
  User me

Host work
";
        assert_eq!(parse_hosts(config), vec!["work", "home", "pi"]);
    }

    #[test]
    fn wildcards_and_negations_are_not_connectable_names() {
        let config = "\
Host *
  User me
Host !blocked prod-*
Host actual
";
        assert_eq!(parse_hosts(config), vec!["actual"]);
    }

    #[test]
    fn the_equals_form_and_comments_work() {
        let config = "\
# Host commented
Host=build server # trailing
HostName ignored
";
        assert_eq!(parse_hosts(config), vec!["build", "server"]);
    }

    #[test]
    fn an_empty_config_has_no_hosts() {
        assert!(parse_hosts("").is_empty());
        assert!(parse_hosts("HostName only\n").is_empty());
    }
}
