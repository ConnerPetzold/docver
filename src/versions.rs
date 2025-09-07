use std::collections::{HashMap, HashSet};

use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub version: String,
    pub title: String,
    pub aliases: HashSet<String>,
}

impl Version {
    pub fn new(version: String, title: Option<String>, aliases: HashSet<String>) -> Self {
        Self {
            version: version.clone(),
            title: title.unwrap_or(version),
            aliases,
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct Versions {
    versions: HashMap<String, Version>,
}

impl Versions {
    pub fn add(
        &mut self,
        version_tag: String,
        title: Option<String>,
        aliases: HashSet<String>,
    ) -> Option<&Version> {
        let version = Version::new(version_tag.clone(), title, aliases);

        self.versions.insert(version_tag.clone(), version);

        self.versions.get(&version_tag)
    }
}

impl Serialize for Versions {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.versions.len()))?;
        let mut values = self.versions.values().collect::<Vec<_>>();
        values.sort_by_key(|v| v.version.clone());
        for version in values {
            seq.serialize_element(version)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Versions {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let items = Vec::<Version>::deserialize(deserializer)?;
        let mut versions: HashMap<String, Version> = HashMap::with_capacity(items.len());
        for v in items {
            if versions.insert(v.version.clone(), v).is_some() {
                return Err(de::Error::custom("duplicate version tag"));
            }
        }
        Ok(Self { versions })
    }
}
