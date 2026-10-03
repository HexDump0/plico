//! Points, rectangles and matrices in page space, and the clipping that cuts
//! a box out of a path.

pub(super) type Matrix = [f64; 6];
pub(super) type Point = (f64, f64);

pub(super) const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `first` then `second`, as PDF concatenates matrices.
pub(super) fn multiply(first: Matrix, second: Matrix) -> Matrix {
    let [a, b, c, d, e, f] = first;
    let [p, q, r, s, t, u] = second;
    [
        a * p + b * r,
        a * q + b * s,
        c * p + d * r,
        c * q + d * s,
        e * p + f * r + t,
        e * q + f * s + u,
    ]
}

pub(super) fn apply([a, b, c, d, e, f]: Matrix, (x, y): Point) -> Point {
    (a * x + c * y + e, b * x + d * y + f)
}

pub(super) fn invert([a, b, c, d, e, f]: Matrix) -> Option<Matrix> {
    let determinant = a * d - b * c;
    if !determinant.is_finite() || determinant.abs() < 1e-12 {
        return None;
    }
    Some([
        d / determinant,
        -b / determinant,
        -c / determinant,
        a / determinant,
        (c * f - d * e) / determinant,
        (b * e - a * f) / determinant,
    ])
}

pub(super) fn translate(x: f64, y: f64) -> Matrix {
    [1.0, 0.0, 0.0, 1.0, x, y]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Rect {
    pub(super) x0: f64,
    pub(super) y0: f64,
    pub(super) x1: f64,
    pub(super) y1: f64,
}

impl Rect {
    pub(super) fn around(points: impl IntoIterator<Item = Point>) -> Option<Rect> {
        let mut found: Option<Rect> = None;
        for (x, y) in points {
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
            found = Some(match found {
                None => Rect {
                    x0: x,
                    y0: y,
                    x1: x,
                    y1: y,
                },
                Some(rect) => Rect {
                    x0: rect.x0.min(x),
                    y0: rect.y0.min(y),
                    x1: rect.x1.max(x),
                    y1: rect.y1.max(y),
                },
            });
        }
        found
    }

    /// The bounds of `self` mapped through `matrix`.
    pub(super) fn transformed(self, matrix: Matrix) -> Option<Rect> {
        Rect::around(self.corners().map(|corner| apply(matrix, corner)))
    }

    pub(super) fn corners(self) -> [Point; 4] {
        [
            (self.x0, self.y0),
            (self.x1, self.y0),
            (self.x1, self.y1),
            (self.x0, self.y1),
        ]
    }

    pub(super) fn width(self) -> f64 {
        self.x1 - self.x0
    }

    pub(super) fn height(self) -> f64 {
        self.y1 - self.y0
    }

    pub(super) fn area(self) -> f64 {
        self.width() * self.height()
    }

    /// Whether the two share more than an edge.
    pub(super) fn overlaps(self, other: Rect) -> bool {
        self.x0 < other.x1 && other.x0 < self.x1 && self.y0 < other.y1 && other.y0 < self.y1
    }

    pub(super) fn overlap_area(self, other: Rect) -> f64 {
        let width = self.x1.min(other.x1) - self.x0.max(other.x0);
        let height = self.y1.min(other.y1) - self.y0.max(other.y0);
        if width > 0.0 && height > 0.0 {
            width * height
        } else {
            0.0
        }
    }

    pub(super) fn contains(self, (x, y): Point) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
}

/// The share of a glyph a box has to cover before the glyph goes. Covering
/// less leaves most of it showing, so the box hides nothing a reader cannot
/// already see; `redact-text.ts` applies the same rule to the preview.
pub(super) const COVERED: f64 = 0.25;

/// Whether `boxes` cover enough of something with these bounds to remove it.
/// Something with no area goes when its centre is covered.
pub(super) fn covered(bounds: Rect, boxes: &[Rect]) -> bool {
    let area = bounds.area();
    if area <= 1e-9 {
        let centre = ((bounds.x0 + bounds.x1) / 2.0, (bounds.y0 + bounds.y1) / 2.0);
        return boxes.iter().any(|area| area.contains(centre));
    }
    boxes
        .iter()
        .map(|area| bounds.overlap_area(*area))
        .sum::<f64>()
        >= COVERED * area
}

/// Points along a cubic Bézier, excluding its start, close enough that no
/// point of the curve is more than `tolerance` from the polyline.
pub(super) fn flatten_cubic(
    start: Point,
    first: Point,
    second: Point,
    end: Point,
    tolerance: f64,
) -> Vec<Point> {
    // The chord error after n steps is at most 3/4 of the larger second
    // difference over n squared.
    let difference = |a: Point, b: Point, c: Point| {
        let (x, y) = (a.0 - 2.0 * b.0 + c.0, a.1 - 2.0 * b.1 + c.1);
        (x * x + y * y).sqrt()
    };
    let bend = difference(start, first, second).max(difference(first, second, end));
    let steps = ((0.75 * bend / tolerance).sqrt().ceil()).clamp(1.0, 256.0) as usize;
    (1..=steps)
        .map(|step| {
            let t = step as f64 / steps as f64;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            (
                a * start.0 + b * first.0 + c * second.0 + d * end.0,
                a * start.1 + b * first.1 + c * second.1 + d * end.1,
            )
        })
        .collect()
}

/// An axis-aligned region, open to infinity where a bound is missing.
#[derive(Clone, Copy)]
struct Region {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

/// A clipping edge: where it lies, whether it bounds x rather than y, and
/// whether the kept side is above it.
type Edge = (f64, bool, bool);

/// Sutherland-Hodgman against each finite edge of `region`. Clipping every
/// contour of a path to the same convex region keeps each point's winding
/// number inside it, so fill rules still apply to the result.
fn clip_to(polygon: &[Point], region: Region) -> Vec<Point> {
    let mut points = polygon.to_vec();
    let edges: [Edge; 4] = [
        (region.x0, true, true),
        (region.x1, true, false),
        (region.y0, false, true),
        (region.y1, false, false),
    ];
    for (limit, across, above) in edges {
        let axis = |(x, y): Point| if across { x } else { y };
        if !limit.is_finite() || points.is_empty() {
            continue;
        }
        let inside = |point: Point| {
            if above {
                axis(point) >= limit
            } else {
                axis(point) <= limit
            }
        };
        // On the edge exactly, so pieces meet without a sliver between them.
        let cross = |a: Point, b: Point| {
            let t = (limit - axis(a)) / (axis(b) - axis(a));
            if across {
                (limit, a.1 + t * (b.1 - a.1))
            } else {
                (a.0 + t * (b.0 - a.0), limit)
            }
        };
        let mut clipped = Vec::with_capacity(points.len() + 4);
        for index in 0..points.len() {
            let current = points[index];
            let previous = points[(index + points.len() - 1) % points.len()];
            match (inside(previous), inside(current)) {
                (true, true) => clipped.push(current),
                (true, false) => clipped.push(cross(previous, current)),
                (false, true) => {
                    clipped.push(cross(previous, current));
                    clipped.push(current);
                }
                (false, false) => {}
            }
        }
        points = clipped;
    }
    points
}

fn polygon_area(points: &[Point]) -> f64 {
    let mut twice = 0.0;
    for index in 0..points.len() {
        let (x0, y0) = points[index];
        let (x1, y1) = points[(index + 1) % points.len()];
        twice += x0 * y1 - x1 * y0;
    }
    twice.abs() / 2.0
}

/// `polygon` with `rect` cut out, as up to four polygons: the parts left of,
/// right of, below and above the box. They tile everything outside it, so the
/// pieces together fill exactly what the polygon filled outside the box.
pub(super) fn subtract(polygon: &[Point], rect: Rect) -> Vec<Vec<Point>> {
    let open = f64::INFINITY;
    [
        Region {
            x0: -open,
            y0: -open,
            x1: rect.x0,
            y1: open,
        },
        Region {
            x0: rect.x1,
            y0: -open,
            x1: open,
            y1: open,
        },
        Region {
            x0: rect.x0,
            y0: -open,
            x1: rect.x1,
            y1: rect.y0,
        },
        Region {
            x0: rect.x0,
            y0: rect.y1,
            x1: rect.x1,
            y1: open,
        },
    ]
    .into_iter()
    .map(|region| clip_to(polygon, region))
    .filter(|piece| piece.len() >= 3 && polygon_area(piece) > 1e-9)
    .collect()
}

/// Whether a closed polygon and a box share any area: an edge crosses the
/// box, a corner of one lies in the other.
pub(super) fn polygon_touches(polygon: &[Point], rect: Rect) -> bool {
    if polygon.iter().any(|point| rect.contains(*point)) {
        return true;
    }
    if rect.corners().iter().any(|corner| winds(polygon, *corner)) {
        return true;
    }
    (0..polygon.len()).any(|index| {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        inside_span(a, b, rect).is_some()
    })
}

/// Whether the polygon goes around `point` under either fill rule.
fn winds(polygon: &[Point], (x, y): Point) -> bool {
    let mut winding = 0;
    for index in 0..polygon.len() {
        let (x0, y0) = polygon[index];
        let (x1, y1) = polygon[(index + 1) % polygon.len()];
        if (y0 <= y) != (y1 <= y) {
            let at = x0 + (y - y0) / (y1 - y0) * (x1 - x0);
            if at > x {
                winding += if y1 > y0 { 1 } else { -1 };
            }
        }
    }
    winding != 0
}

/// The part of segment `a`-`b` inside `rect`, as a range of its parameter
/// (Liang-Barsky), if any.
fn inside_span(a: Point, b: Point, rect: Rect) -> Option<(f64, f64)> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut low, mut high) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-dx, a.0 - rect.x0),
        (dx, rect.x1 - a.0),
        (-dy, a.1 - rect.y0),
        (dy, rect.y1 - a.1),
    ] {
        if p.abs() < 1e-12 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            low = low.max(t);
        } else {
            high = high.min(t);
        }
        if low > high {
            return None;
        }
    }
    (high - low > 1e-9).then_some((low, high))
}

/// The parts of segment `a`-`b` outside every box, in order along it.
pub(super) fn segment_outside(a: Point, b: Point, rects: &[Rect]) -> Vec<(Point, Point)> {
    let mut cuts = rects
        .iter()
        .filter_map(|rect| inside_span(a, b, *rect))
        .collect::<Vec<_>>();
    if cuts.is_empty() {
        return vec![(a, b)];
    }
    cuts.sort_by(|x, y| x.0.total_cmp(&y.0));
    let at = |t: f64| (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
    let mut kept = Vec::new();
    let mut from = 0.0;
    for (low, high) in cuts {
        if low > from + 1e-9 {
            kept.push((at(from), at(low)));
        }
        from = from.max(high);
    }
    if from < 1.0 - 1e-9 {
        kept.push((at(from), b));
    }
    kept
}
