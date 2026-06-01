use std::sync::mpsc::Receiver;
use eframe::egui;
use uuid::Uuid;

use crate::client::{AsyncHttpClient, ClientMessage, HttpRequest, HttpResponse, KeyValue};
use crate::storage::{ApiCollection, CollectionStorage, SavedRequest};
use crate::theme;

// Tabs for configuring request parameters
#[derive(PartialEq)]
enum RequestTab {
    Headers,
    Body,
    GraphQL,
    Params,
}

// State representing the currently open request in the editor
struct RequestEditorState {
    id: String,
    name: String,
    method: String,
    url: String,
    headers: Vec<KeyValue>,
    body: String,
    graphql_query: String,
    graphql_variables: String,
    active_tab: RequestTab,
}

impl RequestEditorState {
    fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "Untitled Request".to_string(),
            method: "GET".to_string(),
            url: "https://jsonplaceholder.typicode.com/users".to_string(),
            headers: vec![KeyValue {
                key: "Content-Type".to_string(),
                value: "application/json".to_string(),
                active: true,
            }],
            body: "".to_string(),
            graphql_query: "".to_string(),
            graphql_variables: r#"{"variables": {}}"#.to_string(),
            active_tab: RequestTab::Headers,
        }
    }

    fn from_saved(saved: &SavedRequest) -> Self {
        Self {
            id: saved.id.clone(),
            name: saved.name.clone(),
            method: saved.method.clone(),
            url: saved.url.clone(),
            headers: saved.headers.clone(),
            body: saved.body.clone(),
            graphql_query: saved.graphql_query.clone().unwrap_or_default(),
            graphql_variables: saved.graphql_variables.clone().unwrap_or_else(|| r#"{"variables": {}}"#.to_string()),
            active_tab: RequestTab::Headers,
        }
    }

    fn to_saved(&self) -> SavedRequest {
        SavedRequest {
            id: self.id.clone(),
            name: self.name.clone(),
            method: self.method.clone(),
            url: self.url.clone(),
            headers: self.headers.clone(),
            body: self.body.clone(),
            graphql_query: Some(self.graphql_query.clone()),
            graphql_variables: Some(self.graphql_variables.clone()),
        }
    }
}

pub struct AeroApp {
    client: AsyncHttpClient,
    storage: CollectionStorage,

    collections: Vec<ApiCollection>,
    active_request: RequestEditorState,
    
    // Asynchronous communication states
    active_rx: Option<Receiver<ClientMessage>>,
    is_loading: bool,
    last_response: Option<Result<HttpResponse, String>>,
    
    // UI temporary variables
    new_header_key: String,
    new_header_val: String,
    selected_col_idx: usize,
}

impl AeroApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Set the custom visual styles and typography
        theme::apply_theme(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let storage = CollectionStorage::new();
        let collections = storage.load_collections();

        Self {
            client: AsyncHttpClient::new(),
            storage,
            collections,
            active_request: RequestEditorState::new(),
            active_rx: None,
            is_loading: false,
            last_response: None,
            new_header_key: "".to_string(),
            new_header_val: "".to_string(),
            selected_col_idx: 0,
        }
    }

    // Helper to generate a cURL representation of the current request
    fn get_curl_string(&self) -> String {
        let mut curl = format!("curl -X {} \"{}\"", self.active_request.method, self.active_request.url);
        for h in &self.active_request.headers {
            if h.active && !h.key.is_empty() {
                curl.push_str(&format!(" \\\n  -H \"{}: {}\"", h.key, h.value));
            }
        }
        if !self.active_request.body.is_empty() && self.active_request.method != "GET" {
            // Escape double quotes inside json
            let escaped_body = self.active_request.body.replace("\"", "\\\"");
            curl.push_str(&format!(" \\\n  -d \"{}\"", escaped_body));
        }
        curl
    }
}

impl eframe::App for AeroApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Process asynchronous message queue responses
        if let Some(ref rx) = self.active_rx {
            if let Ok(msg) = rx.try_recv() {
                match msg {
                    ClientMessage::RequestStarted => {
                        self.is_loading = true;
                    }
                    ClientMessage::RequestCompleted(res) => {
                        self.is_loading = false;
                        self.last_response = Some(res);
                        self.active_rx = None;
                    }
                }
            }
        }

        // Force UI update frame-by-frame while loading to ensure responsive updates / spinners
        if self.is_loading {
            ctx.request_repaint();
        }

        // 2. Render Sidebar
        egui::SidePanel::left("sidebar_panel")
            .frame(egui::Frame::none().fill(theme::COLOR_BG_SIDEBAR))
            .width_range(240.0..=300.0)
            .show(ctx, |ui| {
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.heading(
                        egui::RichText::new("✦ AeroClient")
                            .color(theme::COLOR_PRIMARY)
                            .strong()
                    );
                });
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(10.0);

                // Sidebar controls
                ui.vertical_centered_justified(|ui| {
                    ui.add_space(4.0);
                    if theme::draw_custom_button(ui, "✚ New Request", theme::COLOR_PRIMARY, egui::Color32::WHITE).clicked() {
                        self.active_request = RequestEditorState::new();
                        self.last_response = None;
                    }
                    ui.add_space(6.0);
                });

                ui.separator();
                ui.add_space(8.0);

                // Collections Section
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new("COLLECTIONS")
                            .color(theme::COLOR_TEXT_MUTED)
                            .strong()
                            .size(11.0)
                    );
                });
                ui.add_space(6.0);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut req_to_load = None;
                    let mut col_to_save = None;

                    for (col_idx, col) in self.collections.iter_mut().enumerate() {
                        egui::collapsing_header::CollapsingState::load_with_default_open(
                            ui.ctx(),
                            egui::Id::new(&col.id),
                            true,
                        )
                        .show_header(ui, |ui| {
                            ui.label(egui::RichText::new(format!("📁 {}", col.name)).strong());
                        })
                        .body(|ui| {
                            for req in &col.requests {
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    
                                    // Highlight method type pill using rich translucent custom helper
                                    theme::draw_method_pill(ui, &req.method);

                                    // Display name with click action
                                    let btn_label = if req.name.is_empty() { "Untitled Request" } else { &req.name };
                                    if ui.selectable_label(self.active_request.id == req.id, btn_label).clicked() {
                                        req_to_load = Some(req.clone());
                                        self.selected_col_idx = col_idx;
                                    }
                                });
                                ui.add_space(2.0);
                            }

                            // Quick add inside collection
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                
                                let add_btn = ui.add(
                                    egui::Button::new(
                                        egui::RichText::new("+ Add Request")
                                            .color(theme::COLOR_PRIMARY)
                                            .strong()
                                            .size(11.0)
                                    )
                                    .fill(egui::Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::new(1.0, theme::COLOR_BORDER))
                                );

                                if add_btn.clicked() {
                                    let new_req = SavedRequest {
                                        id: Uuid::new_v4().to_string(),
                                        name: "New Request".to_string(),
                                        method: "GET".to_string(),
                                        url: "https://jsonplaceholder.typicode.com/posts".to_string(),
                                        headers: vec![],
                                        body: "".to_string(),
                                        graphql_query: None,
                                        graphql_variables: None,
                                    };
                                    col.requests.push(new_req.clone());
                                    req_to_load = Some(new_req);
                                    self.selected_col_idx = col_idx;
                                    col_to_save = Some(col_idx);
                                }
                            });
                        });
                        ui.add_space(8.0);
                    }

                    if let Some(req) = req_to_load {
                        self.active_request = RequestEditorState::from_saved(&req);
                        self.last_response = None;
                    }

                    if let Some(col_idx) = col_to_save {
                        let _ = self.storage.save_collection(&self.collections[col_idx]);
                    }
                });
            });

        // 3. Render Workspace & Response Main Panel
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);

            // Workspace Header: Name and Save options
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.active_request.name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save_btn = ui.add(
                        egui::Button::new(
                            egui::RichText::new("💾 Save Request")
                                .color(theme::COLOR_TEXT_ACTIVE)
                                .strong()
                        )
                        .fill(theme::COLOR_BG_SIDEBAR)
                        .stroke(egui::Stroke::new(1.0, theme::COLOR_BORDER))
                    );
                    if save_btn.clicked() {
                        if !self.collections.is_empty() {
                            let col = &mut self.collections[self.selected_col_idx];
                            if let Some(pos) = col.requests.iter().position(|r| r.id == self.active_request.id) {
                                col.requests[pos] = self.active_request.to_saved();
                            } else {
                                col.requests.push(self.active_request.to_saved());
                            }
                            let _ = self.storage.save_collection(col);
                        }
                    }
                });
            });
            ui.add_space(10.0);

            // Row 1: Method + URL Bar + Send Button
            ui.horizontal(|ui| {
                // Method selection dropdown
                egui::ComboBox::from_id_source("method_combo")
                    .selected_text(
                        egui::RichText::new(&self.active_request.method)
                            .color(theme::get_method_color(&self.active_request.method))
                            .strong()
                    )
                    .width(100.0)
                    .show_ui(ui, |ui| {
                        for m in &["GET", "POST", "PUT", "DELETE", "PATCH"] {
                            ui.selectable_value(
                                &mut self.active_request.method,
                                m.to_string(),
                                egui::RichText::new(*m).color(theme::get_method_color(m)).strong()
                            );
                        }
                    });

                // URL Text Edit
                let url_field = ui.add(
                    egui::TextEdit::singleline(&mut self.active_request.url)
                        .hint_text("Enter Request URL (e.g. jsonplaceholder.typicode.com/users)")
                        .desired_width(ui.available_width() - 110.0)
                );

                // Send Button
                if self.is_loading {
                    ui.add(egui::Spinner::new());
                } else {
                    let send_btn = theme::draw_custom_button(
                        ui,
                        "Send ➤",
                        theme::COLOR_PRIMARY,
                        egui::Color32::WHITE
                    );
                    if send_btn.clicked() || (url_field.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter))) {
                        let mut final_headers = self.active_request.headers.clone();
                        let final_method = if self.active_request.active_tab == RequestTab::GraphQL {
                            // GraphQL queries are always POST
                            "POST".to_string()
                        } else {
                            self.active_request.method.clone()
                        };

                        let final_body = if self.active_request.active_tab == RequestTab::GraphQL {
                            // Check if Content-Type is already set, if not append it
                            if !final_headers.iter().any(|h| h.key.to_lowercase() == "content-type") {
                                final_headers.push(KeyValue {
                                    key: "Content-Type".to_string(),
                                    value: "application/json".to_string(),
                                    active: true,
                                });
                            }
                            let vars_json = serde_json::from_str::<serde_json::Value>(&self.active_request.graphql_variables)
                                .unwrap_or(serde_json::json!({}));
                            serde_json::json!({
                                "query": self.active_request.graphql_query,
                                "variables": vars_json
                            }).to_string()
                        } else {
                            self.active_request.body.clone()
                        };

                        let http_req = HttpRequest {
                            id: self.active_request.id.clone(),
                            method: final_method,
                            url: self.active_request.url.clone(),
                            headers: final_headers,
                            body: final_body,
                        };
                        self.active_rx = Some(self.client.send(http_req));
                        self.is_loading = true;
                    }
                }
            });

            ui.add_space(12.0);

            // Workspace Layout split: Request Builder (Top half) & Response Viewer (Bottom half)
            let height = ui.available_height();
            
            // Tab Selector for Request Builder
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for (tab, name) in &[
                    (RequestTab::Headers, "Headers"),
                    (RequestTab::Body, "Body"),
                    (RequestTab::GraphQL, "GraphQL"),
                    (RequestTab::Params, "Params"),
                ] {
                    let is_selected = self.active_request.active_tab == *tab;
                    let text_color = if is_selected {
                        theme::COLOR_PRIMARY
                    } else {
                        theme::COLOR_TEXT_MUTED
                    };
                    
                    let tab_btn = ui.add(
                        egui::Button::new(
                            egui::RichText::new(*name)
                                .color(text_color)
                                .strong()
                                .size(13.0)
                        )
                        .fill(if is_selected { theme::COLOR_BG_INPUT } else { egui::Color32::TRANSPARENT })
                        .stroke(egui::Stroke::new(1.0, if is_selected { theme::COLOR_BORDER } else { egui::Color32::TRANSPARENT }))
                    );
                    if tab_btn.clicked() {
                        self.active_request.active_tab = match tab {
                            RequestTab::Headers => RequestTab::Headers,
                            RequestTab::Body => RequestTab::Body,
                            RequestTab::GraphQL => RequestTab::GraphQL,
                            RequestTab::Params => RequestTab::Params,
                        };
                    }
                }
            });
            ui.add_space(4.0);

            // Render Request Tab body
            egui::Frame::none()
                .fill(theme::COLOR_BG_INPUT)
                .stroke(egui::Stroke::new(1.0, theme::COLOR_BORDER))
                .inner_margin(8.0)
                .rounding(6.0)
                .show(ui, |ui| {
                    ui.set_height(height * 0.35); // Take 35% of space
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        match self.active_request.active_tab {
                            RequestTab::Headers => {
                                ui.label(egui::RichText::new("HTTP Headers").strong().color(theme::COLOR_TEXT_MUTED));
                                ui.add_space(4.0);
                                
                                let mut to_remove = None;
                                for (idx, h) in self.active_request.headers.iter_mut().enumerate() {
                                    ui.horizontal(|ui| {
                                        ui.checkbox(&mut h.active, "");
                                        ui.text_edit_singleline(&mut h.key);
                                        ui.label(":");
                                        ui.text_edit_singleline(&mut h.value);
                                        if ui.button("❌").clicked() {
                                            to_remove = Some(idx);
                                        }
                                    });
                                }
                                if let Some(idx) = to_remove {
                                    self.active_request.headers.remove(idx);
                                }

                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.text_edit_singleline(&mut self.new_header_key).highlight();
                                    ui.label(":");
                                    ui.text_edit_singleline(&mut self.new_header_val);
                                    if ui.button("➕ Add Header").clicked() {
                                        if !self.new_header_key.is_empty() {
                                            self.active_request.headers.push(KeyValue {
                                                key: self.new_header_key.clone(),
                                                value: self.new_header_val.clone(),
                                                active: true,
                                            });
                                            self.new_header_key.clear();
                                            self.new_header_val.clear();
                                        }
                                    }
                                });
                            }
                            RequestTab::Body => {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Raw JSON / Text Body").strong().color(theme::COLOR_TEXT_MUTED));
                                });
                                ui.add_space(4.0);
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.active_request.body)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(ui.available_width() - 8.0)
                                        .desired_rows(6)
                                        .hint_text(r#"{"key": "value"}"#)
                                );
                            }
                            RequestTab::GraphQL => {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("GraphQL Query").strong().color(theme::COLOR_TEXT_MUTED));
                                });
                                ui.add_space(4.0);
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.active_request.graphql_query)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(ui.available_width() - 8.0)
                                        .desired_rows(6)
                                        .hint_text("query GetUsers {\n  users {\n    id\n    name\n  }\n}")
                                );
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Variables (JSON)").strong().color(theme::COLOR_TEXT_MUTED));
                                });
                                ui.add_space(4.0);
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.active_request.graphql_variables)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(ui.available_width() - 8.0)
                                        .desired_rows(4)
                                        .hint_text(r#"{"id": 1}"#)
                                );
                            }
                            RequestTab::Params => {
                                ui.label(egui::RichText::new("Query Parameters").strong().color(theme::COLOR_TEXT_MUTED));
                                ui.add_space(4.0);
                                ui.label("Parameters can be placed directly in the URL address bar.");
                            }
                        }
                    });
                });

            ui.add_space(12.0);

            // Response Box Header
            ui.horizontal(|ui| {
                ui.heading("Response");
                
                // Copy Curl Button
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("📋 Copy cURL").clicked() {
                        let curl_str = self.get_curl_string();
                        ctx.output_mut(|o| o.copied_text = curl_str);
                    }
                });
            });
            ui.add_space(6.0);

            // Render Response Box
            egui::Frame::none()
                .fill(theme::COLOR_BG_MAIN)
                .stroke(egui::Stroke::new(1.0, theme::COLOR_BORDER))
                .inner_margin(12.0)
                .rounding(8.0)
                .show(ui, |ui| {
                    ui.set_height(ui.available_height() - 8.0);
                    
                    if self.is_loading {
                        ui.vertical_centered(|ui| {
                            ui.add_space(40.0);
                            ui.add(egui::Spinner::new().size(32.0));
                            ui.add_space(10.0);
                            ui.label(
                                egui::RichText::new("Sending Request...")
                                    .color(theme::COLOR_TEXT_MUTED)
                                    .size(15.0)
                            );
                        });
                    } else if let Some(ref response_result) = self.last_response {
                        match response_result {
                            Ok(res) => {
                                // Status bar values
                                ui.horizontal(|ui| {
                                    // Status code pill decoration
                                    let status_color = if res.status >= 200 && res.status < 300 {
                                        theme::COLOR_GET
                                    } else {
                                        theme::COLOR_DELETE
                                    };

                                    egui::Frame::none()
                                        .fill(status_color.linear_multiply(0.15))
                                        .stroke(egui::Stroke::new(1.0, status_color))
                                        .rounding(4.0)
                                        .inner_margin(egui::vec2(6.0, 3.0))
                                        .show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(format!("{} {}", res.status, res.status_text))
                                                    .color(status_color)
                                                    .strong()
                                            );
                                        });

                                    ui.add_space(10.0);
                                    ui.label(egui::RichText::new(format!("Time: {} ms", res.elapsed_ms)).color(theme::COLOR_TEXT_MUTED));
                                    
                                    ui.add_space(10.0);
                                    let kb_size = res.size_bytes as f32 / 1024.0;
                                    ui.label(egui::RichText::new(format!("Size: {:.2} KB", kb_size)).color(theme::COLOR_TEXT_MUTED));
                                });

                                ui.add_space(8.0);
                                ui.separator();
                                ui.add_space(8.0);

                                // Response Content tabs (Body and Response Headers)
                                egui::ScrollArea::both().show(ui, |ui| {
                                    if res.content_type.starts_with("image/") {
                                        ctx.include_bytes("bytes://response_image", res.body_bytes.clone());
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new("Response Image").strong().color(theme::COLOR_PRIMARY));
                                        });
                                        ui.add_space(4.0);
                                        ui.vertical_centered(|ui| {
                                            ui.add(
                                                egui::Image::new("bytes://response_image")
                                                    .max_width(ui.available_width() - 24.0)
                                                    .max_height(350.0)
                                                    .rounding(egui::Rounding::same(6.0))
                                            );
                                        });
                                    } else {
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new("Response Body").strong().color(theme::COLOR_PRIMARY));
                                        });
                                        ui.add_space(4.0);
                                        
                                        // Formatted code viewer
                                        let mut body_str = res.body.clone();
                                        ui.add(
                                            egui::TextEdit::multiline(&mut body_str)
                                                .font(egui::TextStyle::Monospace)
                                                .desired_width(ui.available_width() - 8.0)
                                                .desired_rows(12)
                                                .lock_focus(true)
                                        );
                                    }
                                });
                            }
                            Err(err) => {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(20.0);
                                    ui.label(
                                        egui::RichText::new(format!("⚠️ Connection Error\n{}", err))
                                            .color(theme::COLOR_DELETE)
                                            .size(14.0)
                                    );
                                });
                            }
                        }
                    } else {
                        // Empty State Welcome dashboard
                        ui.vertical_centered(|ui| {
                            ui.add_space(20.0);
                            
                            // Center card container
                            egui::Frame::none()
                                .fill(theme::COLOR_BG_SIDEBAR)
                                .stroke(egui::Stroke::new(1.0, theme::COLOR_BORDER))
                                .rounding(egui::Rounding::same(8.0))
                                .inner_margin(24.0)
                                .show(ui, |ui| {
                                    ui.set_width(450.0);
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            egui::RichText::new("✦ AeroClient")
                                                .color(theme::COLOR_PRIMARY)
                                                .size(24.0)
                                                .strong()
                                        );
                                        ui.add_space(4.0);
                                        ui.label(
                                            egui::RichText::new("The high-performance, lightweight API companion")
                                                .color(theme::COLOR_TEXT_MUTED)
                                                .size(12.0)
                                        );
                                        ui.add_space(16.0);
                                        ui.separator();
                                        ui.add_space(16.0);
                                    });

                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 8.0;
                                        
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new("🚀 REST Engine").color(theme::COLOR_GET).strong());
                                            ui.label("- Full HTTP methods support with live status tags.");
                                        });
                                        
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new("🌌 GraphQL IDE").color(theme::COLOR_PATCH).strong());
                                            ui.label("- Separate query and variable boxes auto-packed.");
                                        });

                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new("🖼️ Image Viewer").color(theme::COLOR_PUT).strong());
                                            ui.label("- Autodetect image streams and draw on screen.");
                                        });

                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new("⚡ Git-Friendly").color(theme::COLOR_POST).strong());
                                            ui.label("- Local JSON collections saved inside ./collections.");
                                        });
                                    });

                                    ui.add_space(16.0);
                                    ui.separator();
                                    ui.add_space(12.0);
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            egui::RichText::new("Select a saved request from the sidebar or click '✚ New Request' to start!")
                                                .color(theme::COLOR_TEXT_ACTIVE)
                                                .strong()
                                                .size(11.0)
                                        );
                                    });
                                });
                        });
                    }
                });
        });
    }
}
