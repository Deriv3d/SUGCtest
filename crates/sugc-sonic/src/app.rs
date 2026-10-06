//! Window, keyboard and audio around a [`genesis::Machine`].

use std::collections::VecDeque;
use std::num::NonZero;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use genesis::{Machine, button, vdp};

const FRAME_TIME: Duration = Duration::from_nanos(16_688_154); // 1 / 59.9227 Hz

/// Shared queue the emulator fills and the audio device drains.
type AudioQueue = Arc<Mutex<VecDeque<f32>>>;

struct QueueSource(AudioQueue);

impl Iterator for QueueSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        // Silence on underrun keeps the stream alive.
        Some(
            self.0
                .lock()
                .map(|mut q| q.pop_front().unwrap_or(0.0))
                .unwrap_or(0.0),
        )
    }
}

impl rodio::Source for QueueSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        NonZero::new(2).expect("2 is non-zero")
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        NonZero::new(genesis::SAMPLE_RATE).expect("non-zero")
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub struct App {
    machine: Machine,
    texture: Option<egui::TextureHandle>,
    next_frame: Instant,
    audio: AudioQueue,
    _sink: Option<rodio::MixerDeviceSink>,
    paused: bool,
}

impl App {
    pub fn new(machine: Machine) -> App {
        let audio: AudioQueue = Arc::new(Mutex::new(VecDeque::new()));
        let sink = rodio::DeviceSinkBuilder::open_default_sink().ok();
        if let Some(s) = &sink {
            s.mixer().add(QueueSource(audio.clone()));
        }
        App {
            machine,
            texture: None,
            next_frame: Instant::now(),
            audio,
            _sink: sink,
            paused: false,
        }
    }

    fn read_pad(ctx: &egui::Context) -> u16 {
        ctx.input(|i| {
            let mut p = 0;
            for (key, bit) in [
                (egui::Key::ArrowUp, button::UP),
                (egui::Key::ArrowDown, button::DOWN),
                (egui::Key::ArrowLeft, button::LEFT),
                (egui::Key::ArrowRight, button::RIGHT),
                (egui::Key::Z, button::A),
                (egui::Key::X, button::B),
                (egui::Key::C, button::C),
                (egui::Key::Enter, button::START),
            ] {
                if i.key_down(key) {
                    p |= bit;
                }
            }
            p
        })
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.key_pressed(egui::Key::P)) {
            self.paused = !self.paused;
        }
        self.machine.hw.pad[0] = Self::read_pad(&ctx);
        let now = Instant::now();
        let mut ran = 0;
        while !self.paused && now >= self.next_frame && ran < 4 {
            self.machine.run_frame();
            if let Ok(mut q) = self.audio.lock() {
                q.extend(self.machine.audio.iter().map(|&s| f32::from(s) / 32768.0));
                // Bound latency to ~100 ms.
                let max = genesis::SAMPLE_RATE as usize / 5;
                while q.len() > max {
                    q.pop_front();
                }
            }
            self.next_frame += FRAME_TIME;
            ran += 1;
        }
        if now > self.next_frame + FRAME_TIME * 4 {
            self.next_frame = now;
        }
        let img = egui::ColorImage::from_rgba_unmultiplied(
            [vdp::MAX_WIDTH, vdp::HEIGHT],
            self.machine.frame(),
        );
        match &mut self.texture {
            Some(t) => t.set(img, egui::TextureOptions::NEAREST),
            None => {
                self.texture = Some(ctx.load_texture("screen", img, egui::TextureOptions::NEAREST))
            }
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ui, |ui| {
                if let Some(t) = &self.texture {
                    let avail = ui.available_size();
                    let scale = (avail.x / vdp::MAX_WIDTH as f32)
                        .min(avail.y / vdp::HEIGHT as f32)
                        .max(1.0)
                        .floor();
                    let size = egui::vec2(vdp::MAX_WIDTH as f32, vdp::HEIGHT as f32) * scale;
                    ui.centered_and_justified(|ui| {
                        ui.add(egui::Image::from_texture(egui::load::SizedTexture::new(
                            t.id(),
                            size,
                        )));
                    });
                }
            });
        ctx.request_repaint_after(self.next_frame.saturating_duration_since(Instant::now()));
    }
}

pub fn run(machine: Machine) -> Result<(), String> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([vdp::MAX_WIDTH as f32 * 3.0, vdp::HEIGHT as f32 * 3.0])
            .with_title("Sonic the Hedgehog (SUGC native)"),
        ..Default::default()
    };
    eframe::run_native(
        "sugc-sonic",
        options,
        Box::new(move |_cc| Ok(Box::new(App::new(machine)))),
    )
    .map_err(|e| e.to_string())
}
