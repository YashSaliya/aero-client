#![allow(dead_code)]
use eframe::egui;

pub const COLOR_BACKGROUND: egui::Color32      = egui::Color32::from_rgb(11, 19, 38);      // #0b1326
pub const COLOR_SURFACE: egui::Color32         = egui::Color32::from_rgb(11, 19, 38);      // #0b1326
pub const COLOR_SURFACE_CONTAINER: egui::Color32 = egui::Color32::from_rgb(23, 31, 51);   // #171f33
pub const COLOR_SURFACE_CONTAINER_LOW: egui::Color32 = egui::Color32::from_rgb(19, 27, 46); // #131b2e
pub const COLOR_SURFACE_CONTAINER_HIGH: egui::Color32 = egui::Color32::from_rgb(34, 42, 61); // #222a3d
pub const COLOR_SURFACE_CONTAINER_HIGHEST: egui::Color32 = egui::Color32::from_rgb(45, 52, 73); // #2d3449
pub const COLOR_SURFACE_CONTAINER_LOWEST: egui::Color32 = egui::Color32::from_rgb(6, 14, 32); // #060e20
pub const COLOR_SURFACE_BRIGHT: egui::Color32  = egui::Color32::from_rgb(49, 57, 77);      // #31394d
pub const COLOR_SURFACE_VARIANT: egui::Color32 = egui::Color32::from_rgb(45, 52, 73);      // #2d3449

// ─── Accent Colors ───────────────────────────────────────────────────────────
pub const COLOR_PRIMARY: egui::Color32           = egui::Color32::from_rgb(208, 188, 255);  // #d0bcff
pub const COLOR_PRIMARY_CONTAINER: egui::Color32 = egui::Color32::from_rgb(160, 120, 255);  // #a078ff
pub const COLOR_ON_PRIMARY: egui::Color32        = egui::Color32::from_rgb(60, 0, 145);     // #3c0091
pub const COLOR_SECONDARY: egui::Color32         = egui::Color32::from_rgb(78, 222, 163);   // #4edea3
pub const COLOR_SECONDARY_CONTAINER: egui::Color32 = egui::Color32::from_rgb(0, 165, 114); // #00a572
pub const COLOR_TERTIARY: egui::Color32          = egui::Color32::from_rgb(255, 185, 95);   // #ffb95f
pub const COLOR_ERROR: egui::Color32             = egui::Color32::from_rgb(255, 180, 171);  // #ffb4ab
pub const COLOR_ERROR_CONTAINER: egui::Color32   = egui::Color32::from_rgb(147, 0, 10);     // #93000a

// ─── Text Colors ─────────────────────────────────────────────────────────────
pub const COLOR_ON_SURFACE: egui::Color32         = egui::Color32::from_rgb(218, 226, 253); // #dae2fd
pub const COLOR_ON_SURFACE_VARIANT: egui::Color32 = egui::Color32::from_rgb(203, 195, 215); // #cbc3d7
pub const COLOR_OUTLINE: egui::Color32            = egui::Color32::from_rgb(149, 142, 160); // #958ea0
pub const COLOR_OUTLINE_VARIANT: egui::Color32    = egui::Color32::from_rgb(73, 68, 84);    // #494454

// ─── Refined Border (~rgba(255,255,255,0.06)) ────────────────────────────────
pub const COLOR_BORDER_SUBTLE: egui::Color32 = egui::Color32::from_rgba_premultiplied(15, 15, 15, 255);
// Visible border for cards/containers
pub const COLOR_BORDER: egui::Color32 = egui::Color32::from_rgb(35, 40, 55);

// ─── Glass Surface (approximate rgba(15,23,42,0.7)) ─────────────────────────
pub const COLOR_GLASS: egui::Color32 = egui::Color32::from_rgba_premultiplied(11, 16, 29, 178);

// ─── HTTP Method Colors (Stitch Design System) ──────────────────────────────
pub const COLOR_GET: egui::Color32    = egui::Color32::from_rgb(78, 222, 163);   // #4edea3 (secondary/mint)
pub const COLOR_POST: egui::Color32   = egui::Color32::from_rgb(139, 92, 246);   // #8B5CF6 (vivid purple)
pub const COLOR_PUT: egui::Color32    = egui::Color32::from_rgb(255, 185, 95);   // #ffb95f (tertiary/amber)
pub const COLOR_DELETE: egui::Color32 = egui::Color32::from_rgb(255, 180, 171);  // #ffb4ab (error/coral)
pub const COLOR_PATCH: egui::Color32  = egui::Color32::from_rgb(160, 120, 255);  // #a078ff (primary-container)

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

/// Draw a compact method pill with Stitch-style rounded corners and translucent bg
pub fn draw_method_pill(ui: &mut egui::Ui, method: &str) {
    let color = get_method_color(method);
    let bg_color = egui::Color32::from_rgba_premultiplied(
        (color.r() as u16 * 30 / 255) as u8,
        (color.g() as u16 * 30 / 255) as u8,
        (color.b() as u16 * 30 / 255) as u8,
        255,
    );

    egui::Frame::none()
        .fill(bg_color)
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

/// Draw an inline method badge for sidebar items (compact, code-style)
pub fn draw_method_badge(ui: &mut egui::Ui, method: &str) {
    let color = get_method_color(method);
    egui::Frame::none()
        .fill(COLOR_BACKGROUND)
        .rounding(egui::Rounding::same(3.0))
        .inner_margin(egui::Margin::symmetric(4.0, 1.0))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(method.to_uppercase())
                    .color(color)
                    .size(10.0)
                    .family(egui::FontFamily::Monospace)
            );
        });
}

/// Draw a primary action button matching Stitch design (solid primary bg)
pub fn draw_primary_button(
    ui: &mut egui::Ui,
    label: &str,
) -> egui::Response {
    let button = egui::Button::new(
        egui::RichText::new(label)
            .color(COLOR_ON_PRIMARY)
            .strong()
            .size(11.0)
    )
    .fill(COLOR_PRIMARY)
    .stroke(egui::Stroke::NONE)
    .rounding(egui::Rounding::same(10.0))
    .min_size(egui::vec2(80.0, 34.0));

    ui.add(button)
}

/// Draw a secondary outlined button
pub fn draw_outlined_button(
    ui: &mut egui::Ui,
    label: &str,
) -> egui::Response {
    let button = egui::Button::new(
        egui::RichText::new(label)
            .color(COLOR_PRIMARY)
            .strong()
            .size(11.0)
    )
    .fill(egui::Color32::TRANSPARENT)
    .stroke(egui::Stroke::new(1.0, COLOR_OUTLINE_VARIANT))
    .rounding(egui::Rounding::same(10.0))
    .min_size(egui::vec2(80.0, 34.0));

    ui.add(button)
}

/// Draw an icon-style small button (for toolbar actions)
pub fn draw_icon_button(
    ui: &mut egui::Ui,
    icon: &str,
) -> egui::Response {
    let button = egui::Button::new(
        egui::RichText::new(icon)
            .color(COLOR_ON_SURFACE_VARIANT)
            .size(14.0)
    )
    .fill(egui::Color32::TRANSPARENT)
    .stroke(egui::Stroke::NONE)
    .min_size(egui::vec2(28.0, 28.0));

    ui.add(button)
}

/// Draw a section label like "RECENT REQUESTS" or "COLLECTIONS"
pub fn draw_section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .color(COLOR_OUTLINE)
            .strong()
            .size(11.0)
            .family(egui::FontFamily::Monospace)
    );
}

/// Draw a glass-surface card frame
pub fn glass_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(COLOR_GLASS)
        .stroke(egui::Stroke::new(0.5, egui::Color32::from_rgba_premultiplied(255, 255, 255, 25)))
        .rounding(egui::Rounding::same(10.0))
        .inner_margin(egui::Margin::same(16.0))
}

/// Draw a card frame for content sections
pub fn card_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(COLOR_SURFACE_CONTAINER_LOW)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(255, 255, 255, 15)))
        .rounding(egui::Rounding::same(10.0))
        .inner_margin(egui::Margin::same(16.0))
}

/// Draw a code editor area frame (darkest surface)
pub fn code_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(COLOR_SURFACE_CONTAINER_LOWEST)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(255, 255, 255, 12)))
        .rounding(egui::Rounding::same(10.0))
        .inner_margin(egui::Margin::same(12.0))
}

/// Draw a status badge (200 OK, 404 etc.)
pub fn draw_status_badge(ui: &mut egui::Ui, status: u16, text: &str) {
    let color = if status >= 200 && status < 300 {
        COLOR_SECONDARY
    } else if status >= 400 {
        COLOR_ERROR
    } else {
        COLOR_TERTIARY
    };

    ui.horizontal(|ui| {
        // Status dot
        let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        ui.painter().circle_filled(dot_rect.center(), 3.5, color);

        // Status text
        ui.label(
            egui::RichText::new(text)
                .color(color)
                .strong()
                .size(12.0)
                .family(egui::FontFamily::Monospace)
        );
    });
}

/// Draw a linear gradient mesh on a given rect
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

/// Apply the complete Stitch-inspired theme
pub fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // 1. Spacing & Rounding
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.visuals.window_rounding = egui::Rounding::same(10.0);
    style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(8.0);
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);
    style.visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);
    style.visuals.widgets.active.rounding = egui::Rounding::same(8.0);

    // 2. Custom Visuals — Deep Navy Dark Mode
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = COLOR_BACKGROUND;
    visuals.extreme_bg_color = COLOR_SURFACE_CONTAINER_LOWEST; // text edit bg
    visuals.faint_bg_color = COLOR_SURFACE_CONTAINER;

    // Non-interactive
    visuals.widgets.noninteractive.bg_fill = COLOR_BACKGROUND;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, COLOR_ON_SURFACE_VARIANT);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(0.5, COLOR_OUTLINE_VARIANT);

    // Inactive (buttons, inputs at rest)
    visuals.widgets.inactive.bg_fill = COLOR_SURFACE_CONTAINER_HIGH;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, COLOR_ON_SURFACE);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(0.5, COLOR_OUTLINE_VARIANT);

    // Hovered
    visuals.widgets.hovered.bg_fill = COLOR_SURFACE_VARIANT;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, COLOR_ON_SURFACE);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, COLOR_PRIMARY.linear_multiply(0.5));

    // Active / Pressed
    visuals.widgets.active.bg_fill = COLOR_PRIMARY_CONTAINER;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, COLOR_PRIMARY);

    // Selection
    visuals.selection.bg_fill = COLOR_PRIMARY_CONTAINER.linear_multiply(0.3);
    visuals.selection.stroke = egui::Stroke::new(1.0, COLOR_PRIMARY);

    // Separator
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(0.5, COLOR_OUTLINE_VARIANT.linear_multiply(0.3));

    style.visuals = visuals;

    // 3. Font Hierarchy
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
        egui::FontId::new(13.0, egui::FontFamily::Proportional),
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
