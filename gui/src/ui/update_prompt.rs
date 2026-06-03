//! Non-silent self-update surface.
//!
//! The wallet NEVER applies an update without explicit user consent. This module
//! renders, as a centered modal over a dimming scrim:
//!
//!   * a hard, NON-dismissable block when the running version is below the
//!     manifest's `min_supported` (with an upgrade call-to-action), and
//!   * a dismissable prompt when a newer version is available, showing the new
//!     version + release notes and an **Apply** button.
//!
//! During an apply it shows coarse progress and, on success, a **Relaunch now**
//! button (the swap is already on disk; relaunch boots the new build, which then
//! confirms its own health or is rolled back — see `crate::update`).

use crate::app::AliceWalletApp;
use crate::ui::theme::THEME;
use crate::ui::widgets;
use crate::update::CheckOutcome;
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};

/// Render the update overlay if anything needs the user's attention. A no-op
/// when there is no actionable outcome (the common case).
pub fn render(ctx: &egui::Context, app: &mut AliceWalletApp) {
    // Decide what (if anything) to show, without holding a borrow on `app`.
    enum Mode {
        Block { current: String, min: String },
        Offer,            // an update is available + not dismissed
        Applying,         // apply in flight
        Relaunch(String), // installed, awaiting relaunch
    }

    let mode = if let Some(ver) = app.update_ui.ready_to_relaunch.clone() {
        Some(Mode::Relaunch(ver))
    } else if app.update_ui.applying {
        Some(Mode::Applying)
    } else {
        match app.update_ui.outcome.as_ref() {
            Some(CheckOutcome::Unsupported {
                current,
                min_supported,
                ..
            }) => Some(Mode::Block {
                current: current.clone(),
                min: min_supported.clone(),
            }),
            Some(CheckOutcome::UpdateAvailable { .. }) if !app.update_ui.dismissed => {
                Some(Mode::Offer)
            }
            // A newer version with no artifact for this platform is informational
            // only — surfaced once as a toast, not a blocking modal.
            _ => None,
        }
    };

    let Some(mode) = mode else {
        return;
    };

    // Dimming scrim that swallows clicks behind the modal.
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("update_scrim"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            ui.allocate_response(screen.size(), egui::Sense::click_and_drag());
            ui.painter().rect_filled(
                screen,
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(0, 0, 0, 180),
            );
        });

    egui::Area::new(egui::Id::new("update_modal"))
        .order(egui::Order::Tooltip)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_max_width(460.0);
            egui::Frame::NONE
                .fill(THEME.bg_panel)
                .corner_radius(CornerRadius::same(16))
                .inner_margin(egui::Margin::same(24))
                .stroke(Stroke::new(1.0, THEME.border_accent))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 12],
                    blur: 40,
                    spread: 0,
                    color: Color32::from_rgba_premultiplied(0, 0, 0, 160),
                })
                .show(ui, |ui| match mode {
                    Mode::Block { current, min } => block_body(ui, app, &current, &min),
                    Mode::Offer => offer_body(ui, app),
                    Mode::Applying => applying_body(ui, app),
                    Mode::Relaunch(ver) => relaunch_body(ui, app, &ver),
                });
        });
}

fn offer_body(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    // Pull the fields we need up-front (immutable borrow ends before the buttons).
    let (current, version, released, notes, has_error) = match app.update_ui.outcome.as_ref() {
        Some(CheckOutcome::UpdateAvailable {
            current, manifest, ..
        }) => (
            current.clone(),
            manifest.version.clone(),
            manifest.released.clone(),
            manifest.notes.clone(),
            app.update_ui.error.clone(),
        ),
        _ => return,
    };

    widgets::heading(ui, "Update available");
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!("Version {version}"))
            .size(15.0)
            .color(THEME.primary_hi)
            .strong(),
    );
    ui.label(
        RichText::new(format!("You're on {current}"))
            .size(12.0)
            .color(THEME.text_dim),
    );
    if !released.trim().is_empty() {
        ui.label(
            RichText::new(format!("Released {released}"))
                .size(11.0)
                .color(THEME.text_dim),
        );
    }
    ui.add_space(12.0);

    if !notes.trim().is_empty() {
        widgets::section_title(ui, "Release notes");
        egui::Frame::NONE
            .fill(THEME.bg_input)
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::same(12))
            .stroke(Stroke::new(1.0, THEME.border))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new(notes).size(12.5).color(THEME.text_mid));
                    });
            });
        ui.add_space(12.0);
    }

    if let Some(err) = has_error.as_ref() {
        error_note(ui, err);
        ui.add_space(8.0);
    }

    ui.label(
        RichText::new(
            "Updates are verified with Alice's signing key before install. Your wallet keys and data are never touched.",
        )
        .size(11.0)
        .color(THEME.text_dim),
    );
    ui.add_space(14.0);

    ui.horizontal(|ui| {
        if widgets::primary_button(ui, "Apply update", true, false).clicked() {
            app.start_update_apply();
        }
        if widgets::ghost_button(ui, "Later").clicked() {
            app.update_ui.dismissed = true;
        }
    });
}

fn applying_body(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    widgets::heading(ui, "Updating Alice Wallet");
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.spinner();
        ui.add_space(8.0);
        let msg = app
            .update_ui
            .progress
            .clone()
            .unwrap_or_else(|| "Working…".to_string());
        ui.label(RichText::new(msg).size(13.0).color(THEME.text_mid));
    });
    ui.add_space(10.0);
    ui.label(
        RichText::new("Please keep the app open until this finishes.")
            .size(11.0)
            .color(THEME.text_dim),
    );

    if let Some(err) = app.update_ui.error.clone() {
        ui.add_space(10.0);
        error_note(ui, &err);
        ui.add_space(8.0);
        if widgets::ghost_button(ui, "Dismiss").clicked() {
            app.update_ui.applying = false;
            app.update_ui.progress = None;
        }
    }
}

fn relaunch_body(ui: &mut egui::Ui, app: &mut AliceWalletApp, version: &str) {
    widgets::heading(ui, "Update installed");
    ui.add_space(6.0);
    ui.label(
        RichText::new(format!(
            "Version {version} is installed. Relaunch to start using it."
        ))
        .size(13.0)
        .color(THEME.text_mid),
    );
    ui.add_space(6.0);
    ui.label(
        RichText::new(
            "If the new version has trouble starting, Alice Wallet automatically restores the previous one.",
        )
        .size(11.0)
        .color(THEME.text_dim),
    );
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        if widgets::primary_button(ui, "Relaunch now", true, false).clicked() {
            app.relaunch_now();
        }
        if widgets::ghost_button(ui, "Later").clicked() {
            // Keep last-known-good around; the swap already happened on disk and
            // the new build takes effect on the next manual restart.
            app.update_ui.ready_to_relaunch = None;
        }
    });
}

fn block_body(ui: &mut egui::Ui, app: &mut AliceWalletApp, current: &str, min: &str) {
    ui.label(
        RichText::new("Update required")
            .size(22.0)
            .color(THEME.danger)
            .strong(),
    );
    ui.add_space(6.0);
    ui.label(
        RichText::new(format!(
            "This version ({current}) is no longer supported. The minimum supported version is {min}. Please update to continue using Alice Wallet safely."
        ))
        .size(13.0)
        .color(THEME.text_mid),
    );
    ui.add_space(14.0);

    // If an artifact exists for this platform, the manifest's `Unsupported`
    // variant still carries it indirectly via a follow-up check; offer an
    // in-app apply when we can, else point at the download page.
    let can_apply = app.app_path.is_some()
        && matches!(
            app.update_ui.outcome.as_ref(),
            Some(CheckOutcome::Unsupported { manifest, .. })
                if manifest.artifact_for_current_platform().is_some()
        );

    if let Some(err) = app.update_ui.error.clone() {
        error_note(ui, &err);
        ui.add_space(8.0);
    }

    if can_apply {
        if widgets::primary_button(ui, "Update now", !app.update_ui.applying, true).clicked() {
            // Reuse the apply path but source the artifact from the Unsupported
            // manifest.
            if let Some(CheckOutcome::Unsupported { manifest, .. }) = app.update_ui.outcome.as_ref()
            {
                if let Some(artifact) = manifest.artifact_for_current_platform().cloned() {
                    let version = manifest.version.clone();
                    app.update_ui.applying = true;
                    app.update_ui.error = None;
                    app.update_ui.progress = Some("Starting update…".to_string());
                    let _ = app
                        .update_tx
                        .send(crate::app::UpdateRequest::Apply { artifact, version });
                }
            }
        }
    } else {
        ui.label(
            RichText::new(
                "Download the latest version from the official Alice Wallet releases page.",
            )
            .size(12.0)
            .color(THEME.text_dim),
        );
        ui.add_space(8.0);
        ui.hyperlink_to(
            RichText::new("Open releases page")
                .size(13.0)
                .color(THEME.primary_hi),
            releases_page_url(),
        );
    }
}

fn error_note(ui: &mut egui::Ui, msg: &str) {
    egui::Frame::NONE
        .fill(THEME.danger_bg)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(10))
        .stroke(Stroke::new(1.0, THEME.danger))
        .show(ui, |ui| {
            ui.label(RichText::new(msg).size(12.0).color(THEME.danger));
        });
}

/// Human-facing releases page derived from the configured update URL (strip the
/// trailing `/latest/download/latest.json` to land on the releases page).
fn releases_page_url() -> String {
    let url = crate::update::update_url();
    url.split("/releases/")
        .next()
        .map(|base| format!("{base}/releases"))
        .unwrap_or(url)
}
