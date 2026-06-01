use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::client::KeyValue;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedRequest {
    pub id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<KeyValue>,
    pub body: String,
    pub graphql_query: Option<String>,
    pub graphql_variables: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "content")]
pub enum CollectionItem {
    Request(SavedRequest),
    Folder(FolderNode),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FolderNode {
    pub id: String,
    pub name: String,
    pub items: Vec<CollectionItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiCollection {
    pub id: String,
    pub name: String,
    pub items: Vec<CollectionItem>,
}

pub struct CollectionStorage {
    dir_path: PathBuf,
}

impl CollectionStorage {
    pub fn new() -> Self {
        let dir_path = PathBuf::from("collections");
        if !dir_path.exists() {
            let _ = fs::create_dir_all(&dir_path);
        }
        Self { dir_path }
    }

    pub fn load_collections(&self) -> Vec<ApiCollection> {
        let mut collections = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.dir_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(col) = serde_json::from_str::<ApiCollection>(&content) {
                            collections.push(col);
                        }
                    }
                }
            }
        }

        // Return a default collection if empty so the user always has a starting place
        if collections.is_empty() {
            let sample_request = SavedRequest {
                id: "req-1".to_string(),
                name: "Get JSONPlaceholder Users".to_string(),
                method: "GET".to_string(),
                url: "https://jsonplaceholder.typicode.com/users".to_string(),
                headers: vec![KeyValue {
                    key: "Accept".to_string(),
                    value: "application/json".to_string(),
                    active: true,
                }],
                body: "".to_string(),
                graphql_query: None,
                graphql_variables: None,
            };

            let nested_folder = FolderNode {
                id: "subfolder-1".to_string(),
                name: "Advanced Examples".to_string(),
                items: vec![CollectionItem::Request(SavedRequest {
                    id: "req-2".to_string(),
                    name: "Post Create User".to_string(),
                    method: "POST".to_string(),
                    url: "https://jsonplaceholder.typicode.com/posts".to_string(),
                    headers: vec![KeyValue {
                        key: "Content-Type".to_string(),
                        value: "application/json".to_string(),
                        active: true,
                    }],
                    body: r#"{"title": "foo", "body": "bar", "userId": 1}"#.to_string(),
                    graphql_query: None,
                    graphql_variables: None,
                })],
            };

            let default_col = ApiCollection {
                id: "default-col-id".to_string(),
                name: "Aero Sample Collection".to_string(),
                items: vec![
                    CollectionItem::Request(sample_request),
                    CollectionItem::Folder(nested_folder),
                ],
            };
            
            let _ = self.save_collection(&default_col);
            collections.push(default_col);
        }

        collections
    }

    pub fn save_collection(&self, collection: &ApiCollection) -> Result<(), String> {
        let filename = format!("{}.json", collection.id);
        let path = self.dir_path.join(filename);
        let content = serde_json::to_string_pretty(collection).map_err(|e| e.to_string())?;
        fs::write(path, content).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn delete_collection(&self, col_id: &str) -> Result<(), String> {
        let filename = format!("{}.json", col_id);
        let path = self.dir_path.join(filename);
        if path.exists() {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
