//! 시나리오 러너: JSON 액션 목록을 가짜 시계로 실행하고 덤프를 모은다.
//! 같은 시나리오를 원본 JS 하니스(`tools/js_harness.js`)도 같은 의미로 실행한다.
//!
//! 액션 의미(양쪽 동일):
//!  - step {ms,n}      : n 번 반복: now+=ms; 기한이 된 타이머 실행; rAF 가 걸려 있으면 tick(now)
//!  - down {x,y}       : svg pointerdown            → 이벤트 뒤 기한된 타이머 실행
//!  - move {x,y}       : window pointermove
//!  - up {click}       : window pointerup, click 이 id 면 그 노드 요소에 click 이벤트(요소가 있을 때만)
//!  - node {id}        : 요소가 있으면 현재 위치(cur)에서 down → up(click=id)
//!  - wheel {x,y,dy}   : svg wheel
//!  - key {key}        : window keydown
//!  - nodekey {id,key} : 노드 요소 keydown(버블링으로 window 핸들러도 실행)
//!  - resize {w,h}     : window resize
//!  - dump {label}     : 상태 덤프

use crate::{Engine, stub_measure};
use serde_json::Value;

pub const START_NOW: f64 = 1000.0;

pub fn run(data: &Value, actions: &[Value], w: f64, h: f64) -> Vec<Value> {
    let mut now = START_NOW;
    let mut en = Engine::new(data, w, h, now, Box::new(stub_measure));
    let mut dumps = vec![];
    for a in actions {
        let g = |k: &str| a.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
        match a["a"].as_str().unwrap() {
            "step" => {
                let (ms, n) = (g("ms"), g("n") as usize);
                for _ in 0..n {
                    now += ms;
                    en.fire_timers(now);
                    if en.raf {
                        en.tick(now);
                    }
                }
            }
            "down" => {
                en.pointer_down(g("x"), g("y"));
                en.fire_timers(now);
            }
            "move" => {
                en.pointer_move(g("x"), g("y"));
                en.fire_timers(now);
            }
            "up" => {
                en.pointer_up(now);
                if let Some(id) = a.get("click").and_then(|v| v.as_u64()) {
                    if en.e.contains_key(&(id as usize)) {
                        en.click_node(id as usize, now);
                    }
                }
                en.fire_timers(now);
            }
            "node" => {
                let id = g("id") as usize;
                if en.e.contains_key(&id) {
                    let (x, y) = {
                        let c = &en.cur[&id];
                        (c.x, c.y)
                    };
                    en.pointer_down(x, y);
                    en.fire_timers(now);
                    en.pointer_up(now);
                    en.click_node(id, now);
                    en.fire_timers(now);
                }
            }
            "wheel" => {
                en.wheel(g("x"), g("y"), g("dy"), now);
                en.fire_timers(now);
            }
            "key" => {
                en.key(a["key"].as_str().unwrap(), now);
                en.fire_timers(now);
            }
            "nodekey" => {
                let id = g("id") as usize;
                if en.e.contains_key(&id) {
                    en.node_key(id, a["key"].as_str().unwrap(), now);
                    en.fire_timers(now);
                }
            }
            "resize" => {
                en.resize(g("w"), g("h"));
                en.fire_timers(now);
            }
            "dump" => dumps.push(en.dump(a["label"].as_str().unwrap_or(""), now)),
            other => panic!("unknown action {other}"),
        }
    }
    dumps
}
