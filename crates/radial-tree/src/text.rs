//! 텍스트: Montserrat 번들, 폭 측정(원본 canvas.measureText 대응), 말줄임, 줄바꿈(힌트),
//! 글리프 단위 그리기(letter-spacing .01em, SVG `paint-order: stroke` 후광 = bg 색 외곽선 근사).
//!
//! 성능: 텍스트 레이아웃/글리프 위치는 (텍스트, 크기, 두께, 자간) 키로 캐시한다(프레임마다 재계산하지 않음).

use gpui::{
    black, point, px, quad, size, transparent_black, App, BorderStyle, Bounds, Corners, Edges,
    Font, FontFeatures, FontId, FontStyle, FontWeight, GlyphId, Hsla, SharedString, TextRun,
    TextSystem, Window, WindowTextSystem,
};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

const MONTSERRAT: &[u8] = include_bytes!("../assets/fonts/Montserrat[wght].ttf");

pub fn register_fonts(ts: &TextSystem) {
    if let Err(e) = ts.add_fonts(vec![Cow::Borrowed(MONTSERRAT)]) {
        eprintln!("font register failed: {e:?}");
    }
}

/// 한 텍스트의 레이아웃 결과(자간 포함 글리프 상대 위치)
pub struct Cached {
    /// 자간 없는 폭(원본 measureText)
    pub width: f64,
    /// 자간 포함 총 폭
    pub total: f64,
    pub ascent: f64,
    pub descent: f64,
    /// (왼쪽 기준 상대 x, 폰트, 글리프)
    pub glyphs: Vec<(f64, FontId, GlyphId)>,
}

type Key = (String, u32, u32, u32);

#[derive(Clone)]
pub struct TextCtx {
    pub ts: Arc<WindowTextSystem>,
    cache: Rc<RefCell<HashMap<Key, Rc<Cached>>>>,
    /// 성능 계측: 이번 구간에 그린 글리프 수
    pub glyph_draws: Rc<Cell<u64>>,
    /// 후광 샘플 수(바깥 원주, 안쪽 원주). RTE_HALO=full 이면 원래의 24(16+8)
    pub halo_samples: (usize, usize),
}

fn font(wt: f64) -> Font {
    Font {
        family: SharedString::from("Montserrat"),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: FontWeight(wt as f32),
        style: FontStyle::Normal,
    }
}

fn is_cjk(c: char) -> bool {
    let u = c as u32;
    u >= 0x2e80 || (0xac00..=0xd7a3).contains(&u)
}

impl TextCtx {
    pub fn new(ts: Arc<WindowTextSystem>, halo_samples: (usize, usize)) -> TextCtx {
        TextCtx {
            ts,
            cache: Rc::new(RefCell::new(HashMap::new())),
            glyph_draws: Rc::new(Cell::new(0)),
            halo_samples,
        }
    }

    pub fn get(&self, text: &str, fs: f64, wt: f64, ls_em: f64) -> Rc<Cached> {
        let key: Key = (
            text.to_string(),
            (fs as f32).to_bits(),
            (wt as f32).to_bits(),
            (ls_em as f32).to_bits(),
        );
        if let Some(c) = self.cache.borrow().get(&key) {
            return c.clone();
        }
        let run = TextRun {
            len: text.len(),
            font: font(wt),
            color: black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let l = self.ts.layout_line(text, px(fs as f32), &[run], None);
        let ls = fs * ls_em;
        let n = text.chars().count() as f64;
        let mut glyphs = vec![];
        // 바이트 인덱스 → 문자 인덱스 (한 번만, 선형)
        let mut char_at_byte: Vec<u32> = vec![0; text.len() + 1];
        let mut ci = 0u32;
        for (bi, ch) in text.char_indices() {
            for k in 0..ch.len_utf8() {
                char_at_byte[bi + k] = ci;
            }
            ci += 1;
        }
        char_at_byte[text.len()] = ci;
        for run in &l.runs {
            for g in &run.glyphs {
                let k = char_at_byte[g.index.min(text.len())] as f64;
                glyphs.push((f32::from(g.position.x) as f64 + k * ls, run.font_id, g.id));
            }
        }
        let width = f32::from(l.width) as f64;
        let c = Rc::new(Cached {
            width,
            total: width + ls * n,
            ascent: f32::from(l.ascent) as f64,
            descent: f32::from(l.descent) as f64,
            glyphs,
        });
        let mut cache = self.cache.borrow_mut();
        if cache.len() > 8000 {
            cache.clear();
        }
        cache.insert(key, c.clone());
        c
    }

    /// 자간 없는 폭(원본 measureText 와 같은 의미)
    pub fn width(&self, text: &str, fs: f64, wt: f64) -> f64 {
        if text.is_empty() {
            return 0.0;
        }
        self.get(text, fs, wt, 0.0).width
    }

    fn total_width(&self, text: &str, fs: f64, wt: f64, ls_em: f64) -> f64 {
        if text.is_empty() {
            return 0.0;
        }
        self.get(text, fs, wt, ls_em).total
    }

    /// (왼쪽 x, 폭(자간 포함), ascent, descent)
    pub fn text_box(
        &self,
        text: &str,
        fs: f64,
        wt: f64,
        anchor: &str,
        x: f64,
    ) -> (f64, f64, f64, f64) {
        let c = self.get(text, fs, wt, 0.01);
        let x0 = match anchor {
            "middle" => x - c.total / 2.0,
            "end" => x - c.total,
            _ => x,
        };
        (x0, c.total, c.ascent, c.descent)
    }

    /// CSS `text-overflow: ellipsis`
    pub fn ellipsize(&self, text: &str, fs: f64, wt: f64, max_w: f64, ls_em: f64) -> String {
        if self.total_width(text, fs, wt, ls_em) <= max_w + 0.01 {
            return text.to_string();
        }
        let mut chars: Vec<char> = text.chars().collect();
        while !chars.is_empty() {
            chars.pop();
            let cand: String = chars.iter().collect::<String>() + "…";
            if self.total_width(&cand, fs, wt, ls_em) <= max_w + 0.01 {
                return cand;
            }
        }
        String::new()
    }

    /// 줄바꿈(`lang=ko`): 공백 뒤, CJK/한글 글자 사이. 줄 끝 공백은 폭에서 제외.
    pub fn wrap(&self, text: &str, fs: f64, wt: f64, max_w: f64, ls_em: f64) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        let mut lines: Vec<String> = vec![];
        let mut start = 0usize;
        while start < chars.len() {
            let mut last_break: Option<usize> = None;
            let mut end = start;
            loop {
                if end >= chars.len() {
                    break;
                }
                let cand: String = chars[start..=end].iter().collect();
                let trimmed = cand.trim_end();
                if self.total_width(trimmed, fs, wt, ls_em) > max_w && end > start {
                    break;
                }
                let c = chars[end];
                let next_is_break =
                    end + 1 < chars.len() && (c == ' ' || is_cjk(c) || is_cjk(chars[end + 1]));
                if next_is_break {
                    last_break = Some(end + 1);
                }
                end += 1;
            }
            let cut = if end >= chars.len() {
                end
            } else {
                last_break.unwrap_or(end.max(start + 1))
            };
            let line: String = chars[start..cut].iter().collect();
            lines.push(line.trim_end().to_string());
            start = cut;
            while start < chars.len() && chars[start] == ' ' {
                start += 1;
            }
        }
        if lines.is_empty() {
            lines.push(String::new());
        }
        lines
    }

    /// 글리프 단위로 그린다. `halo` 가 Some 이면 그 색의 외곽선(반지름 1.5px 원주 + 0.75px 원주 샘플)을 먼저 그린다.
    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &self,
        window: &mut Window,
        _cx: &mut App,
        text: &str,
        x: f64,
        baseline: f64,
        anchor: &str,
        fs: f64,
        wt: f64,
        fill: Hsla,
        halo: Option<Hsla>,
        ls_em: f64,
        underline: bool,
    ) {
        if text.is_empty() {
            return;
        }
        let c = self.get(text, fs, wt, ls_em);
        let x0 = match anchor {
            "middle" => x - c.total / 2.0,
            "end" => x - c.total,
            _ => x,
        };
        let fsp = px(fs as f32);
        let mut draws = 0u64;
        if let Some(h) = halo {
            let (n_out, n_in) = self.halo_samples;
            for (ring_r, n, phase) in [
                (1.5f64, n_out, 0.0f64),
                (0.75f64, n_in, std::f64::consts::PI / 8.0),
            ] {
                for i in 0..n {
                    let a = phase + i as f64 * 2.0 * std::f64::consts::PI / n as f64;
                    let (dx, dy) = (ring_r * a.cos(), ring_r * a.sin());
                    for (gx, fid, gid) in &c.glyphs {
                        let _ = window.paint_glyph(
                            point(px((x0 + gx + dx) as f32), px((baseline + dy) as f32)),
                            *fid,
                            *gid,
                            fsp,
                            h,
                        );
                        draws += 1;
                    }
                }
            }
        }
        for (gx, fid, gid) in &c.glyphs {
            let _ = window.paint_glyph(
                point(px((x0 + gx) as f32), px(baseline as f32)),
                *fid,
                *gid,
                fsp,
                fill,
            );
            draws += 1;
        }
        self.glyph_draws.set(self.glyph_draws.get() + draws);
        if underline {
            let thick = (fs * 0.05).max(1.0);
            let b = Bounds {
                origin: point(px(x0 as f32), px((baseline + fs * 0.12) as f32)),
                size: size(px(c.total as f32), px(thick as f32)),
            };
            window.paint_quad(quad(
                b,
                Corners::all(px(0.0)),
                fill,
                Edges::all(px(0.0)),
                transparent_black(),
                BorderStyle::Solid,
            ));
        }
    }
}
