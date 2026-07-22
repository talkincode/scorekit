//! Static instrument registry: family taxonomy, ranges, envelopes, roles and
//! aliases for every `Instrument` variant.
//!
//! This is the factual backbone of instrument resolution (`resolver.rs`).
//! Everything here is hand-authored, rule-based data — deliberately coarse
//! first-phase values (documented approximations, not measurements), and
//! deliberately *not* derived from audio analysis or learned models: the
//! registry must be deterministic and reviewable as plain text.

use crate::schema::{Articulation, Instrument, instrument_key};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Instrument family — the primary axis of substitution decisions. Fallback
/// never crosses families unless the caller explicitly allows it, and never
/// treats any family (strings in particular) as a default absorber.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Strings,
    Woodwinds,
    Brass,
    Percussion,
    Keyboards,
    Plucked,
    Guitars,
    Bass,
    Synth,
    /// Reserved: no built-in instrument maps here yet; valid in resolver
    /// configuration so policies stay stable as the vocabulary grows.
    Ethnic,
    Vocals,
    /// Reserved: ambience/SFX live in the texture system, not the melodic
    /// instrument enum; valid in resolver configuration for forward
    /// compatibility.
    Textures,
}

/// Musical role an instrument credibly serves. A track's role is derived
/// from its `pattern` when scoring fallback candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Melody,
    CounterMelody,
    HarmonicSupport,
    Bass,
    Pad,
    Rhythm,
}

/// Coarse attack/release speed classes — enough to keep a plucked decay from
/// standing in for a held pad, without pretending to measure real envelopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Speed {
    Fast,
    Medium,
    Slow,
}

impl Speed {
    fn ordinal(self) -> i8 {
        match self {
            Speed::Fast => 0,
            Speed::Medium => 1,
            Speed::Slow => 2,
        }
    }

    /// 1.0 identical, 0.5 adjacent, 0.0 opposite.
    pub fn similarity(self, other: Speed) -> f32 {
        match (self.ordinal() - other.ordinal()).abs() {
            0 => 1.0,
            1 => 0.5,
            _ => 0.0,
        }
    }
}

/// Coarse amplitude-envelope profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Envelope {
    pub attack: Speed,
    pub release: Speed,
    /// Can hold a note indefinitely (bowed/blown/organ/synth) as opposed to
    /// a struck/plucked decay.
    pub sustain: bool,
}

/// Coarse timbre coordinates in 0.0..=1.0 — the lowest-weight scoring axis.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Timbre {
    pub brightness: f32,
    pub warmth: f32,
}

/// Everything the resolver knows about one instrument.
#[derive(Debug, Clone, Copy)]
pub struct InstrumentSpec {
    pub family: Family,
    /// Finer grouping inside the family (e.g. `violin_family` vs `section`).
    pub subfamily: &'static str,
    /// Sounding range as inclusive MIDI note numbers (C4 = 60).
    pub range: (u8, u8),
    /// Playing techniques the instrument can credibly perform.
    pub articulations: &'static [Articulation],
    pub envelope: Envelope,
    pub roles: &'static [Role],
    /// Electronic/synthesized timbre; substituting synth for an acoustic
    /// request is gated by `FallbackPolicy::allow_synth`.
    pub synthetic: bool,
    pub timbre: Timbre,
}

impl InstrumentSpec {
    /// Semitones of `self`'s range that `candidate` can also sound, as a
    /// fraction of `self`'s range (1.0 = full coverage, 0.0 = disjoint).
    pub fn range_coverage_by(&self, candidate: &InstrumentSpec) -> f32 {
        let (lo, hi) = self.range;
        let (clo, chi) = candidate.range;
        let overlap_lo = lo.max(clo);
        let overlap_hi = hi.min(chi);
        if overlap_lo > overlap_hi {
            return 0.0;
        }
        f32::from(overlap_hi - overlap_lo + 1) / f32::from(hi - lo + 1)
    }
}

/// Every `Instrument` variant, in enum declaration order. Guarded by a test
/// against the exported JSON schema so it cannot silently fall out of sync.
pub const ALL: [Instrument; 60] = {
    use Instrument::*;
    [
        Piano,
        BrightPiano,
        Epiano,
        Harpsichord,
        Celesta,
        Glockenspiel,
        MusicBox,
        Vibraphone,
        Marimba,
        Xylophone,
        TubularBells,
        Organ,
        Accordion,
        Guitar,
        SteelGuitar,
        ElectricGuitar,
        MutedGuitar,
        Bass,
        PickedBass,
        FretlessBass,
        SlapBass,
        SynthBass,
        Violin,
        Viola,
        Cello,
        Contrabass,
        TremoloStrings,
        Pizzicato,
        Harp,
        Timpani,
        Strings,
        SlowStrings,
        SynthStrings,
        Choir,
        Voice,
        Trumpet,
        Trombone,
        Tuba,
        Horn,
        Brass,
        Sax,
        Oboe,
        EnglishHorn,
        Bassoon,
        Clarinet,
        Piccolo,
        Flute,
        Recorder,
        PanFlute,
        Whistle,
        Ocarina,
        SquareLead,
        SawLead,
        Pad,
        WarmPad,
        ChoirPad,
        BowedPad,
        HaloPad,
        SweepPad,
        Drums,
    ]
};

const ART_BOWED: &[Articulation] = &[
    Articulation::Sustain,
    Articulation::Staccato,
    Articulation::Spiccato,
    Articulation::Pizzicato,
    Articulation::Tremolo,
    Articulation::Mute,
];
const ART_BRASS: &[Articulation] = &[
    Articulation::Sustain,
    Articulation::Staccato,
    Articulation::Mute,
];
const ART_BASIC: &[Articulation] = &[Articulation::Sustain, Articulation::Staccato];
const ART_GUITAR: &[Articulation] = &[
    Articulation::Sustain,
    Articulation::Staccato,
    Articulation::Mute,
];
const ART_ROLL: &[Articulation] = &[
    Articulation::Sustain,
    Articulation::Staccato,
    Articulation::Tremolo,
];
const ART_PIZZ: &[Articulation] = &[
    Articulation::Sustain,
    Articulation::Staccato,
    Articulation::Pizzicato,
];
const ART_PAD: &[Articulation] = &[Articulation::Sustain];

const SUS_MED: Envelope = Envelope {
    attack: Speed::Medium,
    release: Speed::Medium,
    sustain: true,
};
const SUS_MED_FAST: Envelope = Envelope {
    attack: Speed::Medium,
    release: Speed::Fast,
    sustain: true,
};
const SUS_FAST: Envelope = Envelope {
    attack: Speed::Fast,
    release: Speed::Fast,
    sustain: true,
};
const SUS_SLOW: Envelope = Envelope {
    attack: Speed::Slow,
    release: Speed::Slow,
    sustain: true,
};
const SUS_CHOIR: Envelope = Envelope {
    attack: Speed::Slow,
    release: Speed::Medium,
    sustain: true,
};
const DECAY_FAST: Envelope = Envelope {
    attack: Speed::Fast,
    release: Speed::Fast,
    sustain: false,
};
const DECAY_MED: Envelope = Envelope {
    attack: Speed::Fast,
    release: Speed::Medium,
    sustain: false,
};
const DECAY_LONG: Envelope = Envelope {
    attack: Speed::Fast,
    release: Speed::Slow,
    sustain: false,
};

/// Registry lookup. Total over the enum — a test enforces that every variant
/// has an entry with a sane range.
pub fn spec(i: Instrument) -> InstrumentSpec {
    use Family as F;
    use Instrument as I;
    use Role::{
        Bass as RB, CounterMelody as RC, HarmonicSupport as RH, Melody as RM, Pad as RP,
        Rhythm as RR,
    };
    #[allow(clippy::too_many_arguments)]
    fn s(
        family: Family,
        subfamily: &'static str,
        low: u8,
        high: u8,
        articulations: &'static [Articulation],
        envelope: Envelope,
        roles: &'static [Role],
        synthetic: bool,
        brightness: f32,
        warmth: f32,
    ) -> InstrumentSpec {
        InstrumentSpec {
            family,
            subfamily,
            range: (low, high),
            articulations,
            envelope,
            roles,
            synthetic,
            timbre: Timbre { brightness, warmth },
        }
    }
    match i {
        I::Piano => s(
            F::Keyboards,
            "piano",
            21,
            108,
            ART_BASIC,
            DECAY_MED,
            &[RM, RC, RH, RB, RR],
            false,
            0.55,
            0.60,
        ),
        I::BrightPiano => s(
            F::Keyboards,
            "piano",
            21,
            108,
            ART_BASIC,
            DECAY_MED,
            &[RM, RC, RH, RB, RR],
            false,
            0.70,
            0.45,
        ),
        I::Epiano => s(
            F::Keyboards,
            "epiano",
            28,
            103,
            ART_BASIC,
            DECAY_MED,
            &[RM, RH, RP],
            false,
            0.45,
            0.70,
        ),
        I::Harpsichord => s(
            F::Keyboards,
            "harpsichord",
            29,
            89,
            ART_BASIC,
            DECAY_FAST,
            &[RM, RH, RR],
            false,
            0.75,
            0.35,
        ),
        I::Celesta => s(
            F::Percussion,
            "mallets",
            60,
            108,
            ART_BASIC,
            DECAY_MED,
            &[RM, RC],
            false,
            0.80,
            0.40,
        ),
        I::Glockenspiel => s(
            F::Percussion,
            "mallets",
            79,
            108,
            ART_BASIC,
            DECAY_MED,
            &[RM, RC],
            false,
            0.95,
            0.15,
        ),
        I::MusicBox => s(
            F::Percussion,
            "mallets",
            60,
            96,
            ART_BASIC,
            DECAY_MED,
            &[RM, RH],
            false,
            0.80,
            0.30,
        ),
        I::Vibraphone => s(
            F::Percussion,
            "mallets",
            53,
            89,
            ART_ROLL,
            DECAY_LONG,
            &[RM, RC, RH, RP],
            false,
            0.60,
            0.50,
        ),
        I::Marimba => s(
            F::Percussion,
            "mallets",
            36,
            96,
            ART_ROLL,
            DECAY_FAST,
            &[RM, RH, RR],
            false,
            0.50,
            0.60,
        ),
        I::Xylophone => s(
            F::Percussion,
            "mallets",
            65,
            108,
            ART_BASIC,
            DECAY_FAST,
            &[RM, RR],
            false,
            0.90,
            0.20,
        ),
        I::TubularBells => s(
            F::Percussion,
            "bells",
            60,
            77,
            ART_BASIC,
            DECAY_LONG,
            &[RM, RH],
            false,
            0.70,
            0.40,
        ),
        I::Organ => s(
            F::Keyboards,
            "organ",
            36,
            96,
            ART_BASIC,
            SUS_FAST,
            &[RM, RH, RP, RB],
            false,
            0.50,
            0.55,
        ),
        I::Accordion => s(
            F::Keyboards,
            "accordion",
            41,
            93,
            ART_BASIC,
            SUS_MED_FAST,
            &[RM, RH, RP],
            false,
            0.55,
            0.55,
        ),
        I::Guitar => s(
            F::Guitars,
            "nylon",
            40,
            83,
            ART_GUITAR,
            DECAY_MED,
            &[RM, RH, RR],
            false,
            0.45,
            0.70,
        ),
        I::SteelGuitar => s(
            F::Guitars,
            "steel",
            40,
            88,
            ART_GUITAR,
            DECAY_MED,
            &[RM, RH, RR],
            false,
            0.65,
            0.55,
        ),
        I::ElectricGuitar => s(
            F::Guitars,
            "electric",
            40,
            88,
            ART_GUITAR,
            DECAY_MED,
            &[RM, RH, RR],
            false,
            0.55,
            0.60,
        ),
        I::MutedGuitar => s(
            F::Guitars,
            "electric",
            40,
            88,
            ART_GUITAR,
            DECAY_FAST,
            &[RH, RR],
            false,
            0.50,
            0.50,
        ),
        I::Bass => s(
            F::Bass,
            "electric",
            28,
            67,
            ART_GUITAR,
            DECAY_MED,
            &[RB],
            false,
            0.35,
            0.70,
        ),
        I::PickedBass => s(
            F::Bass,
            "electric",
            28,
            67,
            ART_GUITAR,
            DECAY_MED,
            &[RB],
            false,
            0.50,
            0.55,
        ),
        I::FretlessBass => s(
            F::Bass,
            "electric",
            28,
            67,
            ART_GUITAR,
            DECAY_MED,
            &[RB],
            false,
            0.35,
            0.75,
        ),
        I::SlapBass => s(
            F::Bass,
            "electric",
            28,
            67,
            ART_GUITAR,
            DECAY_FAST,
            &[RB, RR],
            false,
            0.65,
            0.45,
        ),
        I::SynthBass => s(
            F::Bass,
            "synth",
            24,
            60,
            ART_BASIC,
            SUS_FAST,
            &[RB],
            true,
            0.45,
            0.60,
        ),
        I::Violin => s(
            F::Strings,
            "violin_family",
            55,
            105,
            ART_BOWED,
            SUS_MED,
            &[RM, RC, RH],
            false,
            0.65,
            0.60,
        ),
        I::Viola => s(
            F::Strings,
            "violin_family",
            48,
            88,
            ART_BOWED,
            SUS_MED,
            &[RM, RC, RH],
            false,
            0.50,
            0.70,
        ),
        I::Cello => s(
            F::Strings,
            "violin_family",
            36,
            84,
            ART_BOWED,
            SUS_MED,
            &[RM, RC, RH, RB],
            false,
            0.45,
            0.75,
        ),
        I::Contrabass => s(
            F::Strings,
            "violin_family",
            28,
            67,
            ART_BOWED,
            SUS_MED,
            &[RB, RH],
            false,
            0.30,
            0.75,
        ),
        I::TremoloStrings => s(
            F::Strings,
            "section",
            28,
            96,
            ART_BOWED,
            SUS_MED,
            &[RH, RP],
            false,
            0.55,
            0.60,
        ),
        I::Pizzicato => s(
            F::Strings,
            "section",
            28,
            96,
            ART_PIZZ,
            DECAY_FAST,
            &[RH, RR],
            false,
            0.55,
            0.55,
        ),
        I::Harp => s(
            F::Plucked,
            "harp",
            24,
            103,
            ART_BASIC,
            DECAY_LONG,
            &[RM, RC, RH],
            false,
            0.60,
            0.55,
        ),
        I::Timpani => s(
            F::Percussion,
            "timpani",
            38,
            57,
            ART_ROLL,
            DECAY_MED,
            &[RR, RB],
            false,
            0.30,
            0.50,
        ),
        I::Strings => s(
            F::Strings,
            "section",
            28,
            96,
            ART_BOWED,
            SUS_MED,
            &[RM, RH, RP],
            false,
            0.55,
            0.65,
        ),
        I::SlowStrings => s(
            F::Strings,
            "section",
            28,
            96,
            ART_BOWED,
            SUS_SLOW,
            &[RH, RP],
            false,
            0.45,
            0.75,
        ),
        I::SynthStrings => s(
            F::Strings,
            "section",
            28,
            96,
            ART_BASIC,
            SUS_MED,
            &[RH, RP],
            true,
            0.50,
            0.60,
        ),
        I::Choir => s(
            F::Vocals,
            "choir",
            40,
            84,
            ART_PAD,
            SUS_CHOIR,
            &[RH, RP],
            false,
            0.40,
            0.80,
        ),
        I::Voice => s(
            F::Vocals,
            "voice",
            48,
            84,
            ART_PAD,
            SUS_MED,
            &[RM, RH, RP],
            false,
            0.45,
            0.75,
        ),
        I::Trumpet => s(
            F::Brass,
            "trumpet",
            52,
            86,
            ART_BRASS,
            SUS_FAST,
            &[RM, RC],
            false,
            0.80,
            0.40,
        ),
        I::Trombone => s(
            F::Brass,
            "trombone",
            40,
            77,
            ART_BRASS,
            SUS_MED_FAST,
            &[RM, RC, RH],
            false,
            0.60,
            0.60,
        ),
        I::Tuba => s(
            F::Brass,
            "tuba",
            26,
            65,
            ART_BASIC,
            SUS_MED,
            &[RB, RH],
            false,
            0.30,
            0.75,
        ),
        I::Horn => s(
            F::Brass,
            "horn",
            41,
            84,
            ART_BRASS,
            SUS_MED,
            &[RM, RC, RH, RP],
            false,
            0.45,
            0.80,
        ),
        I::Brass => s(
            F::Brass,
            "section",
            40,
            86,
            ART_BRASS,
            SUS_MED_FAST,
            &[RM, RH, RR],
            false,
            0.70,
            0.50,
        ),
        I::Sax => s(
            F::Woodwinds,
            "sax",
            49,
            80,
            ART_BASIC,
            SUS_MED_FAST,
            &[RM, RC],
            false,
            0.60,
            0.65,
        ),
        I::Oboe => s(
            F::Woodwinds,
            "double_reed",
            58,
            93,
            ART_BASIC,
            SUS_MED_FAST,
            &[RM, RC],
            false,
            0.70,
            0.50,
        ),
        I::EnglishHorn => s(
            F::Woodwinds,
            "double_reed",
            52,
            84,
            ART_BASIC,
            SUS_MED,
            &[RM, RC],
            false,
            0.55,
            0.65,
        ),
        I::Bassoon => s(
            F::Woodwinds,
            "double_reed",
            34,
            75,
            ART_BASIC,
            SUS_MED,
            &[RM, RC, RH, RB],
            false,
            0.40,
            0.70,
        ),
        I::Clarinet => s(
            F::Woodwinds,
            "single_reed",
            50,
            94,
            ART_BASIC,
            SUS_MED_FAST,
            &[RM, RC, RH],
            false,
            0.50,
            0.70,
        ),
        I::Piccolo => s(
            F::Woodwinds,
            "flutes",
            74,
            108,
            ART_BASIC,
            SUS_FAST,
            &[RM],
            false,
            0.90,
            0.20,
        ),
        I::Flute => s(
            F::Woodwinds,
            "flutes",
            60,
            98,
            ART_BASIC,
            SUS_MED_FAST,
            &[RM, RC],
            false,
            0.65,
            0.50,
        ),
        I::Recorder => s(
            F::Woodwinds,
            "flutes",
            72,
            98,
            ART_BASIC,
            SUS_MED_FAST,
            &[RM],
            false,
            0.60,
            0.45,
        ),
        I::PanFlute => s(
            F::Woodwinds,
            "flutes",
            60,
            96,
            ART_BASIC,
            SUS_MED_FAST,
            &[RM],
            false,
            0.50,
            0.60,
        ),
        I::Whistle => s(
            F::Woodwinds,
            "flutes",
            72,
            108,
            ART_BASIC,
            SUS_MED_FAST,
            &[RM],
            false,
            0.75,
            0.30,
        ),
        I::Ocarina => s(
            F::Woodwinds,
            "flutes",
            72,
            96,
            ART_BASIC,
            SUS_MED_FAST,
            &[RM],
            false,
            0.55,
            0.50,
        ),
        I::SquareLead => s(
            F::Synth,
            "lead",
            36,
            96,
            ART_BASIC,
            SUS_FAST,
            &[RM, RC],
            true,
            0.70,
            0.35,
        ),
        I::SawLead => s(
            F::Synth,
            "lead",
            36,
            96,
            ART_BASIC,
            SUS_FAST,
            &[RM, RC],
            true,
            0.80,
            0.30,
        ),
        I::Pad => s(
            F::Synth,
            "pad",
            24,
            96,
            ART_PAD,
            SUS_SLOW,
            &[RH, RP],
            true,
            0.50,
            0.60,
        ),
        I::WarmPad => s(
            F::Synth,
            "pad",
            24,
            96,
            ART_PAD,
            SUS_SLOW,
            &[RH, RP],
            true,
            0.35,
            0.80,
        ),
        I::ChoirPad => s(
            F::Synth,
            "pad",
            24,
            96,
            ART_PAD,
            SUS_SLOW,
            &[RH, RP],
            true,
            0.40,
            0.70,
        ),
        I::BowedPad => s(
            F::Synth,
            "pad",
            24,
            96,
            ART_PAD,
            SUS_SLOW,
            &[RH, RP],
            true,
            0.45,
            0.65,
        ),
        I::HaloPad => s(
            F::Synth,
            "pad",
            24,
            96,
            ART_PAD,
            SUS_SLOW,
            &[RH, RP],
            true,
            0.55,
            0.60,
        ),
        I::SweepPad => s(
            F::Synth,
            "pad",
            24,
            96,
            ART_PAD,
            SUS_SLOW,
            &[RH, RP],
            true,
            0.60,
            0.50,
        ),
        I::Drums => s(
            F::Percussion,
            "kit",
            35,
            81,
            ART_PAD,
            DECAY_FAST,
            &[RR],
            false,
            0.50,
            0.40,
        ),
    }
}

/// Related-family affinity for cross-family fallback (symmetric). Only these
/// pairs are reachable with `allow_cross_family`; anything else needs an
/// explicit `allowed_families` entry. Strings deliberately have no special
/// standing — substituting *into* strings additionally requires the caller
/// to allowlist the strings family (see `resolver.rs`).
const RELATED: &[(Family, Family)] = &[
    (Family::Brass, Family::Synth),
    (Family::Strings, Family::Synth),
    (Family::Vocals, Family::Synth),
    (Family::Woodwinds, Family::Brass),
    (Family::Guitars, Family::Plucked),
    (Family::Bass, Family::Guitars),
    (Family::Keyboards, Family::Percussion),
    (Family::Keyboards, Family::Plucked),
    (Family::Strings, Family::Plucked),
    (Family::Bass, Family::Strings),
    (Family::Vocals, Family::Strings),
];

/// 0.45 for related families, 0.0 otherwise (same family is scored
/// separately and higher).
pub fn family_affinity(a: Family, b: Family) -> f32 {
    if RELATED
        .iter()
        .any(|&(x, y)| (x == a && y == b) || (x == b && y == a))
    {
        0.45
    } else {
        0.0
    }
}

/// Accepted spellings that are true synonyms of a canonical instrument —
/// never "close enough" different instruments (those are the fallback
/// system's job, visibly). Keys are in normalized form (see `normalize`).
const ALIASES: &[(&str, Instrument)] = &[
    ("acoustic_guitar", Instrument::Guitar),
    ("acoustic_piano", Instrument::Piano),
    ("alto_sax", Instrument::Sax),
    ("bass_guitar", Instrument::Bass),
    ("bright_acoustic_piano", Instrument::BrightPiano),
    ("choir_aahs", Instrument::Choir),
    ("church_organ", Instrument::Organ),
    ("classical_guitar", Instrument::Guitar),
    ("contra_bass", Instrument::Contrabass),
    ("cor_anglais", Instrument::EnglishHorn),
    ("double_bass", Instrument::Contrabass),
    ("drum_kit", Instrument::Drums),
    ("drumkit", Instrument::Drums),
    ("electric_bass", Instrument::Bass),
    ("electric_piano", Instrument::Epiano),
    ("fiddle", Instrument::Violin),
    ("fingered_bass", Instrument::Bass),
    ("french_horn", Instrument::Horn),
    ("glock", Instrument::Glockenspiel),
    ("grand_piano", Instrument::Piano),
    ("kettledrums", Instrument::Timpani),
    ("new_age_pad", Instrument::Pad),
    ("nylon_guitar", Instrument::Guitar),
    ("pan_pipes", Instrument::PanFlute),
    ("panpipes", Instrument::PanFlute),
    ("percussion", Instrument::Drums),
    ("pizzicato_strings", Instrument::Pizzicato),
    ("rhodes", Instrument::Epiano),
    ("saw_wave", Instrument::SawLead),
    ("saxophone", Instrument::Sax),
    ("square_wave", Instrument::SquareLead),
    ("string_ensemble", Instrument::Strings),
    ("string_section", Instrument::Strings),
    ("synth_pad", Instrument::Pad),
    ("upright_bass", Instrument::Contrabass),
    ("vibes", Instrument::Vibraphone),
    ("voice_oohs", Instrument::Voice),
];

/// Lowercase, trim, and map separators (`-`, space) to `_` so that
/// `French Horn`, `french-horn` and `french_horn` all normalize identically.
pub fn normalize(raw: &str) -> String {
    raw.trim()
        .chars()
        .map(|c| match c {
            '-' | ' ' => '_',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

fn canonical_map() -> &'static BTreeMap<String, Instrument> {
    static MAP: OnceLock<BTreeMap<String, Instrument>> = OnceLock::new();
    MAP.get_or_init(|| ALL.iter().map(|&i| (instrument_key(i), i)).collect())
}

fn alias_map() -> &'static BTreeMap<&'static str, Instrument> {
    static MAP: OnceLock<BTreeMap<&'static str, Instrument>> = OnceLock::new();
    MAP.get_or_init(|| ALIASES.iter().copied().collect())
}

/// A name resolved to a canonical instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NameResolution {
    pub instrument: Instrument,
    /// True when the spelling went through the alias table rather than
    /// matching a canonical key (after normalization).
    pub alias: bool,
}

/// Resolve a free-form instrument spelling: canonical key first, then the
/// alias table, both after `normalize`. `None` = unknown instrument.
pub fn resolve_name(raw: &str) -> Option<NameResolution> {
    let n = normalize(raw);
    if let Some(&instrument) = canonical_map().get(&n) {
        return Some(NameResolution {
            instrument,
            alias: false,
        });
    }
    alias_map()
        .get(n.as_str())
        .map(|&instrument| NameResolution {
            instrument,
            alias: true,
        })
}

/// Up to three canonical names sharing a substring with the input, for
/// unknown-instrument error messages. Deterministic (alphabetical).
pub fn suggest(raw: &str) -> Vec<String> {
    let n = normalize(raw);
    if n.is_empty() {
        return Vec::new();
    }
    canonical_map()
        .keys()
        .filter(|k| k.contains(&n) || n.contains(k.as_str()))
        .take(3)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ranges_are_valid_and_nonempty() {
        for &i in &ALL {
            let sp = spec(i);
            assert!(
                sp.range.0 < sp.range.1,
                "{}: empty or inverted range",
                instrument_key(i)
            );
            assert!(!sp.roles.is_empty(), "{}: no roles", instrument_key(i));
            assert!(
                !sp.articulations.is_empty(),
                "{}: no articulations",
                instrument_key(i)
            );
            assert!(
                sp.articulations.contains(&Articulation::Sustain),
                "{}: sustain must be universally supported",
                instrument_key(i)
            );
            assert!(
                (0.0..=1.0).contains(&sp.timbre.brightness)
                    && (0.0..=1.0).contains(&sp.timbre.warmth),
                "{}: timbre out of 0..=1",
                instrument_key(i)
            );
            assert!(!sp.subfamily.is_empty());
        }
    }

    #[test]
    fn all_covers_every_enum_variant_per_json_schema() {
        // The exported scene schema lists every Instrument serialization name;
        // comparing against it keeps `ALL` honest when the enum grows.
        let schema: serde_json::Value =
            serde_json::from_str(&crate::schema::schema_json()).unwrap();
        let names: std::collections::BTreeSet<String> = schema["$defs"]["Instrument"]["enum"]
            .as_array()
            .expect("Instrument enum in schema")
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        let ours: std::collections::BTreeSet<String> =
            ALL.iter().map(|&i| instrument_key(i)).collect();
        assert_eq!(names, ours);
        assert_eq!(ALL.len(), names.len(), "duplicate entries in ALL");
    }

    #[test]
    fn aliases_resolve_and_never_shadow_canonical_names() {
        for &(alias, instrument) in ALIASES {
            assert_eq!(alias, normalize(alias), "alias `{alias}` not normalized");
            assert!(
                canonical_map().get(alias).is_none(),
                "alias `{alias}` collides with a canonical name"
            );
            let r = resolve_name(alias).unwrap();
            assert_eq!(r.instrument, instrument);
            assert!(r.alias);
        }
        // Canonical spellings resolve as non-alias, including sloppy casing.
        let r = resolve_name("Violin").unwrap();
        assert_eq!(r.instrument, Instrument::Violin);
        assert!(!r.alias);
        let r = resolve_name("French Horn").unwrap();
        assert_eq!(r.instrument, Instrument::Horn);
        assert!(r.alias);
        assert!(resolve_name("theremin").is_none());
    }

    #[test]
    fn family_affinity_is_symmetric_and_strings_have_no_bonus() {
        for &(a, b) in RELATED {
            assert_eq!(family_affinity(a, b), family_affinity(b, a));
            assert!(a != b, "related pair must cross families");
        }
        // The requirement's core rule: brass/woodwinds never relate to
        // strings by default — strings must not absorb missing instruments.
        assert_eq!(family_affinity(Family::Brass, Family::Strings), 0.0);
        assert_eq!(family_affinity(Family::Woodwinds, Family::Strings), 0.0);
        assert_eq!(family_affinity(Family::Brass, Family::Brass), 0.0);
    }

    #[test]
    fn range_coverage_is_a_fraction_of_the_requested_range() {
        let viola = spec(Instrument::Viola);
        let violin = spec(Instrument::Violin);
        let cov = viola.range_coverage_by(&violin);
        assert!(cov > 0.7 && cov < 1.0, "{cov}");
        let piccolo = spec(Instrument::Piccolo);
        let tuba = spec(Instrument::Tuba);
        assert_eq!(piccolo.range_coverage_by(&tuba), 0.0);
        assert_eq!(viola.range_coverage_by(&viola), 1.0);
    }
}
