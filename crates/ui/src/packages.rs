//! The password dialog for a package's encrypted archives. It opens when
//! unpacking finds them encrypted, or from the package's menu; the password
//! goes to the daemon with the request to unpack, and is not kept.

use eframe::egui;
use udm_domain::id::PackageId;
use udm_domain::package::{Package, Password, Unpack};

use crate::i18n::{tr, trf};
use crate::Action;

pub struct PasswordDialog {
    pub id: PackageId,
    name: String,
    /// The last password given did not open the archives.
    wrong: bool,
    password: String,
    shown: bool,
    /// The field takes the focus once.
    focused: bool,
}

impl PasswordDialog {
    pub fn new(p: &Package) -> Self {
        PasswordDialog {
            id: p.id,
            name: p.name.to_string(),
            wrong: matches!(p.unpack, Unpack::NeedsPassword { wrong: true }),
            password: String::new(),
            shown: false,
            focused: false,
        }
    }
}

/// Whether the dialog stays open.
pub fn show(ctx: &egui::Context, d: &mut PasswordDialog, connected: bool, actions: &mut Vec<Action>) -> bool {
    let mut open = true;
    let mut close = false;
    crate::dialog::Window::new(tr("Archive password"))
        .id(egui::Id::new(("package_password", d.id.get())))
        .dlu_width(260.0)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(trf!("The archives of {name} are protected by a password.", name = d.name));
            if d.wrong {
                ui.colored_label(ui.visuals().error_fg_color, tr("That password did not open them. Try again."));
            }
            let field =
                ui.add(egui::TextEdit::singleline(&mut d.password).password(!d.shown).desired_width(f32::INFINITY));
            crate::a11y::label(&field, tr("Password"));
            if !d.focused {
                field.request_focus();
                d.focused = true;
            }
            ui.checkbox(&mut d.shown, tr("Show the password"));
            let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.horizontal(|ui| {
                let ready = connected && !d.password.is_empty();
                if (ui.add_enabled(ready, egui::Button::new(tr("Unpack"))).clicked() || (entered && ready)) && !close {
                    let password = Password::new(std::mem::take(&mut d.password));
                    actions.push(Action::UnpackPackage(d.id, password));
                    close = true;
                }
                close |= ui.button(tr("Cancel")).clicked();
            });
        });
    open && !close
}
