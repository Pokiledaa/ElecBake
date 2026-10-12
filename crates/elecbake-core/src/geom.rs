//! Points and segments in Altium raw units (10,000 units = 1 mil).

use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

impl Point {
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

impl Rect {
    /// Smallest rectangle containing every point, or `None` for no points.
    pub fn around(points: impl IntoIterator<Item = Point>) -> Option<Rect> {
        let mut points = points.into_iter();
        let first = points.next()?;
        let mut rect = Rect {
            min: first,
            max: first,
        };
        for p in points {
            rect.min = Point::new(rect.min.x.min(p.x), rect.min.y.min(p.y));
            rect.max = Point::new(rect.max.x.max(p.x), rect.max.y.max(p.y));
        }
        Some(rect)
    }
}

/// True when `p` lies on the segment from `a` to `b`, ends included.
pub fn on_segment(p: Point, a: Point, b: Point) -> bool {
    let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    cross == 0
        && p.x >= a.x.min(b.x)
        && p.x <= a.x.max(b.x)
        && p.y >= a.y.min(b.y)
        && p.y <= a.y.max(b.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_on_segment_end_and_middle() {
        let (a, b) = (Point::new(0, 0), Point::new(10, 0));
        assert!(on_segment(Point::new(0, 0), a, b));
        assert!(on_segment(Point::new(5, 0), a, b));
        assert!(on_segment(Point::new(10, 0), a, b));
    }

    #[test]
    fn point_off_segment() {
        let (a, b) = (Point::new(0, 0), Point::new(10, 0));
        assert!(!on_segment(Point::new(11, 0), a, b));
        assert!(!on_segment(Point::new(5, 1), a, b));
    }

    #[test]
    fn point_on_diagonal_segment() {
        let (a, b) = (Point::new(0, 0), Point::new(10, 10));
        assert!(on_segment(Point::new(4, 4), a, b));
        assert!(!on_segment(Point::new(4, 5), a, b));
    }

    #[test]
    fn rect_around_points() {
        let rect = Rect::around([Point::new(3, -1), Point::new(-2, 7)]).unwrap();
        assert_eq!(
            rect,
            Rect {
                min: Point::new(-2, -1),
                max: Point::new(3, 7)
            }
        );
        assert_eq!(Rect::around([]), None);
    }
}
