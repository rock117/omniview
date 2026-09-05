//! DNS cache domain types.

#[derive(Debug, Clone)]
pub struct DnsCacheEntry {
    pub name: String,
    pub record_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Default)]
pub struct DnsCacheSnapshot {
    pub entries: Vec<DnsCacheEntry>,
    pub error: Option<String>,
    pub supports_remove_entry: bool,
}

pub fn filter_dns_entries<'a>(
    entries: &'a [DnsCacheEntry],
    query: &str,
) -> Vec<&'a DnsCacheEntry> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return entries.iter().collect();
    }
    entries
        .iter()
        .filter(|e| {
            e.name.to_lowercase().contains(&q)
                || e.data.to_lowercase().contains(&q)
                || e.record_type.to_lowercase().contains(&q)
        })
        .collect()
}
