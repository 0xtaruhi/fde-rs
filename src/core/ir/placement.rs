use crate::domain::ClusterKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cluster {
    pub name: String,
    pub kind: ClusterKind,
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub capacity: usize,
    #[serde(default)]
    pub x: Option<usize>,
    #[serde(default)]
    pub y: Option<usize>,
    #[serde(default)]
    pub z: Option<usize>,
    #[serde(default)]
    pub fixed: bool,
}

impl Cluster {
    pub fn new(name: impl Into<String>, kind: ClusterKind) -> Self {
        Self {
            name: name.into(),
            kind,
            ..Self::default()
        }
    }

    pub fn logic(name: impl Into<String>) -> Self {
        Self::new(name, ClusterKind::Logic)
    }

    pub fn with_member(mut self, member: impl Into<String>) -> Self {
        self.members.push(member.into());
        self
    }

    pub fn with_members<I, S>(mut self, members: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.members.extend(members.into_iter().map(Into::into));
        self
    }

    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    pub fn at(mut self, x: usize, y: usize) -> Self {
        self.x = Some(x);
        self.y = Some(y);
        self
    }

    pub fn at_slot(mut self, x: usize, y: usize, z: usize) -> Self {
        self.x = Some(x);
        self.y = Some(y);
        self.z = Some(z);
        self
    }

    pub fn fixed_at(mut self, x: usize, y: usize) -> Self {
        self.x = Some(x);
        self.y = Some(y);
        self.fixed = true;
        self
    }

    pub fn fixed_at_slot(mut self, x: usize, y: usize, z: usize) -> Self {
        self.x = Some(x);
        self.y = Some(y);
        self.z = Some(z);
        self.fixed = true;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlacementSite {
    pub cluster: String,
    pub x: usize,
    pub y: usize,
    #[serde(default)]
    pub z: usize,
    #[serde(default)]
    pub fixed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Placement {
    #[serde(default)]
    pub sites: Vec<PlacementSite>,
}
