use std::collections::HashMap;

pub fn query_params(url: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    if let Some(query) = url.split('?').nth(1) {
        for pair in query.split('&') {
            let mut it = pair.split('=');
            if let (Some(key), Some(val)) = (it.next(), it.next()) {
                params.insert(key.to_string(), val.to_string());
            }
        }
    }
    params
}
