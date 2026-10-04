//! Photos of paper turned into pages: the sheet found in the photo, the
//! perspective taken out, the light evened, and the result written as a PDF.

use lopdf::{Document, Object, Stream, dictionary};

use crate::compression::{PixelColors, encode_jpeg, write_compressed};
use crate::images::prepare_image;

/// Top left, top right, bottom right and bottom left, as x and y fractions
/// of the photo.
pub type Corners = [f32; 8];

/// The whole photo, for when nothing better is known.
pub const FULL_PHOTO: Corners = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScanLook {
    /// The photo as taken, only straightened.
    Original,
    /// Paper made white and shadows lifted, colour kept.
    Document,
    Grayscale,
    /// One bit per pixel: the smallest, for text.
    BlackWhite,
}

#[derive(Clone, Copy, Debug)]
pub enum ScanPaper {
    /// Each page shaped like its scan, this many points across its short side.
    Fit(f32),
    /// Every page this size, turned to suit each scan.
    Sheet(f32, f32),
}

/// The photo the work runs on. The detector looks at a copy this many
/// pixels on its long side: enough to place an edge within a fraction of a
/// percent, and quick enough to run on every frame of a camera preview.
const DETECT_SIDE: usize = 512;

/// How far a page may reach past the photo and still count as found.
const SLACK: f32 = 0.03;

/// Finds the sheet of paper in an RGBA photo, or `None` when no four edges
/// make a convincing one.
pub fn find_page(rgba: &[u8], width: u32, height: u32) -> Option<Corners> {
    let (width, height) = (width as usize, height as usize);
    if width < 16 || height < 16 || rgba.len() < width * height * 4 {
        return None;
    }
    let small = shrink(rgba, width, height, DETECT_SIDE);
    let edges = edges(&small);
    let lines = lines(&edges);
    let quad = best_quad(&edges, &lines)?;
    let (w, h) = (small.width as f32, small.height as f32);
    let mut corners = [0.0; 8];
    for (index, point) in quad.iter().enumerate() {
        corners[index * 2] = (point.0 / w).clamp(0.0, 1.0);
        corners[index * 2 + 1] = (point.1 / h).clamp(0.0, 1.0);
    }
    Some(corners)
}

/// Straightens the area inside `corners`, turns it `turns` quarter turns
/// clockwise, applies `look`, and encodes it: JPEG, or for black and white a
/// one-bit PNG. Its long side is at most `max_side` pixels and never more
/// than the photo had to give.
pub fn scan_image(
    rgba: &[u8],
    width: u32,
    height: u32,
    corners: Corners,
    turns: u8,
    look: ScanLook,
    max_side: u32,
) -> Result<Vec<u8>, String> {
    let (width, height) = (width as usize, height as usize);
    if width == 0 || height == 0 || rgba.len() < width * height * 4 {
        return Err("The photo could not be read.".into());
    }
    if corners.iter().any(|value| !value.is_finite()) {
        return Err("Choose the corners of the page.".into());
    }
    let mut quad = [(0.0f32, 0.0f32); 4];
    for (index, point) in quad.iter_mut().enumerate() {
        *point = (
            corners[index * 2].clamp(-SLACK, 1.0 + SLACK) * width as f32,
            corners[index * 2 + 1].clamp(-SLACK, 1.0 + SLACK) * height as f32,
        );
    }
    // A quarter turn clockwise puts what was the bottom left at the top left.
    quad.rotate_right(usize::from(turns % 4));
    if polygon_area(&quad) < 64.0 {
        return Err("The page is too small to scan. Move its corners apart.".into());
    }
    let (out_width, out_height) = output_size(&quad, width, height, max_side.max(16) as usize);
    let rgb = warp(rgba, width, height, &quad, out_width, out_height);
    encode(rgb, out_width, out_height, look)
}

/// One page per image, in order. Images are JPEG or PNG as `scan_image`
/// writes them.
pub fn scans_to_pdf_bytes(images: &[&[u8]], paper: ScanPaper) -> Result<Vec<u8>, String> {
    if images.is_empty() {
        return Err("Add at least one page to scan.".into());
    }
    let valid = match paper {
        ScanPaper::Fit(short) => short.is_finite() && (36.0..=7_200.0).contains(&short),
        ScanPaper::Sheet(a, b) => {
            a.is_finite()
                && b.is_finite()
                && (36.0..=14_400.0).contains(&a)
                && (36.0..=14_400.0).contains(&b)
        }
    };
    if !valid {
        return Err("Choose a valid page size.".into());
    }
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let mut kids = Vec::with_capacity(images.len());
    for (index, bytes) in images.iter().enumerate() {
        let mut image = prepare_image(bytes)
            .map_err(|error| format!("Page {} could not be added: {error}", index + 1))?;
        let (image_width, image_height) = (image.width as f32, image.height as f32);
        let aspect = image_width / image_height;
        let (page_width, page_height) = match paper {
            ScanPaper::Fit(short) => {
                let (width, height) = if aspect < 1.0 {
                    (short, short / aspect)
                } else {
                    (short * aspect, short)
                };
                let shrink = (14_400.0 / width.max(height)).min(1.0);
                (width * shrink, height * shrink)
            }
            ScanPaper::Sheet(a, b) => {
                let (short, long) = (a.min(b), a.max(b));
                if aspect > 1.0 {
                    (long, short)
                } else {
                    (short, long)
                }
            }
        };
        // A straightened photo's proportions are an estimate, so a scan within
        // a tenth of the sheet's shape fills it rather than leaving slivers.
        let page_aspect = page_width / page_height;
        let (draw_width, draw_height) = if (aspect / page_aspect - 1.0).abs() < 0.1 {
            (page_width, page_height)
        } else {
            let scale = (page_width / image_width).min(page_height / image_height);
            (image_width * scale, image_height * scale)
        };
        let x = (page_width - draw_width) / 2.0;
        let y = (page_height - draw_height) / 2.0;
        if let Some(mask) = image.mask.take() {
            let mask_id = document.add_object(mask);
            image.stream.dict.set("SMask", mask_id);
        }
        let image_id = document.add_object(image.stream);
        let content =
            format!("q\n{draw_width:.4} 0 0 {draw_height:.4} {x:.4} {y:.4} cm\n/Im0 Do\nQ\n");
        let content_id = document.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![Object::Integer(0), Object::Integer(0), Object::Real(page_width), Object::Real(page_height)],
            "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image_id } },
            "Contents" => content_id,
        });
        kids.push(Object::Reference(page_id));
    }
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => images.len() as i64,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document.compress();
    write_compressed(document)
}

struct Planes {
    width: usize,
    height: usize,
    /// Red, green and blue, each row by row, 0 to 255.
    rgb: [Vec<f32>; 3],
}

/// An area-averaged copy at most `side` pixels on its long side.
fn shrink(rgba: &[u8], width: usize, height: usize, side: usize) -> Planes {
    let scale = (side as f32 / width.max(height) as f32).min(1.0);
    let out_width = ((width as f32 * scale).round() as usize).max(1);
    let out_height = ((height as f32 * scale).round() as usize).max(1);
    let mut sums = [
        vec![0.0f32; out_width * out_height],
        vec![0.0f32; out_width * out_height],
        vec![0.0f32; out_width * out_height],
    ];
    let mut counts = vec![0u32; out_width * out_height];
    for y in 0..height {
        let out_y = (y * out_height / height).min(out_height - 1);
        for x in 0..width {
            let out_x = (x * out_width / width).min(out_width - 1);
            let at = (y * width + x) * 4;
            let cell = out_y * out_width + out_x;
            for (channel, sum) in sums.iter_mut().enumerate() {
                sum[cell] += f32::from(rgba[at + channel]);
            }
            counts[cell] += 1;
        }
    }
    for (cell, count) in counts.iter().enumerate() {
        let count = (*count).max(1) as f32;
        for sum in &mut sums {
            sum[cell] /= count;
        }
    }
    Planes {
        width: out_width,
        height: out_height,
        rgb: sums,
    }
}

/// A 5-tap binomial blur, which is close to a Gaussian of sigma 1.
fn blur(plane: &[f32], width: usize, height: usize) -> Vec<f32> {
    const TAPS: [f32; 5] = [1.0 / 16.0, 4.0 / 16.0, 6.0 / 16.0, 4.0 / 16.0, 1.0 / 16.0];
    let mut across = vec![0.0; plane.len()];
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            for (tap, weight) in TAPS.iter().enumerate() {
                let sx = (x as isize + tap as isize - 2).clamp(0, width as isize - 1) as usize;
                sum += plane[y * width + sx] * weight;
            }
            across[y * width + x] = sum;
        }
    }
    let mut out = vec![0.0; plane.len()];
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            for (tap, weight) in TAPS.iter().enumerate() {
                let sy = (y as isize + tap as isize - 2).clamp(0, height as isize - 1) as usize;
                sum += across[sy * width + x] * weight;
            }
            out[y * width + x] = sum;
        }
    }
    out
}

struct Edges {
    width: usize,
    height: usize,
    /// Edge strength after thinning, 0 where there is no edge.
    strength: Vec<f32>,
    /// The direction across each edge, in radians from 0 to pi.
    normal: Vec<f32>,
}

/// Canny's edges, taken on whichever colour channel changes most at each
/// pixel, so white paper on a pale but coloured desk still has an edge.
fn edges(planes: &Planes) -> Edges {
    let (width, height) = (planes.width, planes.height);
    let blurred = planes
        .rgb
        .each_ref()
        .map(|plane| blur(plane, width, height));
    let mut magnitude = vec![0.0f32; width * height];
    let mut gx = vec![0.0f32; width * height];
    let mut gy = vec![0.0f32; width * height];
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            let at = y * width + x;
            for plane in &blurred {
                let p = |dx: isize, dy: isize| {
                    plane[(y as isize + dy) as usize * width + (x as isize + dx) as usize]
                };
                let dx = p(1, -1) + 2.0 * p(1, 0) + p(1, 1) - p(-1, -1) - 2.0 * p(-1, 0) - p(-1, 1);
                let dy = p(-1, 1) + 2.0 * p(0, 1) + p(1, 1) - p(-1, -1) - 2.0 * p(0, -1) - p(1, -1);
                let strength = dx.hypot(dy);
                if strength > magnitude[at] {
                    magnitude[at] = strength;
                    gx[at] = dx;
                    gy[at] = dy;
                }
            }
        }
    }
    // Thin each edge to its crest across the edge.
    let mut thin = vec![0.0f32; width * height];
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            let at = y * width + x;
            let strength = magnitude[at];
            if strength <= 0.0 {
                continue;
            }
            let (ux, uy) = (gx[at] / strength, gy[at] / strength);
            let step_x = ux.round() as isize;
            let step_y = uy.round() as isize;
            let ahead =
                magnitude[(y as isize + step_y) as usize * width + (x as isize + step_x) as usize];
            let behind =
                magnitude[(y as isize - step_y) as usize * width + (x as isize - step_x) as usize];
            if strength >= ahead && strength > behind {
                thin[at] = strength;
            }
        }
    }
    // Thresholds follow the photo: strong edges are the top tenth of crests,
    // with a floor so a flat photo's noise is never an edge.
    let mut crests: Vec<f32> = thin.iter().copied().filter(|value| *value > 0.0).collect();
    let high = if crests.is_empty() {
        f32::INFINITY
    } else {
        let at = crests.len() * 9 / 10;
        let (_, value, _) = crests.select_nth_unstable_by(at, f32::total_cmp);
        value.clamp(40.0, 240.0)
    };
    let low = high * 0.4;
    let mut kept = vec![false; width * height];
    let mut stack = Vec::new();
    for (at, value) in thin.iter().enumerate() {
        if *value >= high && !kept[at] {
            kept[at] = true;
            stack.push(at);
            while let Some(at) = stack.pop() {
                let (x, y) = (at % width, at / width);
                for dy in -1isize..=1 {
                    for dx in -1isize..=1 {
                        let nx = x as isize + dx;
                        let ny = y as isize + dy;
                        if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
                            continue;
                        }
                        let next = ny as usize * width + nx as usize;
                        if !kept[next] && thin[next] >= low {
                            kept[next] = true;
                            stack.push(next);
                        }
                    }
                }
            }
        }
    }
    let mut normal = vec![0.0f32; width * height];
    let strength = thin
        .iter()
        .enumerate()
        .map(|(at, value)| {
            if kept[at] {
                normal[at] = gy[at].atan2(gx[at]).rem_euclid(std::f32::consts::PI);
                *value
            } else {
                0.0
            }
        })
        .collect();
    Edges {
        width,
        height,
        strength,
        normal,
    }
}

/// A line `x cos(theta) + y sin(theta) = rho`.
#[derive(Clone, Copy, Debug)]
struct Line {
    theta: f32,
    rho: f32,
}

const ANGLES: usize = 180;
/// Each edge pixel votes only for lines within this many degrees of its own
/// direction, so text and texture spread no votes across the space.
const VOTE_SPREAD: isize = 6;

/// The strongest straight lines among the edges.
fn lines(edges: &Edges) -> Vec<Line> {
    let (width, height) = (edges.width, edges.height);
    let diagonal = ((width * width + height * height) as f32).sqrt().ceil() as usize;
    let rhos = diagonal * 2 + 1;
    let mut votes = vec![0u32; ANGLES * rhos];
    let trig: Vec<(f32, f32)> = (0..ANGLES)
        .map(|angle| {
            let theta = (angle as f32).to_radians();
            (theta.cos(), theta.sin())
        })
        .collect();
    for y in 0..height {
        for x in 0..width {
            let at = y * width + x;
            if edges.strength[at] <= 0.0 {
                continue;
            }
            let center = edges.normal[at].to_degrees().round() as isize;
            for offset in -VOTE_SPREAD..=VOTE_SPREAD {
                let angle = (center + offset).rem_euclid(ANGLES as isize) as usize;
                let (cos, sin) = trig[angle];
                let rho = x as f32 * cos + y as f32 * sin;
                let bin = (rho.round() as isize + diagonal as isize) as usize;
                votes[angle * rhos + bin] += 1;
            }
        }
    }
    let shortest = width.min(height) as u32;
    let floor = (shortest / 6).max(12);
    let mut peaks = Vec::new();
    for angle in 0..ANGLES {
        for bin in 0..rhos {
            let count = votes[angle * rhos + bin];
            if count < floor {
                continue;
            }
            let mut highest = true;
            'window: for da in -3isize..=3 {
                let other_angle = angle as isize + da;
                // Past either end, the same lines continue with rho negated.
                let (other_angle, mirror) = if other_angle < 0 {
                    (other_angle + ANGLES as isize, true)
                } else if other_angle >= ANGLES as isize {
                    (other_angle - ANGLES as isize, true)
                } else {
                    (other_angle, false)
                };
                for db in -4isize..=4 {
                    let rho = bin as isize - diagonal as isize + db;
                    let other_bin = if mirror { -rho } else { rho } + diagonal as isize;
                    if other_bin < 0 || other_bin >= rhos as isize || (da == 0 && db == 0) {
                        continue;
                    }
                    let other = votes[other_angle as usize * rhos + other_bin as usize];
                    if other > count || (other == count && (da, db) < (0, 0)) {
                        highest = false;
                        break 'window;
                    }
                }
            }
            if highest {
                peaks.push((count, angle, bin));
            }
        }
    }
    peaks.sort_unstable_by_key(|peak| std::cmp::Reverse(peak.0));
    let mut found: Vec<Line> = Vec::new();
    for (_, angle, bin) in peaks {
        let line = Line {
            theta: (angle as f32).to_radians(),
            rho: bin as f32 - diagonal as f32,
        };
        if found.iter().all(|other| !same_line(*other, line)) {
            found.push(line);
        }
        if found.len() == 20 {
            break;
        }
    }
    found
}

fn same_line(a: Line, b: Line) -> bool {
    use std::f32::consts::PI;
    let (mut theta, mut rho) = (b.theta, b.rho);
    if (a.theta - theta).abs() > PI / 2.0 {
        theta += if theta < a.theta { PI } else { -PI };
        rho = -rho;
    }
    (a.theta - theta).abs() < 4f32.to_radians() && (a.rho - rho).abs() < 8.0
}

/// How far apart two lines' directions are, from 0 to pi / 2.
fn angle_between(a: Line, b: Line) -> f32 {
    use std::f32::consts::PI;
    let difference = (a.theta - b.theta).rem_euclid(PI);
    difference.min(PI - difference)
}

fn intersect(a: Line, b: Line) -> Option<(f32, f32)> {
    let (ca, sa) = (a.theta.cos(), a.theta.sin());
    let (cb, sb) = (b.theta.cos(), b.theta.sin());
    let determinant = ca * sb - sa * cb;
    if determinant.abs() < 1e-4 {
        return None;
    }
    Some((
        (a.rho * sb - b.rho * sa) / determinant,
        (ca * b.rho - cb * a.rho) / determinant,
    ))
}

/// Picks the four lines whose quadrilateral the edges trace best, favouring
/// larger ones, and fits each side to its edge pixels.
fn best_quad(edges: &Edges, lines: &[Line]) -> Option<[(f32, f32); 4]> {
    let (width, height) = (edges.width as f32, edges.height as f32);
    let mut sides: Vec<Line> = lines.to_vec();
    // The photo's own edges stand in for a side of a page that runs out of
    // the frame. They carry no edge pixels, so they count for a third.
    let borders = [
        Line {
            theta: 0.0,
            rho: 0.0,
        },
        Line {
            theta: 0.0,
            rho: width - 1.0,
        },
        Line {
            theta: std::f32::consts::FRAC_PI_2,
            rho: 0.0,
        },
        Line {
            theta: std::f32::consts::FRAC_PI_2,
            rho: height - 1.0,
        },
    ];
    let first_border = sides.len();
    sides.extend(borders);
    let mut pairs = Vec::new();
    for a in 0..sides.len() {
        for b in a + 1..sides.len() {
            if a >= first_border && b >= first_border {
                continue;
            }
            if angle_between(sides[a], sides[b]) < 30f32.to_radians() {
                pairs.push((a, b));
            }
        }
    }
    let area = width * height;
    let mut best: Option<(f32, [(f32, f32); 4])> = None;
    for (index, &(a, b)) in pairs.iter().enumerate() {
        for &(c, d) in &pairs[index + 1..] {
            if [c, d].iter().any(|side| *side == a || *side == b) {
                continue;
            }
            if angle_between(sides[a], sides[c]) < 35f32.to_radians() {
                continue;
            }
            let (Some(p0), Some(p1), Some(p2), Some(p3)) = (
                intersect(sides[a], sides[c]),
                intersect(sides[c], sides[b]),
                intersect(sides[b], sides[d]),
                intersect(sides[d], sides[a]),
            ) else {
                continue;
            };
            let Some(quad) = ordered([p0, p1, p2, p3], width, height) else {
                continue;
            };
            let covered = polygon_area(&quad) / area;
            if covered < 0.1 {
                continue;
            }
            let mut total = 0.0;
            let mut length = 0.0;
            let mut weakest = f32::INFINITY;
            for side in 0..4 {
                let from = quad[side];
                let to = quad[(side + 1) % 4];
                let on_border = is_border(from, to, width, height);
                let support = if on_border {
                    0.35
                } else {
                    support(edges, from, to)
                };
                let span = (to.0 - from.0).hypot(to.1 - from.1);
                total += support * span;
                length += span;
                weakest = weakest.min(support);
            }
            let coverage = total / length;
            if weakest < 0.3 || coverage < 0.5 {
                continue;
            }
            let score = coverage * coverage * covered.sqrt();
            if best.is_none_or(|(highest, _)| score > highest) {
                best = Some((score, quad));
            }
        }
    }
    let (score, quad) = best?;
    // Four borders alone are the whole photo, which says nothing was found.
    if score < 0.3 {
        return None;
    }
    Some(refine(edges, quad))
}

/// The corners in clockwise order from the top left, or `None` when they do
/// not make a convex quadrilateral inside the photo with sensible angles.
fn ordered(points: [(f32, f32); 4], width: f32, height: f32) -> Option<[(f32, f32); 4]> {
    let slack_x = width * SLACK;
    let slack_y = height * SLACK;
    if points.iter().any(|(x, y)| {
        *x < -slack_x || *y < -slack_y || *x > width + slack_x || *y > height + slack_y
    }) {
        return None;
    }
    let cx = points.iter().map(|p| p.0).sum::<f32>() / 4.0;
    let cy = points.iter().map(|p| p.1).sum::<f32>() / 4.0;
    let mut sorted = points;
    sorted.sort_by(|a, b| {
        (a.1 - cy)
            .atan2(a.0 - cx)
            .total_cmp(&(b.1 - cy).atan2(b.0 - cx))
    });
    let start = (0..4)
        .min_by(|&i, &j| (sorted[i].0 + sorted[i].1).total_cmp(&(sorted[j].0 + sorted[j].1)))
        .unwrap_or(0);
    sorted.rotate_left(start);
    for corner in 0..4 {
        let previous = sorted[(corner + 3) % 4];
        let point = sorted[corner];
        let next = sorted[(corner + 1) % 4];
        let (ax, ay) = (previous.0 - point.0, previous.1 - point.1);
        let (bx, by) = (next.0 - point.0, next.1 - point.1);
        // Clockwise on screen, where y points down, turns the same way at
        // every corner when the shape is convex.
        let cross = bx * ay - by * ax;
        let lengths = ax.hypot(ay) * bx.hypot(by);
        if cross <= 0.0 || lengths < 1.0 {
            return None;
        }
        let angle = ((ax * bx + ay * by) / lengths)
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees();
        if !(35.0..=145.0).contains(&angle) {
            return None;
        }
    }
    Some(sorted)
}

fn is_border(from: (f32, f32), to: (f32, f32), width: f32, height: f32) -> bool {
    let near = |a: f32, b: f32| (a - b).abs() < 1.5;
    (near(from.0, 0.0) && near(to.0, 0.0))
        || (near(from.1, 0.0) && near(to.1, 0.0))
        || (near(from.0, width - 1.0) && near(to.0, width - 1.0))
        || (near(from.1, height - 1.0) && near(to.1, height - 1.0))
}

/// The share of a side along which an edge runs the same way, within a
/// pixel of it.
fn support(edges: &Edges, from: (f32, f32), to: (f32, f32)) -> f32 {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx.hypot(dy);
    if length < 1.0 {
        return 0.0;
    }
    let normal = dx.atan2(-dy).rem_euclid(std::f32::consts::PI);
    let steps = length.ceil() as usize;
    let mut hits = 0;
    let mut inside = 0;
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let x = (from.0 + dx * t).round() as isize;
        let y = (from.1 + dy * t).round() as isize;
        if x < 0 || y < 0 || x >= edges.width as isize || y >= edges.height as isize {
            continue;
        }
        inside += 1;
        'near: for oy in -1..=1 {
            for ox in -1..=1 {
                let (nx, ny) = (x + ox, y + oy);
                if nx < 0 || ny < 0 || nx >= edges.width as isize || ny >= edges.height as isize {
                    continue;
                }
                let at = ny as usize * edges.width + nx as usize;
                if edges.strength[at] > 0.0 {
                    let turn = (edges.normal[at] - normal).rem_euclid(std::f32::consts::PI);
                    if turn.min(std::f32::consts::PI - turn) < 20f32.to_radians() {
                        hits += 1;
                        break 'near;
                    }
                }
            }
        }
    }
    // A stretch of side outside the photo can hold no edge; judge the rest.
    if inside < steps / 2 {
        return 0.0;
    }
    hits as f32 / inside as f32
}

/// Moves each side onto a least-squares fit of the edge pixels within two
/// pixels of it, which places it far finer than the line search's grid.
fn refine(edges: &Edges, quad: [(f32, f32); 4]) -> [(f32, f32); 4] {
    let mut fitted = [None; 4];
    for (side, fit) in fitted.iter_mut().enumerate() {
        let from = quad[side];
        let to = quad[(side + 1) % 4];
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = dx.hypot(dy);
        if length < 8.0 {
            continue;
        }
        let (ux, uy) = (dx / length, dy / length);
        let normal = dx.atan2(-dy).rem_euclid(std::f32::consts::PI);
        let mut points = Vec::new();
        let min_x = from.0.min(to.0).floor().max(0.0) as usize;
        let max_x = (from.0.max(to.0).ceil() as usize + 3).min(edges.width);
        let min_y = from.1.min(to.1).floor().max(0.0) as usize;
        let max_y = (from.1.max(to.1).ceil() as usize + 3).min(edges.height);
        for y in min_y.saturating_sub(3)..max_y {
            for x in min_x.saturating_sub(3)..max_x {
                let at = y * edges.width + x;
                if edges.strength[at] <= 0.0 {
                    continue;
                }
                let (px, py) = (x as f32 - from.0, y as f32 - from.1);
                let along = px * ux + py * uy;
                let across = px * uy - py * ux;
                if across.abs() > 2.0 || along < length * 0.05 || along > length * 0.95 {
                    continue;
                }
                let turn = (edges.normal[at] - normal).rem_euclid(std::f32::consts::PI);
                if turn.min(std::f32::consts::PI - turn) < 15f32.to_radians() {
                    points.push((x as f32, y as f32, edges.strength[at]));
                }
            }
        }
        if (points.len() as f32) < length * 0.3 {
            continue;
        }
        // Total least squares: the line through the weighted centre along
        // the points' main direction.
        let weight: f32 = points.iter().map(|p| p.2).sum();
        let mx = points.iter().map(|p| p.0 * p.2).sum::<f32>() / weight;
        let my = points.iter().map(|p| p.1 * p.2).sum::<f32>() / weight;
        let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
        for (x, y, w) in &points {
            sxx += w * (x - mx) * (x - mx);
            sxy += w * (x - mx) * (y - my);
            syy += w * (y - my) * (y - my);
        }
        let direction = 0.5 * (2.0 * sxy).atan2(sxx - syy);
        let theta = direction + std::f32::consts::FRAC_PI_2;
        let line = Line {
            theta,
            rho: mx * theta.cos() + my * theta.sin(),
        };
        // Only a nudge: a fit that turns the side noticeably found something else.
        let original_theta = dx.atan2(-dy);
        let turn = (theta - original_theta).rem_euclid(std::f32::consts::PI);
        if turn.min(std::f32::consts::PI - turn) < 3f32.to_radians() {
            *fit = Some(line);
        }
    }
    let mut refined = quad;
    for corner in 0..4 {
        // Corner `corner` is where side `corner - 1` meets side `corner`.
        let before = fitted[(corner + 3) % 4];
        let after = fitted[corner];
        let side_line = |side: usize| {
            let from = quad[side];
            let to = quad[(side + 1) % 4];
            let theta = (to.0 - from.0).atan2(-(to.1 - from.1));
            Line {
                theta,
                rho: from.0 * theta.cos() + from.1 * theta.sin(),
            }
        };
        if before.is_none() && after.is_none() {
            continue;
        }
        let a = before.unwrap_or_else(|| side_line((corner + 3) % 4));
        let b = after.unwrap_or_else(|| side_line(corner));
        if let Some(point) = intersect(a, b)
            && (point.0 - quad[corner].0).hypot(point.1 - quad[corner].1) < 6.0
        {
            refined[corner] = point;
        }
    }
    refined
}

fn polygon_area(quad: &[(f32, f32); 4]) -> f32 {
    let mut twice = 0.0;
    for index in 0..4 {
        let (x0, y0) = quad[index];
        let (x1, y1) = quad[(index + 1) % 4];
        twice += x0 * y1 - x1 * y0;
    }
    twice.abs() / 2.0
}

/// The straightened page's size: its proportions recovered from the
/// perspective, its scale the most the photo resolves, at most `max_side`.
fn output_size(
    quad: &[(f32, f32); 4],
    width: usize,
    height: usize,
    max_side: usize,
) -> (usize, usize) {
    let distance = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).hypot(a.1 - b.1);
    let across = distance(quad[0], quad[1]).max(distance(quad[3], quad[2]));
    let down = distance(quad[0], quad[3]).max(distance(quad[1], quad[2]));
    let seen = across / down;
    let ratio = match true_aspect(quad, width as f32, height as f32) {
        Some(ratio) if (0.5..=2.0).contains(&(ratio / seen)) => ratio,
        _ => seen,
    };
    // Tall enough that neither side is drawn smaller than the photo shows it.
    let mut out_height = down.max(across / ratio);
    let mut out_width = out_height * ratio;
    let longest = out_width.max(out_height);
    if longest > max_side as f32 {
        let scale = max_side as f32 / longest;
        out_width *= scale;
        out_height *= scale;
    }
    (
        (out_width.round() as usize).max(1),
        (out_height.round() as usize).max(1),
    )
}

/// The rectangle's width over its height before the camera's perspective,
/// assuming square pixels and a lens centred on the photo (Zhang and He,
/// "Whiteboard scanning and image enhancement", 2007). `None` when the
/// corners do not determine it.
fn true_aspect(quad: &[(f32, f32); 4], width: f32, height: f32) -> Option<f32> {
    let (cx, cy) = (width / 2.0, height / 2.0);
    let point = |p: (f32, f32)| [f64::from(p.0 - cx), f64::from(p.1 - cy), 1.0];
    let (m1, m2, m3, m4) = (
        point(quad[0]),
        point(quad[1]),
        point(quad[3]),
        point(quad[2]),
    );
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let k2 = dot(cross(m1, m4), m3) / dot(cross(m2, m4), m3);
    let k3 = dot(cross(m1, m4), m2) / dot(cross(m3, m4), m2);
    if !k2.is_finite() || !k3.is_finite() {
        return None;
    }
    let n2 = [k2 * m2[0] - m1[0], k2 * m2[1] - m1[1], k2 * m2[2] - m1[2]];
    let n3 = [k3 * m3[0] - m1[0], k3 * m3[1] - m1[1], k3 * m3[2] - m1[2]];
    let planar = |n: [f64; 3]| n[0] * n[0] + n[1] * n[1];
    let diagonal = f64::from(width.hypot(height));
    // Opposite sides that stay parallel carry no depth: the view is head on.
    if n2[2].abs() < 1e-6 || n3[2].abs() < 1e-6 || (n2[2] * n3[2]).abs() < 1e-9 {
        return Some((planar(n2) / planar(n3)).sqrt() as f32);
    }
    let focal_squared = -(n2[0] * n3[0] + n2[1] * n3[1]) / (n2[2] * n3[2]);
    let focal = focal_squared.sqrt();
    // A phone's lens sees about as far as the photo is wide; anything far
    // from that means the assumptions do not hold for this photo.
    if !focal.is_finite() || focal < 0.25 * diagonal || focal > 25.0 * diagonal {
        return None;
    }
    let ratio = ((planar(n2) / focal_squared + n2[2] * n2[2])
        / (planar(n3) / focal_squared + n3[2] * n3[2]))
        .sqrt();
    (ratio.is_finite() && ratio > 0.0).then_some(ratio as f32)
}

/// RGB pixels of the area inside `quad`, mapped onto an upright rectangle
/// with bilinear sampling. What falls outside the photo is white.
fn warp(
    rgba: &[u8],
    width: usize,
    height: usize,
    quad: &[(f32, f32); 4],
    out_width: usize,
    out_height: usize,
) -> Vec<u8> {
    let [(x0, y0), (x1, y1), (x2, y2), (x3, y3)] = quad.map(|(x, y)| (f64::from(x), f64::from(y)));
    // Heckbert's square-to-quadrilateral mapping, unit square to `quad`.
    let sx = x0 - x1 + x2 - x3;
    let sy = y0 - y1 + y2 - y3;
    let (a, b, c, d, e, f, g, h);
    if sx.abs() < 1e-9 && sy.abs() < 1e-9 {
        (a, b, c) = (x1 - x0, x2 - x1, x0);
        (d, e, f) = (y1 - y0, y2 - y1, y0);
        (g, h) = (0.0, 0.0);
    } else {
        let (dx1, dx2, dy1, dy2) = (x1 - x2, x3 - x2, y1 - y2, y3 - y2);
        let denominator = dx1 * dy2 - dx2 * dy1;
        g = (sx * dy2 - dx2 * sy) / denominator;
        h = (dx1 * sy - sx * dy1) / denominator;
        (a, b, c) = (x1 - x0 + g * x1, x3 - x0 + h * x3, x0);
        (d, e, f) = (y1 - y0 + g * y1, y3 - y0 + h * y3, y0);
    }
    let mut out = vec![255u8; out_width * out_height * 3];
    let max_x = width as f64 - 1.0;
    let max_y = height as f64 - 1.0;
    for oy in 0..out_height {
        let v = (oy as f64 + 0.5) / out_height as f64;
        for ox in 0..out_width {
            let u = (ox as f64 + 0.5) / out_width as f64;
            let w = g * u + h * v + 1.0;
            let x = (a * u + b * v + c) / w - 0.5;
            let y = (d * u + e * v + f) / w - 0.5;
            if !(-0.5..=max_x + 0.5).contains(&x) || !(-0.5..=max_y + 0.5).contains(&y) {
                continue;
            }
            let x = x.clamp(0.0, max_x);
            let y = y.clamp(0.0, max_y);
            let (left, top) = (x.floor() as usize, y.floor() as usize);
            let (right, bottom) = ((left + 1).min(width - 1), (top + 1).min(height - 1));
            let (fx, fy) = ((x - left as f64) as f32, (y - top as f64) as f32);
            let at = (oy * out_width + ox) * 3;
            for channel in 0..3 {
                let p = |px: usize, py: usize| f32::from(rgba[(py * width + px) * 4 + channel]);
                let upper = p(left, top) + (p(right, top) - p(left, top)) * fx;
                let lower = p(left, bottom) + (p(right, bottom) - p(left, bottom)) * fx;
                out[at + channel] = (upper + (lower - upper) * fy).round() as u8;
            }
        }
    }
    out
}

fn luminance(r: f32, g: f32, b: f32) -> f32 {
    0.299 * r + 0.587 * g + 0.114 * b
}

/// The paper's colour under each part of the page: the brightest third of
/// each cell, spread over neighbours so text does not darken it, with cells
/// covered by a picture filled in from around them. Returned per pixel,
/// interpolated between cell centres.
fn paper_color(rgb: &[u8], width: usize, height: usize) -> Vec<[f32; 3]> {
    let cell = (width.max(height) / 48).max(8);
    let columns = width.div_ceil(cell);
    let rows = height.div_ceil(cell);
    let mut cells = vec![[0.0f32; 3]; columns * rows];
    for row in 0..rows {
        for column in 0..columns {
            let (x0, y0) = (column * cell, row * cell);
            let (x1, y1) = ((x0 + cell).min(width), (y0 + cell).min(height));
            let mut histogram = [0u32; 64];
            for y in y0..y1 {
                for x in x0..x1 {
                    let at = (y * width + x) * 3;
                    let lum = luminance(
                        f32::from(rgb[at]),
                        f32::from(rgb[at + 1]),
                        f32::from(rgb[at + 2]),
                    );
                    histogram[(lum as usize / 4).min(63)] += 1;
                }
            }
            let total: u32 = histogram.iter().sum();
            let mut above = 0;
            let mut threshold = 0;
            for bin in (0..64).rev() {
                above += histogram[bin];
                if above * 3 >= total {
                    threshold = bin;
                    break;
                }
            }
            let mut sum = [0.0f32; 3];
            let mut count = 0.0f32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let at = (y * width + x) * 3;
                    let (r, g, b) = (
                        f32::from(rgb[at]),
                        f32::from(rgb[at + 1]),
                        f32::from(rgb[at + 2]),
                    );
                    if (luminance(r, g, b) as usize / 4).min(63) >= threshold {
                        sum[0] += r;
                        sum[1] += g;
                        sum[2] += b;
                        count += 1.0;
                    }
                }
            }
            cells[row * columns + column] = sum.map(|value| value / count.max(1.0));
        }
    }
    let lum = |c: [f32; 3]| luminance(c[0], c[1], c[2]);
    // Each cell takes its brightest neighbour, so a cell full of text reads
    // as the paper around it.
    let spread: Vec<[f32; 3]> = (0..cells.len())
        .map(|index| {
            let (row, column) = ((index / columns) as isize, (index % columns) as isize);
            let mut brightest = cells[index];
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (r, c) = (row + dy, column + dx);
                    if r < 0 || c < 0 || r >= rows as isize || c >= columns as isize {
                        continue;
                    }
                    let other = cells[r as usize * columns + c as usize];
                    if lum(other) > lum(brightest) {
                        brightest = other;
                    }
                }
            }
            brightest
        })
        .collect();
    // Cells far darker than the page's paper are a picture, not shade; they
    // take the paper around them instead.
    let mut levels: Vec<f32> = spread.iter().map(|c| lum(*c)).collect();
    let middle = levels.len() / 2;
    let (_, median, _) = levels.select_nth_unstable_by(middle, f32::total_cmp);
    let median = *median;
    let mut known: Vec<bool> = spread.iter().map(|c| lum(*c) >= median * 0.55).collect();
    let mut filled = spread;
    if known.iter().all(|k| !k) {
        known.iter_mut().for_each(|k| *k = true);
    }
    while known.iter().any(|k| !k) {
        let mut next_known = known.clone();
        let mut next = filled.clone();
        for index in 0..filled.len() {
            if known[index] {
                continue;
            }
            let (row, column) = ((index / columns) as isize, (index % columns) as isize);
            let mut sum = [0.0f32; 3];
            let mut count = 0.0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (r, c) = (row + dy, column + dx);
                    if r < 0 || c < 0 || r >= rows as isize || c >= columns as isize {
                        continue;
                    }
                    let other = r as usize * columns + c as usize;
                    if known[other] {
                        for channel in 0..3 {
                            sum[channel] += filled[other][channel];
                        }
                        count += 1.0;
                    }
                }
            }
            if count > 0.0 {
                next[index] = sum.map(|value| value / count);
                next_known[index] = true;
            }
        }
        filled = next;
        known = next_known;
    }
    // Two passes of a 3x3 average take the steps out between cells.
    for _ in 0..2 {
        let source = filled.clone();
        for (index, cell) in filled.iter_mut().enumerate() {
            let (row, column) = ((index / columns) as isize, (index % columns) as isize);
            let mut sum = [0.0f32; 3];
            let mut count = 0.0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (r, c) = (row + dy, column + dx);
                    if r < 0 || c < 0 || r >= rows as isize || c >= columns as isize {
                        continue;
                    }
                    for channel in 0..3 {
                        sum[channel] += source[r as usize * columns + c as usize][channel];
                    }
                    count += 1.0;
                }
            }
            *cell = sum.map(|value| value / count);
        }
    }
    let mut paper = vec![[0.0f32; 3]; width * height];
    for y in 0..height {
        let gy = ((y as f32 + 0.5) / cell as f32 - 0.5).clamp(0.0, (rows - 1) as f32);
        let (r0, fy) = (gy.floor() as usize, gy.fract());
        let r1 = (r0 + 1).min(rows - 1);
        for x in 0..width {
            let gx = ((x as f32 + 0.5) / cell as f32 - 0.5).clamp(0.0, (columns - 1) as f32);
            let (c0, fx) = (gx.floor() as usize, gx.fract());
            let c1 = (c0 + 1).min(columns - 1);
            let mut color = [0.0f32; 3];
            for (channel, value) in color.iter_mut().enumerate() {
                let top = filled[r0 * columns + c0][channel] * (1.0 - fx)
                    + filled[r0 * columns + c1][channel] * fx;
                let bottom = filled[r1 * columns + c0][channel] * (1.0 - fx)
                    + filled[r1 * columns + c1][channel] * fx;
                *value = (top + (bottom - top) * fy).max(16.0);
            }
            paper[y * width + x] = color;
        }
    }
    paper
}

/// Paper this bright, as a share of the paper estimate, becomes white.
const PAPER_WHITE: f32 = 0.9;

fn encode(rgb: Vec<u8>, width: usize, height: usize, look: ScanLook) -> Result<Vec<u8>, String> {
    let too_large = || "The page is too large to scan.".to_string();
    let w16 = u16::try_from(width).map_err(|_| too_large())?;
    let h16 = u16::try_from(height).map_err(|_| too_large())?;
    let jpeg = |pixels: &[u8], colors| {
        encode_jpeg(pixels, w16, h16, colors, 85)
            .ok_or_else(|| "The page could not be encoded.".to_string())
    };
    if look == ScanLook::Original {
        return jpeg(&rgb, PixelColors::Rgb);
    }
    let paper = paper_color(&rgb, width, height);
    // Each pixel over its paper: 1 is clean paper, ink is well below.
    let lifted: Vec<[f32; 3]> = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .zip(&paper)
        .map(|(pixel, paper)| {
            [
                f32::from(pixel[0]) / paper[0],
                f32::from(pixel[1]) / paper[1],
                f32::from(pixel[2]) / paper[2],
            ]
        })
        .collect();
    match look {
        ScanLook::BlackWhite => {
            let shade: Vec<f32> = lifted.iter().map(|c| luminance(c[0], c[1], c[2])).collect();
            let threshold = otsu(&shade).clamp(0.55, 0.85);
            let row_bytes = width.div_ceil(8);
            let mut bits = vec![0xFFu8; row_bytes * height];
            for y in 0..height {
                for x in 0..width {
                    if shade[y * width + x] >= threshold {
                        continue;
                    }
                    // A lone dark pixel is grain, not ink.
                    let mut neighbours = 0;
                    for dy in -1isize..=1 {
                        for dx in -1isize..=1 {
                            let (nx, ny) = (x as isize + dx, y as isize + dy);
                            if (dx, dy) != (0, 0)
                                && nx >= 0
                                && ny >= 0
                                && nx < width as isize
                                && ny < height as isize
                                && shade[ny as usize * width + nx as usize] < threshold
                            {
                                neighbours += 1;
                            }
                        }
                    }
                    if neighbours > 0 {
                        bits[y * row_bytes + x / 8] &= !(0x80 >> (x % 8));
                    }
                }
            }
            let mut encoded = Vec::new();
            let mut encoder = png::Encoder::new(&mut encoded, width as u32, height as u32);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::One);
            encoder.set_compression(png::Compression::High);
            encoder.set_filter(png::Filter::NoFilter);
            let mut writer = encoder.write_header().map_err(|_| too_large())?;
            writer.write_image_data(&bits).map_err(|_| too_large())?;
            writer.finish().map_err(|_| too_large())?;
            Ok(encoded)
        }
        ScanLook::Grayscale | ScanLook::Document => {
            let mut shades: Vec<f32> = lifted.iter().map(|c| luminance(c[0], c[1], c[2])).collect();
            // The darkest ink becomes black, unless the page has none.
            let at = shades.len() / 100;
            let (_, ink, _) = shades.select_nth_unstable_by(at, f32::total_cmp);
            let black = ink.clamp(0.0, 0.35);
            let level = |value: f32| {
                let stretched = ((value - black) / (PAPER_WHITE - black)).clamp(0.0, 1.0);
                // Slightly darker mid-tones keep thin strokes from washing out.
                (stretched.powf(1.15) * 255.0).round() as u8
            };
            if look == ScanLook::Grayscale {
                let gray: Vec<u8> = lifted
                    .iter()
                    .map(|c| level(luminance(c[0], c[1], c[2])))
                    .collect();
                return jpeg(&gray, PixelColors::Gray);
            }
            let mut out = Vec::with_capacity(lifted.len() * 3);
            for color in &lifted {
                let gray = luminance(color[0], color[1], color[2]);
                for channel in color {
                    // A little more colour, which flat light takes out of ink.
                    out.push(level(gray + (channel - gray) * 1.2));
                }
            }
            jpeg(&out, PixelColors::Rgb)
        }
        ScanLook::Original => unreachable!(),
    }
}

/// Otsu's threshold over values from 0 to about 1.
fn otsu(values: &[f32]) -> f32 {
    const BINS: usize = 128;
    let mut histogram = [0u32; BINS];
    for value in values {
        histogram[((value / 1.2).clamp(0.0, 1.0) * (BINS - 1) as f32) as usize] += 1;
    }
    let total = values.len() as f64;
    let sum: f64 = histogram
        .iter()
        .enumerate()
        .map(|(bin, count)| bin as f64 * f64::from(*count))
        .sum();
    let (mut weight_below, mut sum_below) = (0.0, 0.0);
    let (mut best, mut best_between) = (BINS / 2, 0.0);
    for (bin, count) in histogram.iter().enumerate() {
        weight_below += f64::from(*count);
        if weight_below == 0.0 {
            continue;
        }
        let weight_above = total - weight_below;
        if weight_above == 0.0 {
            break;
        }
        sum_below += bin as f64 * f64::from(*count);
        let mean_below = sum_below / weight_below;
        let mean_above = (sum - sum_below) / weight_above;
        let between = weight_below * weight_above * (mean_below - mean_above).powi(2);
        if between > best_between {
            best_between = between;
            best = bin;
        }
    }
    (best as f32 + 0.5) / (BINS - 1) as f32 * 1.2
}
