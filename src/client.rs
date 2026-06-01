use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
    pub active: bool,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub id: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<KeyValue>,
    pub body: String,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub content_type: String,
    pub body: String,
    pub body_bytes: Vec<u8>,
    pub elapsed_ms: u128,
    pub size_bytes: usize,
}

pub enum ClientMessage {
    RequestStarted,
    RequestCompleted(Result<HttpResponse, String>),
}

pub struct AsyncHttpClient {
    sender: Sender<(HttpRequest, Sender<ClientMessage>)>,
}

impl AsyncHttpClient {
    pub fn new() -> Self {
        let (tx, rx) = channel::<(HttpRequest, Sender<ClientMessage>)>();

        // Spawn background worker thread that creates a tokio runtime
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async move {
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(30))
                    .build()
                    .unwrap();

                while let Ok((req, progress_tx)) = rx.recv() {
                    let client_clone = client.clone();
                    tokio::spawn(async move {
                        let _ = progress_tx.send(ClientMessage::RequestStarted);
                        let start = Instant::now();

                        let result = execute_request(client_clone, req, start).await;
                        let _ = progress_tx.send(ClientMessage::RequestCompleted(result));
                    });
                }
            });
        });

        Self { sender: tx }
    }

    pub fn send(&self, req: HttpRequest) -> Receiver<ClientMessage> {
        let (tx, rx) = channel();
        let _ = self.sender.send((req, tx));
        rx
    }
}

async fn execute_request(
    client: reqwest::Client,
    req: HttpRequest,
    start: Instant,
) -> Result<HttpResponse, String> {
    // 1. Build Method
    let method = match req.method.to_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        _ => reqwest::Method::GET,
    };

    // 2. Format URL (ensure http/https prefix)
    let mut url_str = req.url.trim().to_string();
    if !url_str.starts_with("http://") && !url_str.starts_with("https://") {
        url_str = format!("http://{}", url_str);
    }

    let mut request_builder = client.request(method, &url_str);

    // 3. Add Headers
    for header in req.headers {
        if header.active && !header.key.trim().is_empty() {
            request_builder = request_builder.header(header.key.trim(), header.value.trim());
        }
    }

    // 4. Add Body for non-GET methods
    if !req.body.is_empty() {
        request_builder = request_builder.body(req.body);
    }

    // 5. Send Request
    let response = request_builder.send().await.map_err(|e| e.to_string())?;

    let elapsed = start.elapsed().as_millis();

    // 6. Read Headers
    let mut res_headers = HashMap::new();
    for (k, v) in response.headers().iter() {
        if let Ok(val_str) = v.to_str() {
            res_headers.insert(k.to_string(), val_str.to_string());
        }
    }

    let status = response.status();
    let status_code = status.as_u16();
    let status_text = status.canonical_reason().unwrap_or("").to_string();

    // 7. Read Body
    let body_bytes = response.bytes().await.map_err(|e| e.to_string())?;
    let raw_vec = body_bytes.to_vec();
    let size_bytes = raw_vec.len();
    let body_text = String::from_utf8_lossy(&raw_vec).into_owned();

    // Format JSON responses automatically
    let formatted_body = if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body_text) {
        serde_json::to_string_pretty(&json_val).unwrap_or(body_text)
    } else {
        body_text
    };

    let content_type = res_headers.get("content-type")
        .or_else(|| res_headers.get("Content-Type"))
        .cloned()
        .unwrap_or_default();

    Ok(HttpResponse {
        status: status_code,
        status_text,
        headers: res_headers,
        content_type,
        body: formatted_body,
        body_bytes: raw_vec,
        elapsed_ms: elapsed,
        size_bytes,
    })
}
