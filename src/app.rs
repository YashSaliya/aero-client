use std::sync::mpsc::Receiver;
use eframe::egui;
use uuid::Uuid;

use crate::client::{AsyncHttpClient, ClientMessage, HttpRequest, HttpResponse, KeyValue};
use crate::storage::{ApiCollection, CollectionItem, CollectionStorage, FolderNode, SavedRequest};
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
    params: Vec<KeyValue>, // Interactive query parameters list
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
            params: vec![],
            body: "".to_string(),
            graphql_query: "".to_string(),
            graphql_variables: r#"{"variables": {}}"#.to_string(),
            active_tab: RequestTab::Headers,
        }
    }

    fn from_saved(saved: &SavedRequest) -> Self {
        let mut editor = Self {
            id: saved.id.clone(),
            name: saved.name.clone(),
            method: saved.method.clone(),
            url: saved.url.clone(),
            headers: saved.headers.clone(),
            params: vec![],
            body: saved.body.clone(),
            graphql_query: saved.graphql_query.clone().unwrap_or_default(),
            graphql_variables: saved.graphql_variables.clone().unwrap_or_else(|| r#"{"variables": {}}"#.to_string()),
            active_tab: RequestTab::Headers,
        };
        // Populate params from saved URL initially
        editor.params = parse_url_params(&saved.url);
        editor
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
    new_param_key: String,
    new_param_val: String,
    selected_col_idx: usize,
    last_synced_url: String, // Tracks URL changes to avoid infinite loop
}

impl AeroApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply_theme(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let storage = CollectionStorage::new();
        let collections = storage.load_collections();
        let active_request = RequestEditorState::new();
        let last_synced_url = active_request.url.clone();

        Self {
            client: AsyncHttpClient::new(),
            storage,
            collections,
            active_request,
            active_rx: None,
            is_loading: false,
            last_response: None,
            new_header_key: "".to_string(),
            new_header_val: "".to_string(),
            new_param_key: "".to_string(),
            new_param_val: "".to_string(),
            selected_col_idx: 0,
            last_synced_url,
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
            let escaped_body = self.active_request.body.replace("\"", "\\\"");
            curl.push_str(&format!(" \\\n  -d \"{}\"", escaped_body));
        }
        curl
    }
}

// Standalone recursive sidebar rendering function to satisfy the Rust Borrow Checker
fn draw_recursive_item(
    ui: &mut egui::Ui,
    item: &mut CollectionItem,
    indices: &mut Vec<usize>,
    req_to_load: &mut Option<SavedRequest>,
    col_idx: usize,
    col_to_save: &mut bool,
    active_req_id: &str,
    selected_col_idx: &mut usize,
) {
    match item {
        CollectionItem::Request(req) => {
            ui.horizontal(|ui| {
                ui.add_space(8.0 * indices.len() as f32);
                theme::draw_method_pill(ui, &req.method);
                
                let btn_label = if req.name.is_empty() { "Untitled Request" } else { &req.name };
                if ui.selectable_label(active_req_id == req.id, btn_label).clicked() {
                    *req_to_load = Some(req.clone());
                    *selected_col_idx = col_idx;
                }

                // Delete request button
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("❌").clicked() {
                        indices.push(999); // Flag deletion
                        *col_to_save = true;
                    }
                });
            });
        }
        CollectionItem::Folder(folder) => {
            let id = egui::Id::new(&folder.id);
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                .show_header(ui, |ui| {
                    ui.label(egui::RichText::new(format!("📁 {}", folder.name)).strong());
                    
                    // Folder operations CRUD
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("❌").clicked() {
                            indices.push(999); // Flag deletion
                            *col_to_save = true;
                        }
                        if ui.small_button("✚").clicked() {
                            // Add request inside folder
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
                            folder.items.push(CollectionItem::Request(new_req));
                            *col_to_save = true;
                        }
                        if ui.small_button("📁").clicked() {
                            // Add subfolder
                            let new_folder = FolderNode {
                                id: Uuid::new_v4().to_string(),
                                name: "New Folder".to_string(),
                                items: vec![],
                            };
                            folder.items.push(CollectionItem::Folder(new_folder));
                            *col_to_save = true;
                        }
                    });
                })
                .body(|ui| {
                    let mut item_indices_to_delete = Vec::new();
                    for (item_idx, sub_item) in folder.items.iter_mut().enumerate() {
                        let mut sub_indices = indices.clone();
                        sub_indices.push(item_idx);
                        
                        draw_recursive_item(
                            ui, 
                            sub_item, 
                            &mut sub_indices, 
                            req_to_load, 
                            col_idx, 
                            col_to_save,
                            active_req_id,
                            selected_col_idx,
                        );
                        
                        if sub_indices.last() == Some(&999) {
                            item_indices_to_delete.push(item_idx);
                        }
                    }

                    if !item_indices_to_delete.is_empty() {
                        for idx in item_indices_to_delete.iter().rev() {
                            folder.items.remove(*idx);
                        }
                        *col_to_save = true;
                    }
                });
    }
}
}

// Helpers for bidirectional URL ➔ Query Parameter Syncing
fn parse_url_params(url_str: &str) -> Vec<KeyValue> {
    let mut params = Vec::new();
    if let Some(pos) = url_str.find('?') {
        let query = &url_str[pos + 1..];
        for pair in query.split('&') {
            if pair.is_empty() { continue; }
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or("").to_string();
            let value = parts.next().unwrap_or("").to_string();
            params.push(KeyValue { key, value, active: true });
        }
    }
    params
}

fn rebuild_url_with_params(url_str: &str, params: &[KeyValue]) -> String {
    let base = if let Some(pos) = url_str.find('?') {
        &url_str[..pos]
    } else {
        url_str
    };
    
    let mut query_parts = Vec::new();
    for param in params {
        if param.active && !param.key.is_empty() {
            query_parts.push(format!("{}={}", param.key, param.value));
        }
    }
    
    if query_parts.is_empty() {
        base.to_string()
    } else {
        format!("{}?{}", base, query_parts.join("&"))
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

        if self.is_loading {
            ctx.request_repaint();
        }

        // Bidirectional Sync Part 1: URL modified by user -> Update Param rows
        if self.active_request.url != self.last_synced_url {
            self.active_request.params = parse_url_params(&self.active_request.url);
            self.last_synced_url = self.active_request.url.clone();
        }

        // 2. Render Sidebar
        egui::SidePanel::left("sidebar_panel")
            .frame(egui::Frame::none().fill(theme::COLOR_BG_SIDEBAR))
            .width_range(250.0..=310.0)
            .show(ctx, |ui| {
                ui.add_space(16.0);
                
                // Branding Header Logo with custom linear gradient line below it!
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.heading(
                        egui::RichText::new("✦ AeroClient")
                            .color(theme::COLOR_PRIMARY)
                            .strong()
                    );
                });
                ui.add_space(6.0);
                
                // Draw a beautiful horizontal linear accent line under branding
                let (accent_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width() - 24.0, 2.0), egui::Sense::hover());
                theme::paint_linear_gradient(ui, accent_rect, theme::COLOR_PRIMARY, theme::COLOR_PATCH);
                
                ui.add_space(10.0);

                // Sidebar controls
                ui.vertical_centered_justified(|ui| {
                    ui.add_space(4.0);
                    if theme::draw_custom_button(ui, "✚ New Request", theme::COLOR_PRIMARY, egui::Color32::WHITE).clicked() {
                        self.active_request = RequestEditorState::new();
                        self.last_response = None;
                        self.last_synced_url = self.active_request.url.clone();
                    }
                    ui.add_space(6.0);
                });

                ui.separator();
                ui.add_space(8.0);

                // Collections Section CRUD Bar
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new("COLLECTIONS")
                            .color(theme::COLOR_TEXT_MUTED)
                            .strong()
                            .size(11.0)
                    );
                    
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("+ Collection").clicked() {
                            let new_col = ApiCollection {
                                id: Uuid::new_v4().to_string(),
                                name: "New Collection".to_string(),
                                items: vec![],
                            };
                            let _ = self.storage.save_collection(&new_col);
                            self.collections.push(new_col);
                        }
                    });
                });
                ui.add_space(6.0);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut req_to_load = None;
                    let mut col_to_save_idx = None;
                    let mut col_to_delete_idx = None;

                    for (col_idx, col) in self.collections.iter_mut().enumerate() {
                        let id = egui::Id::new(&col.id);
                        let mut folder_save_flag = false;
                        
                        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                            .show_header(ui, |ui| {
                                ui.label(egui::RichText::new(format!("📁 {}", col.name)).strong());
                                
                                // Collection operation buttons
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("❌").clicked() {
                                        col_to_delete_idx = Some(col_idx);
                                    }
                                    if ui.small_button("✚").clicked() {
                                        // Add request inside collection root
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
                                        col.items.push(CollectionItem::Request(new_req.clone()));
                                        req_to_load = Some(new_req);
                                        self.selected_col_idx = col_idx;
                                        col_to_save_idx = Some(col_idx);
                                    }
                                    if ui.small_button("📁").clicked() {
                                        // Add nested folder inside collection root
                                        let new_folder = FolderNode {
                                            id: Uuid::new_v4().to_string(),
                                            name: "New Folder".to_string(),
                                            items: vec![],
                                        };
                                        col.items.push(CollectionItem::Folder(new_folder));
                                        col_to_save_idx = Some(col_idx);
                                    }
                                });
                            })
                            .body(|ui| {
                                let mut item_indices_to_delete = Vec::new();
                                for (item_idx, item) in col.items.iter_mut().enumerate() {
                                    let mut indices = vec![item_idx];
                                    draw_recursive_item(
                                        ui, 
                                        item, 
                                        &mut indices, 
                                        &mut req_to_load, 
                                        col_idx, 
                                        &mut folder_save_flag,
                                        &self.active_request.id,
                                        &mut self.selected_col_idx,
                                    );
                                    
                                    if indices.last() == Some(&999) {
                                        item_indices_to_delete.push(item_idx);
                                    }
                                }

                                if !item_indices_to_delete.is_empty() {
                                    for idx in item_indices_to_delete.iter().rev() {
                                        col.items.remove(*idx);
                                    }
                                    folder_save_flag = true;
                                }
                            });

                        if folder_save_flag {
                            col_to_save_idx = Some(col_idx);
                        }
                        ui.add_space(8.0);
                    }

                    if let Some(req) = req_to_load {
                        self.active_request = RequestEditorState::from_saved(&req);
                        self.last_response = None;
                        self.last_synced_url = self.active_request.url.clone();
                    }

                    if let Some(col_idx) = col_to_save_idx {
                        let _ = self.storage.save_collection(&self.collections[col_idx]);
                    }

                    if let Some(idx) = col_to_delete_idx {
                        let _ = self.storage.delete_collection(&self.collections[idx].id);
                        self.collections.remove(idx);
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
                            
                            // Nested search and update function
                            fn update_nested_item(items: &mut [CollectionItem], target: &SavedRequest) -> bool {
                                for item in items.iter_mut() {
                                    match item {
                                        CollectionItem::Request(req) => {
                                            if req.id == target.id {
                                                *req = target.clone();
                                                return true;
                                            }
                                        }
                                        CollectionItem::Folder(folder) => {
                                            if update_nested_item(&mut folder.items, target) {
                                                return true;
                                            }
                                        }
                                    }
                                }
                                false
                            }

                            let target_saved = self.active_request.to_saved();
                            if !update_nested_item(&mut col.items, &target_saved) {
                                // Add to collection root if not found nested
                                col.items.push(CollectionItem::Request(target_saved));
                            }
                            let _ = self.storage.save_collection(col);
                        }
                    }
                });
            });
            ui.add_space(10.0);

            // Row 1: Method + URL Bar + Send Button
            ui.horizontal(|ui| {
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

                // Send Button with a GORGEOUS linear gradient background
                if self.is_loading {
                    ui.add(egui::Spinner::new());
                } else {
                    let send_btn = theme::draw_gradient_button(
                        ui, 
                        "Send ➤", 
                        theme::COLOR_PRIMARY, 
                        theme::COLOR_PATCH
                    );
                    
                    if send_btn.clicked() || (url_field.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter))) {
                        let mut final_headers = self.active_request.headers.clone();
                        let final_method = if self.active_request.active_tab == RequestTab::GraphQL {
                            "POST".to_string()
                        } else {
                            self.active_request.method.clone()
                        };

                        let final_body = if self.active_request.active_tab == RequestTab::GraphQL {
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
                                ui.label(egui::RichText::new("Interactive Query Parameters").strong().color(theme::COLOR_TEXT_MUTED));
                                ui.add_space(4.0);
                                
                                let mut to_remove = None;
                                let mut params_changed = false;
                                
                                for (idx, p) in self.active_request.params.iter_mut().enumerate() {
                                    ui.horizontal(|ui| {
                                        if ui.checkbox(&mut p.active, "").changed() {
                                            params_changed = true;
                                        }
                                        if ui.text_edit_singleline(&mut p.key).changed() {
                                            params_changed = true;
                                        }
                                        ui.label("=");
                                        if ui.text_edit_singleline(&mut p.value).changed() {
                                            params_changed = true;
                                        }
                                        if ui.button("❌").clicked() {
                                            to_remove = Some(idx);
                                            params_changed = true;
                                        }
                                    });
                                }
                                
                                if let Some(idx) = to_remove {
                                    self.active_request.params.remove(idx);
                                }
                                
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.text_edit_singleline(&mut self.new_param_key);
                                    ui.label("=");
                                    ui.text_edit_singleline(&mut self.new_param_val);
                                    if ui.button("➕ Add Parameter").clicked() {
                                        if !self.new_param_key.is_empty() {
                                            self.active_request.params.push(KeyValue {
                                                key: self.new_param_key.clone(),
                                                value: self.new_param_val.clone(),
                                                active: true,
                                            });
                                            self.new_param_key.clear();
                                            self.new_param_val.clear();
                                            params_changed = true;
                                        }
                                    }
                                });
                                
                                // Bidirectional Sync Part 2: Params grid changed by user -> Rebuild URL
                                if params_changed {
                                    self.active_request.url = rebuild_url_with_params(&self.active_request.url, &self.active_request.params);
                                    self.last_synced_url = self.active_request.url.clone();
                                }
                            }
                        }
                    });
                });

            ui.add_space(12.0);

            // Response Box Header
            ui.horizontal(|ui| {
                ui.heading("Response");
                
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
                                ui.horizontal(|ui| {
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
                                        
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&res.body)
                                                    .font(egui::FontId::monospace(13.0))
                                            )
                                            .selectable(true)
                                            .wrap(false)
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
                        // Empty State Welcome dashboard with stunning linear gradients
                        ui.vertical_centered(|ui| {
                            ui.add_space(20.0);
                            
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
