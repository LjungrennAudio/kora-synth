use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, EguiState};
use rand::Rng;
use std::sync::Arc;

#[derive(Clone)]
struct KoraString {
    buffer: Vec<f32>,
    pos: usize,
}

impl KoraString {
    fn new(freq: f32, sample_rate: f32) -> Self {
        let length = (sample_rate / freq).max(2.0) as usize;
        let mut rng = rand::thread_rng();
        let buffer: Vec<f32> = (0..length).map(|_| rng.gen_range(-0.8..0.8)).collect();
        Self { buffer, pos: 0 }
    }

    fn pluck(&mut self, intensity: f32) {
        let mut rng = rand::thread_rng();
        for s in &mut self.buffer {
            *s = rng.gen_range(-1.0..1.0) * intensity;
        }
        self.pos = 0;
    }

    fn get_sample(&mut self, feedback: f32, decay: f32) -> f32 {
        if self.buffer.is_empty() {
            return 0.0;
        }
        let sample = self.buffer[self.pos];
        let next = (self.pos + 1) % self.buffer.len();
        let avg = (sample + self.buffer[next]) * 0.5 * feedback;
        self.buffer[self.pos] = avg * decay;
        self.pos = next;
        sample * 0.7
    }
}

const KORA_TUNING: [f32; 21] = [
    87.31, 98.00, 110.00, 116.54, 130.81, 146.83, 164.81, 174.61, 196.00,
    220.00, 233.08, 261.63, 293.66, 329.63, 349.23, 392.00, 440.00, 466.16,
    523.25, 587.33, 659.25,
];

struct LowShelf {
    sample_rate: f32,
    b0: f32, b1: f32, b2: f32, a1: f32, a2: f32,
    x1: f32, x2: f32, y1: f32, y2: f32,
}

impl LowShelf {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }

    fn set_coeffs(&mut self, freq: f32, gain_db: f32, q: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / self.sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);
        let sqrt_a = a.sqrt();

        let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha);
        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha;

        self.b0 = b0 / a0; self.b1 = b1 / a0; self.b2 = b2 / a0;
        self.a1 = a1 / a0; self.a2 = a2 / a0;
    }

    fn process(&mut self, x0: f32) -> f32 {
        let y0 = self.b0 * x0 + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1; self.x1 = x0;
        self.y2 = self.y1; self.y1 = y0;
        y0
    }
}

struct GourdResonator {
    delay: Vec<f32>,
    write_pos: usize,
    sample_rate: f32,
    damp_state: f32,
}

impl GourdResonator {
    fn new(sample_rate: f32) -> Self {
        let max_len = (sample_rate / 40.0).ceil() as usize;
        Self {
            delay: vec![0.0; max_len],
            write_pos: 0,
            sample_rate,
            damp_state: 0.0,
        }
    }

    fn process(&mut self, input: f32, freq: f32, feedback: f32, damping: f32) -> f32 {
        let delay_len = (self.sample_rate / freq).clamp(2.0, self.delay.len() as f32 - 1.0) as usize;
        let read_pos = (self.write_pos + self.delay.len() - delay_len) % self.delay.len();
        let out = self.delay[read_pos];

        self.damp_state = out * (1.0 - damping) + self.damp_state * damping;
        self.delay[self.write_pos] = input + self.damp_state * feedback;
        self.write_pos = (self.write_pos + 1) % self.delay.len();
        out
    }
}

struct KoraSynth {
    strings: Vec<KoraString>,
    shelf: LowShelf,
    body: GourdResonator,
}

impl KoraSynth {
    fn new(sample_rate: f32) -> Self {
        let strings = KORA_TUNING.iter().map(|&f| KoraString::new(f, sample_rate)).collect();
        Self {
            strings,
            shelf: LowShelf::new(sample_rate),
            body: GourdResonator::new(sample_rate),
        }
    }

    fn pluck_closest(&mut self, midi_note: u8, velocity: f32, tune_semitones: f32) {
        let target_freq = 440.0 * 2.0_f32.powf((midi_note as f32 - 69.0 + tune_semitones) / 12.0);
        let mut best_idx = 0;
        let mut best_diff = f32::MAX;
        for (i, &tuned) in KORA_TUNING.iter().enumerate() {
            let diff = (tuned - target_freq).abs();
            if diff < best_diff {
                best_diff = diff;
                best_idx = i;
            }
        }
        let intensity = velocity.clamp(0.3, 1.0);
        self.strings[best_idx].pluck(intensity);
    }

    fn get_next_sample(
        &mut self,
        feedback: f32,
        decay: f32,
        mix: f32,
        shelf_freq: f32,
        shelf_gain: f32,
        body_freq: f32,
        body_feedback: f32,
        body_damping: f32,
    ) -> f32 {
        let mut sum = 0.0;
        for s in &mut self.strings {
            sum += s.get_sample(feedback, decay);
        }
        let dry = (sum * mix).clamp(-0.95, 0.95);
        self.shelf.set_coeffs(shelf_freq, shelf_gain, 0.707);
        let shaped = self.shelf.process(dry);
        let resonated = self.body.process(shaped, body_freq, body_feedback, body_damping);
        (shaped * 0.75 + resonated * 0.25).clamp(-0.95, 0.95)
    }
}

#[derive(Params)]
struct KoraVstParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,
    #[id = "decay"]
    pub decay: FloatParam,
    #[id = "feedback"]
    pub feedback: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "shelf_freq"]
    pub shelf_freq: FloatParam,
    #[id = "shelf_gain"]
    pub shelf_gain: FloatParam,
    #[id = "body_freq"]
    pub body_freq: FloatParam,
    #[id = "body_feedback"]
    pub body_feedback: FloatParam,
    #[id = "body_damping"]
    pub body_damping: FloatParam,
    #[id = "tune"]
    pub tune_semitones: FloatParam,
}

impl Default for KoraVstParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(320, 420),
            decay: FloatParam::new("Decay", 0.992, FloatRange::Linear { min: 0.9, max: 0.999 })
                .with_smoother(SmoothingStyle::Linear(20.0)),
            feedback: FloatParam::new("Feedback", 0.985, FloatRange::Linear { min: 0.9, max: 0.999 })
                .with_smoother(SmoothingStyle::Linear(20.0)),
            mix: FloatParam::new("Mix", 0.045, FloatRange::Linear { min: 0.01, max: 0.1 })
                .with_smoother(SmoothingStyle::Linear(20.0)),
            shelf_freq: FloatParam::new(
                "Body Warmth Freq",
                300.0,
                FloatRange::Skewed { min: 100.0, max: 600.0, factor: FloatRange::skew_factor(-1.0) },
            )
            .with_unit(" Hz")
            .with_smoother(SmoothingStyle::Linear(30.0)),
            shelf_gain: FloatParam::new(
                "Body Warmth Gain",
                6.0,
                FloatRange::Linear { min: 0.0, max: 12.0 },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(30.0)),
            body_freq: FloatParam::new(
                "Gourd Resonance",
                110.0,
                FloatRange::Skewed { min: 60.0, max: 200.0, factor: FloatRange::skew_factor(-1.0) },
            )
            .with_unit(" Hz")
            .with_smoother(SmoothingStyle::Linear(30.0)),
            body_feedback: FloatParam::new(
                "Gourd Sustain",
                0.35,
                FloatRange::Linear { min: 0.0, max: 0.6 },
            )
            .with_smoother(SmoothingStyle::Linear(30.0)),
            body_damping: FloatParam::new(
                "Gourd Damping",
                0.25,
                FloatRange::Linear { min: 0.0, max: 0.6 },
            )
            .with_smoother(SmoothingStyle::Linear(30.0)),
            tune_semitones: FloatParam::new(
                "Master Tune",
                0.0,
                FloatRange::Linear { min: -12.0, max: 12.0 },
            )
            .with_unit(" st")
            .with_smoother(SmoothingStyle::Linear(10.0)),
        }
    }
}

struct KoraVst {
    params: Arc<KoraVstParams>,
    synth: Option<KoraSynth>,
}

impl Default for KoraVst {
    fn default() -> Self {
        Self {
            params: Arc::new(KoraVstParams::default()),
            synth: None,
        }
    }
}

impl Plugin for KoraVst {
    const NAME: &'static str = "KoraSynth";
    const VENDOR: &'static str = "RYOModular";
    const URL: &'static str = "https://ryomodular.com";
    const EMAIL: &'static str = "wofl@whispr.dev";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[],
        aux_output_ports: &[],
        names: PortNames::const_default(),
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |_, _| {},
            move |ctx, setter, _state| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("KoraSynth");
                    ui.add(egui::Slider::from_get_set(0.9..=0.999, |v| {
                        if let Some(v) = v { setter.set_parameter(&params.decay, v as f32); }
                        params.decay.value() as f64
                    }).text("Decay"));
                    ui.add(egui::Slider::from_get_set(0.9..=0.999, |v| {
                        if let Some(v) = v { setter.set_parameter(&params.feedback, v as f32); }
                        params.feedback.value() as f64
                    }).text("Feedback"));
                    ui.add(egui::Slider::from_get_set(0.01..=0.1, |v| {
                        if let Some(v) = v { setter.set_parameter(&params.mix, v as f32); }
                        params.mix.value() as f64
                    }).text("Mix"));
                    ui.add(egui::Slider::from_get_set(100.0..=600.0, |v| {
                        if let Some(v) = v { setter.set_parameter(&params.shelf_freq, v as f32); }
                        params.shelf_freq.value() as f64
                    }).text("Body Warmth Freq"));
                    ui.add(egui::Slider::from_get_set(0.0..=12.0, |v| {
                        if let Some(v) = v { setter.set_parameter(&params.shelf_gain, v as f32); }
                        params.shelf_gain.value() as f64
                    }).text("Body Warmth Gain"));
                    ui.add(egui::Slider::from_get_set(60.0..=200.0, |v| {
                        if let Some(v) = v { setter.set_parameter(&params.body_freq, v as f32); }
                        params.body_freq.value() as f64
                    }).text("Gourd Resonance"));
                    ui.add(egui::Slider::from_get_set(0.0..=0.6, |v| {
                        if let Some(v) = v { setter.set_parameter(&params.body_feedback, v as f32); }
                        params.body_feedback.value() as f64
                    }).text("Gourd Sustain"));
                    ui.add(egui::Slider::from_get_set(0.0..=0.6, |v| {
                        if let Some(v) = v { setter.set_parameter(&params.body_damping, v as f32); }
                        params.body_damping.value() as f64
                    }).text("Gourd Damping"));
                    ui.add(egui::Slider::from_get_set(-12.0..=12.0, |v| {
                        if let Some(v) = v { setter.set_parameter(&params.tune_semitones, v as f32); }
                        params.tune_semitones.value() as f64
                    }).text("Master Tune (semitones)"));
                });
            },
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.synth = Some(KoraSynth::new(buffer_config.sample_rate));
        true
    }

    fn reset(&mut self) {}

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let synth = match &mut self.synth {
            Some(s) => s,
            None => return ProcessStatus::Normal,
        };

        let mut next_event = context.next_event();
        let mut sample_id = 0;

        for channel_samples in buffer.iter_samples() {
            let tune = self.params.tune_semitones.smoothed.next();
        
            while let Some(event) = next_event {
                if event.timing() as usize > sample_id {
                    break;
                }
                if let NoteEvent::NoteOn { note, velocity, .. } = event {
                    synth.pluck_closest(note, velocity, tune);
                }
                next_event = context.next_event();
            }
            sample_id += 1;
        
            let decay = self.params.decay.smoothed.next();
            let feedback = self.params.feedback.smoothed.next();
            let mix = self.params.mix.smoothed.next();
            let shelf_freq = self.params.shelf_freq.smoothed.next();
            let shelf_gain = self.params.shelf_gain.smoothed.next();
            let body_freq = self.params.body_freq.smoothed.next();
            let body_feedback = self.params.body_feedback.smoothed.next();
            let body_damping = self.params.body_damping.smoothed.next();
        
            let sample = synth.get_next_sample(
                feedback, decay, mix,
                shelf_freq, shelf_gain,
                body_freq, body_feedback, body_damping,
            );
        
            for out in channel_samples {
                *out = sample;
            }
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for KoraVst {
    const CLAP_ID: &'static str = "com.ryomodular.kora";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("21-string Kora Physical Modeling Synth");
    const CLAP_MANUAL_URL: Option<&'static str> = Some("https://ryomodular.com");
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::Instrument, ClapFeature::Synthesizer];
}

impl Vst3Plugin for KoraVst {
    const VST3_CLASS_ID: [u8; 16] = *b"RyomKoraSynth001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Synth, Vst3SubCategory::Instrument];
}

nih_export_clap!(KoraVst);
nih_export_vst3!(KoraVst);
