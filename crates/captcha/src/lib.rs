//! Captcha crate: local ddddocr ONNX solver using `common_old.onnx`.
//!
//! Preprocessing matches the original Python reference exactly:
//!   - RGBA PNG → extract & invert alpha channel → grayscale
//!   - Else → convert to grayscale (L)
//!   - Resize to height=64, preserve aspect ratio (Lanczos)
//!   - Normalize: `(px / 255.0 - 0.5) / 0.5`  → `[-1, 1]`
//!   - Input tensor shape: `[1, 1, 64, W]` f32
//!
//! Model output shape: `(seq_len, 1, 8210)` CTC logits.
//! Greedy CTC decode: argmax over last dim, collapse repeats, skip blank (idx 0).

mod charset;

use crate::charset::CHARSET;
use base64::{engine::general_purpose, Engine as _};
use image::{
    imageops::{self, FilterType},
    DynamicImage,
};
use ndarray::Array4;
use ort::{session::Session, value::Tensor};
use std::{
    sync::{Mutex, OnceLock},
    time::Instant,
};

// ── model bytes embedded at compile time ──────────────────────────────────────
static MODEL_BYTES: &[u8] = include_bytes!("../../../models/common_old.onnx");

static SESSION: OnceLock<Result<Mutex<Session>, String>> = OnceLock::new();

// ── public types ──────────────────────────────────────────────────────────────

/// Detailed outcome from an OCR attempt — useful for UI and console debugging.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OcrDebugResult {
    pub text: Option<String>,
    pub debug_info: String,
}

// ── session init ─────────────────────────────────────────────────────────────

fn get_session() -> Result<&'static Mutex<Session>, String> {
    SESSION
        .get_or_init(|| {
            let start = Instant::now();
            println!("[OCR] Initializing ddddocr session …");
            let env = ort::init().build().map_err(|e| format!("ort env: {e}"))?;
            let sess = Session::builder(&env)
                .map_err(|e| format!("ort builder: {e}"))?
                .commit_from_memory(MODEL_BYTES)
                .map_err(|e| format!("ort load model: {e}"))?;
            println!("[OCR] Session ready in {:?}", start.elapsed());
            Ok(Mutex::new(sess))
        })
        .as_ref()
        .map_err(Clone::clone)
}

// ── preprocessing ─────────────────────────────────────────────────────────────

/// Decode raw image bytes → grayscale f32 tensor `[1, 1, 64, W]`.
fn preprocess(image_bytes: &[u8]) -> Result<(Array4<f32>, u32), String> {
    let img = image::load_from_memory(image_bytes).map_err(|e| format!("image decode: {e}"))?;

    // RGBA: extract alpha channel and invert (text = dark on white bg).
    let gray: image::GrayImage = match img {
        DynamicImage::ImageRgba8(ref rgba) => {
            let (w, h) = rgba.dimensions();
            let mut g = image::GrayImage::new(w, h);
            for (x, y, px) in rgba.enumerate_pixels() {
                let alpha = px[3];
                // invert: alpha=255 (text) → 0 (black), alpha=0 (bg) → 255 (white)
                g.put_pixel(x, y, image::Luma([255 - alpha]));
            }
            g
        }
        _ => img.to_luma8(),
    };

    // Resize to height=64, preserve aspect ratio.
    let (orig_w, orig_h) = gray.dimensions();
    if orig_h == 0 {
        return Err("image has zero height".to_string());
    }
    let target_h = 64_u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let new_w = ((f64::from(orig_w) * (64.0 / f64::from(orig_h))).round() as u32).max(1);

    let resized = imageops::resize(&gray, new_w, target_h, FilterType::Lanczos3);

    // Build f32 tensor [1, 1, 64, W], normalised to [-1, 1].
    let mut tensor = Array4::<f32>::zeros([1, 1, target_h as usize, new_w as usize]);
    for (y, row) in resized.rows().enumerate() {
        for (x, px) in row.enumerate() {
            let v = f32::from(px[0]);
            tensor[[0, 0, y, x]] = (v / 255.0 - 0.5) / 0.5;
        }
    }

    Ok((tensor, new_w))
}

// ── CTC decode ────────────────────────────────────────────────────────────────

/// Greedy CTC decode. Class 0 is blank; non-alphanumeric classes are ignored.
pub fn ctc_decode(logits: &[f32], seq_len: usize, num_classes: usize) -> String {
    let mut result = String::new();
    let mut last_idx = 0usize;

    for t in 0..seq_len {
        let row = &logits[t * num_classes..(t + 1) * num_classes];
        let idx = row
            .iter()
            .enumerate()
            .filter(|(i, _)| *i == 0 || CHARSET.contains_key(i))
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map_or(0, |(i, _)| i);

        if idx != 0 && idx != last_idx {
            if let Some(ch) = CHARSET.get(&idx) {
                result.push(*ch);
            }
        }
        last_idx = idx;
    }

    result
}

// ── core solver ───────────────────────────────────────────────────────────────

/// Solve raw image bytes (PNG or JPEG). Returns text + debug info.
pub fn solve_captcha_bytes_debug(image_bytes: &[u8]) -> OcrDebugResult {
    let start = Instant::now();
    println!("[OCR] Running inference on {} bytes", image_bytes.len());

    let sess_mutex = match get_session() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[OCR] session error: {e}");
            return OcrDebugResult {
                text: None,
                debug_info: e,
            };
        }
    };

    // Preprocess
    let (array, width) = match preprocess(image_bytes) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[OCR] preprocess: {e}");
            return OcrDebugResult {
                text: None,
                debug_info: e,
            };
        }
    };
    println!("[OCR] Input tensor [1, 1, 64, {width}]");

    // Build ort Tensor from ndarray
    let flat: Vec<f32> = array.into_raw_vec_and_offset().0;
    let input_tensor = match Tensor::from_array(([1_usize, 1, 64, width as usize], flat)) {
        Ok(t) => t,
        Err(e) => {
            let msg = format!("tensor build: {e}");
            eprintln!("[OCR] {msg}");
            return OcrDebugResult {
                text: None,
                debug_info: msg,
            };
        }
    };

    // Run
    let mut sess = match sess_mutex.lock() {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("session lock: {e}");
            eprintln!("[OCR] {msg}");
            return OcrDebugResult {
                text: None,
                debug_info: msg,
            };
        }
    };
    let outputs = match sess.run(ort::inputs!["input1" => input_tensor]) {
        Ok(o) => o,
        Err(e) => {
            let msg = format!("inference: {e}");
            eprintln!("[OCR] {msg}");
            return OcrDebugResult {
                text: None,
                debug_info: msg,
            };
        }
    };

    // Extract output tensor — shape (seq_len, 1, 8210)
    let (shape, logits) = match outputs["387"].try_extract_tensor::<f32>() {
        Ok(t) => t,
        Err(e) => {
            let msg = format!("output extract: {e}");
            eprintln!("[OCR] {msg}");
            return OcrDebugResult {
                text: None,
                debug_info: msg,
            };
        }
    };

    let seq_len = usize::try_from(shape[0]).unwrap_or(0);
    let num_classes = usize::try_from(shape[2]).unwrap_or(0);

    let text = ctc_decode(logits, seq_len, num_classes);
    let elapsed = start.elapsed();

    if text.is_empty() {
        let msg =
            format!("ddddocr: no chars decoded (seq={seq_len}, cls={num_classes}, {elapsed:?})");
        println!("[OCR] {msg}");
        OcrDebugResult {
            text: None,
            debug_info: msg,
        }
    } else {
        let msg = format!("ddddocr → {text:?} (seq={seq_len}, {elapsed:?})");
        println!("[OCR] {msg}");
        OcrDebugResult {
            text: Some(text),
            debug_info: msg,
        }
    }
}

/// Solve raw image bytes. Returns `Ok(text)` or `Err(debug_info)`.
pub fn solve_captcha_bytes(image_bytes: &[u8]) -> Result<String, String> {
    let r = solve_captcha_bytes_debug(image_bytes);
    r.text.ok_or(r.debug_info)
}

/// Solve a base64 data URI captcha. Returns text + debug info.
pub fn solve_local_ocr_debug(data_uri: &str) -> OcrDebugResult {
    if let Some(bytes) = decode_captcha_bytes(data_uri) {
        solve_captcha_bytes_debug(&bytes)
    } else {
        let msg = format!("base64 decode failed (len={})", data_uri.len());
        eprintln!("[OCR] {msg}");
        OcrDebugResult {
            text: None,
            debug_info: msg,
        }
    }
}

/// Solve a base64 data URI captcha.
pub fn solve_local_ocr(data_uri: &str) -> Option<String> {
    solve_local_ocr_debug(data_uri).text
}

// ── async fallback ────────────────────────────────────────────────────────────

/// Try local solver first; on failure fall back to HTTP OCR microservice.
pub async fn try_solve_ocr_debug(data_uri: &str, endpoint: &str) -> OcrDebugResult {
    let local = solve_local_ocr_debug(data_uri);
    if local.text.is_some() {
        return local;
    }

    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    else {
        return local;
    };

    let raw_b64 = data_uri.split_once(";base64,").map_or(data_uri, |(_, b)| b);
    let body = serde_json::json!({ "image": raw_b64 });

    if let Ok(resp) = client.post(endpoint).json(&body).send().await {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if json["success"].as_bool().unwrap_or(false) {
                if let Some(text) = json["text"].as_str() {
                    let trimmed = text.trim().to_string();
                    if !trimmed.is_empty() {
                        let msg = format!("{} | HTTP fallback → {:?}", local.debug_info, trimmed);
                        println!("[OCR] {msg}");
                        return OcrDebugResult {
                            text: Some(trimmed),
                            debug_info: msg,
                        };
                    }
                }
            }
        }
    }

    local
}

/// Try local solver, fall back to HTTP OCR microservice.
pub async fn try_solve_ocr(data_uri: &str, endpoint: &str) -> Option<String> {
    try_solve_ocr_debug(data_uri, endpoint).await.text
}

// ── utilities ─────────────────────────────────────────────────────────────────

/// Decode a base64 data URI into raw image bytes.
pub fn decode_captcha_bytes(data_uri: &str) -> Option<Vec<u8>> {
    let raw = data_uri.split_once(";base64,").map_or(data_uri, |(_, b)| b);
    general_purpose::STANDARD.decode(raw).ok()
}

/// Helper: load from a file path (useful in tests / CLI).
pub fn solve_captcha_file(path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    solve_captcha_bytes(&bytes)
}

// ── tests ─────────────────────────────────────────────────────────────────────
#[test]
fn charset_preserves_alphanumeric_model_indices() {
    let charset = &*charset::CHARSET;

    assert_eq!(charset.len(), 62, "should contain 26 letters and 10 digits");

    assert!(!charset.contains_key(&0), "index 0 is the CTC blank");

    assert!(
        charset.keys().all(|&index| index < 8210),
        "all indices must fit within the model's 8210 classes"
    );

    assert!(
        charset.values().all(char::is_ascii_alphanumeric),
        "charset should contain only ASCII letters and digits"
    );
}
