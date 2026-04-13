use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use sysinfo::System;

#[derive(Clone, Debug)]
pub struct LockfileCredentials {
    pub port: u16,
    pub password: String,
    pub protocol: String,
}

pub fn discover_lockfile() -> Result<LockfileCredentials> {
    if let Ok(override_path) = std::env::var("SCRIMORA_LINK_LOCKFILE") {
        return parse_lockfile(PathBuf::from(override_path));
    }

    let mut system = System::new_all();
    system.refresh_all();

    let candidates = system
        .processes()
        .values()
        .filter_map(|process| process.exe().and_then(|path| path.parent()))
        .filter_map(|directory| {
            let candidate = directory.join("lockfile");

            if candidate.exists() {
                Some(candidate)
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>();

    match candidates.len() {
        0 => Err(anyhow!("No running League client lockfile was found.")),
        1 => {
            let path = candidates
                .into_iter()
                .next()
                .expect("a single lockfile candidate");

            parse_lockfile(path)
        }
        _ => Err(anyhow!(
            "Multiple running League clients were detected. Scrimora Link currently supports one client at a time."
        )),
    }
}

fn parse_lockfile(path: PathBuf) -> Result<LockfileCredentials> {
    let contents = fs::read_to_string(&path)?;
    parse_lockfile_contents(&contents).map(|(port, password, protocol)| LockfileCredentials {
        port,
        password,
        protocol,
    })
}

fn parse_lockfile_contents(contents: &str) -> Result<(u16, String, String)> {
    let parts = contents.trim().split(':').collect::<Vec<_>>();

    if parts.len() != 5 {
        return Err(anyhow!("The League lockfile format was not recognized."));
    }

    Ok((
        parts[2].parse::<u16>()?,
        parts[3].to_string(),
        parts[4].to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::parse_lockfile_contents;

    #[test]
    fn parses_the_standard_league_lockfile_shape() {
        let (port, password, protocol) =
            parse_lockfile_contents("LeagueClientUx:14752:51923:s3cr3t:https")
                .expect("lockfile to parse");

        assert_eq!(port, 51923);
        assert_eq!(password, "s3cr3t");
        assert_eq!(protocol, "https");
    }
}
