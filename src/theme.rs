use eframe::egui;

// Color definitions for a premium, sleek dark mode (Obsidian / Space Navy)
pub const COLOR_BG_MAIN: egui::Color32 = egui::Color32::from_rgb(11, 12, 14);       // Dark background
pub const COLOR_BG_SIDEBAR: egui::Color32 = egui::Color32::from_rgb(16, 18, 22);    // Slightly lighter sidebar
pub const COLOR_BG_INPUT: egui::Color32 = egui::Color32::from_rgb(20, 23, 30);      // Input fields
pub const COLOR_BORDER: egui::Color32 = egui::Color32::from_rgb(29, 33, 42);         // Sleek dividers/borders
pub const COLOR_PRIMARY: egui::Color32 = egui::Color32::from_rgb(94, 106, 210);     // Indigo accent
pub const COLOR_PRIMARY_HOVER: egui::Color32 = egui::Color32::from_rgb(112, 124, 230);

// Text colors
pub const COLOR_TEXT_ACTIVE: egui::Color32 = egui::Color32::from_rgb(230, 232, 234);
pub const COLOR_TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(139, 149, 165);

// Method colors (Postman style)
pub const COLOR_GET: egui::Color32 = egui::Color32::from_rgb(10, 191, 110);    // Emerald
pub const COLOR_POST: egui::Color32 = egui::Color32::from_rgb(255, 180, 0);   // Amber
pub const COLOR_PUT: egui::Color32 = egui::Color32::from_rgb(9, 132, 227);    // Blue
pub const COLOR_DELETE: egui::Color32 = egui::Color32::from_rgb(235, 77, 75);  // Red
pub const COLOR_PATCH: egui::Color32 = egui::Color32::from_rgb(155, 89, 182); // Purple

pub fn get_method_color(method: &str) -> egui::Color32 {
    match method.to_uppercase().as_str() {
        "GET" => COLOR_GET,
        "POST" => COLOR_POST,
        "PUT" => COLOR_PUT,
        "DELETE" => COLOR_DELETE,
        "PATCH" => COLOR_PATCH,
        _ => COLOR_PRIMARY,
    }
}

pub fn draw_method_pill(ui: &mut egui::Ui, method: &str) {
    let color = get_method_color(method);
    let bg_color = color.linear_multiply(0.12);
    
    egui::Frame::none()
        .fill(bg_color)
        .stroke(egui::Stroke::new(1.0, color.linear_multiply(0.6)))
        .rounding(egui::Rounding::same(4.0))
        .inner_margin(egui::Margin::symmetric(6.0, 2.0))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(method.to_uppercase())
                    .color(color)
                    .strong()
                    .size(10.0)
            );
        });
}

pub fn draw_custom_button(
    ui: &mut egui::Ui,
    label: &str,
    fill_color: egui::Color32,
    text_color: egui::Color32,
) -> egui::Response {
    let button = egui::Button::new(
        egui::RichText::new(label)
            .color(text_color)
            .strong()
    )
    .fill(fill_color)
    .stroke(egui::Stroke::new(1.0, fill_color.linear_multiply(0.8)))
    .min_size(egui::vec2(80.0, 28.0));
    
    ui.add(button)
}

pub fn paint_linear_gradient(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    left_color: egui::Color32,
    right_color: egui::Color32,
) {
    let mut mesh = egui::Mesh::default();
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.left_top(),
        uv: egui::epaint::WHITE_UV,
        color: left_color,
    });
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.left_bottom(),
        uv: egui::epaint::WHITE_UV,
        color: left_color,
    });
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.right_bottom(),
        uv: egui::epaint::WHITE_UV,
        color: right_color,
    });
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.right_top(),
        uv: egui::epaint::WHITE_UV,
        color: right_color,
    });
    mesh.indices.push(0);
    mesh.indices.push(1);
    mesh.indices.push(2);
    mesh.indices.push(0);
    mesh.indices.push(2);
    mesh.indices.push(3);
    ui.painter().add(egui::Shape::mesh(mesh));
}

pub fn draw_gradient_button(
    ui: &mut egui::Ui,
    label: &str,
    left_color: egui::Color32,
    right_color: egui::Color32,
) -> egui::Response {
    let desired_size = egui::vec2(90.0, 30.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
    
    if ui.is_rect_visible(rect) {
        paint_linear_gradient(ui, rect, left_color, right_color);
        
        let stroke_color = if response.hovered() {
            egui::Color32::WHITE
        } else {
            left_color.linear_multiply(0.8)
        };
        ui.painter().rect_stroke(rect, egui::Rounding::same(6.0), egui::Stroke::new(1.0, stroke_color));
        
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::new(13.0, egui::FontFamily::Proportional),
            egui::Color32::WHITE,
        );
    }
    
    response
}

pub fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // 1. Spacing & Rounding
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.visuals.window_rounding = egui::Rounding::same(8.0);
    style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(6.0);
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(6.0);
    style.visuals.widgets.hovered.rounding = egui::Rounding::same(6.0);
    style.visuals.widgets.active.rounding = egui::Rounding::same(6.0);

    // 2. Custom Visuals / Dark Mode Colors override
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = COLOR_BG_MAIN;
    visuals.extreme_bg_color = COLOR_BG_INPUT; // text edit fields background
    visuals.widgets.noninteractive.bg_fill = COLOR_BG_MAIN;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, COLOR_TEXT_MUTED);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, COLOR_BORDER);

    // Inactive elements (e.g. standard buttons)
    visuals.widgets.inactive.bg_fill = COLOR_BG_SIDEBAR;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, COLOR_TEXT_ACTIVE);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, COLOR_BORDER);

    // Hovered elements
    visuals.widgets.hovered.bg_fill = COLOR_BORDER;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, COLOR_PRIMARY);

    // Active elements (clicked buttons, selected tabs)
    visuals.widgets.active.bg_fill = COLOR_PRIMARY;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, COLOR_PRIMARY_HOVER);

    style.visuals = visuals;
    ctx.set_style(style);

    // 3. Fonts Configuration
    // We adjust sizes of standard proportional and monospace fonts for modern hierarchy
    let mut style = (*ctx.style()).clone();
    
    // Set custom sizes for font styles
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(20.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(11.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        egui::FontId::new(13.0, egui::FontFamily::Monospace),
    );

    ctx.set_style(style);
}
