/// IPADIC dictionary generator for JaIM.
///
/// Reads IPADIC CSV files (EUC-JP), filters and transforms entries,
/// and outputs `builtin_dict.rs` with a static const array.
///
/// Usage: cargo run --bin generate-dict
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

const IPADIC_DIR: &str = "/usr/share/mecab/dic/ipadic";

/// Output path relative to project root.
const OUTPUT_PATH: &str = "src/core/dictionary/builtin_dict.rs";

/// IPADIC source file with its cost threshold (None = no threshold).
struct DictSource {
    filename: &'static str,
    max_cost: Option<i32>,
    /// Which conjugation forms to include.
    /// None = all forms, Some(list) = only matching forms.
    allowed_forms: Option<&'static [&'static str]>,
}

/// Common conjugation forms for daily Japanese input.
/// 基本形: base (食べる), 連用タ接続: ta-form (食べた), 連用形: te-form (食べて),
/// 未然形: negative stem (食べない), 仮定形: conditional (食べれば)
const PRACTICAL_FORMS: &[&str] = &["基本形", "連用タ接続", "連用形", "未然形", "仮定形"];

const SOURCES: &[DictSource] = &[
    // --- Nouns ---
    DictSource { filename: "Noun.csv", max_cost: Some(8000), allowed_forms: None },
    DictSource { filename: "Noun.adjv.csv", max_cost: Some(8000), allowed_forms: None },
    DictSource { filename: "Noun.adverbal.csv", max_cost: None, allowed_forms: None },
    DictSource { filename: "Noun.verbal.csv", max_cost: Some(8000), allowed_forms: None },
    DictSource { filename: "Noun.demonst.csv", max_cost: None, allowed_forms: None },
    DictSource { filename: "Noun.nai.csv", max_cost: None, allowed_forms: None },
    DictSource { filename: "Noun.number.csv", max_cost: None, allowed_forms: None },
    DictSource { filename: "Noun.others.csv", max_cost: None, allowed_forms: None },
    DictSource { filename: "Noun.place.csv", max_cost: Some(5500), allowed_forms: None },
    DictSource { filename: "Noun.name.csv", max_cost: Some(5500), allowed_forms: None },
    DictSource { filename: "Noun.org.csv", max_cost: Some(5500), allowed_forms: None },
    DictSource { filename: "Noun.proper.csv", max_cost: Some(6000), allowed_forms: None },
    // --- Verbs (practical conjugation forms) ---
    DictSource { filename: "Verb.csv", max_cost: None, allowed_forms: Some(PRACTICAL_FORMS) },
    // --- Adjectives (practical conjugation forms) ---
    DictSource { filename: "Adj.csv", max_cost: None, allowed_forms: Some(PRACTICAL_FORMS) },
    // --- Small categories (no threshold) ---
    DictSource { filename: "Adverb.csv", max_cost: None, allowed_forms: None },
    DictSource { filename: "Postp.csv", max_cost: None, allowed_forms: None },
    DictSource { filename: "Postp-col.csv", max_cost: None, allowed_forms: None },
    DictSource { filename: "Auxil.csv", max_cost: None, allowed_forms: Some(PRACTICAL_FORMS) },
    DictSource { filename: "Conjunction.csv", max_cost: None, allowed_forms: None },
    DictSource { filename: "Interjection.csv", max_cost: None, allowed_forms: None },
    DictSource { filename: "Prefix.csv", max_cost: None, allowed_forms: None },
    DictSource { filename: "Suffix.csv", max_cost: None, allowed_forms: None },
    DictSource { filename: "Adnominal.csv", max_cost: None, allowed_forms: None },
];

#[derive(Clone)]
struct Entry {
    reading: String,
    surface: String,
    pos: &'static str,
    frequency: u32,
}

/// Intermediate entry that carries conjugation metadata for compound generation.
struct ParsedEntry {
    entry: Entry,
    conjugation_type: String,
    conjugation_form: String,
}

/// Conjugation types that take voiced auxiliaries (だ/で) instead of (た/て).
const VOICED_CONJUGATION_TYPES: &[&str] = &[
    "五段・ガ行",
    "五段・バ行",
    "五段・マ行",
    "五段・ナ行",
];

/// Conjugation types where 連用形 can take た/て directly (ichidan-style).
const ICHIDAN_TYPES: &[&str] = &[
    "一段",
    "一段・クレル",
    "カ変・来ル",
    "サ変・スル",
    "サ変・−スル",
];

fn main() {
    let mut entries: Vec<Entry> = Vec::new();
    let mut parsed_for_compounds: Vec<ParsedEntry> = Vec::new();

    for source in SOURCES {
        let path = Path::new(IPADIC_DIR).join(source.filename);
        if !path.exists() {
            eprintln!("WARNING: {} not found, skipping", path.display());
            continue;
        }

        let raw = fs::read(&path).expect("failed to read file");
        let (utf8, _, had_errors) = encoding_rs::EUC_JP.decode(&raw);
        if had_errors {
            eprintln!("WARNING: encoding errors in {}", source.filename);
        }

        let mut count = 0;
        for line in utf8.lines() {
            if let Some(parsed) = parse_line(line, source) {
                // Collect verb/adjective conjugation stems for compound generation
                let dominated_by_kana = parsed.entry.surface == parsed.entry.reading;
                if !dominated_by_kana
                    && matches!(parsed.entry.pos, "Verb" | "Adjective")
                    && matches!(parsed.conjugation_form.as_str(), "連用タ接続" | "連用形")
                {
                    parsed_for_compounds.push(ParsedEntry {
                        entry: parsed.entry.clone(),
                        conjugation_type: parsed.conjugation_type,
                        conjugation_form: parsed.conjugation_form,
                    });
                }
                entries.push(parsed.entry);
                count += 1;
            }
        }
        eprintln!("{}: {} entries", source.filename, count);
    }

    // Generate compound entries (verb stem + auxiliary)
    let compounds = generate_compounds(&parsed_for_compounds);
    eprintln!("Generated {} compound entries", compounds.len());
    entries.extend(compounds);

    eprintln!("Total before dedup: {}", entries.len());

    // Deduplicate: same reading+surface+pos → keep highest frequency
    let mut dedup: HashMap<(String, String, String), Entry> = HashMap::new();
    for entry in entries {
        let key = (entry.reading.clone(), entry.surface.clone(), entry.pos.to_string());
        dedup
            .entry(key)
            .and_modify(|existing| {
                if entry.frequency > existing.frequency {
                    *existing = entry.clone();
                }
            })
            .or_insert(entry);
    }

    let mut final_entries: Vec<Entry> = dedup.into_values().collect();
    // Sort by frequency descending, then reading, then surface for stable output
    final_entries.sort_by(|a, b| {
        b.frequency
            .cmp(&a.frequency)
            .then_with(|| a.reading.cmp(&b.reading))
            .then_with(|| a.surface.cmp(&b.surface))
    });

    eprintln!("Total after dedup: {}", final_entries.len());

    write_output(&final_entries);
    eprintln!("Written to {}", OUTPUT_PATH);
}

/// Parse a single IPADIC CSV line into a ParsedEntry, or None if filtered out.
fn parse_line(line: &str, source: &DictSource) -> Option<ParsedEntry> {
    let fields: Vec<&str> = line.split(',').collect();
    if fields.len() < 13 {
        return None;
    }

    let surface = fields[0];
    let cost: i32 = fields[3].parse().ok()?;
    let pos_major = fields[4]; // 品詞
    let pos_sub = fields[5]; // 品詞細分類1
    let conjugation_type = fields[8]; // 活用型
    let conjugation_form = fields[9]; // 活用形
    let reading_katakana = fields[11]; // 読み (katakana)

    // Filter by cost threshold (skip for single-kanji entries to ensure full coverage)
    let is_single_kanji = surface.chars().count() == 1
        && surface.chars().next().map_or(false, |c| ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3400}'..='\u{4DBF}').contains(&c));
    if !is_single_kanji {
        if let Some(max) = source.max_cost {
            if cost >= max {
                return None;
            }
        }
    }

    // Filter by allowed conjugation forms (verbs/adjectives/auxiliaries)
    if let Some(forms) = source.allowed_forms {
        if !forms.iter().any(|&f| f == conjugation_form) {
            return None;
        }
    }

    // Skip entries with no reading
    if reading_katakana.is_empty() || reading_katakana == "*" {
        return None;
    }

    // Convert katakana reading to hiragana
    let reading = katakana_to_hiragana(reading_katakana);

    // Skip if surface == reading (pure kana words add no value for kana→kanji conversion)
    // Exception: keep functional words and verb/adjective conjugations
    // as they are needed for segmentation even without kanji conversion
    let dominated_by_kana = surface == reading
        && !matches!(
            pos_major,
            "助詞" | "助動詞" | "接続詞" | "副詞" | "感動詞" | "連体詞" | "動詞" | "形容詞"
        );
    if dominated_by_kana {
        return None;
    }

    // Skip single-katakana surfaces with single-hiragana readings (e.g., を→ヲ, が→ガ)
    // These are rarely useful in normal Japanese input and pollute candidate lists.
    if reading.chars().count() == 1 && surface.chars().count() == 1 {
        let sc = surface.chars().next().unwrap();
        if ('\u{30A1}'..='\u{30F6}').contains(&sc) {
            return None;
        }
    }

    // Map POS
    let pos = map_pos(pos_major, pos_sub);

    // Convert cost to frequency (lower cost = more common)
    let frequency = cost_to_frequency(cost);

    Some(ParsedEntry {
        entry: Entry {
            reading,
            surface: surface.to_string(),
            pos,
            frequency,
        },
        conjugation_type: conjugation_type.to_string(),
        conjugation_form: conjugation_form.to_string(),
    })
}

/// Generate compound entries by combining verb/adjective stems with auxiliaries.
///
/// For each 連用タ接続 stem, generates:
///   stem + た/だ (past tense, e.g., 食べた, 読んだ)
///   stem + て/で (te-form, e.g., 食べて, 読んで)
///
/// For each 連用形 stem of ichidan-type verbs, generates:
///   stem + た (past tense, e.g., 食べた)
///   stem + て (te-form, e.g., 食べて)
fn generate_compounds(parsed: &[ParsedEntry]) -> Vec<Entry> {
    let mut compounds = Vec::new();

    for p in parsed {
        let ctype = p.conjugation_type.as_str();
        let cform = p.conjugation_form.as_str();

        // Determine which auxiliaries to append
        let suffixes: &[(&str, &str)] = match cform {
            "連用タ接続" => {
                // Voiced consonant stems (ガ/バ/マ/ナ行) take だ/で
                if VOICED_CONJUGATION_TYPES.iter().any(|&v| ctype.starts_with(v)) {
                    &[("だ", "だ"), ("で", "で")]
                } else {
                    &[("た", "た"), ("て", "て")]
                }
            }
            "連用形" => {
                // Only ichidan-style verbs form past/te directly from 連用形
                if ICHIDAN_TYPES.iter().any(|&t| ctype.starts_with(t)) {
                    &[("た", "た"), ("て", "て")]
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        for &(reading_suffix, surface_suffix) in suffixes {
            // Boost compound frequency: conjugated forms (食べた, 走った) are at least
            // as common as their stems, but IPADIC stems have lower frequency.
            // Without boost, high-frequency short words (他+ベタ) beat compounds (食べた).
            let boosted_freq = (p.entry.frequency as f64 * 2.0).min(20000.0) as u32;
            compounds.push(Entry {
                reading: format!("{}{}", p.entry.reading, reading_suffix),
                surface: format!("{}{}", p.entry.surface, surface_suffix),
                pos: p.entry.pos,
                frequency: boosted_freq,
            });
        }
    }

    compounds
}

/// Convert katakana string to hiragana.
fn katakana_to_hiragana(s: &str) -> String {
    s.chars()
        .map(|c| {
            if ('\u{30A1}'..='\u{30F6}').contains(&c) {
                // Katakana → Hiragana (offset 0x60)
                char::from_u32(c as u32 - 0x60).unwrap_or(c)
            } else if c == '\u{30FC}' {
                // Katakana long vowel mark ー → keep as-is (used in loanwords)
                'ー'
            } else {
                c
            }
        })
        .collect()
}

/// Map IPADIC POS tags to JaIM PartOfSpeech variant name.
fn map_pos(major: &str, sub: &str) -> &'static str {
    match major {
        "名詞" => {
            if sub == "接尾" {
                "Suffix"
            } else {
                "Noun"
            }
        }
        "動詞" => "Verb",
        "形容詞" => "Adjective",
        "副詞" => "Adverb",
        "助詞" => "Particle",
        "助動詞" => "Auxiliary",
        "接続詞" => "Conjunction",
        "感動詞" => "Interjection",
        "接頭詞" => "Prefix",
        _ => "Other", // 連体詞, フィラー, etc.
    }
}

/// Convert IPADIC cost to frequency score.
/// IPADIC: lower cost = more common. Range roughly -7000 to +16000.
/// JaIM: higher frequency = more common.
fn cost_to_frequency(cost: i32) -> u32 {
    let freq = 10000 - cost;
    freq.clamp(1, 20000) as u32
}

/// Write the generated builtin_dict.rs file.
fn write_output(entries: &[Entry]) {
    let mut out = fs::File::create(OUTPUT_PATH).expect("failed to create output file");

    writeln!(out, "/// Auto-generated from IPADIC. Do not edit manually.").unwrap();
    writeln!(out, "/// Re-generate with: cargo run --bin generate-dict").unwrap();
    writeln!(
        out,
        "/// Source: /usr/share/mecab/dic/ipadic/ (mecab-ipadic package)"
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "use super::PartOfSpeech;").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "/// Built-in dictionary entries: (reading, surface, pos, frequency)"
    )
    .unwrap();
    writeln!(
        out,
        "pub const BUILTIN_ENTRIES: &[(&str, &str, PartOfSpeech, u32)] = &["
    )
    .unwrap();

    for entry in entries {
        // Escape any backslashes or quotes in strings
        let reading = entry.reading.replace('\\', "\\\\").replace('"', "\\\"");
        let surface = entry.surface.replace('\\', "\\\\").replace('"', "\\\"");
        writeln!(
            out,
            "    (\"{}\", \"{}\", PartOfSpeech::{}, {}),",
            reading, surface, entry.pos, entry.frequency
        )
        .unwrap();
    }

    writeln!(out, "];").unwrap();
}
