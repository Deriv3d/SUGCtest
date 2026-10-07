//! Game-agnostic asset browser.
//!
//! A game plugs in an [`AssetSource`] that lists its assets and decodes them on demand into a
//! few generic kinds ([`Asset`]). The browser shows a grouped list, previews images, plays
//! audio and shows text. Nothing here knows about any particular game or file format.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::num::NonZero;

use eframe::egui;

/// One listed asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetEntry {
    /// Stable id the source uses to load it.
    pub id: usize,
    /// Group shown as a collapsible header (e.g. the container file).
    pub group: String,
    /// Label inside the group.
    pub label: String,
}

/// A decoded asset in a generic form.
#[derive(Debug, Clone, PartialEq)]
pub enum Asset {
    /// Tightly packed RGBA8 pixels.
    Image {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    /// Interleaved signed 16-bit PCM.
    Audio {
        channels: u16,
        sample_rate: u32,
        pcm: Vec<i16>,
    },
    /// Lines of text.
    Text(Vec<String>),
    /// Anything else: a short description of what it is.
    Info(Vec<String>),
}

/// Supplies and decodes assets. Implemented per game.
pub trait AssetSource {
    fn entries(&self) -> &[AssetEntry];
    fn load(&self, id: usize) -> Result<Asset, String>;
}

/// Summary statistics of a whole source, for headless checks.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LoadReport {
    pub images: usize,
    pub audio: usize,
    pub text: usize,
    pub info: usize,
    pub failed: Vec<(usize, String)>,
}

/// Decode every asset once and count the kinds (no window).
pub fn load_all(source: &dyn AssetSource) -> LoadReport {
    let mut r = LoadReport::default();
    for e in source.entries() {
        match source.load(e.id) {
            Ok(Asset::Image { .. }) => r.images += 1,
            Ok(Asset::Audio { .. }) => r.audio += 1,
            Ok(Asset::Text(_)) => r.text += 1,
            Ok(Asset::Info(_)) => r.info += 1,
            Err(err) => r.failed.push((e.id, err)),
        }
    }
    r
}

/// Open the browser window.
pub fn run(title: &str, source: Box<dyn AssetSource>) -> Result<(), String> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        title,
        options,
        Box::new(move |_cc| Ok(Box::new(Browser::new(source)))),
    )
    .map_err(|e| e.to_string())
}

struct Browser {
    source: Box<dyn AssetSource>,
    groups: BTreeMap<String, Vec<usize>>,
    filter: String,
    selected: Option<usize>,
    current: Option<Result<Asset, String>>,
    texture: Option<egui::TextureHandle>,
    zoom: f32,
    audio: Option<(rodio::MixerDeviceSink, rodio::Player)>,
    audio_error: Option<String>,
}

impl Browser {
    fn new(source: Box<dyn AssetSource>) -> Browser {
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, e) in source.entries().iter().enumerate() {
            groups.entry(e.group.clone()).or_default().push(i);
        }
        Browser {
            source,
            groups,
            filter: String::new(),
            selected: None,
            current: None,
            texture: None,
            zoom: 1.0,
            audio: None,
            audio_error: None,
        }
    }

    fn select(&mut self, ctx: &egui::Context, index: usize) {
        self.stop_audio();
        self.selected = Some(index);
        let id = self.source.entries()[index].id;
        let loaded = self.source.load(id);
        self.texture = match &loaded {
            Ok(Asset::Image {
                width,
                height,
                rgba,
            }) => {
                let img = egui::ColorImage::from_rgba_unmultiplied(
                    [*width as usize, *height as usize],
                    rgba,
                );
                Some(ctx.load_texture(format!("asset-{id}"), img, egui::TextureOptions::NEAREST))
            }
            _ => None,
        };
        self.current = Some(loaded);
    }

    fn play(&mut self, channels: u16, rate: u32, pcm: &[i16]) {
        self.stop_audio();
        let (Some(ch), Some(sr)) = (NonZero::new(channels), NonZero::new(rate)) else {
            self.audio_error = Some("invalid channel count or sample rate".into());
            return;
        };
        match rodio::DeviceSinkBuilder::open_default_sink() {
            Ok(sink) => {
                let player = rodio::Player::connect_new(sink.mixer());
                let samples: Vec<f32> = pcm.iter().map(|&s| f32::from(s) / 32768.0).collect();
                player.append(rodio::buffer::SamplesBuffer::new(ch, sr, samples));
                self.audio = Some((sink, player));
                self.audio_error = None;
            }
            Err(e) => self.audio_error = Some(e.to_string()),
        }
    }

    fn stop_audio(&mut self) {
        if let Some((_, player)) = self.audio.take() {
            player.stop();
        }
    }
}

impl eframe::App for Browser {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut clicked = None;
        egui::Panel::left("assets").min_size(320.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Filter");
                ui.text_edit_singleline(&mut self.filter);
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                let filter = self.filter.to_lowercase();
                for (group, items) in &self.groups {
                    egui::CollapsingHeader::new(format!("{group} ({})", items.len()))
                        .id_salt(group)
                        .show(ui, |ui| {
                            for &i in items {
                                let label = &self.source.entries()[i].label;
                                if !filter.is_empty() && !label.to_lowercase().contains(&filter) {
                                    continue;
                                }
                                if ui
                                    .selectable_label(self.selected == Some(i), label)
                                    .clicked()
                                {
                                    clicked = Some(i);
                                }
                            }
                        });
                }
            });
        });
        if let Some(i) = clicked {
            self.select(&ctx, i);
        }

        let mut play_request = None;
        egui::CentralPanel::default().show(ui, |ui| match (&self.current, self.selected) {
            (None, _) | (_, None) => {
                ui.heading("Select an asset on the left.");
            }
            (Some(Err(e)), _) => {
                ui.colored_label(egui::Color32::RED, format!("Could not load: {e}"));
            }
            (Some(Ok(asset)), Some(sel)) => {
                ui.heading(&self.source.entries()[sel].label);
                match asset {
                    Asset::Image { width, height, .. } => {
                        ui.horizontal(|ui| {
                            ui.label(format!("{width}×{height}"));
                            ui.add(egui::Slider::new(&mut self.zoom, 0.25..=4.0).text("zoom"));
                        });
                        if let Some(tex) = &self.texture {
                            let size = egui::vec2(*width as f32, *height as f32) * self.zoom;
                            egui::ScrollArea::both().show(ui, |ui| {
                                ui.add(egui::Image::from_texture(egui::load::SizedTexture::new(
                                    tex.id(),
                                    size,
                                )));
                            });
                        }
                    }
                    Asset::Audio {
                        channels,
                        sample_rate,
                        pcm,
                    } => {
                        let secs = pcm.len() as f32 / (*channels as f32 * *sample_rate as f32);
                        ui.label(format!("{channels} ch, {sample_rate} Hz, {secs:.1} s"));
                        ui.horizontal(|ui| {
                            if ui.button("▶ Play").clicked() {
                                play_request = Some((*channels, *sample_rate, pcm.clone()));
                            }
                            if ui.button("■ Stop").clicked() {
                                play_request = Some((0, 0, Vec::new()));
                            }
                        });
                        if let Some(e) = &self.audio_error {
                            ui.colored_label(egui::Color32::RED, e);
                        }
                    }
                    Asset::Text(lines) => {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (n, line) in lines.iter().enumerate() {
                                ui.label(format!("{n:5}  {line}"));
                            }
                        });
                    }
                    Asset::Info(lines) => {
                        for line in lines {
                            ui.label(line);
                        }
                    }
                }
            }
        });
        match play_request {
            Some((0, _, _)) => self.stop_audio(),
            Some((ch, rate, pcm)) => self.play(ch, rate, &pcm),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(Vec<AssetEntry>);

    impl AssetSource for Fake {
        fn entries(&self) -> &[AssetEntry] {
            &self.0
        }
        fn load(&self, id: usize) -> Result<Asset, String> {
            match id {
                0 => Ok(Asset::Image {
                    width: 1,
                    height: 1,
                    rgba: vec![1, 2, 3, 4],
                }),
                1 => Ok(Asset::Audio {
                    channels: 1,
                    sample_rate: 8000,
                    pcm: vec![0; 8],
                }),
                2 => Ok(Asset::Text(vec!["hello".into()])),
                _ => Err("nope".into()),
            }
        }
    }

    #[test]
    fn load_all_counts_kinds() {
        let entries = (0..4)
            .map(|id| AssetEntry {
                id,
                group: "g".into(),
                label: format!("{id}"),
            })
            .collect();
        let r = load_all(&Fake(entries));
        assert_eq!((r.images, r.audio, r.text, r.info), (1, 1, 1, 0));
        assert_eq!(r.failed, vec![(3, "nope".to_string())]);
    }
}
