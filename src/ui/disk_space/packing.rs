//! Circle packing for the bubble view: the front-chain method from Wang et
//! al.'s "Visualization of large hierarchical data by circle packing", the
//! same one d3's `packSiblings` uses. Circles are placed largest first, each
//! touching two circles on the outside edge of the pile, so it grows outward
//! in a tight, roughly round cluster.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Circle {
    pub x: f64,
    pub y: f64,
    pub r: f64,
}

/// Places circles with these radii without overlaps, in the same order.
/// Pass them largest first for the tightest result.
pub fn pack(radii: &[f64]) -> Vec<Circle> {
    let mut circles: Vec<Circle> = radii.iter().map(|&r| Circle { r, ..Circle::default() }).collect();
    let n = circles.len();
    if n < 2 {
        return circles;
    }
    circles[0].x = -circles[1].r;
    circles[1].x = circles[0].r;
    if n == 2 {
        return circles;
    }
    circles[2] = place(circles[1], circles[0], circles[2]);

    // The front chain: a ring of the circles on the outside of the pile,
    // as next and previous links by index.
    let mut next = vec![0; n];
    let mut prev = vec![0; n];
    (next[0], next[1], next[2]) = (1, 2, 0);
    (prev[0], prev[1], prev[2]) = (2, 0, 1);
    let (mut a, mut b) = (0, 1);

    let mut i = 3;
    'pack: while i < n {
        circles[i] = place(circles[a], circles[b], circles[i]);

        // Look for the nearest circle on the chain that the new one overlaps,
        // walking both ways and measuring distance along the chain.
        let (mut j, mut k) = (next[b], prev[a]);
        let (mut along_j, mut along_k) = (circles[b].r, circles[a].r);
        loop {
            if along_j <= along_k {
                if intersects(circles[j], circles[i]) {
                    b = j;
                    next[a] = b;
                    prev[b] = a;
                    continue 'pack;
                }
                along_j += circles[j].r;
                j = next[j];
            } else {
                if intersects(circles[k], circles[i]) {
                    a = k;
                    next[a] = b;
                    prev[b] = a;
                    continue 'pack;
                }
                along_k += circles[k].r;
                k = prev[k];
            }
            if j == next[k] {
                break;
            }
        }

        // It fits: link it in between a and b.
        prev[i] = a;
        next[i] = b;
        next[a] = i;
        prev[b] = i;
        b = i;

        // The next circle goes next to the pair closest to the middle.
        let mut best = score(&circles, a, next[a]);
        let mut node = next[b];
        while node != b {
            let candidate = score(&circles, node, next[node]);
            if candidate < best {
                a = node;
                best = candidate;
            }
            node = next[node];
        }
        b = next[a];
        i += 1;
    }
    circles
}

/// Puts `c` touching both `a` and `b`, on the left of the line from `b` to `a`.
fn place(b: Circle, a: Circle, c: Circle) -> Circle {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let d2 = dx * dx + dy * dy;
    if d2 == 0. {
        return Circle { x: a.x + c.r, y: a.y, r: c.r };
    }
    let a2 = (a.r + c.r).powi(2);
    let b2 = (b.r + c.r).powi(2);
    if a2 > b2 {
        let x = (d2 + b2 - a2) / (2. * d2);
        let y = (b2 / d2 - x * x).max(0.).sqrt();
        Circle { x: b.x - x * dx - y * dy, y: b.y - x * dy + y * dx, r: c.r }
    } else {
        let x = (d2 + a2 - b2) / (2. * d2);
        let y = (a2 / d2 - x * x).max(0.).sqrt();
        Circle { x: a.x + x * dx - y * dy, y: a.y + x * dy + y * dx, r: c.r }
    }
}

/// Overlapping by more than a rounding error.
fn intersects(a: Circle, b: Circle) -> bool {
    let reach = a.r + b.r - 1e-6;
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    reach > 0. && reach * reach > dx * dx + dy * dy
}

/// How far the weighted middle of two neighbours is from the origin, squared.
fn score(circles: &[Circle], a: usize, b: usize) -> f64 {
    let (a, b) = (circles[a], circles[b]);
    let ab = a.r + b.r;
    let x = (a.x * b.r + b.x * a.r) / ab;
    let y = (a.y * b.r + b.y * a.r) / ab;
    x * x + y * y
}

/// The box around every circle: `(left, top, right, bottom)`.
pub fn extent(circles: &[Circle]) -> (f64, f64, f64, f64) {
    circles.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |(l, t, r, b), c| (l.min(c.x - c.r), t.min(c.y - c.r), r.max(c.x + c.r), b.max(c.y + c.r)))
}

/// Scales and moves circles so their box sits centered in `width` by `height`.
pub fn fit(circles: &[Circle], width: f64, height: f64) -> Vec<Circle> {
    if circles.is_empty() || width <= 0. || height <= 0. {
        return Vec::new();
    }
    let (left, top, right, bottom) = extent(circles);
    let (w, h) = ((right - left).max(f64::EPSILON), (bottom - top).max(f64::EPSILON));
    let scale = (width / w).min(height / h);
    let (dx, dy) = ((width - w * scale) / 2. - left * scale, (height - h * scale) / 2. - top * scale);
    circles.iter().map(|c| Circle { x: c.x * scale + dx, y: c.y * scale + dy, r: c.r * scale }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_no_overlaps(circles: &[Circle]) {
        for (ix, a) in circles.iter().enumerate() {
            for b in &circles[ix + 1..] {
                let distance = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
                assert!(distance >= a.r + b.r - 1e-6, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn small_counts() {
        assert!(pack(&[]).is_empty());
        assert_eq!(pack(&[3.]), vec![Circle { x: 0., y: 0., r: 3. }]);
        let two = pack(&[3., 2.]);
        assert_eq!((two[0].x, two[1].x), (-2., 3.));
        assert_no_overlaps(&pack(&[3., 2., 1.]));
    }

    #[test]
    fn many_circles_never_overlap_and_stay_tight() {
        // Sizes like a real scan: a few big files and a long tail.
        let radii: Vec<f64> = (1..=300).map(|n| (1e9 / (n as f64).powf(1.3)).sqrt()).collect();
        let circles = pack(&radii);
        assert_eq!(circles.len(), 300);
        assert!(circles.iter().zip(&radii).all(|(c, &r)| c.r == r));
        assert_no_overlaps(&circles);

        // The pile should cover a good share of its own bounding box.
        let (l, t, r, b) = extent(&circles);
        let area: f64 = circles.iter().map(|c| std::f64::consts::PI * c.r * c.r).sum();
        assert!(area / ((r - l) * (b - t)) > 0.5);
    }

    #[test]
    fn equal_circles_pack_without_overlaps() {
        assert_no_overlaps(&pack(&[5.; 120]));
    }

    #[test]
    fn fits_inside_the_box_and_centers() {
        let circles = fit(&pack(&[10., 8., 6., 4., 2.]), 400., 200.);
        let (l, t, r, b) = extent(&circles);
        assert!(l >= -1e-9 && t >= -1e-9 && r <= 400. + 1e-9 && b <= 200. + 1e-9);
        // The tighter side touches both edges.
        assert!((t.abs() < 1e-6 && (b - 200.).abs() < 1e-6) || (l.abs() < 1e-6 && (r - 400.).abs() < 1e-6));
        assert!(fit(&circles, 0., 10.).is_empty());
    }
}
