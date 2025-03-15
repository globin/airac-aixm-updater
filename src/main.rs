#![allow(clippy::print_stderr, reason = "temp")]
mod aixm;
mod load_es;

use std::path::{Path, PathBuf};

use aixm::load_aixm_files;
use chrono::{DateTime, SecondsFormat, Utc};
use eframe::{CreationContext, Frame, NativeOptions};
use egui::{Button, Context, Label, RichText, ScrollArea, Stroke, TextWrapMode, Widget as _};
use load_es::load_euroscope_files;
use rfd::FileDialog;
use snafu::Snafu;
use tokio::{
    runtime::{self, Runtime},
    sync::mpsc::{self, error::SendError},
    try_join,
};
use tracing::{Level, error, info};
use tracing_subscriber::EnvFilter;
use vatsim_parser::{ese::EseError, sct::SctError};

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

type AiracUpdaterResult<T = ()> = Result<T, Error>;
#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("Could not process {}", filename.display()))]
    Processing { filename: PathBuf },

    #[snafu(display("Could not read directory: source"))]
    ReadDir { source: std::io::Error },

    #[snafu(display("Could not open AIXM ({}): {source}", filename.display()))]
    OpenAixm {
        filename: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Could not read AIXM ({}): {source}", filename.display()))]
    ReadAixm {
        filename: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Could not open .ese ({}): {source}", filename.display()))]
    OpenEse {
        filename: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Could not read .ese ({}): {source}", filename.display()))]
    ReadEse {
        filename: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Could not parse .ese ({}): {source}", filename.display()))]
    ParseEse {
        filename: PathBuf,
        #[snafu(source(from(EseError, Box::new)))]
        source: Box<EseError>,
    },

    #[snafu(display("Could not open .sct ({}): {source}", filename.display()))]
    OpenSct {
        filename: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Could not read .sct ({}): {source}", filename.display()))]
    ReadSct {
        filename: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Could not parse .sct ({}): {source}", filename.display()))]
    ParseSct {
        filename: PathBuf,
        #[snafu(source(from(SctError, Box::new)))]
        source: Box<SctError>,
    },

    #[snafu(context(false))]
    Send { source: SendError<Message> },
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
            info!("{}", msg.content);
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
                                    _ => ui.style().visuals.text_color(),

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

async fn spawn_jobs(dir: impl AsRef<Path>, tx: mpsc::Sender<Message>) -> AiracUpdaterResult {
    let (es_files, aixm) = try_join!(
        load_euroscope_files(dir.as_ref(), tx.clone()),
        load_aixm_files(tx.clone())
    )?;

    let temp = es_files
        .into_iter()
        .map(|es_file| es_file.combine_with_aixm(&aixm))
        .collect::<Vec<_>>();

    Ok(())
}
