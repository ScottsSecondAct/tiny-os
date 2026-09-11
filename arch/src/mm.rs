pub trait PageAllocator {
    fn alloc_page(&mut self) -> Option<usize>;
    fn alloc_pages(&mut self, count: usize) -> Option<usize>;
    fn free_page(&mut self, pa: usize);
    fn total_pages(&self) -> usize;
    fn used_pages(&self) -> usize;

    fn free_pages(&self) -> usize {
        self.total_pages() - self.used_pages()
    }
}
