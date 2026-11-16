/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use std::ops::{Mul, RangeBounds, RangeInclusive};

pub fn intersect(a: RangeInclusive<f32>, b: RangeInclusive<f32>) -> Option<RangeInclusive<f32>> {
    use std::ops::Bound::*;

    let start = a.start().max(*b.start());
    let end = a.end().min(*b.end());

    let output = start..=end;

    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}

pub trait RangeExt {
    /// Computes the numeric span (width) of a range.
    ///
    /// May return [f32::INFINITY] if the range is unbounded, and may return negative numbers when `start > end`.
    fn span(&self) -> f32;

    /// Computes the middle element of the range (average of start and end bound).
    ///
    /// For one-sided bounds, returns +-infinity; for unbounded range and empty ranges returns NaN.
    fn middle(&self) -> f32;
}

impl<T: RangeBounds<f32>> RangeExt for T {
    fn span(&self) -> f32 {
        use std::ops::Bound::*;

        match (self.start_bound(), self.end_bound()) {
            (Unbounded, _) | (_, Unbounded) => f32::INFINITY,
            (Included(x) | Excluded(x), Included(y) | Excluded(y)) => y - x,
        }
    }

    fn middle(&self) -> f32 {
        use std::ops::Bound::*;

        match (self.start_bound(), self.end_bound()) {
            (Unbounded, Unbounded) => f32::NAN,
            (Unbounded, _) => f32::NEG_INFINITY,
            (_, Unbounded) => f32::INFINITY,
            (Included(x) | Excluded(x), Included(y) | Excluded(y)) => (x + y) / 2.0,
        }
    }
}

pub trait SquaredExt: Sized + Copy + Mul<Self, Output = Self> {
    #[inline(always)]
    fn squared(self) -> Self {
        self * self
    }
}

impl SquaredExt for u8 {}
impl SquaredExt for i8 {}
impl SquaredExt for u16 {}
impl SquaredExt for i16 {}
impl SquaredExt for u32 {}
impl SquaredExt for i32 {}
impl SquaredExt for u64 {}
impl SquaredExt for i64 {}
impl SquaredExt for usize {}
impl SquaredExt for isize {}
impl SquaredExt for f32 {}
impl SquaredExt for f64 {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intersect() {
        assert_eq!(intersect(0.0..=10.0, 5.0..=15.0), Some(5.0..=10.0));
    }
}
