#![allow(clippy::print_stderr, reason = "temp")]
mod aixm;
mod aixm_combine;
mod aixm_dfs;
mod error;
mod load_es;

use std::path::{Path, PathBuf};

use aixm::load_aixm_files;
use chrono::{DateTime, SecondsFormat, Utc};
use eframe::{CreationContext, Frame, NativeOptions};
use egui::{Button, Context, Label, RichText, ScrollArea, Stroke, TextWrapMode, Widget as _};
use load_es::load_euroscope_files;
use rfd::FileDialog;
use tokio::{
    runtime::{self, Runtime},
    sync::mpsc::{self},
    task::spawn_blocking,
    try_join,
};
use tracing::{Level, debug, error, info, trace, warn};
use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result {
    let env_filter =
        EnvFilter::try_from_env("AIRAC_UPDATER_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let native_options = NativeOptions::default();
    eframe::run_native(
        "VATGER AIRAC Updater",
        native_options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

struct Message {
    content: String,
    level: Level,
    time: DateTime<Utc>,
}
impl Message {
    fn new(content: String, level: Level) -> Self {
        Self {
            content,
            level,
            time: Utc::now(),
        }
    }

    fn debug(content: String) -> Self {
        Self::new(content, Level::DEBUG)
    }

    fn info(content: String) -> Self {
        Self::new(content, Level::INFO)
    }

    fn error(content: String) -> Self {
        Self::new(content, Level::ERROR)
    }
}

struct App {
    picked_path: Option<PathBuf>,
    rt: Runtime,
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,
    log_buffer: Vec<Message>,
}

impl App {
    fn new(cc: &CreationContext<'_>) -> Self {
        cc.egui_ctx.set_zoom_factor(1.5);

        let (tx, rx) = mpsc::channel(32);
        Self {
            picked_path: None,
            rt: runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
            tx,
            rx,
            log_buffer: vec![],
        }
    }

    fn handle_log_rx(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg.level {
                Level::TRACE => trace!("{}", msg.content),
                Level::DEBUG => debug!("{}", msg.content),
                Level::INFO => info!("{}", msg.content),
                Level::WARN => warn!("{}", msg.content),
                Level::ERROR => error!("{}", msg.content),
            };
            self.log_buffer.push(msg);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        self.handle_log_rx();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("AIRAC Updater");

            ui.add_space(10.);

            if ui.button("Choose AIRAC folder…").clicked() {
                if let Some(path) = FileDialog::new().pick_folder() {
                    self.log_buffer = vec![];
                    info!("AIRAC path chosen: {}", path.display());
                    self.picked_path = Some(path);
                }
            }

            if let Some(picked_path) = &self.picked_path {
                ui.horizontal(|ui| {
                    ui.label("AIRAC folder:");
                    ui.monospace(picked_path.display().to_string());
                });
            }

            ui.add_space(10.);

            ui.label("This tool will augment all .sct and .ese files, contained in the folder chosen above, with AIRAC data from DFS AIXM files.");
            ui.hyperlink("https://aip.dfs.de/datasets/");
            ui.label("The original files will remain as backup, suffixed with the time stamp of execution.");

            ui.add_space(10.);

            if ui.add_enabled(self.picked_path.is_some(), Button::new("Start Processing…")).clicked() {
                if let Some(p) = &self.picked_path {
                    let path = PathBuf::from(p);
                    self.log_buffer = vec![];
                    self.rt.spawn(spawn_jobs(path, self.tx.clone()));
                } else {
                    error!("Path not found");
                }
            }

            ui.add_space(10.);

            egui::Frame::new().stroke(Stroke::new(1., ui.style().visuals.text_color())).show(ui, |ui|
                ScrollArea::both().stick_to_bottom(true).auto_shrink(false).show(ui, |ui| {
                    for msg in &self.log_buffer {
                        Label::new(
                            RichText::new(
                                format!(
                                    "[{}] {}",
                                    msg.time.to_rfc3339_opts(SecondsFormat::Millis, true),
                                    msg.content
                                )
                            )
                                .size(12.)
                                .line_height(Some(18.))
                                .color(match msg.level {
                                    Level::ERROR => ui.style().visuals.error_fg_color,
                                    Level::WARN => ui.style().visuals.warn_fg_color,
                                    Level::INFO => ui.style().visuals.text_color(),
                                    Level::TRACE | Level::DEBUG => ui.style().visuals.gray_out(ui.style().visuals.text_color()),
                                })
                        )
                            .wrap_mode(TextWrapMode::Extend)
                            .ui(ui);
                    }
                })
            );
        });
    }
}

async fn spawn_jobs(dir: impl AsRef<Path>, tx: mpsc::Sender<Message>) {
    let (es_files, aixm) = match try_join!(
        load_euroscope_files(dir.as_ref(), tx.clone()),
        load_aixm_files(tx.clone())
    ) {
        Ok(ok) => ok,
        Err(e) => {
            if let Err(e) = tx.send(Message::error(e.to_string())).await {
                error!("{e}");
            }
            return;
        }
    };

    let blocking_tx = tx.clone();
    match spawn_blocking(move || {
        es_files
            .into_iter()
            .map(|es_file| es_file.combine_with_aixm(&aixm, blocking_tx.clone()))
            .collect::<Vec<_>>()
    })
    .await
    {
        Ok(files) => {
            for file in files {
                if let Err(e) = file.write_file().await {
                    if let Err(e) = tx.send(Message::error(e.to_string())).await {
                        error!("{e}");
                    }
                }
            }
        }
        Err(e) => error!("{e}"),
    }
}
