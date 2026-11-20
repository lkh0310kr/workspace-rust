//! 사용: rte-dump <scenario.json> <W> <H>  → stdout 에 덤프 배열(JSON)
use std::fs;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data: serde_json::Value = serde_json::from_str(include_str!("../../data/data.json")).unwrap();
    let sc: serde_json::Value = serde_json::from_str(&fs::read_to_string(&a[1]).unwrap()).unwrap();
    let (w, h): (f64, f64) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let dumps = rte_engine::scenario::run(&data, sc["actions"].as_array().unwrap(), w, h);
    println!("{}", serde_json::to_string(&dumps).unwrap());
}
