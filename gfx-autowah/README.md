# gfx-autowah

> **Status: CODED** (v0.1.0, 2026-11-12) — follower relative to the playing's long-term level, LP / BP / HP (2 or 4 poles), wah voice, BP + dry, vowel, drive into the filter, bass keep, envelope / LFO / S&H sources synced to the host; structural changes dip the wet 5 ms; 5 behaviour tests, bench green.

golem's envelope filter. Stands for golem's `autowah` block (today guitarix gxautowah), keeping its
parameter id ([golem contract](../docs/golem-contract.md)): `wah` 0 – 1, which here means **sensitivity**
(in [gfx-wah](../gfx-wah/) it is the pedal position — hence two plugins). The aim: the Mu-Tron III / Q-Tron
family done right — a follower that works at any input level, low-pass / band-pass / high-pass, up / down,
the "chewy" saturated filter, and dry lows for bass.

## 1. What golem has today

guitarix gxautowah [V]: a peak follower (instant attack, ~100 ms release) drives a Cry Baby–style
resonance; the catalogue's "level × 20 × wah" mapping only covers the top ~26 dB of input level, so the
effect depends on how hot the signal arrives.

## 2. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[I]** inference.

| Reference | Facts | What we take |
|---|---|---|
| [Mu-Tron III](https://mu-tron.com/vintage-musitronics/mu-tron-iii/) [V] | State-variable filter, cutoff set by LDRs lit by one LED; mode LP / BP / HP, Peak, Gain (sensitivity), Range low / high, Drive up / down | The reference |
| [EHX Q-Tron+](https://www.ehx.com/wp-content/uploads/2021/07/q-tron-plus-manual.pdf) [V] | LP / BP / HP / **Mix** (BP + dry), Response slow / fast, Range, Drive up / down, Peak, Gain, Boost; more gain **saturates the filter** — the "chewy" sound | BP + dry, saturation |
| [Moog MF-101](https://www.fullcompass.com/common/files/86271-MF101SManual.pdf) [V] | Drive, 8-octave cutoff, resonance to self-oscillation, envelope amount, mix, smooth / fast, **2-pole / 4-pole** | Poles, mix |
| [Boss AW-3](https://www.boss.info/id/products/aw-3) [V] | Up, Down, Sharp, Humanizer, Tempo; decay; bass input | Direction, tempo mode |
| [sst-filters](https://github.com/surge-synthesizer/sst-filters) (GPL-3) [V] | Cytomic SVF, ladders | Filter references |

### What we conclude

1. **A follower in dB**, with a sidechain high-pass (80 – 150 Hz, so low strings do not dominate), separate
   attack and release: `wah` sets the dB window, so it works at any input level.
2. Position → cutoff **exponentially** between `low` and `high`; up / down.
3. LP / BP / HP / Wah / BP + dry; 2 or 4 poles; resonance; **saturation in the filter** (ADAA).
4. An opto response (level-dependent, lagging release, like the Mu-Tron's LDR).
5. LFO, sample & hold and envelope + LFO sources; dry lows for bass.

## 3. Design

```
in ─┬───────────────────────────────────────────────────────────────── dry (below bass_keep) ──┐
    └▶ sidechain HPF ▶ follower (dB, attack / release / response) ▶ position (wah = window) ─┐ │
                       source: envelope · LFO · S&H · envelope + LFO ───────────────────────┘ │
    in ▶ drive (ADAA) ▶ TPT SVF ×1 or ×2 (type, peak, cutoff = low · (high / low)^pos) ▶ mix ▶ level ┴▶ out
```

## 4. Parameters

Catalogue id kept.

| id | Name | Type | Range | Catalogue | Default | Role |
|---|---|---|---|---|---|---|
| `wah` | Sensitivity | f | 0 – 1 | same | 0.3 | Envelope window: higher = reacts to softer playing |
| `type` | Type | e | Low-pass · Band-pass · High-pass · Wah · Band-pass + Dry · Vowel | new | Band-pass | Wah = gfx-wah's GCB-95 voice. Vowel = two formant band-passes morphing a → e → i → o → u with the sweep (talk-box / Boss AW-3 Humanizer) |
| `poles` | Poles | e | 2 · 4 | new | 2 | |
| `direction` | Direction | e | Up · Down | new | Up | |
| `low` | Low | f | 50 – 2,000 Hz | new | 250 Hz | |
| `high` | High | f | 200 – 8,000 Hz | new | 2,500 Hz | |
| `peak` | Peak | f | 0.5 – 20 | new | 4 | Q |
| `attack` | Attack | f | 0.5 – 100 ms | new | 5 ms | |
| `release` | Release | f | 10 – 2,000 ms | new | 150 ms | |
| `response` | Response | e | Fast · Smooth · Opto | new | Fast | Opto = level-dependent lagging release |
| `drive` | Drive | f | 0 – 24 dB | new | 0 dB | Saturation into the filter ("chew") |
| `mix` | Mix | f | 0 – 100 % | new | 100 % | |
| `level` | Level | f | −20 – +12 dB | new | 0 dB | Check plain ↔ normalized on REAPER (negative minimum) |
| `bass_keep` | Bass Keep | f | 0 (off) – 300 Hz | new | 0 | Linkwitz-Riley: dry lows |
| `source` | Source | e | Envelope · LFO · S&H · Envelope + LFO | new | Envelope | |
| `rate` | Rate | f | 0.05 – 10 Hz | new | 1 Hz | LFO / S&H |
| `sync` | Sync | e | Off · divisions | new | Off | |

## 5. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Funk guitar "quack" (Mu-Tron III) | Band-pass, Up, `low` 250 Hz, `high` 2.5 kHz, `peak` 5, `wah` 0.5 | Level-independent follower: same quack at any guitar volume |
| Bootsy bass | Low-pass, Up, `low` 80 Hz, `high` 1.2 kHz, `peak` 6, `bass_keep` 120 Hz | Dry lows: the bottom never thins |
| Jerry Garcia leads | Band-pass + Dry, Up, `response` Opto, `release` 250 ms | Opto lag |
| Downward "ow" sweep | `direction` Down | |
| Chewy, saturated filter | `drive` 12 dB, `peak` 8 | Saturation inside the filter |
| Synth-like 4-pole sweep (Moog) | Low-pass, `poles` 4, `peak` 10 | |
| Auto-wah with a wah voice | `type` Wah | |
| Talk-box vowels | `type` Vowel, Up, `wah` 0.6 | Formant morph |
| Tempo-synced filter sweep | `source` LFO, `sync` 1/4 | |
| Random stepped filter (synth) | `source` S&H, `sync` 1/16 | |
| Keys / clavinet | Band-pass, `attack` 2 ms, `release` 100 ms | |

## 6. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| Level independence | the same phrase at −30, −20, −10 dBFS sweeps the same range (± 10 %) at a fixed `wah` |
| Sweep law | cutoff exponential between `low` and `high`; up / down mirrored |
| Stability | peak 20, 4 poles, audio-rate sweep: no instability, no click |
| Drive | saturates in the filter: THD grows with `drive` |
| Bass | `bass_keep` 150 Hz on a low E: fundamental untouched |
| Vowel | formant pairs within 5 % of the target vowel table at 5 sweep positions |
| Listening | funk.wav and bass (to record) with golem's autowah presets, against gxautowah |
