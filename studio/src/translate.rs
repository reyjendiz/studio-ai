//! Offline RU->EN translation (Helsinki-NLP/opus-mt-ru-en) on the CPU with candle, so it never competes with the
//! image models for VRAM. Same behaviour as the old Python server: only Cyrillic text is translated, sentence by
//! sentence, and photo terms the model mangles are swapped for fixed English terms before translation.
use anyhow::{anyhow, Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::marian::{Config, MTModel};
use std::collections::HashMap;
use std::path::Path;
use unicode_normalization::UnicodeNormalization;

const SP: char = '\u{2581}'; // SentencePiece word boundary "▁"

/// SentencePiece unigram model read straight from the .spm protobuf (pieces + scores), segmented with Viterbi.
struct Unigram {
    pieces: HashMap<String, f32>,
    max_len: usize,
    unk_score: f32,
}

impl Unigram {
    fn load(path: &Path) -> Result<Self> {
        let buf = std::fs::read(path).with_context(|| format!("не найден {}", path.display()))?;
        let mut pieces = HashMap::new();
        let mut pos = 0;
        while pos < buf.len() {
            let (key, n) = varint(&buf[pos..])?;
            pos += n;
            match key & 7 {
                2 => {
                    let (len, n) = varint(&buf[pos..])?;
                    pos += n;
                    let body = &buf[pos..pos + len as usize];
                    pos += len as usize;
                    if key >> 3 == 1 {
                        // ModelProto.pieces: SentencePiece { string piece = 1; float score = 2; Type type = 3; }
                        let (mut piece, mut score, mut kind) = (String::new(), 0f32, 1u64);
                        let mut p = 0;
                        while p < body.len() {
                            let (k, n) = varint(&body[p..])?;
                            p += n;
                            match (k >> 3, k & 7) {
                                (1, 2) => {
                                    let (l, n) = varint(&body[p..])?;
                                    p += n;
                                    piece = String::from_utf8_lossy(&body[p..p + l as usize]).into_owned();
                                    p += l as usize;
                                }
                                (2, 5) => {
                                    score = f32::from_le_bytes(body[p..p + 4].try_into()?);
                                    p += 4;
                                }
                                (3, 0) => {
                                    let (v, n) = varint(&body[p..])?;
                                    p += n;
                                    kind = v;
                                }
                                (_, w) => p += skip(w, &body[p..])?,
                            }
                        }
                        if kind == 1 || kind == 4 {
                            pieces.insert(piece, score); // normal and user-defined pieces; control/unknown are never produced
                        }
                    }
                }
                w => pos += skip(w, &buf[pos..])?,
            }
        }
        let max_len = pieces.keys().map(|p| p.chars().count()).max().unwrap_or(1);
        let unk_score = pieces.values().cloned().fold(f32::MAX, f32::min) - 10.0;
        Ok(Self { pieces, max_len, unk_score })
    }

    /// nmt_nfkc normalisation + "▁" word boundaries, then the highest-scoring segmentation.
    fn encode(&self, text: &str) -> Vec<String> {
        let norm: String = text.nfkc().collect();
        let words: Vec<&str> = norm.split_whitespace().collect();
        let s: Vec<char> = words.iter().flat_map(|w| std::iter::once(SP).chain(w.chars())).collect();
        let n = s.len();
        let mut best = vec![f32::NEG_INFINITY; n + 1];
        let mut back = vec![0usize; n + 1];
        best[0] = 0.0;
        for i in 0..n {
            if best[i] == f32::NEG_INFINITY {
                continue;
            }
            let mut matched = false;
            for j in i + 1..=(i + self.max_len).min(n) {
                let sub: String = s[i..j].iter().collect();
                if let Some(&sc) = self.pieces.get(&sub) {
                    matched |= j == i + 1;
                    if best[i] + sc > best[j] {
                        best[j] = best[i] + sc;
                        back[j] = i;
                    }
                }
            }
            if !matched && best[i] + self.unk_score > best[i + 1] {
                best[i + 1] = best[i] + self.unk_score; // a character no piece covers becomes <unk>
                back[i + 1] = i;
            }
        }
        let mut out = Vec::new();
        let mut j = n;
        while j > 0 {
            let i = back[j];
            out.push(s[i..j].iter().collect());
            j = i;
        }
        out.reverse();
        out
    }
}

fn varint(b: &[u8]) -> Result<(u64, usize)> {
    let mut v = 0u64;
    for (i, &x) in b.iter().enumerate().take(10) {
        v |= ((x & 0x7f) as u64) << (7 * i);
        if x & 0x80 == 0 {
            return Ok((v, i + 1));
        }
    }
    Err(anyhow!("повреждён файл токенизатора"))
}

fn skip(wire: u64, b: &[u8]) -> Result<usize> {
    Ok(match wire {
        0 => varint(b)?.1,
        1 => 8,
        5 => 4,
        2 => {
            let (l, n) = varint(b)?;
            n + l as usize
        }
        _ => return Err(anyhow!("повреждён файл токенизатора")),
    })
}

pub struct Translator {
    model: MTModel,
    cfg: Config,
    src: Unigram,
    vocab: HashMap<String, u32>,
    ids: HashMap<u32, String>,
    unk: u32,
}

/// Photo/art terms the generic model mangles ("глубина резкости" -> "depth of sharpness"); replaced before translation.
const GLOSSARY: &[(&str, &str)] = &[
    (r"(?:малая|небольшая|малой|небольшой) глубин\w+ резкости", "shallow depth of field"),
    (r"глубин\w+ резкости", "depth of field"),
    (r"боке", "bokeh"),
    (r"пл[её]ночн\w+ зерн\w+", "film grain"),
    (r"золот\w+ час\w*", "golden hour"),
    (r"крупн\w+ план\w*", "close-up shot"),
    (r"общ\w+ план\w*", "wide shot"),
    (r"фотореалистичн\w+", "photorealistic"),
    (r"кинематографичн\w+", "cinematic"),
    (r"мягк\w+ (?:свет\w*|освещени\w+)", "soft light"),
    (r"естественн\w+ текстур\w+ кожи", "natural skin texture"),
    (r"резк\w+ фокус\w*", "sharp focus"),
];

pub fn has_cyrillic(s: &str) -> bool {
    s.chars().any(|c| matches!(c, 'А'..='я' | 'Ё' | 'ё'))
}

impl Translator {
    pub fn load(dir: &Path) -> Result<Self> {
        let cfg = Config::opus_mt_en_ru(); // identical dimensions to opus-mt-ru-en's config.json
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[dir.join("model.safetensors")], DType::F32, &Device::Cpu)? };
        let model = MTModel::new(&cfg, vb)?;
        let vocab: HashMap<String, u32> = serde_json::from_slice(&std::fs::read(dir.join("vocab.json"))?)?;
        let ids = vocab.iter().map(|(k, v)| (*v, k.clone())).collect();
        let unk = *vocab.get("<unk>").unwrap_or(&1);
        Ok(Self { model, src: Unigram::load(&dir.join("source.spm"))?, cfg, vocab, ids, unk })
    }

    fn sentence(&mut self, text: &str) -> Result<String> {
        let mut input: Vec<u32> = self.src.encode(text).iter().map(|p| *self.vocab.get(p).unwrap_or(&self.unk)).collect();
        input.truncate(255);
        input.push(self.cfg.eos_token_id);
        let dev = Device::Cpu;
        self.model.reset_kv_cache();
        let enc = self.model.encoder().forward(&Tensor::new(input.as_slice(), &dev)?.unsqueeze(0)?, 0)?;
        let mut out = vec![self.cfg.decoder_start_token_id];
        // shortcut: greedy decoding (the Python version used 4 beams); upgrade to beam search if translations get worse
        for step in 0..256 {
            let start = if step == 0 { 0 } else { out.len() - 1 };
            let x = Tensor::new(&out[start..], &dev)?.unsqueeze(0)?;
            let logits = self.model.decode(&x, &enc, start)?.squeeze(0)?;
            let mut last: Vec<f32> = logits.get(logits.dim(0)? - 1)?.to_vec1()?;
            last[self.cfg.pad_token_id as usize] = f32::NEG_INFINITY; // Marian never emits <pad> (bad_words_ids in transformers)
            let tok = last.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|(i, _)| i as u32).unwrap();
            if tok == self.cfg.eos_token_id {
                break;
            }
            out.push(tok);
        }
        let text: String = out[1..].iter().filter_map(|i| self.ids.get(i)).filter(|p| !p.starts_with('<')).map(|p| p.as_str()).collect();
        Ok(text.replace(SP, " ").trim().to_string())
    }

    /// Translates only strings that contain Cyrillic; everything else passes through unchanged.
    pub fn translate(&mut self, text: &str) -> Result<String> {
        if !has_cyrillic(text) {
            return Ok(text.to_string());
        }
        // glossary terms travel as opaque tokens: the model garbles English inside Russian
        let mut terms = Vec::new();
        let mut t = text.to_string();
        for (pat, en) in GLOSSARY {
            let re = regex::Regex::new(&format!("(?i){pat}"))?;
            while let Some(m) = re.find(&t) {
                let tok = format!("qz{}zq", terms.len());
                terms.push(*en);
                t.replace_range(m.range(), &tok);
            }
        }
        let split = regex::Regex::new(r"[.!?]\s+|\n+")?;
        let mut parts = Vec::new();
        let mut last = 0;
        for m in split.find_iter(&t) {
            let end = m.start() + m.as_str().trim_end().len();
            parts.push(t[last..end].to_string());
            last = m.end();
        }
        parts.push(t[last..].to_string());
        let mut out = Vec::new();
        for p in parts.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
            out.push(self.sentence(p)?);
        }
        let mut res = out.join(" ");
        for (n, en) in terms.iter().enumerate() {
            let re = regex::Regex::new(&format!("(?i)qz{n}zq"))?;
            if re.is_match(&res) {
                res = re.replace_all(&res, *en).into_owned();
            } else {
                res += &format!(", {en}"); // the model dropped the token: keep the term anyway
            }
        }
        Ok(res)
    }
}
