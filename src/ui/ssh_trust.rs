use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;

#[derive(Clone)]
pub(super) enum Resume {
    Open,
    Test { dialog: u64, sentinel: bool },
}
pub(super) struct Pending {
    config: Connection,
    key: SshHostKey,
    resume: Resume,
    focus: FocusHandle,
    previous: Option<FocusHandle>,
}

pub(super) fn needs_confirmation(config: &Connection) -> bool {
    config.ssh.enable
        && !config
            .ssh
            .trusted_host
            .as_ref()
            .is_some_and(|key| key.host == config.ssh.addr && key.port == config.ssh.port)
}
impl RdmApp {
    pub(super) fn request_ssh_trust(
        &mut self,
        config: Connection,
        resume: Resume,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ssh_trust.is_some() {
            return;
        }
        self.ssh_probe_epoch += 1;
        let epoch = self.ssh_probe_epoch;
        let probe = config.clone();
        self.cpu_job(
            window,
            cx,
            move || Ok(Client::connect(&probe, 0).map(|_| ())),
            move |app, result, w, cx| {
                if app.ssh_probe_epoch != epoch {
                    return;
                }
                if let Resume::Test { dialog, .. } = &resume {
                    if app.dialog.as_ref().is_none_or(|form| form.id != *dialog) {
                        return;
                    }
                }
                match result {
                    Err(error) => {
                        if let Some(error) = error.downcast_ref::<crate::redis::SshHostKeyError>()
                            && error.expected.is_none()
                        {
                            let focus = cx.focus_handle();
                            let previous = w.focused(cx);
                            focus.focus(w, cx);
                            app.ssh_trust = Some(Pending {
                                config,
                                key: error.observed.clone(),
                                resume,
                                focus,
                                previous,
                            });
                        } else {
                            app.toast(error.to_string(), true);
                        }
                    }
                    Ok(()) => app.toast(
                        "SSH trust configuration changed; retry the connection",
                        true,
                    ),
                }
            },
        );
    }
    fn finish_ssh_trust(&mut self, accept: bool, w: &mut Window, cx: &mut Context<Self>) {
        let Some(mut pending) = self.ssh_trust.take() else {
            return;
        };
        self.ssh_probe_epoch += 1;
        if let Some(focus) = pending.previous.take() {
            focus.focus(w, cx);
        }
        if !accept {
            cx.notify();
            return;
        }
        pending.config.ssh.trusted_host = Some(pending.key);
        match pending.resume {
            Resume::Open => {
                let name = pending.config.name.clone();
                if flatten(&self.connections).iter().any(|c| c.name == name) {
                    let mut connections = self.connections.clone();
                    let saved = crate::storage::save_connection(
                        &mut connections,
                        Some(&name),
                        pending.config.clone(),
                    )
                    .and_then(|_| self.store.save("connections.yaml", &connections));
                    if let Err(error) = saved {
                        self.toast(error.to_string(), true);
                        return;
                    }
                    self.connections = connections;
                }
                self.open_connection(pending.config, w, cx);
            }
            Resume::Test { dialog, sentinel } => {
                if let Some(form) = &mut self.dialog
                    && form.id == dialog
                {
                    form.data["ssh"]["trusted_host"] =
                        serde_json::to_value(pending.config.ssh.trusted_host).unwrap();
                    self.test_connection(sentinel, w, cx);
                }
            }
        }
        cx.notify();
    }
    pub(super) fn render_ssh_trust(&self, cx: &Context<Self>) -> AnyElement {
        let pending = self.ssh_trust.as_ref().unwrap();
        let key = &pending.key;
        div()
            .id("ssh-trust-overlay")
            .test_support()
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x00000066))
            .flex()
            .items_center()
            .justify_center()
            .track_focus(&pending.focus)
            .on_key_down(cx.listener(|app, event: &KeyDownEvent, w, cx| {
                if event.keystroke.key == "escape" {
                    app.finish_ssh_trust(false, w, cx);
                }
                cx.stop_propagation();
            }))
            .child(
                div()
                    .id("ssh-trust-dialog")
                    .test_support()
                    .role(Role::Dialog)
                    .aria_label(self.tr("ssh_trust.title"))
                    .w(px(540.))
                    .p_5()
                    .rounded_lg()
                    .bg(cx.theme().background)
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(self.tr("ssh_trust.title"))
                    .child(format!("{}:{} ({})", key.host, key.port, key.algorithm))
                    .child(div().text_sm().child(key.fingerprint.clone()))
                    .child(self.tr("ssh_trust.verify"))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                self.btn(
                                    "ssh-trust-cancel",
                                    self.tr("common.cancel"),
                                    "",
                                    cx,
                                    |app, w, cx| app.finish_ssh_trust(false, w, cx),
                                )
                                .tab_stop(true),
                            )
                            .child(
                                self.btn(
                                    "ssh-trust-accept",
                                    self.tr("ssh_trust.accept"),
                                    "",
                                    cx,
                                    |app, w, cx| app.finish_ssh_trust(true, w, cx),
                                )
                                .tab_stop(true),
                            ),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(feature = "ui-qa")]
impl RdmApp {
    pub(super) fn qa_show_ssh_trust(
        &mut self,
        scene: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pref.general.theme = if scene.ends_with("dark") {
            "dark"
        } else {
            "light"
        }
        .into();
        self.apply_theme(w, cx);
        let before = self.store.connections().unwrap();
        let config = flatten(&self.connections)[0].clone();
        let focus = cx.focus_handle();
        let previous = w.focused(cx);
        focus.focus(w, cx);
        let key = SshHostKey {
            host: "fixture.example".into(),
            port: 2222,
            algorithm: "Ed25519".into(),
            fingerprint: "SHA256:abcdefghijklmnopqrstuvwx0123456789ABCDEFGHIJK".into(),
        };
        self.ssh_trust = Some(Pending {
            config: config.clone(),
            key: key.clone(),
            resume: Resume::Open,
            focus: focus.clone(),
            previous: previous.clone(),
        });
        self.finish_ssh_trust(false, w, cx);
        assert!(self.ssh_trust.is_none());
        assert_eq!(self.store.connections().unwrap(), before);
        focus.focus(w, cx);
        self.ssh_trust = Some(Pending {
            config,
            key,
            resume: Resume::Open,
            focus,
            previous,
        });
    }
}
