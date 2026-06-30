//! GNOME-style desktop widgets — notifications, alt-tab switcher, power dialog,
//! calendar dropdown, quick-settings toggle grid, and battery indicator.

use crate::graphics::framebuffer::{self, Color, Rect};
use crate::graphics::get_default_font;
use alloc::format;
use core::cmp::min;
use heapless::{String as HString, Vec};

use super::window_manager::{colors, WindowId, MENU_BAR_HEIGHT};

pub const MAX_NOTIFICATIONS: usize = 16;
pub const NOTIF_BODY_LEN: usize = 128;
pub const BANNER_TIMEOUT_S: u64 = 5;

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u32,
    pub app_name: HString<32>,
    pub summary: HString<64>,
    pub body: HString<NOTIF_BODY_LEN>,
    pub timestamp_s: u64,
    pub read: bool,
}

pub struct NotificationSystem {
    pub notifications: Vec<Notification, MAX_NOTIFICATIONS>,
    pub next_id: u32,
    pub banner_id: Option<u32>,
    pub banner_until: u64,
    pub do_not_disturb: bool,
}

impl NotificationSystem {
    pub const fn new() -> Self {
        Self {
            notifications: Vec::new(),
            next_id: 1,
            banner_id: None,
            banner_until: 0,
            do_not_disturb: false,
        }
    }

    pub fn push(&mut self, app: &str, summary: &str, body: &str) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let now = crate::time::uptime_ms() / 1000;
        let mut notif = Notification {
            id, app_name: HString::new(), summary: HString::new(),
            body: HString::new(), timestamp_s: now, read: false,
        };
        let _ = notif.app_name.push_str(app);
        let _ = notif.summary.push_str(summary);
        let _ = notif.body.push_str(body);
        let _ = self.notifications.insert(0, notif);
        if self.notifications.len() > MAX_NOTIFICATIONS {
            self.notifications.truncate(MAX_NOTIFICATIONS);
        }
        if !self.do_not_disturb {
            self.banner_id = Some(id);
            self.banner_until = now + BANNER_TIMEOUT_S;
        }
        id
    }

    pub fn dismiss_banner(&mut self) { self.banner_id = None; self.banner_until = 0; }
    pub fn clear_all(&mut self) { self.notifications.clear(); self.dismiss_banner(); }

    pub fn mark_all_read(&mut self) {
        for n in &mut self.notifications { n.read = true; }
    }

    pub fn unread_count(&self) -> usize {
        self.notifications.iter().filter(|n| !n.read).count()
    }

    pub fn tick(&mut self) {
        let now = crate::time::uptime_ms() / 1000;
        if self.banner_until != 0 && now >= self.banner_until {
            self.dismiss_banner();
        }
    }
}

fn fill_rounded_rect(rect: Rect, color: Color, _radius: usize) {
    framebuffer::fill_rect(rect, color);
}

pub fn render_banner(ns: &NotificationSystem, screen_w: usize) {
    let banner_id = match ns.banner_id { Some(id) => id, None => return };
    let notif = match ns.notifications.iter().find(|n| n.id == banner_id) { Some(n) => n, None => return };
    let font = get_default_font();
    let sw = notif.summary.len() * font.char_width;
    let bw = notif.body.len() * font.char_width;
    let w = (sw.max(bw) + 48).min(screen_w - 80);
    let h = 56;
    let x = screen_w.saturating_sub(w) / 2;
    let y = MENU_BAR_HEIGHT + 8;
    let rect = Rect::new(x, y, w, h);
    framebuffer::fill_rect(Rect::new(x + 3, y + 3, w, h), Color::new(0, 0, 0, 80));
    fill_rounded_rect(rect, Color::rgb(38, 38, 42), 8);
    framebuffer::draw_rect(rect, Color::rgb(60, 60, 66), 1);
    framebuffer::fill_rect(Rect::new(x, y + 4, 3, h - 8), colors::DOCK_ICON_ACCENT);
    crate::graphics::draw_text(notif.app_name.as_str(), x + 12, y + 8, Color::rgb(160, 160, 170), font);
    crate::graphics::draw_text(notif.summary.as_str(), x + 12, y + 22, colors::TEXT_COLOR_WHITE, font);
    crate::graphics::draw_text(notif.body.as_str(), x + 12, y + 38, Color::rgb(180, 180, 190), font);
}

// ---------------------------------------------------------------------------
// Calendar / Notification Center dropdown
// ---------------------------------------------------------------------------

pub fn render_calendar_dropdown(ns: &NotificationSystem, screen_w: usize, open: bool) {
    if !open { return; }
    let font = get_default_font();
    let pw = 360;
    let ph = 420;
    let px = screen_w.saturating_sub(pw + 8);
    let py = MENU_BAR_HEIGHT + 4;
    let panel = Rect::new(px, py, pw, ph);
    framebuffer::fill_rect(Rect::new(px + 4, py + 4, pw, ph), Color::new(0, 0, 0, 100));
    fill_rounded_rect(panel, Color::rgb(32, 32, 36), 10);
    framebuffer::draw_rect(panel, Color::rgb(55, 55, 62), 1);
    let now = crate::time::system_time();
    let dow = ((now / 86400) + 4) % 7;
    let dows = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    let mon = ((now / (86400 * 30)) + 1) % 12;
    let mons = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let dnum = (now / 86400) % 30 + 1;
    let date_str = format!("{} {} {}", dows[dow as usize], mons[mon as usize], dnum);
    crate::graphics::draw_text(&date_str, px + 16, py + 14, colors::TEXT_COLOR_WHITE, font);
    crate::graphics::draw_text("Notifications", px + 16, py + 44, Color::rgb(160, 160, 170), font);
    let clear_x = px + pw - 60;
    fill_rounded_rect(Rect::new(clear_x, py + 40, 44, 18), Color::rgb(50, 50, 56), 4);
    crate::graphics::draw_text("Clear", clear_x + 6, py + 42, Color::rgb(180, 180, 190), font);
    let mut ly = py + 66;
    let lb = py + ph - 120;
    for n in &ns.notifications {
        if ly + 44 > lb { break; }
        let ir = Rect::new(px + 12, ly, pw - 24, 40);
        let bg = if n.read { Color::rgb(38, 38, 44) } else { Color::rgb(44, 44, 52) };
        fill_rounded_rect(ir, bg, 6);
        if !n.read { framebuffer::fill_rect(Rect::new(px + 18, ly + 16, 6, 6), colors::DOCK_ICON_ACCENT); }
        crate::graphics::draw_text(n.summary.as_str(), px + 30, ly + 6, colors::TEXT_COLOR_WHITE, font);
        crate::graphics::draw_text(n.body.as_str(), px + 30, ly + 22, Color::rgb(170, 170, 180), font);
        ly += 46;
    }
    if ns.notifications.is_empty() {
        crate::graphics::draw_text("No notifications", px + 16, ly + 4, Color::rgb(120, 120, 130), font);
    }
    let cy = py + ph - 100;
    crate::graphics::draw_text("Calendar", px + 16, cy, Color::rgb(160, 160, 170), font);
    let dls = ["S", "M", "T", "W", "T", "F", "S"];
    let cw = (pw - 32) / 7;
    for (i, dl) in dls.iter().enumerate() {
        crate::graphics::draw_text(dl, px + 16 + i * cw + cw / 2 - 3, cy + 20, Color::rgb(120, 120, 130), font);
    }
    let today = dnum as usize;
    let ws = today.saturating_sub(today % 7);
    for i in 0..7 {
        let day = ws + i;
        if day == 0 || day > 31 { continue; }
        let ds = format!("{}", day);
        let cx = px + 16 + i * cw;
        if day == today {
            fill_rounded_rect(Rect::new(cx, cy + 38, cw - 2, 20), colors::DOCK_ICON_ACCENT, 4);
            crate::graphics::draw_text(&ds, cx + 4, cy + 40, colors::TEXT_COLOR_WHITE, font);
        } else {
            crate::graphics::draw_text(&ds, cx + 4, cy + 40, Color::rgb(180, 180, 190), font);
        }
    }
}

// ---------------------------------------------------------------------------
// Alt-Tab window switcher
// ---------------------------------------------------------------------------

pub struct AltTabSwitcher {
    pub open: bool,
    pub window_ids: Vec<WindowId, 64>,
    pub selected: usize,
}

impl AltTabSwitcher {
    pub const fn new() -> Self { Self { open: false, window_ids: Vec::new(), selected: 0 } }

    pub fn open(&mut self, windows: &[WindowId]) {
        self.window_ids.clear();
        for &w in windows { let _ = self.window_ids.push(w); }
        self.selected = 0;
        self.open = !self.window_ids.is_empty();
    }

    pub fn close(&mut self) { self.open = false; self.window_ids.clear(); self.selected = 0; }
    pub fn next(&mut self) { if !self.window_ids.is_empty() { self.selected = (self.selected + 1) % self.window_ids.len(); } }
    pub fn prev(&mut self) { if !self.window_ids.is_empty() { self.selected = (self.selected + self.window_ids.len() - 1) % self.window_ids.len(); } }
    pub fn current(&self) -> Option<WindowId> { self.window_ids.get(self.selected).copied() }
}

pub fn render_alt_tab(switcher: &AltTabSwitcher, titles: &[(WindowId, &str)], sw: usize, sh: usize) {
    if !switcher.open || switcher.window_ids.is_empty() { return; }
    let font = get_default_font();
    let iw = 140; let ih = 80; let gap = 12;
    let maxv = 6usize;
    let vis = min(switcher.window_ids.len(), maxv);
    let tw = vis * iw + (vis - 1) * gap + 32;
    let th = ih + 56;
    let x = sw.saturating_sub(tw) / 2;
    let y = sh.saturating_sub(th) / 2;
    framebuffer::fill_rect(Rect::new(0, 0, sw, sh), Color::new(0, 0, 0, 120));
    fill_rounded_rect(Rect::new(x, y, tw, th), Color::rgb(30, 30, 34), 12);
    framebuffer::draw_rect(Rect::new(x, y, tw, th), Color::rgb(55, 55, 62), 1);
    let start = switcher.selected / maxv * maxv;
    for i in 0..vis {
        let idx = start + i;
        if idx >= switcher.window_ids.len() { break; }
        let wid = switcher.window_ids[idx];
        let ix = x + 16 + i * (iw + gap);
        let iy = y + 16;
        let ir = Rect::new(ix, iy, iw, ih);
        let sel = idx == switcher.selected;
        fill_rounded_rect(ir, if sel { Color::rgb(55, 55, 65) } else { Color::rgb(40, 40, 46) }, 8);
        framebuffer::draw_rect(ir, if sel { colors::DOCK_ICON_ACCENT } else { Color::rgb(60, 60, 68) }, if sel { 2 } else { 1 });
        let title = titles.iter().find(|(id, _)| *id == wid).map(|(_, t)| *t).unwrap_or("Unknown");
        crate::graphics::draw_text(title, ix + 10, iy + 10, colors::TEXT_COLOR_WHITE, font);
        let pv = Rect::new(ix + 10, iy + 30, iw - 20, ih - 42);
        framebuffer::draw_rect(pv, Color::rgb(70, 70, 80), 1);
        let tb = Rect::new(pv.x, pv.y, pv.width, 12);
        framebuffer::fill_rect(tb, Color::rgb(80, 80, 90));
    }
}

// ---------------------------------------------------------------------------
// Power dialog (shutdown / restart / logoff)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction { Shutdown, Restart, Logoff, Cancel }

pub struct PowerDialog {
    pub open: bool,
    pub hovered: Option<PowerAction>,
}

impl PowerDialog {
    pub const fn new() -> Self { Self { open: false, hovered: None } }
    pub fn open(&mut self) { self.open = true; self.hovered = None; }
    pub fn close(&mut self) { self.open = false; self.hovered = None; }
}

pub fn render_power_dialog(dialog: &PowerDialog, sw: usize, sh: usize) {
    if !dialog.open { return; }
    let font = get_default_font();
    framebuffer::fill_rect(Rect::new(0, 0, sw, sh), Color::new(0, 0, 0, 140));
    let dw = 400; let dh = 200;
    let dx = sw.saturating_sub(dw) / 2;
    let dy = sh.saturating_sub(dh) / 2;
    fill_rounded_rect(Rect::new(dx, dy, dw, dh), Color::rgb(36, 36, 40), 12);
    framebuffer::draw_rect(Rect::new(dx, dy, dw, dh), Color::rgb(60, 60, 68), 1);
    crate::graphics::draw_text("Power Off", dx + 16, dy + 16, colors::TEXT_COLOR_WHITE, font);
    crate::graphics::draw_text("Choose an action:", dx + 16, dy + 44, Color::rgb(170, 170, 180), font);
    let bw = 100; let bh = 60; let bgap = 12;
    let bx_start = dx + 16;
    let by = dy + 80;
    let actions = [
        (PowerAction::Shutdown, "Shutdown", Color::rgb(200, 60, 50)),
        (PowerAction::Restart, "Restart", Color::rgb(80, 140, 220)),
        (PowerAction::Logoff, "Log Off", Color::rgb(120, 180, 80)),
    ];
    for (i, (action, label, acolor)) in actions.iter().enumerate() {
        let bx = bx_start + i * (bw + bgap);
        let br = Rect::new(bx, by, bw, bh);
        let is_hov = dialog.hovered == Some(*action);
        let bg = if is_hov { Color::rgb(55, 55, 65) } else { Color::rgb(44, 44, 52) };
        fill_rounded_rect(br, bg, 8);
        framebuffer::draw_rect(br, if is_hov { *acolor } else { Color::rgb(60, 60, 68) }, if is_hov { 2 } else { 1 });
        let lx = bx + (bw.saturating_sub(label.len() * font.char_width)) / 2;
        let ly = by + (bh.saturating_sub(font.char_height)) / 2;
        crate::graphics::draw_text(label, lx, ly, colors::TEXT_COLOR_WHITE, font);
    }
    let cancel_r = Rect::new(dx + dw - 90, dy + dh - 36, 74, 24);
    let ch = dialog.hovered == Some(PowerAction::Cancel);
    fill_rounded_rect(cancel_r, if ch { Color::rgb(55, 55, 65) } else { Color::rgb(44, 44, 52) }, 6);
    framebuffer::draw_rect(cancel_r, Color::rgb(60, 60, 68), 1);
    crate::graphics::draw_text("Cancel", cancel_r.x + 8, cancel_r.y + 5, Color::rgb(180, 180, 190), font);
}

pub fn power_dialog_action_at(dialog: &PowerDialog, sw: usize, sh: usize, x: usize, y: usize) -> Option<PowerAction> {
    if !dialog.open { return None; }
    let dw = 400; let dh = 200;
    let dx = sw.saturating_sub(dw) / 2;
    let dy = sh.saturating_sub(dh) / 2;
    let bw = 100; let bh = 60; let bgap = 12;
    let bx_start = dx + 16;
    let by = dy + 80;
    let actions = [PowerAction::Shutdown, PowerAction::Restart, PowerAction::Logoff];
    for (i, action) in actions.iter().enumerate() {
        let bx = bx_start + i * (bw + bgap);
        if Rect::new(bx, by, bw, bh).contains(x, y) { return Some(*action); }
    }
    let cancel_r = Rect::new(dx + dw - 90, dy + dh - 36, 74, 24);
    if cancel_r.contains(x, y) { return Some(PowerAction::Cancel); }
    None
}

// ---------------------------------------------------------------------------
// Quick-settings toggle grid (GNOME 43+ style)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleId { Wifi, Bluetooth, DarkMode, DoNotDisturb, Airplane, PowerSaver }

pub struct QuickToggle {
    pub id: ToggleId,
    pub label: &'static str,
    pub active: bool,
}

impl QuickToggle {
    pub const fn new(id: ToggleId, label: &'static str, active: bool) -> Self {
        Self { id, label, active }
    }
}

pub fn default_toggles(wifi_on: bool, bt_on: bool, dark: bool, dnd: bool) -> Vec<QuickToggle, 6> {
    let mut v = Vec::new();
    let _ = v.push(QuickToggle::new(ToggleId::Wifi, "Wi-Fi", wifi_on));
    let _ = v.push(QuickToggle::new(ToggleId::Bluetooth, "BT", bt_on));
    let _ = v.push(QuickToggle::new(ToggleId::DarkMode, "Dark", dark));
    let _ = v.push(QuickToggle::new(ToggleId::DoNotDisturb, "DND", dnd));
    let _ = v.push(QuickToggle::new(ToggleId::Airplane, "Airpl", false));
    let _ = v.push(QuickToggle::new(ToggleId::PowerSaver, "Saver", false));
    v
}

pub fn render_quick_toggles(
    toggles: &[QuickToggle],
    panel_x: usize, panel_y: usize, panel_w: usize,
) -> usize {
    let font = get_default_font();
    let cols = 2;
    let tw = (panel_w - 48) / cols;
    let th = 44;
    let gap = 8;
    let mut rendered_h = 0;
    for (i, tog) in toggles.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let tx = panel_x + 16 + col * (tw + gap);
        let ty = panel_y + 16 + row * (th + gap);
        let tr = Rect::new(tx, ty, tw, th);
        let bg = if tog.active { colors::DOCK_ICON_ACCENT } else { Color::rgb(50, 50, 56) };
        fill_rounded_rect(tr, bg, 8);
        let border = if tog.active { Color::rgb(255, 140, 60) } else { Color::rgb(65, 65, 72) };
        framebuffer::draw_rect(tr, border, 1);
        let lx = tx + 12;
        let ly = ty + (th.saturating_sub(font.char_height)) / 2;
        let tc = if tog.active { colors::TEXT_COLOR_WHITE } else { Color::rgb(180, 180, 190) };
        crate::graphics::draw_text(tog.label, lx, ly, tc, font);
        rendered_h = ty + th - panel_y + gap;
    }
    rendered_h
}

pub fn toggle_at_point(
    toggles: &[QuickToggle],
    panel_x: usize, panel_y: usize, panel_w: usize,
    x: usize, y: usize,
) -> Option<ToggleId> {
    let cols = 2;
    let tw = (panel_w - 48) / cols;
    let th = 44;
    let gap = 8;
    for (i, tog) in toggles.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let tx = panel_x + 16 + col * (tw + gap);
        let ty = panel_y + 16 + row * (th + gap);
        if Rect::new(tx, ty, tw, th).contains(x, y) {
            return Some(tog.id);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Battery indicator
// ---------------------------------------------------------------------------

pub struct BatteryState {
    pub present: bool,
    pub charging: bool,
    pub percent: u8,
}

impl BatteryState {
    pub const fn none() -> Self { Self { present: false, charging: false, percent: 0 } }
    pub const fn new(percent: u8, charging: bool) -> Self {
        Self { present: true, charging, percent }
    }
}

pub fn render_battery_indicator(
    bat: &BatteryState,
    x: usize, y: usize,
) -> usize {
    if !bat.present { return 0; }
    let font = get_default_font();
    let icon_w = 24;
    let icon_h = 14;
    let pct_str = format!("{}%", bat.percent);
    let text_w = pct_str.len() * font.char_width;
    let total_w = icon_w + 4 + text_w + 4;

    // Battery outline
    let body = Rect::new(x, y + 3, icon_w - 4, icon_h);
    framebuffer::draw_rect(body, colors::TEXT_COLOR_WHITE, 1);
    let tip = Rect::new(x + icon_w - 4, y + 6, 4, icon_h - 6);
    framebuffer::fill_rect(tip, colors::TEXT_COLOR_WHITE);

    // Fill level
    let fill_w = (icon_w - 6) * bat.percent as usize / 100;
    let fill_color = if bat.percent < 20 {
        Color::rgb(220, 60, 50)
    } else if bat.percent < 40 {
        Color::rgb(240, 180, 40)
    } else {
        Color::rgb(80, 200, 100)
    };
    if fill_w > 0 {
        framebuffer::fill_rect(Rect::new(x + 1, y + 4, fill_w, icon_h - 2), fill_color);
    }

    // Charging bolt
    if bat.charging {
        crate::graphics::draw_text("Z", x + 4, y + 4, colors::TEXT_COLOR_WHITE, font);
    }

    crate::graphics::draw_text(&pct_str, x + icon_w + 2, y + 4, colors::TEXT_COLOR_WHITE, font);
    total_w
}

// ---------------------------------------------------------------------------
// Brightness / Volume slider
// ---------------------------------------------------------------------------

pub fn render_slider(
    x: usize, y: usize, w: usize, label: &str, value: u8, max: u8,
) {
    let font = get_default_font();
    crate::graphics::draw_text(label, x, y, Color::rgb(180, 180, 190), font);
    let track_y = y + font.char_height + 6;
    let track = Rect::new(x, track_y, w, 6);
    fill_rounded_rect(track, Color::rgb(50, 50, 56), 3);
    let fill_w = if max > 0 { w * value as usize / max as usize } else { 0 };
    if fill_w > 0 {
        fill_rounded_rect(Rect::new(x, track_y, fill_w, 6), colors::DOCK_ICON_ACCENT, 3);
    }
    let knob_x = x + fill_w;
    let knob_r = 6;
    for dy in -(knob_r as isize)..=knob_r as isize {
        for dx in -(knob_r as isize)..=knob_r as isize {
            if dx * dx + dy * dy <= knob_r as isize * knob_r as isize {
                let px = knob_x as isize + dx;
                let py = track_y as isize + 3 + dy;
                if px >= 0 && py >= 0 {
                    framebuffer::set_pixel(px as usize, py as usize, colors::TEXT_COLOR_WHITE);
                }
            }
        }
    }
}
