use hound;
use rand::Rng;

struct KoraString {
    buffer: Vec<f32>,
    pos: usize,
    feedback: f32,
    decay: f32,
}

impl KoraString {
    fn new(freq: f32, sample_rate: f32, decay: f32) -> Self {
        let length = (sample_rate / freq).max(1.0) as usize;
        let mut buffer = vec![0.0; length];
        let mut rng = rand::thread_rng();
        for sample in &mut buffer {
            *sample = rng.gen_range(-1.0..1.0) * 0.5;
        }
        Self {
            buffer,
            pos: 0,
            feedback: 0.98,
            decay,
        }
    }

    fn get_sample(&mut self) -> f32 {
        let sample = self.buffer[self.pos];
        let next_pos = (self.pos + 1) % self.buffer.len();
        let avg = (sample + self.buffer[next_pos]) * 0.5 * self.feedback;
        self.buffer[self.pos] = avg * self.decay;
        self.pos = next_pos;
        sample
    }

    fn pluck(&mut self, intensity: f32) {
        let mut rng = rand::thread_rng();
        for sample in &mut self.buffer {
            *sample = rng.gen_range(-1.0..1.0) * intensity;
        }
        self.pos = 0;
    }
}

struct KoraSynth {
    strings: Vec<KoraString>,
    sample_rate: f32,
}

impl KoraSynth {
    fn new(sample_rate: f32) -> Self {
        let base_freqs = [65.4, 73.4, 82.4, 98.0, 110.0, 130.8, 196.0, 246.9, 261.6, 329.6];
        let strings: Vec<KoraString> = base_freqs
            .iter()
            .map(|&f| KoraString::new(f, sample_rate, 0.995))
            .collect();
        Self { strings, sample_rate }
    }

    fn play_note(&mut self, note_index: usize, intensity: f32) {
        if note_index < self.strings.len() {
            self.strings[note_index].pluck(intensity);
        }
    }

    fn get_next_sample(&mut self) -> f32 {
        let mut sum = 0.0;
        for string in &mut self.strings {
            sum += string.get_sample() * 0.12; // Gentle mix
        }
        sum.clamp(-1.0, 1.0)
    }
}

fn main() {
    println!("🪕 Kora Physical Modeling Synth - Basic Karplus-Strong");
    println!("Generating 'kora_demo.wav'...");

    const SAMPLE_RATE: u32 = 44100;
    const DURATION_SECS: u32 = 45;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut writer = hound::WavWriter::create("kora_demo.wav", spec).expect("Failed to create WAV");
    let mut synth = KoraSynth::new(SAMPLE_RATE as f32);

    let sequence = vec![0, 4, 7, 9, 5, 2, 7, 4, 0, 9];
    let mut seq_index = 0;
    let samples_per_note = (SAMPLE_RATE as f32 * 0.35) as usize;
    let mut sample_in_note = 0;

    for _ in 0..(SAMPLE_RATE * DURATION_SECS) {
        let sample = synth.get_next_sample();
        writer.write_sample(sample).expect("Write failed");

        sample_in_note += 1;
        if sample_in_note >= samples_per_note {
            let note_idx = sequence[seq_index % sequence.len()];
            synth.play_note(note_idx, 0.85);
            seq_index += 1;
            sample_in_note = 0;
        }
    }

    writer.finalize().expect("Finalize failed");
    println!("✅ kora_demo.wav ready! Load it up and jam.");
    println!("Next: Add MIDI input, more strings, or gourd resonance filter?");
}