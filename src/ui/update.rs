use super::*;
use crate::update::{CHECK_FILE, CheckError, CheckRecord, CheckResult, Checker, Release, Start};
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;
#[cfg(feature = "ui-qa")]
#[path = "update_qa.rs"]
pub(super) mod qa;

#[derive(Default)]
pub(super) struct State {
    pub checker: Checker,
    record: CheckRecord,
    pub release: Option<Release>,
    notice: bool,
    dismissed: bool,
    loading: Option<u64>,
    pub focus: Option<FocusHandle>,
    #[cfg(feature = "ui-qa")]
    pub client: Option<Arc<dyn http_client::HttpClient>>,
    #[cfg(feature = "ui-qa")]
    pub opened_urls: Vec<String>,
}

impl State {
    pub fn new(store: &Store) -> Self {
        Self {
            record: store.load(CHECK_FILE).unwrap_or_default(),
            #[cfg(feature = "ui-qa")]
            client: Some(http_client::FakeHttpClient::with_404_response()),
            ..Default::default()
        }
    }
}

impl RdmApp {
    pub(super) fn schedule_update_check(&self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(3)).await;
            let _ = this.update_in(cx, |app, window, cx| app.check_update(false, window, cx));
        })
        .detach();
    }

    pub(super) fn check_update(
        &mut self,
        manual: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = chrono::Utc::now().timestamp();
        if !manual && !self.updates.record.due(now) {
            return;
        }
        let start = self.updates.checker.start(manual, Instant::now());
        if let Start::Cached(result) = start {
            self.show_update_result(result, manual, window, cx);
            return;
        }
        if manual && self.updates.loading.is_none() {
            self.updates.loading = Some(self.loading_message(self.tr("updates.checking")));
        }
        if matches!(start, Start::Pending) {
            cx.notify();
            return;
        }
        self.updates.record.last_check_at = Some(now);
        let store = self.store.clone();
        #[cfg(feature = "ui-qa")]
        let client = self.updates.client.clone();
        let task = cx.background_executor().spawn(async move {
            if let Err(error) = store.save(
                CHECK_FILE,
                &CheckRecord {
                    last_check_at: Some(now),
                },
            ) {
                eprintln!("Unable to save update check time: {error}");
            }
            #[cfg(feature = "ui-qa")]
            let client = client.ok_or(CheckError::Network)?;
            #[cfg(not(feature = "ui-qa"))]
            let client = reqwest_client::ReqwestClient::user_agent(concat!(
                "RedisDock/",
                env!("CARGO_PKG_VERSION")
            ))
            .map(|client| Arc::new(client) as Arc<dyn http_client::HttpClient>)
            .map_err(|_| CheckError::Network)?;
            crate::update::fetch(client.as_ref()).await
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |app, window, cx| {
                let manual = app.updates.checker.finish(result.clone(), Instant::now());
                if let Some(id) = app.updates.loading.take() {
                    app.destroy_message(id, cx);
                }
                app.show_update_result(result, manual, window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn update_dialog_available(&self) -> bool {
        self.dialog.is_none()
            && self.exiting_dialog.is_none()
            && self.ssh_trust.is_none()
            && self.tab_close_warnings.is_empty()
            && self.pref.behavior.welcomed
    }

    fn show_update_result(
        &mut self,
        result: CheckResult,
        manual: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(Some(release)) => {
                self.updates.release = Some(release);
                if manual && self.update_dialog_available() {
                    self.show_update_details(window, cx);
                } else if manual || !self.updates.dismissed {
                    self.updates.notice = true;
                }
            }
            Ok(None) => {
                self.updates.release = None;
                self.updates.notice = false;
                if manual {
                    self.toast(self.tr("updates.up_to_date"), false);
                }
            }
            Err(error) if manual => self.toast(
                self.tr(match error {
                    CheckError::Network => "updates.network_error",
                    CheckError::RateLimited => "updates.rate_limited",
                    CheckError::NoRelease => "updates.no_release",
                    CheckError::InvalidResponse => "updates.invalid_response",
                }),
                true,
            ),
            Err(_) => {}
        }
        cx.notify();
    }

    pub(super) fn show_update_details(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.update_dialog_available() || self.updates.release.is_none() {
            return;
        }
        self.updates.notice = false;
        self.open_dialog(dialogs::DialogKind::Update, window, cx);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.updates.focus = Some(focus);
    }

    fn open_update_url(&mut self, url: String, cx: &mut Context<Self>) {
        // Only the validated Release model supplies these links.
        #[cfg(feature = "ui-qa")]
        self.updates.opened_urls.push(url);
        #[cfg(not(feature = "ui-qa"))]
        cx.open_url(&url);
        cx.notify();
    }

    pub(super) fn render_update_notice(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(release) = &self.updates.release else {
            return div().into_any_element();
        };
        if !self.updates.notice || !self.update_dialog_available() {
            return div().into_any_element();
        }
        div()
            .id("update-notice")
            .test_support()
            .occlude()
            .absolute()
            .bottom(px(16.))
            .right(px(16.))
            .w(px(365.))
            .p(px(16.))
            .rounded(px(4.))
            .shadow_lg()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(px(14.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div().flex_1().child(
                    self.tr("updates.available")
                        .replace("{version}", &release.version),
                ),
            )
            .child(
                self.btn(
                    "update-details",
                    self.tr("updates.details"),
                    "",
                    cx,
                    |app, w, cx| app.show_update_details(w, cx),
                )
                .tab_stop(true),
            )
            .child(
                self.icon_btn(
                    "update-dismiss",
                    "close",
                    self.tr("menu.close"),
                    cx,
                    |app, _, cx| {
                        app.updates.notice = false;
                        app.updates.dismissed = true;
                        cx.notify();
                    },
                )
                .accessibility_label(self.tr("menu.close"))
                .tab_stop(true),
            )
            .into_any_element()
    }

    pub(super) fn update_details_body(&self, cx: &Context<Self>) -> AnyElement {
        let release = self.updates.release.as_ref().unwrap();
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .min_w_0()
            .child(
                self.tr("updates.current_version")
                    .replace("{version}", env!("CARGO_PKG_VERSION")),
            )
            .child(
                self.tr("updates.available")
                    .replace("{version}", &release.version),
            )
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.tr("updates.install_hint")),
            )
            .when(release.downloads.is_empty(), |body| {
                body.child(self.tr("updates.no_package"))
            })
            .child(
                div()
                    .id("update-notes")
                    .test_support()
                    .whitespace_normal()
                    .min_w_0()
                    .child(if release.notes.trim().is_empty() {
                        self.tr("updates.no_notes")
                    } else {
                        release.notes.clone()
                    }),
            )
            .into_any_element()
    }

    pub(super) fn update_download_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
        let release = self.updates.release.as_ref().unwrap();
        let page = release.page_url.clone();
        let mut buttons = div().flex().flex_wrap().gap(px(8.)).min_w_0().child(
            self.btn(
                "update-release-page",
                self.tr("updates.release_page"),
                "",
                cx,
                move |app, _, cx| app.open_update_url(page.clone(), cx),
            )
            .tab_stop(true),
        );
        for (index, download) in release.downloads.iter().enumerate() {
            let url = download.url.clone();
            buttons = buttons.child(
                self.btn(
                    SharedString::from(format!("update-download-{index}")),
                    self.tr("updates.download")
                        .replace("{format}", &download.label),
                    "",
                    cx,
                    move |app, _, cx| app.open_update_url(url.clone(), cx),
                )
                .primary()
                .tab_stop(true),
            );
        }
        buttons.into_any_element()
    }
}
