use std::collections::HashSet;

use lodestone_core::{ActiveProfile, SourceName, definition::SearchAgent};

pub fn search_agents(
    profiles: &[ActiveProfile],
    source: SourceName,
) -> impl Iterator<Item = &SearchAgent> {
    profiles.iter().flat_map(move |active| {
        active
            .revision
            .definition
            .definition()
            .enabled_search_agents(source)
    })
}

fn push_keywords(agent: &SearchAgent, seen: &mut HashSet<String>, into: &mut Vec<String>) {
    for keyword in &agent.keywords {
        let keyword = keyword.trim();
        if !keyword.is_empty() && seen.insert(keyword.to_lowercase()) {
            into.push(keyword.to_owned());
        }
    }
}

pub fn keyword_union(profiles: &[ActiveProfile], source: SourceName) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut union = Vec::new();
    for agent in search_agents(profiles, source) {
        push_keywords(agent, &mut seen, &mut union);
    }
    union
}

pub struct QueryGroup<K> {
    pub native: K,
    pub keywords: Vec<String>,
}

pub fn query_groups<K: PartialEq>(
    profiles: &[ActiveProfile],
    source: SourceName,
    native: impl Fn(&SearchAgent) -> K,
) -> Vec<QueryGroup<K>> {
    let mut groups: Vec<(QueryGroup<K>, HashSet<String>)> = Vec::new();
    for agent in search_agents(profiles, source) {
        let key = native(agent);
        let position = match groups.iter().position(|(group, _)| group.native == key) {
            Some(position) => position,
            None => {
                groups.push((
                    QueryGroup {
                        native: key,
                        keywords: Vec::new(),
                    },
                    HashSet::new(),
                ));
                groups.len() - 1
            }
        };
        let (group, seen) = &mut groups[position];
        push_keywords(agent, seen, &mut group.keywords);
    }
    groups
        .into_iter()
        .map(|(group, _)| group)
        .filter(|group| !group.keywords.is_empty())
        .collect()
}
