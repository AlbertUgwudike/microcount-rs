//! Convex hulls of pixel sets and the pixel area of polygons.

/// Andrew's monotone chain. Returns the hull counter-clockwise without collinear points;
/// fewer than three vertices means the points are collinear (MATLAB `convhull` errors).
pub fn convex_hull(pts: &mut [(i64, i64)]) -> Vec<(i64, i64)> {
    pts.sort_unstable();
    let cross = |o: (i64, i64), a: (i64, i64), b: (i64, i64)| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut hull: Vec<(i64, i64)> = Vec::with_capacity(2 * pts.len());
    let mut last = None;
    for &p in pts.iter() {
        if last == Some(p) {
            continue;
        }
        last = Some(p);
        while hull.len() >= 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0 {
            hull.pop();
        }
        hull.push(p);
    }
    let lower = hull.len() + 1;
    let mut last = None;
    for &p in pts.iter().rev() {
        if last == Some(p) {
            continue;
        }
        last = Some(p);
        while hull.len() >= lower && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0 {
            hull.pop();
        }
        hull.push(p);
    }
    hull.pop();
    hull
}

fn ceil_div(n: i64, d: i64) -> i64 {
    let (n, d) = if d < 0 { (-n, -d) } else { (n, d) };
    n.div_euclid(d) + if n.rem_euclid(d) != 0 { 1 } else { 0 }
}

/// Number of pixel centres covered by a convex polygon with integer vertices, using a
/// half-open rule in both axes so the count tracks the polygon area the way
/// `poly2mask` does (an axis-aligned n×m box covers n·m pixels).
pub fn polygon_pixel_count(hull: &[(i64, i64)]) -> u64 {
    if hull.len() < 3 {
        return 0;
    }
    let xmin = hull.iter().map(|p| p.0).min().unwrap();
    let xmax = hull.iter().map(|p| p.0).max().unwrap();
    let n = hull.len();
    let mut total = 0u64;
    for x in xmin..xmax {
        let mut lo = i64::MAX;
        let mut hi = i64::MIN;
        for i in 0..n {
            let (x1, y1) = hull[i];
            let (x2, y2) = hull[(i + 1) % n];
            if x1 == x2 || x < x1.min(x2) || x > x1.max(x2) {
                continue;
            }
            let dx = x2 - x1;
            let c = ceil_div(y1 * dx + (x - x1) * (y2 - y1), dx);
            lo = lo.min(c);
            hi = hi.max(c);
        }
        if hi > lo {
            total += (hi - lo) as u64;
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_hull_and_count() {
        let mut pts = vec![];
        for x in 0..4 {
            for y in 0..3 {
                pts.push((x, y));
            }
        }
        let hull = convex_hull(&mut pts);
        assert_eq!(hull.len(), 4);
        assert_eq!(polygon_pixel_count(&hull), 6);
    }

    #[test]
    fn collinear_points_have_no_hull() {
        let mut pts = vec![(0, 0), (1, 1), (2, 2), (2, 2)];
        assert!(convex_hull(&mut pts).len() < 3);
    }
}
