//! A minimal row-major 2-D image container shared by every processing stage.
//!
//! Coordinates are 0-based `(x, y)` = `(column, row)` throughout the crate. Where the
//! MATLAB original depended on column-major ordering (component labels, tie breaks) the
//! relevant functions reproduce that ordering explicitly.

use rayon::prelude::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Image<T> {
    pub w: usize,
    pub h: usize,
    pub data: Vec<T>,
}

pub type Mask = Image<bool>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

impl Rect {
    pub fn new(x: usize, y: usize, w: usize, h: usize) -> Self {
        Self { x, y, w, h }
    }

    pub fn x1(&self) -> usize {
        self.x + self.w
    }

    pub fn y1(&self) -> usize {
        self.y + self.h
    }

    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    pub fn area(&self) -> usize {
        self.w * self.h
    }

    pub fn intersect(&self, o: &Rect) -> Rect {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = self.x1().min(o.x1());
        let y1 = self.y1().min(o.y1());
        if x1 <= x0 || y1 <= y0 {
            Rect::new(x0, y0, 0, 0)
        } else {
            Rect::new(x0, y0, x1 - x0, y1 - y0)
        }
    }

    pub fn translate(&self, dx: usize, dy: usize) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

impl<T: Copy + Send + Sync> Image<T> {
    pub fn filled(w: usize, h: usize, v: T) -> Self {
        Self { w, h, data: vec![v; w * h] }
    }

    pub fn from_vec(w: usize, h: usize, data: Vec<T>) -> Self {
        assert_eq!(data.len(), w * h, "image buffer does not match {w}x{h}");
        Self { w, h, data }
    }

    #[inline]
    pub fn idx(&self, x: usize, y: usize) -> usize {
        y * self.w + x
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> T {
        self.data[y * self.w + x]
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, v: T) {
        let i = y * self.w + x;
        self.data[i] = v;
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn row(&self, y: usize) -> &[T] {
        &self.data[y * self.w..(y + 1) * self.w]
    }

    pub fn rect(&self) -> Rect {
        Rect::new(0, 0, self.w, self.h)
    }

    pub fn map<U: Copy + Send + Sync>(&self, f: impl Fn(T) -> U + Sync) -> Image<U> {
        Image {
            w: self.w,
            h: self.h,
            data: self.data.par_iter().map(|&v| f(v)).collect(),
        }
    }

    pub fn zip_map<U: Copy + Send + Sync, V: Copy + Send + Sync>(
        &self,
        other: &Image<U>,
        f: impl Fn(T, U) -> V + Sync,
    ) -> Image<V> {
        assert_eq!((self.w, self.h), (other.w, other.h));
        Image {
            w: self.w,
            h: self.h,
            data: self
                .data
                .par_iter()
                .zip(other.data.par_iter())
                .map(|(&a, &b)| f(a, b))
                .collect(),
        }
    }

    pub fn crop(&self, r: Rect) -> Image<T> {
        assert!(r.x1() <= self.w && r.y1() <= self.h, "crop outside image");
        let mut data = Vec::with_capacity(r.area());
        for y in r.y..r.y1() {
            data.extend_from_slice(&self.data[y * self.w + r.x..y * self.w + r.x1()]);
        }
        Image { w: r.w, h: r.h, data }
    }

    /// Pads `n` pixels of `v` on every side (MATLAB `padarray(img, [n n], v)`).
    pub fn pad(&self, n: usize, v: T) -> Image<T> {
        let w = self.w + 2 * n;
        let h = self.h + 2 * n;
        let mut out = Image::filled(w, h, v);
        for y in 0..self.h {
            let dst = (y + n) * w + n;
            out.data[dst..dst + self.w].copy_from_slice(self.row(y));
        }
        out
    }

    /// Removes `n` pixels from every side (`isocrop`).
    pub fn unpad(&self, n: usize) -> Image<T> {
        if self.w <= 2 * n || self.h <= 2 * n {
            return Image { w: 0, h: 0, data: vec![] };
        }
        self.crop(Rect::new(n, n, self.w - 2 * n, self.h - 2 * n))
    }

    /// Rotates by `quarter_turns` × 90° counter-clockwise (MATLAB `imrotate` with
    /// multiples of 90°, which is exact).
    pub fn rotate_ccw(&self, quarter_turns: i32) -> Image<T> {
        let q = quarter_turns.rem_euclid(4);
        let (w, h) = (self.w, self.h);
        match q {
            0 => self.clone(),
            1 => {
                // B(i, j) = A(j, w-1-i): rows of B = w, cols of B = h.
                let mut data = Vec::with_capacity(w * h);
                for i in 0..w {
                    for j in 0..h {
                        data.push(self.get(w - 1 - i, j));
                    }
                }
                Image { w: h, h: w, data }
            }
            2 => {
                let mut data = self.data.clone();
                data.reverse();
                Image { w, h, data }
            }
            _ => {
                // B(i, j) = A(h-1-j, i)
                let mut data = Vec::with_capacity(w * h);
                for i in 0..w {
                    for j in 0..h {
                        data.push(self.get(i, h - 1 - j));
                    }
                }
                Image { w: h, h: w, data }
            }
        }
    }
}

impl Mask {
    pub fn count(&self) -> u64 {
        self.data.par_iter().filter(|&&b| b).count() as u64
    }

    /// Tight bounding box of the set pixels (`bounding_box.m`).
    pub fn bbox(&self) -> Option<Rect> {
        let rows: Vec<Option<(usize, usize)>> = self
            .data
            .par_chunks(self.w.max(1))
            .map(|row| {
                let first = row.iter().position(|&b| b)?;
                let last = row.iter().rposition(|&b| b)?;
                Some((first, last))
            })
            .collect();
        let mut x0 = usize::MAX;
        let mut x1 = 0;
        let mut y0 = usize::MAX;
        let mut y1 = 0;
        for (y, r) in rows.iter().enumerate() {
            if let Some((a, b)) = r {
                x0 = x0.min(*a);
                x1 = x1.max(*b);
                y0 = y0.min(y);
                y1 = y;
            }
        }
        (y0 != usize::MAX).then(|| Rect::new(x0, y0, x1 - x0 + 1, y1 - y0 + 1))
    }

    pub fn and(&self, o: &Mask) -> Mask {
        self.zip_map(o, |a, b| a && b)
    }

    pub fn or(&self, o: &Mask) -> Mask {
        self.zip_map(o, |a, b| a || b)
    }

    pub fn and_not(&self, o: &Mask) -> Mask {
        self.zip_map(o, |a, b| a && !b)
    }
}

/// Sets every pixel outside `mask` to `v` in place.
pub fn apply_mask<T: Copy + Send + Sync>(img: &mut Image<T>, mask: &Mask, v: T) {
    img.data
        .par_iter_mut()
        .zip(mask.data.par_iter())
        .for_each(|(p, &m)| {
            if !m {
                *p = v
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations_match_matlab_rot90() {
        // A = [1 2 3; 4 5 6]; rot90(A) = [3 6; 2 5; 1 4]
        let a = Image::from_vec(3, 2, vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(a.rotate_ccw(1).data, vec![3, 6, 2, 5, 1, 4]);
        // rot90(A, -1) = [4 1; 5 2; 6 3]
        assert_eq!(a.rotate_ccw(-1).data, vec![4, 1, 5, 2, 6, 3]);
        assert_eq!(a.rotate_ccw(1).rotate_ccw(-1), a);
        assert_eq!(a.rotate_ccw(2).data, vec![6, 5, 4, 3, 2, 1]);
    }

    #[test]
    fn bbox_and_pad() {
        let mut m = Mask::filled(5, 4, false);
        m.set(1, 2, true);
        m.set(3, 1, true);
        assert_eq!(m.bbox(), Some(Rect::new(1, 1, 3, 2)));
        assert_eq!(m.pad(2, false).unpad(2), m);
    }
}
