//! # Tag Management System
//!
//! A module for managing hierarchical tags with parent-child relationships,
//! matchability checks, and ancestor retrieval.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TagNode {
    pub id: String,
    pub name: String,
    pub desc: Option<String>,
    pub children: Option<Vec<TagNode>>,
    /// If a tag is matchable, it will add to ancestor point calculation
    pub is_matchable: bool,
}

/// A system to manage hierarchical tags, allowing for parent-child relationships,
/// matchability checks, and ancestor retrieval.
#[derive(Clone)]
pub struct TagSystem {
    parent_map: HashMap<String, String>,
    matchable_map: HashMap<String, bool>,
}

impl TagSystem {
    /// Loads the tag system from a JSON file.
    pub fn from_json(content: &str) -> Result<Self, serde_json::Error> {
        let nodes: Vec<TagNode> = serde_json::from_str(content)?;

        let mut parent_map = HashMap::new();
        let mut matchable_map = HashMap::new();

        Self::build_maps(&nodes, None, &mut parent_map, &mut matchable_map);

        Ok(TagSystem {
            parent_map,
            matchable_map,
        })
    }

    /// Recursively builds the parent and matchable maps from the tag nodes.
    fn build_maps(
        nodes: &[TagNode],
        parent_id: Option<&str>,
        parent_map: &mut HashMap<String, String>,
        matchable_map: &mut HashMap<String, bool>,
    ) {
        for node in nodes {
            matchable_map.insert(node.id.clone(), node.is_matchable);

            if let Some(parent) = parent_id {
                parent_map.insert(node.id.clone(), parent.to_string());
            }

            if let Some(children) = &node.children {
                Self::build_maps(children, Some(&node.id), parent_map, matchable_map);
            }
        }
    }

    /// Gets the parent tag ID of a given tag ID, if it exists.
    pub fn get_parent(&self, tag_id: &str) -> Option<&String> {
        self.parent_map.get(tag_id)
    }

    /// Checks if a tag is matchable.
    pub fn is_matchable(&self, tag_id: &str) -> bool {
        *self.matchable_map.get(tag_id).unwrap_or(&false)
    }

    /// Retrieves all ancestor tag IDs of a given tag ID.
    pub fn get_all_ancestors(&self, tag_id: &str) -> Vec<String> {
        let mut ancestors = Vec::new();
        let mut current = tag_id;

        while let Some(parent) = self.parent_map.get(current) {
            ancestors.push(parent.clone());
            current = parent;
        }

        ancestors
    }
}
