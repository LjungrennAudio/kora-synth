use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use midir::{MidiInput, MidiInputConnection};
use rand::Rng;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use crossbeam_channel::{bounded, Sender, Receiver};

/// KoraString: Karplus-Strong model tuned to one kora string
struct KoraString {
    buffer: Vec<f32>,
    pos: usize,
    feedback: f32,
    decay: f32,
}

impl KoraString {
    fn new(freq: f32, sample_rate: f32, decay: f32) -> Self {
        let length = (sample_rate / freq).max(2.0) as usize;
        let mut buffer = vec![0.0; length];
        let mut rng = rand::thread_rng();
        for s in &mut buffer {
            *s = rng.gen_range(-0.8..0.8);
        }
        Self {
            buffer,
            pos: 0,
            feedback: 0.985,
            decay,
        }
    }

    fn pluck(&mut self, intensity: f32) {
        let mut rng = rand::thread_rng();
        for s in &mut self.buffer {
            *s = rng.gen_range(-1.0..1.0) * intensity;
        }
        self.pos = 0;
    }

    fn get_sample(&mut self) -> f32 {
        if self.buffer.is_empty() {
            return 0.0;
        }
        let sample = self.buffer[self.pos];
        let next = (self.pos + 1) % self.buffer.len();
        let avg = (sample + self.buffer[next]) * 0.5 * self.feedback;
        self.buffer[self.pos] = avg * self.decay;
        self.pos = next;
        sample * 0.7 // slight gain
    }
}

/// 21-string Kora tuned to approximate Silaba (F tonic, Mande tradition like Diabaté)
/// Frequencies chosen for playable range and modeling stability (low end ~F2)
const KORA_TUNING: [f32; 21] = [
    87.31,  // 1  F2  (lowest, right or left hand start)
    98.00,  // 2  G2
    110.00, // 3  A2
    116.54, // 4  Bb2
    130.81, // 5  C3
    146.83, // 6  D3
    164.81, // 7  E3
    174.61, // 8  F3
    196.00, // 9  G3
    220.00, // 10 A3
    233.08, // 11 Bb3
    261.63, // 12 C4
    293.66, // 13 D4
    329.63, // 14 E4
    349.23, // 15 F4
    392.00, // 16 G4
    440.00, // 17 A4
    466.16, // 18 Bb4
    523.25, // 19 C5
    587.33, // 20 D5
    659.25, // 21 E5
];

struct KoraSynth {
    strings: Vec<KoraString>,
    sample_rate: f32,
}

impl KoraSynth {
    fn new(sample_rate: f32) -> Self {
        let strings: Vec<KoraString> = KORA_TUNING
            .iter()
            .map(|&f| KoraString::new(f, sample_rate, 0.992))
            .collect();
        Self { strings, sample_rate }
    }

    /// Pluck the string whose tuned pitch is closest to the target MIDI frequency
    fn pluck_closest(&mut self, midi_note: u8, velocity: f32) {
        let target_freq = 440.0 * 2.0_f32.powf((midi_note as f32 - 69.0) / 12.0);
        let mut best_idx = 0;
        let mut best_diff = f32::MAX;

        for (i, &tuned) in KORA_TUNING.iter().enumerate() {
            let diff = (tuned - target_freq).abs();
            if diff < best_diff {
                best_diff = diff;
                best_idx = i;
            }
        }

        let intensity = (velocity / 127.0).clamp(0.3, 1.0);
        self.strings[best_idx].pluck(intensity);
    }

    fn get_next_sample(&mut self) -> f32 {
        let mut sum = 0.0_f32;
        for s in &mut self.strings {
            sum += s.get_sample();
        }
        // Soft global mix + gentle body resonance simulation (simple low shelf feel)
        let mixed = sum * 0.045;
        mixed.clamp(-0.95, 0.95)
    }
}

fn main() {
    println!("🪕 Kora Real-Time MIDI Synth (Karplus-Strong + 21-string Silaba tuning)");
    println!("Connect a MIDI keyboard/controller. Play notes — closest kora string will be plucked.");
    println!("Press Ctrl+C to stop. (Works with virtual MIDI too)");

    // MIDI setup
    let midi_in = MidiInput::new("Kora MIDI Input").expect("MIDI input failed");
    let in_ports = midi_in.ports();
    if in_ports.is_empty() {
        println!("No MIDI input ports found. Connect a controller and restart.");
        return;
    }

    println!("Available MIDI ports:");
    for (i, p) in in_ports.iter().enumerate() {
        println!("  {}: {}", i, midi_in.port_name(p).unwrap_or_default());
    }
    // Use first port by default (change index if needed)
    let in_port = &in_ports[0];
    println!("Using port 0: {}", midi_in.port_name(in_port).unwrap_or_default());

    // Channel for MIDI events to audio thread
    let (tx, rx): (Sender<(u8, f32)>, Receiver<(u8, f32)>) = bounded(64);

    // Audio setup
    let host = cpal::default_host();
    let device = host.default_output_device().expect("No audio output device");
    let config = device.default_output_config().expect("No default config");
    let sample_rate = config.sample_rate().0 as f32;

    let synth = Arc::new(Mutex::new(KoraSynth::new(sample_rate)));

    // MIDI connection (moves tx)
    let synth_clone = synth.clone();
    let _conn: MidiInputConnection<()> = midi_in
        .connect(
            in_port,
            "kora-midi",
            move |_, message, _| {
                if message.len() >= 3 {
                    let status = message[0];
                    let note = message[1];
                    let vel = message[2] as f32;

                    if status & 0xF0 == 0x90 && vel > 0.0 {
                        // Note On
                        if let Ok(mut s) = synth_clone.lock() {
                            s.pluck_closest(note, vel);
                        }
                        let _ = tx.send((note, vel));
                    } else if status & 0xF0 == 0x80 || (status & 0xF0 == 0x90 && vel == 0.0) {
                        // Note Off - optional: could damp string but for now let natural decay
                    }
                }
            },
            (),
        )
        .expect("Failed to connect MIDI");

    // Audio callback
    let stream = device
        .build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                if let Ok(mut s) = synth.lock() {
                    for sample in data.iter_mut() {
                        *sample = s.get_next_sample();
                    }
                }
            },
            |err| eprintln!("Audio stream error: {}", err),
            None,
        )
        .expect("Failed to build audio stream");

    stream.play().expect("Failed to start audio");

    println!("\n🎹 Synth running! Play your MIDI keyboard now...");
    println!("Tip: Lower notes pluck lower kora strings. Try C3–E5 range.");

    // Keep alive
    std::thread::sleep(Duration::from_secs(300)); // 5 minutes, or Ctrl+C
    println!("\nStopped.");
}

