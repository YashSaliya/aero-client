use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::client::KeyValue;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub name: String,
    pub variables: Vec<KeyValue>,
}

pub struct EnvironmentStorage {
    dir_path: PathBuf,
}

impl EnvironmentStorage {
    pub fn new() -> Self {
        let dir_path = PathBuf::from("environments");
        if !dir_path.exists() {
            let _ = fs::create_dir_all(&dir_path);
        }
        Self { dir_path }
    }

    pub fn load_environments(&self) -> Vec<Environment> {
        let mut environments = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.dir_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(env) = serde_json::from_str::<Environment>(&content) {
                            environments.push(env);
                        }
                    }
                }
            }
        }

        // Return a default Development environment if empty
        if environments.is_empty() {
            let default_env = Environment {
                id: "env-dev".to_string(),
                name: "Development".to_string(),
                variables: vec![
                    KeyValue {
                        key: "base_url".to_string(),
                        value: "https://jsonplaceholder.typicode.com".to_string(),
                        active: true,
                    },
                    KeyValue {
                        key: "api_token".to_string(),
                        value: "dev_token_xyz".to_string(),
                        active: true,
                    },
                ],
            };
            let _ = self.save_environment(&default_env);
            environments.push(default_env);
        }

        environments
    }

    pub fn save_environment(&self, env: &Environment) -> Result<(), String> {
        let filename = format!("{}.json", env.id);
        let path = self.dir_path.join(filename);
        let content = serde_json::to_string_pretty(env).map_err(|e| e.to_string())?;
        fs::write(path, content).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_environment(&self, env_id: &str) -> Result<(), String> {
        let filename = format!("{}.json", env_id);
        let path = self.dir_path.join(filename);
        if path.exists() {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

// Function to substitute template variables like {{base_url}} with actual values
pub fn substitute_variables(text: &str, env: &Option<Environment>) -> String {
    let env = match env {
        Some(e) => e,
        None => return text.to_string(),
    };

    let mut result = text.to_string();
    for var in &env.variables {
        if var.active && !var.key.is_empty() {
            let placeholder = format!("{{{{{}}}}}", var.key); // Replaces {{key}}
            result = result.replace(&placeholder, &var.value);
        }
    }
    result
}
