use std::io::Cursor;
use tiny_http::{Header, Response, StatusCode};

pub type Resp = Response<Cursor<Vec<u8>>>;

pub fn json_resp(code: u16, body: &str) -> Resp {
    let data = body.as_bytes().to_vec();
    Response::new(
        StatusCode(code),
        vec![
            Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap(),
            Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
        ],
        Cursor::new(data.clone()),
        Some(data.len()),
        None,
    )
}

pub fn query_params(url: &str) -> std::collections::HashMap<String, String> {
    let mut params = std::collections::HashMap::new();
    if let Some(pos) = url.find('?') {
        let query = &url[pos + 1..];
        for part in query.split('&') {
            let mut kv = part.splitn(2, '=');
            if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                params.insert(k.to_string(), v.to_string());
            } else if let Some(k) = kv.next() {
                params.insert(k.to_string(), String::new());
            }
        }
    }
    params
}
