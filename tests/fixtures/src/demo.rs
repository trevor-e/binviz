// Test program for binviz fixtures: small, but with enough structure
// (inlining, generics, enums, statics, closures) to produce interesting DWARF.
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

pub enum Shape {
    Circle { center: Point, radius: f64 },
    Rect { min: Point, max: Point },
    Polygon(Vec<Point>),
}

#[inline(always)]
fn square(v: f64) -> f64 {
    v * v
}

#[inline(never)]
pub fn distance(a: Point, b: Point) -> f64 {
    (square(a.x - b.x) + square(a.y - b.y)).sqrt()
}

impl Shape {
    #[inline(never)]
    pub fn area(&self) -> f64 {
        match self {
            Shape::Circle { radius, .. } => std::f64::consts::PI * square(*radius),
            Shape::Rect { min, max } => (max.x - min.x).abs() * (max.y - min.y).abs(),
            Shape::Polygon(points) => {
                let mut sum = 0.0;
                for i in 0..points.len() {
                    let a = points[i];
                    let b = points[(i + 1) % points.len()];
                    sum += a.x * b.y - b.x * a.y;
                }
                sum.abs() / 2.0
            }
        }
    }
}

#[inline(never)]
fn largest<T: PartialOrd + Copy>(items: &[T]) -> Option<T> {
    let mut best = *items.first()?;
    for &item in items {
        if item > best {
            best = item;
        }
    }
    Some(best)
}

pub static GREETING: &str = "hello from binviz";
pub static mut COUNTER: u32 = 0;
static TABLE: [u16; 8] = [1, 1, 2, 3, 5, 8, 13, 21];

#[inline(never)]
fn word_lengths(text: &str) -> BTreeMap<usize, usize> {
    let mut counts = BTreeMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word.len()).or_insert(0) += 1;
    }
    counts
}

fn main() {
    let shapes = vec![
        Shape::Circle { center: Point { x: 0.0, y: 0.0 }, radius: 2.0 },
        Shape::Rect { min: Point { x: 1.0, y: 1.0 }, max: Point { x: 4.0, y: 3.0 } },
        Shape::Polygon(vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 4.0, y: 0.0 },
            Point { x: 0.0, y: 3.0 },
        ]),
    ];
    let areas: Vec<f64> = shapes.iter().map(Shape::area).collect();
    let d = distance(Point { x: 0.0, y: 0.0 }, Point { x: 3.0, y: 4.0 });
    unsafe {
        COUNTER += shapes.len() as u32;
    }
    let counts = word_lengths(GREETING);
    println!(
        "{GREETING}: areas {:?}, largest {:?}, distance {d}, fib {}, words {:?}, counter {}",
        areas,
        largest(&areas),
        TABLE.iter().copied().map(u32::from).sum::<u32>(),
        counts,
        unsafe { COUNTER }
    );
}
