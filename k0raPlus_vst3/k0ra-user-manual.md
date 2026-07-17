

K0raSynth — User Manual

\-----------------------

KoraSynth is a physically-modeled emulation of the 21-string West African kora, built in Rust using the nih\_plug framework and running as a VST3/CLAP instrument. Rather than playing back samples, it synthesizes each string's vibration in real time using a classic algorithm called Karplus-Strong synthesis, then shapes the result through a simulated gourd resonator body.





What Is Kora Synthesis, Actually

\--------------------------------

A real kora has 21 strings stretched across a large calabash gourd body, each tuned to a fixed pitch, plucked directly by the player's fingers rather than fretted like a guitar. Because the strings are fixed-pitch, this plugin works the same way — it doesn't pitch-shift a single oscillator, it has 21 independent virtual strings sitting ready, and playing a MIDI note simply "plucks" whichever string is tuned closest to that note.





The Math: Karplus-Strong String Model

\-------------------------------------

Each string is modeled using the Karplus-Strong algorithm, one of the most elegant tricks in digital audio: fill a short buffer with random noise, then repeatedly feed it back through itself with slight averaging and decay, and it naturally settles into a decaying pitched tone.



* Step 1 — Excitation. When plucked, the string's delay-line buffer (length = sample\_rate ÷ frequency) is filled with random values scaled by velocity, simulating the noisy transient of a finger striking a string.



* Step 2 — Resonance. Each sample, the algorithm reads the current buffer position, averages it with the next position, and multiplies by a feedback coefficient:

&#x09;

𝑦𝑛 = 	(𝑥𝑛 + (𝑥𝑛 + 1)) × 𝑓

&#x09;    /2



That averaging step acts as a simple low-pass filter — it's why plucked strings sound bright at first and mellow out as they decay, exactly like a real string losing its high harmonics fastest.



* Step 3 — Decay. The result is then multiplied by a decay factor before being written back into the buffer, controlling how long the string rings out before falling silent.



Because buffer length is inversely proportional to frequency, low strings get long buffers (longer delay = lower pitch) and high strings get short ones — this is the same math that determines why a longer guitar string sounds lower than a short one.





The Gourd Resonator Body

\------------------------

A real kora's calabash body doesn't just project sound — it physically colors it, boosting certain frequencies and adding its own short resonant "bloom" after each pluck. Two DSP stages recreate this:



Low-shelf filter — a biquad filter (RBJ Audio-EQ-Cookbook formula) that boosts frequencies below a set cutoff, mimicking the warm bass lift a gourd body naturally provides.



Body resonator — a short comb-filter delay line tuned to a low fundamental frequency, with feedback and damping controls, recreating the way a wooden/gourd cavity keeps "singing" briefly after being excited.



Both stages run once on the combined (summed) output of all 21 strings, not per-string — this matches how a real kora works acoustically, since all strings share one resonating body rather than having 21 separate bodies.





MIDI Implementation

\-------------------

Input: Basic MIDI (Note On/Off, velocity) — no MIDI output.



Note-to-string mapping: on Note On, the plugin calculates the target frequency from the MIDI note number using the standard equal-temperament formula

𝑓 = 440 × 2(𝑛 − 69) /12, then plucks whichever of the 21 fixed-tuned strings is closest in frequency.



Velocity: mapped directly to pluck intensity (clamped between 0.3 and 1.0), so soft playing produces a gentler noise burst and quieter tone, hard playing produces a sharper, louder pluck.



Polyphony caveat: because each MIDI note simply re-triggers its nearest fixed string, playing two notes that map to the same string will cut off the first pluck — this mirrors a real kora, where you can't play two different pitches on one string simultaneously.



No pitch bend or aftertouch support currently — notes always snap to the nearest of the 21 fixed tunings, they don't glide.





Controls Reference

\----------------	--------------------------------------------------------------	----------	-----------------------------------------------------

Control			What it does							Range		Musical effect

================	==============================================================	==========	=====================================================

Decay			How quickly plucked strings lose energy internally		0.9–0.999	Higher = strings ring out longer, more sustain

Feedback		Strength of the string's internal averaging feedback		0.9–0.999	Higher = brighter, more resonant tone; too high risks 														instability

Mix			Overall output level of the summed string signal		0.01–0.1	Controls overall loudness before body shaping

Body Warmth Freq	Low-shelf filter cutoff frequency				100–600 Hz	Lower = warmth boost applies to a wider low range

Body Warmth Gain	Low-shelf boost amount						0–12 dB		Higher = boomier, thicker low end

Gourd Resonance		Body resonator's tuned frequency				60–200 Hz	Simulates gourd size — lower = bigger, deeper body

Gourd Sustain		Body resonator's feedback amount				0–0.6		How long the body "sings" after each pluck

Gourd Damping		How quickly high frequencies die out inside the body resonance	0–0.6		Higher = duller, warmer resonance tail





Practical Tuning Tips

Start with Gourd Sustain low (around 0.2–0.35) — pushing it toward the top of its 0.6 range combined with high string Feedback can compound into audible runaway resonance, since both stages feed back independently.



Body Warmth Freq around 250–350 Hz with Gain around 4–8 dB gives a natural "wooden" warmth without muddying the plucked attack.



Lowering Gourd Resonance toward 60–80 Hz simulates a larger, deeper-bodied instrument; pushing it toward 150–200 Hz gives a tighter, smaller-sounding gourd.



Because all 21 strings are fixed-pitch, this instrument is best played diatonically/pentatonically in the kora's traditional tuning rather than chromatically — notes that fall between the 21 available pitches will simply snap to the nearest string.

