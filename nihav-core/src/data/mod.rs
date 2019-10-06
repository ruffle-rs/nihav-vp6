pub struct GenericCache<T: Copy> {
    pub height: usize,
    pub stride: usize,
    pub xpos:   usize,
    pub data:   Vec<T>,
    pub default: T,
}

impl<T:Copy> GenericCache<T> {
    pub fn new(height: usize, stride: usize, default: T) -> Self {
        let mut ret = Self {
                stride,
                height,
                xpos:   0,
                data:   Vec::with_capacity((height + 1) * stride),
                default,
            };
        ret.reset();
        ret
    }
    pub fn full_size(&self) -> usize { self.stride * (self.height + 1) }
    pub fn reset(&mut self) {
        self.data.truncate(0);
        let size = self.full_size();
        self.data.resize(size, self.default);
        self.xpos = self.stride + 1;
    }
    pub fn update_row(&mut self) {
        for i in 0..self.stride {
            self.data[i] = self.data[self.height * self.stride + i];
        }
        self.data.truncate(self.stride);
        let size = self.full_size();
        self.data.resize(size, self.default);
        self.xpos = self.stride + 1;
    }
}

