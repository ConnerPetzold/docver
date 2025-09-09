use core::fmt;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Write};

use anyhow::Context;
use git_cmd::git_in_dir;
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

pub const VERSIONS_FILE: &str = "versions.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Version {
    #[serde(rename = "version")]
    pub tag: String,
    pub title: Option<String>,
}

impl Version {
    pub fn new(tag: String, title: Option<String>) -> Self {
        Self { tag, title }
    }
}

fn parse_semver_like(tag: &str) -> Option<semver::Version> {
    let trimmed = tag.trim_start_matches(['v', 'V']);
    if let Ok(v) = semver::Version::parse(trimmed) {
        return Some(v);
    }
    // Extract the leading numeric dotted prefix (e.g., "0.8" from "0.8_or_older")
    let numeric_prefix: String = trimmed
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();

    if !numeric_prefix.is_empty() {
        // Try to coerce incomplete versions like "1" or "1.2" into full MAJOR.MINOR.PATCH
        let mut parts = numeric_prefix.split('.').collect::<Vec<_>>();
        if parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        {
            while parts.len() < 3 {
                parts.push("0");
            }
            let coerced = parts.join(".");
            return semver::Version::parse(&coerced).ok();
        }
    }

    None
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        let a = parse_semver_like(&self.tag);
        let b = parse_semver_like(&other.tag);
        match (a, b) {
            // reverse semver order: higher versions come first
            (Some(va), Some(vb)) => vb.cmp(&va),
            // reverse the semver vs non-semver ordering so non-semver comes first
            (Some(_), None) => Greater,
            (None, Some(_)) => Less,
            // reverse lexicographic for non-semver vs non-semver
            (None, None) => other.tag.cmp(&self.tag),
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.tag)?;
        if let Some(title) = &self.title {
            write!(f, " ({})", title)?;
        }
        Ok(())
    }
}

#[derive(Default, Debug, Clone)]
pub struct Versions {
    pub versions: HashMap<String, Version>,
    pub aliases: HashMap<String, String>,
}

impl Versions {
    pub fn from_git(remote_rev: &str) -> Self {
        git_in_dir(
            ".".into(),
            &["show", format!("{}:{}", remote_rev, VERSIONS_FILE).as_str()],
        )
        .and_then(|s| {
            serde_json::from_str(&s).context(format!("Failed to parse {}", VERSIONS_FILE))
        })
        .unwrap_or_default()
    }

    pub fn by_alias(&self, alias: &str) -> Option<&Version> {
        self.aliases.get(alias).and_then(|v| self.versions.get(v))
    }

    pub fn by_tag(&self, tag: &str) -> Option<&Version> {
        self.versions.get(tag)
    }

    pub fn search(&self, tag_or_alias: &str) -> Vec<&Version> {
        self.versions
            .values()
            .filter(|v| {
                v.tag == tag_or_alias
                    || self
                        .aliases
                        .get(tag_or_alias)
                        .map(|av| av == &v.tag)
                        .is_some()
            })
            .collect()
    }

    pub fn add(
        &mut self,
        version_tag: String,
        title: Option<String>,
        aliases: HashSet<String>,
    ) -> Option<&Version> {
        let version = Version::new(version_tag.clone(), title);

        self.versions.insert(version_tag.clone(), version);
        for alias in aliases {
            self.aliases.insert(alias, version_tag.clone());
        }

        self.versions.get(&version_tag)
    }

    pub fn netlify_rewrites(&self, default_alias: String) -> String {
        let mut result = String::new();
        let mut default_tag: Option<String> = None;

        for (alias, tag) in &self.aliases {
            writeln!(result, "/{}/* /{}/:splat 200", alias, tag)
                .expect("Failed to write to netlify redirects string");

            if *alias == default_alias {
                default_tag = Some(tag.clone());
            }
        }

        if let Some(default_tag) = default_tag {
            writeln!(result, "/* /{}/:splat 200", default_tag)
                .expect("Failed to write to netlify redirects string");
        }

        result
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct VersionWithAliases {
    version: String,
    title: Option<String>,
    aliases: HashSet<String>,
}

impl Serialize for Versions {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.versions.len()))?;
        let mut versions = self.versions.values().collect::<Vec<_>>();
        versions.sort();
        for version in versions {
            let title = version.title.clone().unwrap_or_else(|| version.tag.clone());
            seq.serialize_element(&VersionWithAliases {
                version: version.tag.clone(),
                title: Some(title),
                aliases: self
                    .aliases
                    .iter()
                    .filter(|(_, v)| **v == version.tag)
                    .map(|(a, _)| a.clone())
                    .collect(),
            })?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Versions {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let items = Vec::<VersionWithAliases>::deserialize(deserializer)?;
        let mut versions: HashMap<String, Version> = HashMap::with_capacity(items.len());
        let mut aliases: HashMap<String, String> = HashMap::new();
        for v in items {
            if versions
                .insert(v.version.clone(), Version::new(v.version.clone(), v.title))
                .is_some()
            {
                return Err(de::Error::custom("duplicate version tag"));
            }
            for alias in v.aliases {
                aliases.insert(alias, v.version.clone());
            }
        }
        Ok(Self { versions, aliases })
    }
}

pub struct VersionsIter<'a> {
    versions_sorted: Vec<&'a Version>,
    index: usize,
    aliases: &'a HashMap<String, String>,
}

impl<'a> Iterator for VersionsIter<'a> {
    type Item = (&'a Version, Vec<&'a str>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.versions_sorted.len() {
            return None;
        }
        let version = self.versions_sorted[self.index];
        self.index += 1;

        let aliases = self
            .aliases
            .iter()
            .filter_map(|(alias, tag)| {
                if tag == &version.tag {
                    Some(alias.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        Some((version, aliases))
    }
}

impl<'a> IntoIterator for &'a Versions {
    type Item = (&'a Version, Vec<&'a str>);
    type IntoIter = VersionsIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        let mut versions_sorted = self.versions.values().collect::<Vec<_>>();
        versions_sorted.sort();
        VersionsIter {
            versions_sorted,
            index: 0,
            aliases: &self.aliases,
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_json_snapshot;

    use super::*;

    #[test]
    fn order_semver_and_dev_versions() {
        let mut versions = vec![
            Version {
                tag: "1.2.3".into(),
                title: None,
            },
            Version {
                tag: "dev".into(),
                title: None,
            },
            Version {
                tag: "v1.10.0".into(),
                title: None,
            },
            Version {
                tag: "1.2.10".into(),
                title: None,
            },
            Version {
                tag: "main".into(),
                title: None,
            },
            Version {
                tag: "v0.8_or_older".into(),
                title: None,
            },
        ];
        versions.sort();

        assert_json_snapshot!(versions, @r#"
        [
          {
            "version": "main",
            "title": null
          },
          {
            "version": "dev",
            "title": null
          },
          {
            "version": "v1.10.0",
            "title": null
          },
          {
            "version": "1.2.10",
            "title": null
          },
          {
            "version": "1.2.3",
            "title": null
          },
          {
            "version": "v0.8_or_older",
            "title": null
          }
        ]
        "#);
    }

    #[test]
    fn serialize_versions_sorted() {
        let mut versions = Versions::default();
        versions.add("1.0.0".into(), Some("1.0.0 title".into()), HashSet::new());
        versions.add("v2.0.0".into(), None, HashSet::from(["stable".into()]));
        versions.add("alpha".into(), Some("alpha title".into()), HashSet::new());

        assert_json_snapshot!(versions, @r#"
        [
          {
            "version": "alpha",
            "title": "alpha title",
            "aliases": []
          },
          {
            "version": "v2.0.0",
            "title": "v2.0.0",
            "aliases": [
              "stable"
            ]
          },
          {
            "version": "1.0.0",
            "title": "1.0.0 title",
            "aliases": []
          }
        ]
        "#);
    }

    #[test]
    fn deserialize_versions_with_aliases() {
        let json = r#"[
            {"version":"dev","title":"Development","aliases":["latest"]},
            {"version":"1.0.0","title":"1.0.0","aliases":["stable"]}
        ]"#;
        let versions: Versions = serde_json::from_str(json).unwrap();
        dbg!(&versions);
        assert_json_snapshot!(versions, @r#"
        [
          {
            "version": "dev",
            "title": "Development",
            "aliases": [
              "latest"
            ]
          },
          {
            "version": "1.0.0",
            "title": "1.0.0",
            "aliases": [
              "stable"
            ]
          }
        ]
        "#);
    }

    #[test]
    fn iterate_versions_with_aliases_pairs() {
        use std::collections::HashSet;

        let mut versions = Versions::default();
        versions.add(
            "1.0.0".into(),
            Some("1.0.0".into()),
            HashSet::from(["stable".into()]),
        );
        versions.add(
            "dev".into(),
            Some("Development".into()),
            HashSet::from(["latest".into()]),
        );

        let view: Vec<_> = (&versions)
            .into_iter()
            .map(|(v, aliases)| {
                serde_json::json!({
                    "version": v.tag,
                    "title": v.title,
                    "aliases": aliases,
                })
            })
            .collect();

        assert_json_snapshot!(view, @r#"
        [
          {
            "aliases": [
              "latest"
            ],
            "title": "Development",
            "version": "dev"
          },
          {
            "aliases": [
              "stable"
            ],
            "title": "1.0.0",
            "version": "1.0.0"
          }
        ]
        "#);
    }
}
