//! 텍스트: Montserrat 번들, 폭 측정(원본 canvas.measureText 대응), 말줄임(`fit` 이후 CSS text-overflow), 줄바꿈(힌트),
//! 글리프 단위 그리기(letter-spacing .01em, SVG `paint-order: stroke` 후광 = bg 색 3px 외곽선 근사).

use gpui::{point, px, quad, size, BorderStyle, Bounds, Corners, Edges, Font, FontFeatures, FontStyle, FontWeight, Hsla, LineLayout, SharedString, TextRun, TextSystem, Window, WindowTextSystem, App, black, transparent_black};
use std::borrow::Cow;
use std::sync::Arc;

const MONTSERRAT: &[u8] = include_bytes!("../assets/fonts/Montserrat[wght].ttf");

pub fn register_fonts(ts: &TextSystem) {
    if let Err(e) = ts.add_fonts(vec![Cow::Borrowed(MONTSERRAT)]) {
        eprintln!("font register failed: {e:?}");
    }
}

#[derive(Clone)]
pub struct TextCtx {
    pub ts: Arc<WindowTextSystem>,
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
    pub fn new(ts: Arc<WindowTextSystem>) -> TextCtx {
        TextCtx { ts }
    }

    pub fn layout(&self, text: &str, fs: f64, wt: f64) -> Arc<LineLayout> {
        let run = TextRun { len: text.len(), font: font(wt), color: black(), background_color: None, underline: None, strikethrough: None };
        self.ts.layout_line(text, px(fs as f32), &[run], None)
    }

    /// 자간 없는 폭(원본 measureText 와 같은 의미)
    pub fn width(&self, text: &str, fs: f64, wt: f64) -> f64 {
        if text.is_empty() {
            return 0.0;
        }
        f32::from(self.layout(text, fs, wt).width) as f64
    }

    fn total_width(&self, text: &str, fs: f64, wt: f64, ls_em: f64) -> f64 {
        self.width(text, fs, wt) + fs * ls_em * text.chars().count() as f64
    }

    /// (왼쪽 x, 폭(자간 포함), ascent, descent)
    pub fn text_box(&self, text: &str, fs: f64, wt: f64, anchor: &str, x: f64) -> (f64, f64, f64, f64) {
        let l = self.layout(text, fs, wt);
        let w = self.total_width(text, fs, wt, 0.01);
        let x0 = match anchor {
            "middle" => x - w / 2.0,
            "end" => x - w,
            _ => x,
        };
        (x0, w, f32::from(l.ascent) as f64, f32::from(l.descent) as f64)
    }

    /// CSS `text-overflow: ellipsis` (한 줄, overflow hidden): 넘치면 "…" 를 붙여 폭에 맞춘다
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

    /// 줄바꿈(`lang=ko`, word-break normal): 공백 뒤, 그리고 CJK/한글 글자 사이에서 끊을 수 있다. 줄 끝 공백은 폭에서 제외.
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
                let next_is_break = end + 1 < chars.len() && (c == ' ' || is_cjk(c) || is_cjk(chars[end + 1]));
                if next_is_break {
                    last_break = Some(end + 1);
                }
                end += 1;
            }
            let cut = if end >= chars.len() { end } else { last_break.unwrap_or(end.max(start + 1)) };
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

    /// 글리프 단위로 그린다. `anchor`: start|middle|end, `baseline`: 기준선 y.
    /// halo 가 있으면 bg 색을 반지름 1.5px 원주(+0.75px 안쪽 원주)에 16+8 번 찍어 `stroke-width:3px; paint-order:stroke` 를 근사한다.
    #[allow(clippy::too_many_arguments)]
    pub fn paint(&self, window: &mut Window, _cx: &mut App, text: &str, x: f64, baseline: f64, anchor: &str, fs: f64, wt: f64, fill: Hsla, halo: Option<Hsla>, ls_em: f64, underline: bool) {
        if text.is_empty() {
            return;
        }
        let l = self.layout(text, fs, wt);
        let ls = fs * ls_em;
        let total = f32::from(l.width) as f64 + ls * text.chars().count() as f64;
        let x0 = match anchor {
            "middle" => x - total / 2.0,
            "end" => x - total,
            _ => x,
        };
        let mut glyphs: Vec<(f64, gpui::FontId, gpui::GlyphId)> = vec![];
        for run in &l.runs {
            for g in &run.glyphs {
                let k = text[..g.index.min(text.len())].chars().count() as f64;
                glyphs.push((x0 + f32::from(g.position.x) as f64 + k * ls, run.font_id, g.id));
            }
        }
        let fsp = px(fs as f32);
        if let Some(h) = halo {
            const R_OUT: f64 = 1.5;
            const R_IN: f64 = 0.75;
            for (ring_r, n, phase) in [(R_OUT, 16usize, 0.0f64), (R_IN, 8usize, std::f64::consts::PI / 8.0)] {
                for i in 0..n {
                    let a = phase + i as f64 * 2.0 * std::f64::consts::PI / n as f64;
                    let (dx, dy) = (ring_r * a.cos(), ring_r * a.sin());
                    for (gx, fid, gid) in &glyphs {
                        let _ = window.paint_glyph(point(px((*gx + dx) as f32), px((baseline + dy) as f32)), *fid, *gid, fsp, h);
                    }
                }
            }
        }
        for (gx, fid, gid) in &glyphs {
            let _ = window.paint_glyph(point(px(*gx as f32), px(baseline as f32)), *fid, *gid, fsp, fill);
        }
        if underline {
            let thick = (fs * 0.05).max(1.0);
            let b = Bounds { origin: point(px(x0 as f32), px((baseline + fs * 0.12) as f32)), size: size(px(total as f32), px(thick as f32)) };
            window.paint_quad(quad(b, Corners::all(px(0.0)), fill, Edges::all(px(0.0)), transparent_black(), BorderStyle::Solid));
        }
    }
}
