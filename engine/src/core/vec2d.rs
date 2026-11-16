/*
 * Copyright (c) 2026. DisorderlyTeam
 */

#[derive(Debug)]
pub struct Array2D<T> {
    width: usize,
    height: usize,
    data: Vec<T>,
}

impl<T> Array2D<T> {
    /// Creates a new 2D array with the given dimensions and initialization function.
    pub fn new_init(width: usize, height: usize, mut init: impl FnMut(usize, usize) -> T) -> Self {
        let mut data = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                data.push(init(x, y));
            }
        }
        Self {
            width,
            height,
            data,
        }
    }

    /// Creates a new 2D array with the given dimensions and content.
    pub fn new(width: usize, height: usize, content: T) -> Self
    where
        T: Clone,
    {
        Self::new_init(width, height, |_, _| content.clone())
    }

    /// Creates a new 2D array with the given dimensions and default content.
    pub fn new_default(width: usize, height: usize) -> Self
    where
        T: Default,
    {
        Self::new_init(width, height, |_, _| Default::default())
    }

    fn coordinates_to_index(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    fn index_to_coordinates(&self, i: usize) -> (usize, usize) {
        (i % self.width, i / self.width)
    }

    pub fn is_within_bounds(&self, x: usize, y: usize) -> bool {
        x < self.width && y < self.height
    }

    #[inline]
    fn assert_in_bounds(&self, x: usize, y: usize) {
        assert!(
            self.is_within_bounds(x, y),
            "Coordinates ({}, {}) are out of bounds for array of size {}x{}",
            x,
            y,
            self.width,
            self.height
        );
    }

    pub fn get(&self, x: usize, y: usize) -> &T {
        self.assert_in_bounds(x, y);
        let index = self.coordinates_to_index(x, y);
        &self.data[index]
    }

    pub fn get_mut(&mut self, x: usize, y: usize) -> &mut T {
        self.assert_in_bounds(x, y);
        let index = self.coordinates_to_index(x, y);
        &mut self.data[index]
    }

    pub fn try_get(&self, x: usize, y: usize) -> Option<&T> {
        if self.is_within_bounds(x, y) {
            let index = self.coordinates_to_index(x, y);
            Some(&self.data[index])
        } else {
            None
        }
    }

    pub fn try_get_mut(&mut self, x: usize, y: usize) -> Option<&mut T> {
        if self.is_within_bounds(x, y) {
            let index = self.coordinates_to_index(x, y);
            Some(&mut self.data[index])
        } else {
            None
        }
    }

    pub fn set(&mut self, x: usize, y: usize, value: T) {
        self.assert_in_bounds(x, y);
        let index = self.coordinates_to_index(x, y);
        self.data[index] = value;
    }

    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }
}

impl<T, const W: usize, const H: usize> From<[[T; W]; H]> for Array2D<T>
where
    T: Copy,
{
    fn from(data: [[T; W]; H]) -> Self {
        Self::new_init(W, H, |x, y| data[y][x])
    }
}
