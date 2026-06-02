use std::sync::mpsc::Receiver;
use eframe::egui;
use uuid::Uuid;

use crate::client::{AsyncHttpClient, ClientMessage, HttpRequest, HttpResponse, KeyValue};
use crate::storage::{ApiCollection, CollectionItem, CollectionStorage, FolderNode, SavedRequest};
use crate::environments::{Environment, EnvironmentStorage, substitute_variables};
use crate::theme;

// ─── Request Config Tabs ────────────────────────────────────────────────────
#[derive(PartialEq)]
enum RequestTab {
    Body,
    Params,
    Headers,
    GraphQL,
}

// ─── Response Viewer Tabs ───────────────────────────────────────────────────
#[derive(PartialEq)]
enum ResponseTab {
    Body,
    Headers,
}

// ─── Active Panel Selector ──────────────────────────────────────────────────
#[derive(PartialEq, Clone)]
enum ActivePanel {
    Request,
    History,
    Collections,
    Environment(usize),
}

// ─── Request Editor State ───────────────────────────────────────────────────
struct RequestEditorState {
    id: String,
    name: String,
    method: String,
    url: String,
    headers: Vec<KeyValue>,
    params: Vec<KeyValue>,
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
            active_tab: RequestTab::Body,
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
            active_tab: RequestTab::Body,
        };
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

// ─── Main Application ──────────────────────────────────────────────────────
pub struct AeroApp {
    client: AsyncHttpClient,
    storage: CollectionStorage,
    env_storage: EnvironmentStorage,

    collections: Vec<ApiCollection>,
    environments: Vec<Environment>,
    history: Vec<SavedRequest>,

    active_request: RequestEditorState,
    active_panel: ActivePanel,
    active_env_idx: Option<usize>,
    response_tab: ResponseTab,

    // Async communication
    active_rx: Option<Receiver<ClientMessage>>,
    is_loading: bool,
    last_response: Option<Result<HttpResponse, String>>,

    // UI temporaries
    new_header_key: String,
    new_header_val: String,
    new_param_key: String,
    new_param_val: String,
    new_env_var_key: String,
    new_env_var_val: String,
    selected_col_idx: usize,
    last_synced_url: String,
}

impl AeroApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply_theme(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let storage = CollectionStorage::new();
        let collections = storage.load_collections();

        let env_storage = EnvironmentStorage::new();
        let environments = env_storage.load_environments();
        let history = storage.load_history();

        let active_request = RequestEditorState::new();
        let last_synced_url = active_request.url.clone();

        Self {
            client: AsyncHttpClient::new(),
            storage,
            env_storage,
            collections,
            environments,
            history,
            active_request,
            active_panel: ActivePanel::Request,
            active_env_idx: Some(0),
            response_tab: ResponseTab::Body,
            active_rx: None,
            is_loading: false,
            last_response: None,
            new_header_key: "".to_string(),
            new_header_val: "".to_string(),
            new_param_key: "".to_string(),
            new_param_val: "".to_string(),
            new_env_var_key: "".to_string(),
            new_env_var_val: "".to_string(),
            selected_col_idx: 0,
            last_synced_url,
        }
    }

    fn get_curl_string(&self) -> String {
        let active_env = self.active_env_idx.and_then(|idx| self.environments.get(idx).cloned());
        let substituted_url = substitute_variables(&self.active_request.url, &active_env);

        let mut curl = format!("curl -X {} \"{}\"", self.active_request.method, substituted_url);
        for h in &self.active_request.headers {
            if h.active && !h.key.is_empty() {
                let sub_val = substitute_variables(&h.value, &active_env);
                curl.push_str(&format!(" \\\n  -H \"{}: {}\"", h.key, sub_val));
            }
        }
        if !self.active_request.body.is_empty() && self.active_request.method != "GET" {
            let sub_body = substitute_variables(&self.active_request.body, &active_env);
            let escaped_body = sub_body.replace("\"", "\\\"");
            curl.push_str(&format!(" \\\n  -d \"{}\"", escaped_body));
        }
        curl
    }
}

// ─── Standalone recursive sidebar renderer ──────────────────────────────────
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
            let is_active = active_req_id == req.id;
            let depth = indices.len();

            ui.horizontal(|ui| {
                ui.add_space(12.0 + 16.0 * depth as f32);

                // Draw action buttons first on the far right to claim the space
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(
                        egui::RichText::new("x").color(theme::COLOR_ERROR.linear_multiply(0.85)).size(11.0)
                    ).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE).min_size(egui::vec2(20.0, 20.0))).clicked() {
                        indices.push(999);
                        *col_to_save = true;
                    }
                });

                theme::draw_method_badge(ui, &req.method);

                let btn_label = if req.name.is_empty() { "Untitled Request" } else { &req.name };
                let text = if is_active {
                    egui::RichText::new(btn_label).color(theme::COLOR_ON_SURFACE).size(13.0)
                } else {
                    egui::RichText::new(btn_label).color(theme::COLOR_ON_SURFACE_VARIANT).size(13.0)
                };

                // Label on the left takes the remaining space and truncates dynamically
                let response = ui.add(
                    egui::Label::new(text).sense(egui::Sense::click()).truncate(true)
                );

                if response.clicked() {
                    *req_to_load = Some(req.clone());
                    *selected_col_idx = col_idx;
                }
            });

            // Draw active highlight bar
            if is_active {
                let last_response = ui.min_rect();
                let bar_rect = egui::Rect::from_min_size(
                    egui::pos2(last_response.left(), last_response.top()),
                    egui::vec2(2.0, last_response.height()),
                );
                ui.painter().rect_filled(bar_rect, 0.0, theme::COLOR_PRIMARY);
            }
        }
        CollectionItem::Folder(folder) => {
            let id = egui::Id::new(&folder.id);
            let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
            let is_open = state.is_open();
            let folder_icon = if is_open { "📂" } else { "📁" };

            state.show_header(ui, |ui| {
                    // 1. Draw action buttons first on the far right to claim space
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(
                            egui::RichText::new("x").color(theme::COLOR_ERROR.linear_multiply(0.85)).size(11.0)
                        ).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE).min_size(egui::vec2(20.0, 20.0))).clicked() {
                            indices.push(999);
                            *col_to_save = true;
                        }
                        if ui.add(egui::Button::new(
                            egui::RichText::new("+").color(theme::COLOR_PRIMARY).size(10.0)
                        ).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE).min_size(egui::vec2(20.0, 20.0))).clicked() {
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
                        if ui.add(egui::Button::new(
                            egui::RichText::new("📁").color(theme::COLOR_TERTIARY).size(10.0)
                        ).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE).min_size(egui::vec2(20.0, 20.0))).clicked() {
                            let new_folder = FolderNode {
                                id: Uuid::new_v4().to_string(),
                                name: "New Folder".to_string(),
                                items: vec![],
                            };
                            folder.items.push(CollectionItem::Folder(new_folder));
                            *col_to_save = true;
                        }
                    });

                    // 2. Label on the left takes the remaining space and truncates dynamically
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("{} {}", folder_icon, folder.name))
                                .color(theme::COLOR_TERTIARY)
                                .strong()
                                .size(13.0)
                        ).truncate(true)
                    );
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

// ─── URL ↔ Query Parameter Sync Helpers ─────────────────────────────────────
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

// ─── Underline Tab Helper ───────────────────────────────────────────────────
fn draw_underline_tab(ui: &mut egui::Ui, label: &str, is_selected: bool) -> bool {
    let how_selected = ui.ctx().animate_bool(ui.make_persistent_id(label), is_selected);

    let text_color = if is_selected {
        theme::COLOR_PRIMARY
    } else {
        theme::COLOR_ON_SURFACE_VARIANT
    };

    let response = ui.add(
        egui::Button::new(
            egui::RichText::new(label.to_uppercase())
                .color(text_color)
                .strong()
                .size(11.0)
                .family(egui::FontFamily::Monospace)
        )
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE)
        .rounding(egui::Rounding::ZERO)
        .min_size(egui::vec2(0.0, 28.0))
    );

    // Draw underline indicator with center-expansion animation
    if how_selected > 0.0 {
        let rect = response.rect;
        let width = rect.width() * how_selected;
        let left = rect.left() + (rect.width() - width) * 0.5;
        let underline_rect = egui::Rect::from_min_size(
            egui::pos2(left, rect.bottom() - 2.0),
            egui::vec2(width, 2.0),
        );
        let color = theme::COLOR_PRIMARY.linear_multiply(how_selected);
        ui.painter().rect_filled(underline_rect, 1.0, color);
    }

    response.clicked()
}


// ═══════════════════════════════════════════════════════════════════════════
// MAIN UPDATE LOOP
// ═══════════════════════════════════════════════════════════════════════════
impl eframe::App for AeroApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll async HTTP responses
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

        // URL modified by user -> sync Param rows
        if self.active_request.url != self.last_synced_url {
            self.active_request.params = parse_url_params(&self.active_request.url);
            self.last_synced_url = self.active_request.url.clone();
        }

        // ─── TOP HEADER BAR ─────────────────────────────────────────────
        egui::TopBottomPanel::top("top_header")
            .frame(
                egui::Frame::none()
                    .fill(theme::COLOR_BACKGROUND)
                    .stroke(egui::Stroke::new(0.5, theme::COLOR_OUTLINE_VARIANT.linear_multiply(0.3)))
                    .inner_margin(egui::Margin::symmetric(16.0, 0.0))
            )
            .exact_height(48.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    // App title
                    ui.label(
                        egui::RichText::new("API Client")
                            .color(theme::COLOR_PRIMARY)
                            .strong()
                            .size(18.0)
                    );

                    ui.add_space(24.0);

                    // Top nav tabs (desktop)
                    let tabs = ["BUILDER", "HISTORY", "COLLECTIONS", "ENV"];
                    let active_tab_idx = match self.active_panel {
                        ActivePanel::Request => 0,
                        ActivePanel::History => 1,
                        ActivePanel::Collections => 2,
                        ActivePanel::Environment(_) => 3,
                    };
                    for (i, tab) in tabs.iter().enumerate() {
                        let is_active = i == active_tab_idx;
                        let text_color = if is_active {
                            theme::COLOR_PRIMARY
                        } else {
                            theme::COLOR_ON_SURFACE_VARIANT
                        };

                        let btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new(*tab)
                                    .color(text_color)
                                    .strong()
                                    .size(11.0)
                                    .family(egui::FontFamily::Monospace)
                            )
                            .fill(if is_active {
                                theme::COLOR_PRIMARY_CONTAINER.linear_multiply(0.2)
                            } else {
                                egui::Color32::TRANSPARENT
                            })
                            .stroke(egui::Stroke::NONE)
                            .rounding(egui::Rounding::same(10.0))
                            .min_size(egui::vec2(0.0, 26.0))
                        );
                        if btn.clicked() {
                            match i {
                                0 => self.active_panel = ActivePanel::Request,
                                1 => self.active_panel = ActivePanel::History,
                                2 => self.active_panel = ActivePanel::Collections,
                                3 => {
                                    if !self.environments.is_empty() {
                                        self.active_panel = ActivePanel::Environment(0);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    // Right side: settings
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("⚙").color(theme::COLOR_ON_SURFACE_VARIANT).size(16.0));
                    });
                });
            });

        // ─── LEFT SIDEBAR ───────────────────────────────────────────────
        egui::SidePanel::left("sidebar_panel")
            .frame(
                egui::Frame::none()
                    .fill(theme::COLOR_SURFACE_CONTAINER)
                    .stroke(egui::Stroke::new(0.5, theme::COLOR_OUTLINE_VARIANT.linear_multiply(0.2)))
            )
            .width_range(260.0..=320.0)
            .show(ctx, |ui| {
                ui.add_space(16.0);

                // Workspace profile header
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    // Workspace icon
                    egui::Frame::none()
                        .fill(theme::COLOR_PRIMARY.linear_multiply(0.2))
                        .stroke(egui::Stroke::new(1.0, theme::COLOR_PRIMARY.linear_multiply(0.3)))
                        .rounding(egui::Rounding::same(10.0))
                        .inner_margin(egui::Margin::same(8.0))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("⚡").size(16.0));
                        });

                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("Personal Workspace")
                                .color(theme::COLOR_ON_SURFACE)
                                .strong()
                                .size(13.0)
                        );
                        ui.label(
                            egui::RichText::new("Pro Plan • v1.4.2")
                                .color(theme::COLOR_ON_SURFACE_VARIANT)
                                .size(11.0)
                                .family(egui::FontFamily::Monospace)
                        );
                    });
                });

                ui.add_space(16.0);

                // Environment selector dropdown
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    let current_env_label = match self.active_env_idx {
                        Some(idx) => self.environments.get(idx).map(|e| e.name.as_str()).unwrap_or("No Env"),
                        None => "No Env",
                    };

                    egui::ComboBox::from_id_source("env_selector")
                        .selected_text(
                            egui::RichText::new(format!("🌐 {}", current_env_label))
                                .color(theme::COLOR_SECONDARY)
                                .size(12.0)
                                .family(egui::FontFamily::Monospace)
                        )
                        .width(ui.available_width() - 32.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.active_env_idx, None, "No Environment");
                            for (idx, env) in self.environments.iter().enumerate() {
                                ui.selectable_value(&mut self.active_env_idx, Some(idx), &env.name);
                            }
                        });
                });

                ui.add_space(12.0);

                // New Request button
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    if theme::draw_primary_button(ui, "+ New Request").clicked() {
                        self.active_request = RequestEditorState::new();
                        self.active_panel = ActivePanel::Request;
                        self.last_response = None;
                        self.last_synced_url = self.active_request.url.clone();
                    }
                });

                ui.add_space(12.0);

                // Separator
                let sep_rect = ui.allocate_space(egui::vec2(ui.available_width(), 1.0));
                ui.painter().rect_filled(sep_rect.1, 0.0, theme::COLOR_OUTLINE_VARIANT.linear_multiply(0.2));

                ui.add_space(12.0);

                // COLLECTIONS section header
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    theme::draw_section_label(ui, "COLLECTIONS");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        if ui.add(egui::Button::new(
                            egui::RichText::new("+").color(theme::COLOR_PRIMARY).size(14.0)
                        ).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE).min_size(egui::vec2(24.0, 24.0))).clicked() {
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

                // Collections tree
                egui::ScrollArea::vertical().id_source("collections_scroll").show(ui, |ui| {
                    let mut req_to_load = None;
                    let mut col_to_save_idx = None;
                    let mut col_to_delete_idx = None;

                    for (col_idx, col) in self.collections.iter_mut().enumerate() {
                    let id = egui::Id::new(&col.id);
                    let mut folder_save_flag = false;

                    let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
                    let is_open = state.is_open();
                    let col_icon = if is_open { "📂" } else { "📁" };

                    state.show_header(ui, |ui| {
                            // 1. Draw action buttons first on the far right to claim space
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let del_btn = ui.add(egui::Button::new(
                                    egui::RichText::new("x").color(theme::COLOR_ERROR.linear_multiply(0.85)).size(11.0)
                                ).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE).min_size(egui::vec2(20.0, 20.0)));
                                if del_btn.clicked() {
                                    col_to_delete_idx = Some(col_idx);
                                }
                                
                                if ui.add(egui::Button::new(
                                    egui::RichText::new("+").color(theme::COLOR_PRIMARY).size(10.0)
                                ).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE).min_size(egui::vec2(20.0, 20.0))).clicked() {
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
                                    self.active_panel = ActivePanel::Request;
                                }
                                
                                if ui.add(egui::Button::new(
                                    egui::RichText::new("📁").color(theme::COLOR_TERTIARY).size(10.0)
                                ).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE).min_size(egui::vec2(20.0, 20.0))).clicked() {
                                    let new_folder = FolderNode {
                                        id: Uuid::new_v4().to_string(),
                                        name: "New Folder".to_string(),
                                        items: vec![],
                                    };
                                    col.items.push(CollectionItem::Folder(new_folder));
                                    col_to_save_idx = Some(col_idx);
                                }
                            });

                            // 2. Label on the left takes the remaining space and truncates dynamically
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("{} {}", col_icon, col.name))
                                        .color(theme::COLOR_TERTIARY)
                                        .strong()
                                        .size(13.0)
                                ).truncate(true)
                            );
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
                        ui.add_space(4.0);
                    }

                    if let Some(req) = req_to_load {
                        self.active_request = RequestEditorState::from_saved(&req);
                        self.last_response = None;
                        self.active_panel = ActivePanel::Request;
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

                // Separator before environments
                ui.add_space(8.0);
                let sep_rect2 = ui.allocate_space(egui::vec2(ui.available_width(), 1.0));
                ui.painter().rect_filled(sep_rect2.1, 0.0, theme::COLOR_OUTLINE_VARIANT.linear_multiply(0.15));
                ui.add_space(8.0);

                // ENVIRONMENTS section
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    theme::draw_section_label(ui, "ENVIRONMENTS");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        if ui.add(egui::Button::new(
                            egui::RichText::new("+").color(theme::COLOR_PRIMARY).size(14.0)
                        ).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE).min_size(egui::vec2(24.0, 24.0))).clicked() {
                            let new_env = Environment {
                                id: Uuid::new_v4().to_string(),
                                name: "New Environment".to_string(),
                                variables: vec![],
                                timeout_ms: 30000,
                                follow_redirects: true,
                                ssl_verification: true,
                            };
                            let _ = self.env_storage.save_environment(&new_env);
                            self.environments.push(new_env);
                            self.active_panel = ActivePanel::Environment(self.environments.len() - 1);
                        }
                    });
                });

                ui.add_space(6.0);

                egui::ScrollArea::vertical().id_source("environments_scroll").max_height(150.0).show(ui, |ui| {
                    let mut env_to_delete_idx = None;

                    for (env_idx, env) in self.environments.iter().enumerate() {
                        let is_editing = matches!(self.active_panel, ActivePanel::Environment(idx) if idx == env_idx);

                        let env_dot_color = match env_idx % 3 {
                            0 => theme::COLOR_SECONDARY,
                            1 => theme::COLOR_TERTIARY,
                            _ => theme::COLOR_ERROR,
                        };

                        ui.horizontal(|ui| {
                            ui.add_space(16.0);

                            // Colored dot
                            let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter().circle_filled(dot_rect.center(), 3.5, env_dot_color);

                            let text = if is_editing {
                                egui::RichText::new(&env.name).color(theme::COLOR_PRIMARY).size(13.0)
                            } else {
                                egui::RichText::new(&env.name).color(theme::COLOR_ON_SURFACE_VARIANT).size(13.0)
                            };

                            let response = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
                            if response.clicked() {
                                self.active_panel = ActivePanel::Environment(env_idx);
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add_space(8.0);
                                if ui.add(egui::Button::new(
                                    egui::RichText::new("x").color(theme::COLOR_OUTLINE).size(11.0)
                                ).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE).min_size(egui::vec2(20.0, 20.0))).clicked() {
                                    env_to_delete_idx = Some(env_idx);
                                }
                            });
                        });
                        ui.add_space(2.0);
                    }

                    if let Some(idx) = env_to_delete_idx {
                        let _ = self.env_storage.delete_environment(&self.environments[idx].id);
                        self.environments.remove(idx);
                        self.active_panel = ActivePanel::Request;
                        self.active_env_idx = None;
                    }
                });
            });

        // ─── CENTRAL PANEL ──────────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(theme::COLOR_BACKGROUND)
                    .inner_margin(egui::Margin::same(0.0))
            )
            .show(ctx, |ui| {
                let target_val = match self.active_panel {
                    ActivePanel::Request => 0.0,
                    ActivePanel::History => 1.0,
                    ActivePanel::Collections => 2.0,
                    ActivePanel::Environment(_) => 3.0,
                };
                let anim_id = ui.make_persistent_id("panel_transition_val");
                let current_val = ui.ctx().animate_value_with_time(anim_id, target_val, 0.25);

                let alpha = (1.0 - (current_val - target_val).abs()).clamp(0.0, 1.0);

                ui.scope(|ui| {
                    if alpha < 0.99 {
                        // Apply transparency to elements inside this scope during transition
                        let visuals = ui.visuals_mut();
                        visuals.widgets.noninteractive.fg_stroke.color = visuals.widgets.noninteractive.fg_stroke.color.linear_multiply(alpha);
                        visuals.widgets.inactive.fg_stroke.color = visuals.widgets.inactive.fg_stroke.color.linear_multiply(alpha);
                        visuals.widgets.hovered.fg_stroke.color = visuals.widgets.hovered.fg_stroke.color.linear_multiply(alpha);
                        visuals.widgets.active.fg_stroke.color = visuals.widgets.active.fg_stroke.color.linear_multiply(alpha);
                    }

                    match self.active_panel.clone() {
                        ActivePanel::Environment(env_idx) => {
                            self.render_environment_panel(ui, env_idx);
                        }
                        ActivePanel::Request => {
                            self.render_request_panel(ui, ctx);
                        }
                        ActivePanel::History => {
                            self.render_history_panel(ui);
                        }
                        ActivePanel::Collections => {
                            self.render_collections_panel(ui);
                        }
                    }
                });
            });
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ENVIRONMENT PANEL
// ═══════════════════════════════════════════════════════════════════════════
impl AeroApp {
    fn render_environment_panel(&mut self, ui: &mut egui::Ui, env_idx: usize) {
        let panel_width = ui.available_width();
        let mut env_to_delete = false;

        egui::Frame::none()
            .fill(theme::COLOR_BACKGROUND)
            .inner_margin(egui::Margin::same(24.0))
            .show(ui, |ui| {
                if let Some(env) = self.environments.get_mut(env_idx) {
                    // Title row
                    ui.horizontal(|ui| {
                        // Active badge
                        egui::Frame::none()
                            .fill(theme::COLOR_SECONDARY.linear_multiply(0.15))
                            .rounding(egui::Rounding::same(4.0))
                            .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("ACTIVE").color(theme::COLOR_SECONDARY).strong().size(10.0).family(egui::FontFamily::Monospace));
                            });

                        ui.add_space(8.0);

                        ui.label(
                            egui::RichText::new(format!("ID: ENV_{}", env.id.to_uppercase().chars().take(8).collect::<String>()))
                                .color(theme::COLOR_ON_SURFACE_VARIANT)
                                .size(11.0)
                                .family(egui::FontFamily::Monospace)
                        );
                    });
                    ui.add_space(4.0);

                    // Environment Name (editable heading)
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut env.name)
                                .font(egui::FontId::new(20.0, egui::FontFamily::Proportional))
                                .text_color(theme::COLOR_ON_SURFACE)
                                .hint_text("Environment Name")
                        );
                        ui.label(
                            egui::RichText::new("Variables")
                                .color(theme::COLOR_ON_SURFACE_VARIANT)
                                .size(20.0)
                        );
                    });
                    ui.add_space(16.0);

                    let use_two_columns = panel_width > 600.0;

                    if use_two_columns {
                        ui.columns(2, |cols| {
                            // Left column: Key-Value Pairs
                            cols[0].group(|ui| {
                                Self::render_env_variables_card(ui, env, &mut self.new_env_var_key, &mut self.new_env_var_val);
                            });

                            // Right column: Settings
                            cols[1].group(|ui| {
                                Self::render_env_settings_card(ui, env);
                            });
                        });
                    } else {
                        Self::render_env_variables_card(ui, env, &mut self.new_env_var_key, &mut self.new_env_var_val);
                        ui.add_space(16.0);
                        Self::render_env_settings_card(ui, env);
                    }

                    ui.add_space(16.0);

                    // Danger Zone
                    egui::Frame::none()
                        .fill(theme::COLOR_ERROR.linear_multiply(0.05))
                        .stroke(egui::Stroke::new(1.0, theme::COLOR_ERROR.linear_multiply(0.3)))
                        .rounding(egui::Rounding::same(10.0))
                        .inner_margin(egui::Margin::same(16.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new("Environment Danger Zone").color(theme::COLOR_ERROR).strong().size(15.0));
                                    ui.label(egui::RichText::new("Deleting an environment is permanent. All variables will be lost.").color(theme::COLOR_ON_SURFACE_VARIANT).size(12.0));
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let del_btn = ui.add(
                                        egui::Button::new(
                                            egui::RichText::new("DELETE ENVIRONMENT")
                                                .color(theme::COLOR_ERROR)
                                                .strong()
                                                .size(11.0)
                                                .family(egui::FontFamily::Monospace)
                                        )
                                        .fill(egui::Color32::TRANSPARENT)
                                        .stroke(egui::Stroke::new(1.0, theme::COLOR_ERROR))
                                        .rounding(egui::Rounding::same(10.0))
                                        .min_size(egui::vec2(0.0, 32.0))
                                    );
                                    if del_btn.clicked() {
                                        env_to_delete = true;
                                    }
                                });
                            });
                        });

                    // Auto-save any environment state edits to disk instantly
                    let _ = self.env_storage.save_environment(env);
                }
            });

        if env_to_delete {
            if let Some(env) = self.environments.get(env_idx) {
                let _ = self.env_storage.delete_environment(&env.id);
                self.environments.remove(env_idx);
                self.active_panel = ActivePanel::Request;
                self.active_env_idx = None;
            }
        }
    }

    fn render_env_variables_card(ui: &mut egui::Ui, env: &mut Environment, new_key: &mut String, new_val: &mut String) {
        theme::card_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Key-Value Pairs").color(theme::COLOR_ON_SURFACE).strong().size(15.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{} VARIABLES", env.variables.len()))
                            .color(theme::COLOR_ON_SURFACE_VARIANT)
                            .size(11.0)
                            .family(egui::FontFamily::Monospace)
                    );
                });
            });

            ui.add_space(12.0);

            // Table header
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Variable")
                        .color(theme::COLOR_ON_SURFACE_VARIANT)
                        .size(11.0)
                        .family(egui::FontFamily::Monospace)
                );
                ui.add_space(ui.available_width() * 0.3);
                ui.label(
                    egui::RichText::new("Value")
                        .color(theme::COLOR_ON_SURFACE_VARIANT)
                        .size(11.0)
                        .family(egui::FontFamily::Monospace)
                );
            });

            ui.add_space(6.0);

            let mut to_remove = None;
            for (var_idx, var) in env.variables.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut var.active, "");
                    let key_response = ui.add(
                        egui::TextEdit::singleline(&mut var.key)
                            .font(egui::FontId::monospace(13.0))
                            .text_color(theme::COLOR_SECONDARY)
                            .desired_width(ui.available_width() * 0.35)
                    );
                    let _ = key_response;

                    ui.add(
                        egui::TextEdit::singleline(&mut var.value)
                            .font(egui::FontId::monospace(13.0))
                            .text_color(theme::COLOR_ON_SURFACE)
                            .desired_width(ui.available_width() - 40.0)
                    );

                    if ui.add(egui::Button::new(
                        egui::RichText::new("x").color(theme::COLOR_OUTLINE).size(11.0)
                    ).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE).min_size(egui::vec2(20.0, 20.0))).clicked() {
                        to_remove = Some(var_idx);
                    }
                });
            }

            if let Some(idx) = to_remove {
                env.variables.remove(idx);
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(new_key)
                        .font(egui::FontId::monospace(13.0))
                        .hint_text("key")
                        .desired_width(ui.available_width() * 0.35)
                );
                ui.add(
                    egui::TextEdit::singleline(new_val)
                        .font(egui::FontId::monospace(13.0))
                        .hint_text("value")
                        .desired_width(ui.available_width() - 140.0)
                );
                if theme::draw_outlined_button(ui, "⊕ ADD NEW VARIABLE").clicked() {
                    if !new_key.is_empty() {
                        env.variables.push(KeyValue {
                            key: new_key.clone(),
                            value: new_val.clone(),
                            active: true,
                        });
                        new_key.clear();
                        new_val.clear();
                    }
                }
            });
        });
    }

    fn render_env_settings_card(ui: &mut egui::Ui, env: &mut Environment) {
        // Execution settings card
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("⚡ Execution").color(theme::COLOR_ON_SURFACE).strong().size(15.0));
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("REQUEST TIMEOUT (MS)").color(theme::COLOR_ON_SURFACE_VARIANT).size(11.0).family(egui::FontFamily::Monospace));
            });
            ui.add_space(4.0);
            let mut timeout_text = env.timeout_ms.to_string();
            if ui.add(
                egui::TextEdit::singleline(&mut timeout_text)
                    .font(egui::FontId::monospace(14.0))
                    .text_color(theme::COLOR_ON_SURFACE)
                    .desired_width(ui.available_width())
            ).changed() {
                if let Ok(val) = timeout_text.parse::<u32>() {
                    env.timeout_ms = val;
                }
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Follow Redirects").color(theme::COLOR_ON_SURFACE_VARIANT).size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(&mut env.follow_redirects, "");
                });
            });
        });

        ui.add_space(12.0);

        // Security settings card
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("🔒 Security").color(theme::COLOR_ON_SURFACE).strong().size(15.0));
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("SSL Verification").color(theme::COLOR_ON_SURFACE_VARIANT).size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(&mut env.ssl_verification, "");
                });
            });
        });
    }

    // ═══════════════════════════════════════════════════════════════════
    // REQUEST BUILDER PANEL
    // ═══════════════════════════════════════════════════════════════════
    fn render_request_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let total_height = ui.available_height();

        // ─── Top Section: URL Bar + Tabs + Body Editor ──────────────
        let top_height = if self.last_response.is_some() || self.is_loading {
            total_height * 0.5
        } else {
            total_height
        };

        egui::Frame::none()
            .fill(theme::COLOR_BACKGROUND)
            .inner_margin(egui::Margin { left: 16.0, right: 16.0, top: 16.0, bottom: 8.0 })
            .show(ui, |ui| {
                ui.set_height(top_height - 32.0);

                // Request name + save button
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.active_request.name)
                            .font(egui::FontId::proportional(16.0))
                            .text_color(theme::COLOR_ON_SURFACE)
                            .desired_width(300.0)
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::draw_outlined_button(ui, "💾 Save").clicked() {
                            if !self.collections.is_empty() {
                                let col = &mut self.collections[self.selected_col_idx];

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
                                    col.items.push(CollectionItem::Request(target_saved));
                                }
                                let _ = self.storage.save_collection(col);
                            }
                        }

                        if theme::draw_outlined_button(ui, "📋 cURL").clicked() {
                            let curl_str = self.get_curl_string();
                            ctx.output_mut(|o| o.copied_text = curl_str);
                        }
                    });
                });

                ui.add_space(8.0);

                // ─── Unified URL Bar (Method + URL + Send) ───────────
                ui.horizontal(|ui| {
                    let send_btn_width = 88.0;
                    let spacing = 8.0;
                    let url_bar_width = ui.available_width() - send_btn_width - spacing;

                    // URL bar container
                    egui::Frame::none()
                        .fill(theme::COLOR_SURFACE_CONTAINER_LOW)
                        .stroke(egui::Stroke::new(1.0, theme::COLOR_OUTLINE_VARIANT.linear_multiply(0.3)))
                        .rounding(egui::Rounding::same(10.0))
                        .inner_margin(egui::Margin::symmetric(0.0, 0.0))
                        .show(ui, |ui| {
                            ui.set_width(url_bar_width);
                            ui.set_height(36.0);
                            ui.horizontal_centered(|ui| {
                                // Method dropdown
                                egui::Frame::none()
                                    .fill(theme::COLOR_SURFACE_CONTAINER_HIGH)
                                    .rounding(egui::Rounding {
                                        nw: 10.0, sw: 10.0, ne: 0.0, se: 0.0,
                                    })
                                    .inner_margin(egui::Margin::symmetric(4.0, 2.0))
                                    .show(ui, |ui| {
                                        egui::ComboBox::from_id_source("method_combo")
                                            .selected_text(
                                                egui::RichText::new(&self.active_request.method)
                                                    .color(theme::get_method_color(&self.active_request.method))
                                                    .strong()
                                                    .size(12.0)
                                                    .family(egui::FontFamily::Monospace)
                                            )
                                            .width(70.0)
                                            .show_ui(ui, |ui| {
                                                for m in &["GET", "POST", "PUT", "DELETE", "PATCH"] {
                                                    ui.selectable_value(
                                                        &mut self.active_request.method,
                                                        m.to_string(),
                                                        egui::RichText::new(*m).color(theme::get_method_color(m)).strong()
                                                    );
                                                }
                                            });
                                    });

                                // URL input
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.active_request.url)
                                        .font(egui::FontId::monospace(13.0))
                                        .text_color(theme::COLOR_ON_SURFACE)
                                        .frame(false)
                                        .hint_text("Enter request URL...")
                                        .desired_width(ui.available_width() - 8.0)
                                );
                            });
                        });

                    ui.add_space(8.0);

                    // Send button
                    if self.is_loading {
                        ui.add(egui::Spinner::new().size(24.0));
                    } else {
                        let send_btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new("▶ SEND")
                                    .color(theme::COLOR_ON_PRIMARY)
                                    .strong()
                                    .size(11.0)
                                    .family(egui::FontFamily::Monospace)
                            )
                            .fill(theme::COLOR_PRIMARY)
                            .stroke(egui::Stroke::NONE)
                            .rounding(egui::Rounding::same(10.0))
                            .min_size(egui::vec2(80.0, 36.0))
                        );

                        if send_btn.clicked() {
                            self.send_request(ctx);
                        }
                    }
                });

                ui.add_space(12.0);

                // ─── Config Tabs (Underline Style) ───────────────────
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    if draw_underline_tab(ui, "Body", self.active_request.active_tab == RequestTab::Body) {
                        self.active_request.active_tab = RequestTab::Body;
                    }
                    if draw_underline_tab(ui, "Params", self.active_request.active_tab == RequestTab::Params) {
                        self.active_request.active_tab = RequestTab::Params;
                    }
                    if draw_underline_tab(ui, "Headers", self.active_request.active_tab == RequestTab::Headers) {
                        self.active_request.active_tab = RequestTab::Headers;
                    }
                    if draw_underline_tab(ui, "GraphQL", self.active_request.active_tab == RequestTab::GraphQL) {
                        self.active_request.active_tab = RequestTab::GraphQL;
                    }
                });

                // Tab underline separator
                let sep_rect = ui.allocate_space(egui::vec2(ui.available_width(), 1.0));
                ui.painter().rect_filled(sep_rect.1, 0.0, theme::COLOR_OUTLINE_VARIANT.linear_multiply(0.2));

                ui.add_space(8.0);

                // ─── Tab Content ─────────────────────────────────────
                egui::ScrollArea::vertical().id_source("request_tab_scroll").show(ui, |ui| {
                    match self.active_request.active_tab {
                        RequestTab::Body => {
                            self.render_body_tab(ui);
                        }
                        RequestTab::Params => {
                            self.render_params_tab(ui);
                        }
                        RequestTab::Headers => {
                            self.render_headers_tab(ui);
                        }
                        RequestTab::GraphQL => {
                            self.render_graphql_tab(ui);
                        }
                    }
                });
            });

        // ─── Response Split Pane (Bottom Half) ──────────────────────
        if self.last_response.is_some() || self.is_loading {
            // Separator bar
            let sep_full = ui.allocate_space(egui::vec2(ui.available_width(), 1.0));
            ui.painter().rect_filled(sep_full.1, 0.0, theme::COLOR_OUTLINE_VARIANT.linear_multiply(0.3));

            self.render_response_pane(ui, ctx);
        }
    }

    // ─── Body Tab ───────────────────────────────────────────────────
    fn render_body_tab(&mut self, ui: &mut egui::Ui) {
        theme::code_frame().show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.active_request.body)
                    .font(egui::TextStyle::Monospace)
                    .text_color(theme::COLOR_ON_SURFACE)
                    .desired_width(ui.available_width())
                    .desired_rows(8)
                    .hint_text(r#"{"key": "value"}"#)
            );
        });
    }

    // ─── Params Tab ─────────────────────────────────────────────────
    fn render_params_tab(&mut self, ui: &mut egui::Ui) {
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("Query Parameters").color(theme::COLOR_ON_SURFACE).strong().size(14.0));
            ui.add_space(8.0);

            let mut to_remove = None;
            let mut params_changed = false;

            for (idx, p) in self.active_request.params.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut p.active, "").changed() {
                        params_changed = true;
                    }
                    if ui.add(
                        egui::TextEdit::singleline(&mut p.key)
                            .font(egui::FontId::monospace(13.0))
                            .text_color(theme::COLOR_PRIMARY)
                            .desired_width(ui.available_width() * 0.35)
                            .hint_text("key")
                    ).changed() {
                        params_changed = true;
                    }
                    ui.label(egui::RichText::new("=").color(theme::COLOR_OUTLINE).size(14.0));
                    if ui.add(
                        egui::TextEdit::singleline(&mut p.value)
                            .font(egui::FontId::monospace(13.0))
                            .text_color(theme::COLOR_ON_SURFACE)
                            .desired_width(ui.available_width() - 40.0)
                            .hint_text("value")
                    ).changed() {
                        params_changed = true;
                    }
                    if ui.add(egui::Button::new(
                        egui::RichText::new("x").color(theme::COLOR_OUTLINE).size(11.0)
                    ).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE).min_size(egui::vec2(20.0, 20.0))).clicked() {
                        to_remove = Some(idx);
                        params_changed = true;
                    }
                });
            }

            if let Some(idx) = to_remove {
                self.active_request.params.remove(idx);
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_param_key)
                        .font(egui::FontId::monospace(13.0))
                        .hint_text("key")
                        .desired_width(ui.available_width() * 0.35)
                );
                ui.label(egui::RichText::new("=").color(theme::COLOR_OUTLINE).size(14.0));
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_param_val)
                        .font(egui::FontId::monospace(13.0))
                        .hint_text("value")
                        .desired_width(ui.available_width() - 140.0)
                );
                if theme::draw_outlined_button(ui, "⊕ Add").clicked() {
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

            if params_changed {
                self.active_request.url = rebuild_url_with_params(&self.active_request.url, &self.active_request.params);
                self.last_synced_url = self.active_request.url.clone();
            }
        });
    }

    // ─── Headers Tab ────────────────────────────────────────────────
    fn render_headers_tab(&mut self, ui: &mut egui::Ui) {
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("HTTP Headers").color(theme::COLOR_ON_SURFACE).strong().size(14.0));
            ui.add_space(8.0);

            let mut to_remove = None;
            for (idx, h) in self.active_request.headers.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut h.active, "");
                    ui.add(
                        egui::TextEdit::singleline(&mut h.key)
                            .font(egui::FontId::monospace(13.0))
                            .text_color(theme::COLOR_PRIMARY)
                            .desired_width(ui.available_width() * 0.35)
                    );
                    ui.label(egui::RichText::new(":").color(theme::COLOR_OUTLINE));
                    ui.add(
                        egui::TextEdit::singleline(&mut h.value)
                            .font(egui::FontId::monospace(13.0))
                            .text_color(theme::COLOR_SECONDARY)
                            .desired_width(ui.available_width() - 40.0)
                    );
                    if ui.add(egui::Button::new(
                        egui::RichText::new("x").color(theme::COLOR_OUTLINE).size(11.0)
                    ).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE).min_size(egui::vec2(20.0, 20.0))).clicked() {
                        to_remove = Some(idx);
                    }
                });
            }
            if let Some(idx) = to_remove {
                self.active_request.headers.remove(idx);
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_header_key)
                        .font(egui::FontId::monospace(13.0))
                        .hint_text("Header-Name")
                        .desired_width(ui.available_width() * 0.35)
                );
                ui.label(egui::RichText::new(":").color(theme::COLOR_OUTLINE));
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_header_val)
                        .font(egui::FontId::monospace(13.0))
                        .hint_text("value")
                        .desired_width(ui.available_width() - 140.0)
                );
                if theme::draw_outlined_button(ui, "⊕ Add").clicked() {
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
        });
    }

    // ─── GraphQL Tab ────────────────────────────────────────────────
    fn render_graphql_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("GraphQL Query").color(theme::COLOR_ON_SURFACE).strong().size(14.0));
        ui.add_space(4.0);
        theme::code_frame().show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.active_request.graphql_query)
                    .font(egui::TextStyle::Monospace)
                    .text_color(theme::COLOR_PRIMARY)
                    .desired_width(ui.available_width())
                    .desired_rows(6)
                    .hint_text("query GetUsers {\n  users {\n    id\n    name\n  }\n}")
            );
        });

        ui.add_space(8.0);
        ui.label(egui::RichText::new("Variables (JSON)").color(theme::COLOR_ON_SURFACE).strong().size(14.0));
        ui.add_space(4.0);
        theme::code_frame().show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.active_request.graphql_variables)
                    .font(egui::TextStyle::Monospace)
                    .text_color(theme::COLOR_SECONDARY)
                    .desired_width(ui.available_width())
                    .desired_rows(4)
                    .hint_text(r#"{"id": 1}"#)
            );
        });
    }

    // ─── Send Request Logic ─────────────────────────────────────────
    fn send_request(&mut self, _ctx: &egui::Context) {
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

        let active_env = self.active_env_idx.and_then(|idx| self.environments.get(idx).cloned());
        let substituted_url = substitute_variables(&self.active_request.url, &active_env);
        let substituted_body = substitute_variables(&final_body, &active_env);

        let mut substituted_headers = Vec::new();
        for h in final_headers {
            substituted_headers.push(KeyValue {
                key: h.key.clone(),
                value: substitute_variables(&h.value, &active_env),
                active: h.active,
            });
        }

        // Get execution settings from active environment
        let (timeout_ms, follow_redirects, ssl_verification) = match active_env {
            Some(ref env) => (env.timeout_ms, env.follow_redirects, env.ssl_verification),
            None => (30000, true, true),
        };

        let http_req = HttpRequest {
            id: self.active_request.id.clone(),
            method: final_method,
            url: substituted_url,
            headers: substituted_headers,
            body: substituted_body,
            timeout_ms,
            follow_redirects,
            ssl_verification,
        };

        // Save original request to history
        let saved_req = self.active_request.to_saved();
        self.history.retain(|r| r.id != saved_req.id);
        self.history.insert(0, saved_req);
        if self.history.len() > 50 {
            self.history.truncate(50);
        }
        let _ = self.storage.save_history(&self.history);

        self.active_rx = Some(self.client.send(http_req));
        self.is_loading = true;
    }

    // ═══════════════════════════════════════════════════════════════
    // RESPONSE PANE (Bottom split)
    // ═══════════════════════════════════════════════════════════════
    fn render_response_pane(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Frame::none()
            .fill(theme::COLOR_SURFACE_CONTAINER_LOW)
            .inner_margin(egui::Margin::same(0.0))
            .show(ui, |ui| {
                if self.is_loading {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.add(egui::Spinner::new().size(28.0));
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("Sending Request...")
                                .color(theme::COLOR_ON_SURFACE_VARIANT)
                                .size(13.0)
                        );
                    });
                    return;
                }

                if let Some(ref response_result) = self.last_response {
                    match response_result {
                        Ok(res) => {
                            // ─── Response Metadata Bar ───────────
                            egui::Frame::none()
                                .fill(theme::COLOR_SURFACE_CONTAINER_LOW)
                                .stroke(egui::Stroke::new(0.5, theme::COLOR_OUTLINE_VARIANT.linear_multiply(0.1)))
                                .inner_margin(egui::Margin::symmetric(16.0, 8.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        // "RESPONSE" label
                                        ui.label(
                                            egui::RichText::new("RESPONSE")
                                                .color(theme::COLOR_OUTLINE)
                                                .strong()
                                                .size(11.0)
                                                .family(egui::FontFamily::Monospace)
                                        );

                                        ui.add_space(12.0);

                                        // Status dot + badge
                                        let status_color = if res.status >= 200 && res.status < 300 {
                                            theme::COLOR_SECONDARY
                                        } else if res.status >= 400 {
                                            theme::COLOR_ERROR
                                        } else {
                                            theme::COLOR_TERTIARY
                                        };

                                        let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                                        ui.painter().circle_filled(dot_rect.center(), 3.5, status_color);

                                        ui.label(
                                            egui::RichText::new(format!("{} {}", res.status, res.status_text))
                                                .color(status_color)
                                                .strong()
                                                .size(12.0)
                                                .family(egui::FontFamily::Monospace)
                                        );

                                        ui.add_space(12.0);

                                        // Timing
                                        ui.label(
                                            egui::RichText::new(format!("{}ms", res.elapsed_ms))
                                                .color(theme::COLOR_ON_SURFACE_VARIANT)
                                                .size(12.0)
                                                .family(egui::FontFamily::Monospace)
                                        );

                                        ui.add_space(8.0);

                                        // Size
                                        let kb_size = res.size_bytes as f32 / 1024.0;
                                        ui.label(
                                            egui::RichText::new(format!("{:.1} KB", kb_size))
                                                .color(theme::COLOR_ON_SURFACE_VARIANT)
                                                .size(12.0)
                                                .family(egui::FontFamily::Monospace)
                                        );

                                        // Right side: response tabs + copy
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if theme::draw_icon_button(ui, "📋").clicked() {
                                                ctx.output_mut(|o| o.copied_text = res.body.clone());
                                            }

                                            ui.add_space(8.0);

                                            // Response tabs
                                            let body_selected = self.response_tab == ResponseTab::Body;
                                            let headers_selected = self.response_tab == ResponseTab::Headers;

                                            if ui.add(egui::Button::new(
                                                egui::RichText::new("HEADERS")
                                                    .color(if headers_selected { theme::COLOR_PRIMARY } else { theme::COLOR_ON_SURFACE_VARIANT })
                                                    .size(10.0)
                                                    .family(egui::FontFamily::Monospace)
                                            ).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE)).clicked() {
                                                self.response_tab = ResponseTab::Headers;
                                            }

                                            if ui.add(egui::Button::new(
                                                egui::RichText::new("BODY")
                                                    .color(if body_selected { theme::COLOR_PRIMARY } else { theme::COLOR_ON_SURFACE_VARIANT })
                                                    .size(10.0)
                                                    .family(egui::FontFamily::Monospace)
                                            ).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE)).clicked() {
                                                self.response_tab = ResponseTab::Body;
                                            }
                                        });
                                    });
                                });

                            // ─── Response Content ────────────────
                            egui::ScrollArea::both().id_source("response_scroll").show(ui, |ui| {
                                egui::Frame::none()
                                    .inner_margin(egui::Margin::same(16.0))
                                    .show(ui, |ui| {
                                        match self.response_tab {
                                            ResponseTab::Body => {
                                                if res.content_type.starts_with("image/") {
                                                    ctx.include_bytes("bytes://response_image", res.body_bytes.clone());
                                                    ui.vertical_centered(|ui| {
                                                        ui.add(
                                                            egui::Image::new("bytes://response_image")
                                                                .max_width(ui.available_width() - 24.0)
                                                                .max_height(350.0)
                                                                .rounding(egui::Rounding::same(8.0))
                                                        );
                                                    });
                                                } else {
                                                    ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(&res.body)
                                                                .font(egui::FontId::monospace(13.0))
                                                                .color(theme::COLOR_ON_SURFACE)
                                                        )
                                                        .selectable(true)
                                                        .wrap(false)
                                                    );
                                                }
                                            }
                                            ResponseTab::Headers => {
                                                for (key, value) in &res.headers {
                                                    ui.horizontal(|ui| {
                                                        ui.label(
                                                            egui::RichText::new(key)
                                                                .color(theme::COLOR_PRIMARY)
                                                                .size(13.0)
                                                                .family(egui::FontFamily::Monospace)
                                                        );
                                                        ui.label(
                                                            egui::RichText::new(":")
                                                                .color(theme::COLOR_OUTLINE)
                                                                .size(13.0)
                                                        );
                                                        ui.label(
                                                            egui::RichText::new(value)
                                                                .color(theme::COLOR_SECONDARY)
                                                                .size(13.0)
                                                                .family(egui::FontFamily::Monospace)
                                                        );
                                                    });
                                                }
                                            }
                                        }
                                    });
                            });
                        }
                        Err(err) => {
                            egui::Frame::none()
                                .fill(theme::COLOR_ERROR.linear_multiply(0.05))
                                .rounding(egui::Rounding::same(10.0))
                                .inner_margin(egui::Margin::same(24.0))
                                .show(ui, |ui| {
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            egui::RichText::new("⚠ Connection Error")
                                                .color(theme::COLOR_ERROR)
                                                .strong()
                                                .size(16.0)
                                        );
                                        ui.add_space(8.0);
                                        ui.label(
                                            egui::RichText::new(err.as_str())
                                                .color(theme::COLOR_ON_SURFACE_VARIANT)
                                                .size(13.0)
                                                .family(egui::FontFamily::Monospace)
                                        );
                                    });
                                });
                        }
                    }
                }
            });
    }

    fn render_history_panel(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none()
            .fill(theme::COLOR_BACKGROUND)
            .inner_margin(egui::Margin::same(24.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(
                        egui::RichText::new("History Log")
                            .color(theme::COLOR_ON_SURFACE)
                            .size(22.0)
                            .strong()
                    );
                    ui.label(
                        egui::RichText::new(format!("• {} requests", self.history.len()))
                            .color(theme::COLOR_ON_SURFACE_VARIANT)
                            .size(13.0)
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::draw_outlined_button(ui, "Clear History").clicked() {
                            self.history.clear();
                            let _ = self.storage.save_history(&self.history);
                        }
                    });
                });

                ui.add_space(16.0);

                if self.history.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.label(egui::RichText::new("No requests in history yet.").color(theme::COLOR_ON_SURFACE_VARIANT).size(14.0));
                        ui.label(egui::RichText::new("Send a request in the Builder to see it logged here.").color(theme::COLOR_OUTLINE).size(11.0));
                    });
                    return;
                }

                egui::ScrollArea::vertical().id_source("history_panel_scroll").show(ui, |ui| {
                    let mut req_to_load = None;
                    let mut req_to_remove = None;

                    for (idx, req) in self.history.iter().enumerate() {
                        theme::card_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                theme::draw_method_badge(ui, &req.method);
                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new(&req.url)
                                        .color(theme::COLOR_ON_SURFACE)
                                        .size(13.0)
                                        .family(egui::FontFamily::Monospace)
                                );

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if theme::draw_primary_button(ui, "Load").clicked() {
                                        req_to_load = Some(req.clone());
                                    }
                                    ui.add_space(8.0);
                                    if theme::draw_outlined_button(ui, "x Remove").clicked() {
                                        req_to_remove = Some(idx);
                                    }
                                });
                            });
                            
                            if !req.name.is_empty() && req.name != "Untitled Request" {
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(format!("Name: {}", req.name))
                                        .color(theme::COLOR_ON_SURFACE_VARIANT)
                                        .size(11.0)
                                );
                            }
                        });
                        ui.add_space(8.0);
                    }

                    if let Some(req) = req_to_load {
                        self.active_request = RequestEditorState::from_saved(&req);
                        self.last_response = None;
                        self.active_panel = ActivePanel::Request;
                        self.last_synced_url = self.active_request.url.clone();
                    }

                    if let Some(idx) = req_to_remove {
                        self.history.remove(idx);
                        let _ = self.storage.save_history(&self.history);
                    }
                });
            });
    }

    fn render_collections_panel(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none()
            .fill(theme::COLOR_BACKGROUND)
            .inner_margin(egui::Margin::same(24.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(
                        egui::RichText::new("Collections")
                            .color(theme::COLOR_ON_SURFACE)
                            .size(22.0)
                            .strong()
                    );
                    ui.label(
                        egui::RichText::new(format!("• {} collections", self.collections.len()))
                            .color(theme::COLOR_ON_SURFACE_VARIANT)
                            .size(13.0)
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::draw_primary_button(ui, "+ Create Collection").clicked() {
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

                ui.add_space(16.0);

                if self.collections.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.label(egui::RichText::new("No collections saved yet.").color(theme::COLOR_ON_SURFACE_VARIANT).size(14.0));
                        ui.label(egui::RichText::new("Create a collection to organize your API requests.").color(theme::COLOR_OUTLINE).size(11.0));
                    });
                    return;
                }

                egui::ScrollArea::vertical().id_source("collections_panel_scroll").show(ui, |ui| {
                    let mut col_to_delete_idx = None;
                    let mut col_to_save_idx = None;
                    let mut req_to_load = None;

                    for (col_idx, col) in self.collections.iter_mut().enumerate() {
                        theme::card_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("📁 {}", col.name))
                                        .color(theme::COLOR_TERTIARY)
                                        .strong()
                                        .size(15.0)
                                );

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if theme::draw_outlined_button(ui, "Delete").clicked() {
                                        col_to_delete_idx = Some(col_idx);
                                    }
                                    ui.add_space(8.0);
                                    if theme::draw_outlined_button(ui, "+ Add Request").clicked() {
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
                                        self.active_panel = ActivePanel::Request;
                                    }
                                    ui.add_space(8.0);
                                    // Rename collection inline input
                                    if ui.add(
                                        egui::TextEdit::singleline(&mut col.name)
                                            .font(egui::FontId::proportional(12.0))
                                            .desired_width(120.0)
                                    ).changed() {
                                        col_to_save_idx = Some(col_idx);
                                    }
                                });
                            });

                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(8.0);

                            // List items inside collection
                            if col.items.is_empty() {
                                ui.label(egui::RichText::new("Empty collection").color(theme::COLOR_ON_SURFACE_VARIANT).size(11.0).italics());
                            } else {
                                for item in &col.items {
                                    match item {
                                        CollectionItem::Request(req) => {
                                            ui.horizontal(|ui| {
                                                theme::draw_method_badge(ui, &req.method);
                                                ui.add_space(4.0);
                                                if ui.add(egui::Link::new(
                                                    egui::RichText::new(&req.name).size(12.0).color(theme::COLOR_PRIMARY)
                                                )).clicked() {
                                                    req_to_load = Some(req.clone());
                                                    self.selected_col_idx = col_idx;
                                                }
                                                ui.label(
                                                    egui::RichText::new(&req.url)
                                                        .color(theme::COLOR_ON_SURFACE_VARIANT)
                                                        .size(11.0)
                                                        .family(egui::FontFamily::Monospace)
                                                );
                                            });
                                        }
                                        CollectionItem::Folder(folder) => {
                                            ui.label(egui::RichText::new(format!("📁 Folder: {}", folder.name)).color(theme::COLOR_TERTIARY).size(12.0).strong());
                                            for sub_item in &folder.items {
                                                if let CollectionItem::Request(sub_req) = sub_item {
                                                    ui.horizontal(|ui| {
                                                        ui.add_space(12.0);
                                                        theme::draw_method_badge(ui, &sub_req.method);
                                                        ui.add_space(4.0);
                                                        if ui.add(egui::Link::new(
                                                            egui::RichText::new(&sub_req.name).size(11.0).color(theme::COLOR_PRIMARY)
                                                        )).clicked() {
                                                            req_to_load = Some(sub_req.clone());
                                                            self.selected_col_idx = col_idx;
                                                        }
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        });
                        ui.add_space(12.0);
                    }

                    if let Some(req) = req_to_load {
                        self.active_request = RequestEditorState::from_saved(&req);
                        self.last_response = None;
                        self.active_panel = ActivePanel::Request;
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
    }
}

