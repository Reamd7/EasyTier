use std::hash::{Hash, Hasher};

pub fn normalized_network_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(trimmed.to_string())
}

pub fn network_id_from_name(name: &str) -> Option<String> {
    let normalized = normalized_network_name(name)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalized.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}
