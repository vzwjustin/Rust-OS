//! Kernel graphical installer wizard (Ubuntu-styled).

use crate::desktop::window_manager::colors;
use crate::graphics;
use crate::graphics::framebuffer::{self, Rect};
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::disk::{enumerate_disks, DiskInfo};
use super::plan::{hash_password, InstallPlan};

/// Wizard screen sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardStep {
    Welcome,
    Language,
    DiskSelection,
    UserAccount,
    Summary,
    Progress,
    Complete,
    Error,
}

/// Interactive wizard state.
pub struct InstallerWizard {
    pub step: WizardStep,
    pub plan: InstallPlan,
    pub disks: Vec<DiskInfo>,
    pub disk_index: usize,
    pub language_index: usize,
    pub field_index: usize,
    pub username_buf: String,
    pub fullname_buf: String,
    pub password_buf: String,
    pub hostname_buf: String,
    pub confirmed: bool,
    pub error_message: String,
    pub progress_percent: u8,
    pub status_line: String,
}

impl InstallerWizard {
    pub fn new(plan: InstallPlan) -> Self {
        let disks = enumerate_disks();
        let disk_index = disks
            .iter()
            .position(|d| d.id == plan.target_device_id)
            .unwrap_or(0);
        let mut wizard = Self {
            step: WizardStep::Welcome,
            plan,
            disks,
            disk_index,
            language_index: 0,
            field_index: 0,
            username_buf: String::new(),
            fullname_buf: String::new(),
            password_buf: String::new(),
            hostname_buf: String::from("rustos"),
            confirmed: false,
            error_message: String::new(),
            progress_percent: 0,
            status_line: String::new(),
        };
        wizard.username_buf = wizard.plan.username.clone();
        wizard.fullname_buf = wizard.plan.full_name.clone();
        wizard.hostname_buf = wizard.plan.hostname.clone();
        wizard
    }

    pub fn advance(&mut self) {
        self.step = match self.step {
            WizardStep::Welcome => WizardStep::Language,
            WizardStep::Language => WizardStep::DiskSelection,
            WizardStep::DiskSelection => WizardStep::UserAccount,
            WizardStep::UserAccount => {
                self.sync_account_fields();
                WizardStep::Summary
            }
            WizardStep::Summary => WizardStep::Progress,
            WizardStep::Complete | WizardStep::Error | WizardStep::Progress => self.step,
        };
    }

    pub fn back(&mut self) {
        self.step = match self.step {
            WizardStep::Language => WizardStep::Welcome,
            WizardStep::DiskSelection => WizardStep::Language,
            WizardStep::UserAccount => WizardStep::DiskSelection,
            WizardStep::Summary => WizardStep::UserAccount,
            _ => self.step,
        };
    }

    pub fn sync_account_fields(&mut self) {
        if !self.username_buf.is_empty() {
            self.plan.username = self.username_buf.clone();
        }
        if !self.fullname_buf.is_empty() {
            self.plan.full_name = self.fullname_buf.clone();
        }
        if !self.hostname_buf.is_empty() {
            self.plan.hostname = self.hostname_buf.clone();
        }
        if !self.password_buf.is_empty() {
            self.plan.password_hash = hash_password(&self.password_buf);
        }
        if let Some(disk) = self.disks.get(self.disk_index) {
            self.plan.target_device_id = disk.id;
        }
        self.plan.language = if self.language_index == 0 {
            String::from("en_US")
        } else {
            String::from("en_GB")
        };
    }

    pub fn handle_key(&mut self, key: u8) {
        match self.step {
            WizardStep::Welcome => match key {
                13 | b'\n' => self.advance(),
                _ => {}
            },
            WizardStep::Language => match key {
                b'1' | b'2' => {
                    self.language_index = if key == b'1' { 0 } else { 1 };
                    self.advance();
                }
                13 => self.advance(),
                _ => {}
            },
            WizardStep::DiskSelection => match key {
                b'j' | b'J' => {
                    if self.disk_index + 1 < self.disks.len() {
                        self.disk_index += 1;
                    }
                }
                b'k' | b'K' => {
                    if self.disk_index > 0 {
                        self.disk_index -= 1;
                    }
                }
                13 => self.advance(),
                _ => {}
            },
            WizardStep::UserAccount => self.handle_account_key(key),
            WizardStep::Summary => match key {
                b'y' | b'Y' | 13 => {
                    self.confirmed = true;
                    self.sync_account_fields();
                }
                b'n' | b'N' => self.back(),
                _ => {}
            },
            WizardStep::Complete => match key {
                13 => {}
                _ => {}
            },
            WizardStep::Progress | WizardStep::Error => {}
        }
    }

    fn handle_account_key(&mut self, key: u8) {
        let buf = match self.field_index {
            0 => &mut self.fullname_buf,
            1 => &mut self.username_buf,
            2 => &mut self.hostname_buf,
            _ => &mut self.password_buf,
        };
        match key {
            9 => self.field_index = (self.field_index + 1) % 4,
            8 => {
                buf.pop();
            }
            13 => self.advance(),
            c if c >= 32 && c < 127 => {
                if buf.len() < 48 {
                    buf.push(c as char);
                }
            }
            _ => {}
        }
    }

    pub fn render(&self) {
        if !graphics::is_graphics_initialized() {
            return;
        }
        let (w, h) = graphics::get_screen_dimensions().unwrap_or((1024, 768));
        draw_gradient_background(w, h);

        let panel_w = w * 3 / 5;
        let panel_h = h * 3 / 4;
        let panel_x = (w - panel_w) / 2;
        let panel_y = (h - panel_h) / 2;
        framebuffer::fill_rect(
            Rect::new(panel_x, panel_y, panel_w, panel_h),
            colors::WINDOW_BACKGROUND,
        );
        framebuffer::draw_rect(
            Rect::new(panel_x, panel_y, panel_w, panel_h),
            colors::BORDER_ACTIVE,
            2,
        );

        let font = graphics::get_default_font();
        let title_y = panel_y + 24;
        graphics::draw_text(
            "Install RustOS",
            panel_x + 24,
            title_y,
            colors::TITLE_BAR_ACTIVE,
            font,
        );

        let body_y = title_y + font.char_height + 20;
        match self.step {
            WizardStep::Welcome => {
                draw_lines(
                    panel_x + 24,
                    body_y,
                    &[
                        "Welcome to the RustOS installer.",
                        "",
                        "This will partition your disk, copy the live",
                        "system, and configure your user account.",
                        "",
                        "[Enter] Continue with Install",
                    ],
                    font,
                );
            }
            WizardStep::Language => {
                draw_lines(
                    panel_x + 24,
                    body_y,
                    &[
                        "Select your language:",
                        "",
                        "[1] English (US)  — default",
                        "[2] English (UK)",
                        "",
                        "[Enter] Continue",
                    ],
                    font,
                );
            }
            WizardStep::DiskSelection => {
                let mut lines: Vec<String> =
                    vec![String::from("Select installation disk:"), String::new()];
                if self.disks.is_empty() {
                    lines.push(String::from("No disks detected."));
                } else {
                    for (i, disk) in self.disks.iter().enumerate() {
                        let marker = if i == self.disk_index { ">" } else { " " };
                        lines.push(format!("{} [{}] {}", marker, i + 1, disk.display_label()));
                    }
                }
                lines.push(String::new());
                lines.push(String::from("[j/k] Move selection  [Enter] Continue"));
                draw_lines_owned(panel_x + 24, body_y, &lines, font);
            }
            WizardStep::UserAccount => {
                draw_lines(
                    panel_x + 24,
                    body_y,
                    &[
                        "Create your account (Tab switches fields):",
                        "",
                        &format!(
                            "Full name: {}{}",
                            self.fullname_buf,
                            if self.field_index == 0 { "_" } else { "" }
                        ),
                        &format!(
                            "Username:  {}{}",
                            self.username_buf,
                            if self.field_index == 1 { "_" } else { "" }
                        ),
                        &format!(
                            "Hostname:  {}{}",
                            self.hostname_buf,
                            if self.field_index == 2 { "_" } else { "" }
                        ),
                        &format!(
                            "Password:  {}{}",
                            "*".repeat(self.password_buf.len()),
                            if self.field_index == 3 { "_" } else { "" }
                        ),
                        "",
                        "[Enter] Continue",
                    ],
                    font,
                );
            }
            WizardStep::Summary => {
                let disk_label = self
                    .disks
                    .get(self.disk_index)
                    .map(|d| d.display_label())
                    .unwrap_or_else(|| String::from("unknown"));
                draw_lines(
                    panel_x + 24,
                    body_y,
                    &[
                        "Ready to install:",
                        "",
                        &format!("Disk:     {}", disk_label),
                        &format!("Hostname: {}", self.hostname_buf),
                        &format!("User:     {}", self.username_buf),
                        &format!("Language: {}", self.plan.language),
                        "",
                        "[Y] Install now   [N] Go back",
                    ],
                    font,
                );
            }
            WizardStep::Progress => {
                let bar_w = panel_w - 80;
                let bar_x = panel_x + 40;
                let bar_y = body_y + 60;
                framebuffer::fill_rect(
                    Rect::new(bar_x, bar_y, bar_w, 16),
                    colors::BUTTON_BACKGROUND,
                );
                let fill = (bar_w * self.progress_percent as usize) / 100;
                if fill > 0 {
                    framebuffer::fill_rect(
                        Rect::new(bar_x, bar_y, fill, 16),
                        colors::DOCK_ICON_ACCENT,
                    );
                }
                draw_lines(
                    panel_x + 24,
                    body_y,
                    &[
                        "Installing RustOS...",
                        "",
                        &self.status_line,
                        &format!("{}%", self.progress_percent),
                    ],
                    font,
                );
            }
            WizardStep::Complete => {
                draw_lines(
                    panel_x + 24,
                    body_y,
                    &[
                        "Installation complete!",
                        "",
                        "RustOS has been installed to your disk.",
                        "Remove the live media and reboot.",
                        "",
                        "[Enter] Reboot",
                    ],
                    font,
                );
            }
            WizardStep::Error => {
                draw_lines(
                    panel_x + 24,
                    body_y,
                    &["Installation failed:", "", &self.error_message],
                    font,
                );
            }
        }

        framebuffer::present();
    }
}

fn draw_gradient_background(w: usize, h: usize) {
    for y in 0..h {
        let factor = y as u32 * 256 / h as u32;
        let r = (colors::DESKTOP_BACKGROUND_TOP.r as u32 * (256 - factor)
            + colors::DESKTOP_BACKGROUND_BOTTOM.r as u32 * factor)
            / 256;
        let g = (colors::DESKTOP_BACKGROUND_TOP.g as u32 * (256 - factor)
            + colors::DESKTOP_BACKGROUND_BOTTOM.g as u32 * factor)
            / 256;
        let b = (colors::DESKTOP_BACKGROUND_TOP.b as u32 * (256 - factor)
            + colors::DESKTOP_BACKGROUND_BOTTOM.b as u32 * factor)
            / 256;
        framebuffer::fill_rect(
            Rect::new(0, y, w, 1),
            graphics::Color::rgb(r as u8, g as u8, b as u8),
        );
    }
}

fn draw_lines(x: usize, mut y: usize, lines: &[&str], font: &graphics::BitmapFont) {
    for line in lines {
        graphics::draw_text(line, x, y, colors::TEXT_COLOR, font);
        y += font.char_height + 4;
    }
}

fn draw_lines_owned(x: usize, mut y: usize, lines: &[String], font: &graphics::BitmapFont) {
    for line in lines {
        graphics::draw_text(line, x, y, colors::TEXT_COLOR, font);
        y += font.char_height + 4;
    }
}

/// Run the graphical wizard; returns the finalized plan when the user confirms.
pub fn run_graphical_wizard(plan: InstallPlan) -> Result<InstallPlan, String> {
    let mut wizard = InstallerWizard::new(plan);
    wizard.render();

    loop {
        while let Some(event) = crate::keyboard::get_key_event() {
            let key = match event {
                crate::keyboard::KeyEvent::CharacterPress(c) => c as u8,
                crate::keyboard::KeyEvent::SpecialPress(sp) => match sp {
                    crate::keyboard::SpecialKey::Enter => 13,
                    crate::keyboard::SpecialKey::Tab => 9,
                    crate::keyboard::SpecialKey::Backspace => 8,
                    _ => continue,
                },
                _ => continue,
            };
            wizard.handle_key(key);
            wizard.render();
            if wizard.step == WizardStep::Summary && wizard.confirmed {
                wizard.sync_account_fields();
                return Ok(wizard.plan);
            }
        }

        if wizard.confirmed && wizard.step == WizardStep::Summary {
            wizard.sync_account_fields();
            return Ok(wizard.plan);
        }

        core::hint::spin_loop();
    }
}

/// Text-mode fallback wizard when framebuffer is unavailable.
pub fn run_text_wizard(mut plan: InstallPlan) -> Result<InstallPlan, String> {
    use crate::println;

    let disks = enumerate_disks();
    println!();
    println!("=== RustOS Installer (text mode) ===");
    if disks.is_empty() {
        return Err(String::from("No storage devices found"));
    }
    for (i, d) in disks.iter().enumerate() {
        println!("  [{}] {}", i + 1, d.display_label());
    }
    plan.target_device_id = disks.first().map(|d| d.id).unwrap_or(1);
    plan.username = String::from("rustos");
    plan.hostname = String::from("rustos");
    plan.password_hash = hash_password("rustos");
    println!("Press Enter to install to first disk...");
    Ok(plan)
}
