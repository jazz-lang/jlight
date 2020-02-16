use crate::util::mem::*;
use crate::util::ptr::*;
use intrusive_collections::{LinkedList, LinkedListLink};

intrusive_adapter!(pub SpaceAdapter = Box<Page> : Page {hook: LinkedListLink});

pub struct Space {
    pub top: Address,
    pub limit: Address,
    pub pages: LinkedList<SpaceAdapter>,
    pub size: usize,
    pub size_limit: usize,
    pub page_size: usize,
    pub allocated_size: usize,
}

impl Space {
    pub fn empty() -> Self {
        Self {
            top: Address::null(),
            limit: Address::null(),
            pages: LinkedList::new(SpaceAdapter::new()),
            size: 0,
            allocated_size: 0,
            page_size: 0,
            size_limit: 0,
        }
    }
    pub fn new(page_size: usize) -> Self {
        let mut pages = LinkedList::new(SpaceAdapter::new());
        let page = Page::new(page_size);
        pages.push_back(Box::new(page));
        let top = Address::from_ptr(&pages.back().get().unwrap().top);
        let limit = Address::from_ptr(&pages.back().get().unwrap().limit);
        let mut space = Space {
            top,
            limit,
            pages,
            size: 0,
            page_size,
            size_limit: 0,
            allocated_size: 0,
        };
        space.compute_size_limit();
        space
    }

    pub fn compute_size_limit(&mut self) {
        self.size_limit = self.size << 1;
    }
    pub fn may_allocate_in_current(&mut self, size: usize) -> bool {
        let even_bytes = size + (size & 0x01);
        let place_in_current = self.top.deref().offset(even_bytes) <= self.limit.deref();
        println!("MMM: {}", place_in_current);
        place_in_current
    }
    pub fn add_page(&mut self, size: usize) {
        let real_size = align_usize(size, page_size());
        let page = Page::new(real_size);
        self.size += real_size;
        self.top = Address::from_ptr(&page.top);
        self.limit = Address::from_ptr(&page.limit);
        self.pages.push_back(Box::new(page));
    }

    pub fn allocate(&mut self, bytes: usize, needs_gc: &mut bool) -> Address {
        let even_bytes = bytes + (bytes & 0x01);
        let place_in_current = self.top.deref().offset(even_bytes) <= self.limit.deref();

        if !place_in_current {
            let mut iter = self.pages.iter();
            let mut head = iter.next_back();
            loop {
                if self.top.deref().offset(even_bytes) > self.limit.deref() && head.is_some() {
                    let old_head = head;
                    head = iter.next_back();
                    self.top = Address::from_ptr(&old_head.unwrap().top);
                    self.limit = Address::from_ptr(&old_head.unwrap().limit);
                } else {
                    break;
                }
            }

            if head.is_none() {
                self.add_page(even_bytes);
            }
        }
        self.allocated_size += even_bytes;
        let result = self.top.deref();
        unsafe {
            *self.top.to_mut_ptr::<*mut u8>() =
                self.top.deref().offset(even_bytes).to_mut_ptr::<u8>();
        }
        result
    }

    pub fn swap(&mut self, space: &mut Space) {
        self.clear();
        while space.pages.is_empty() != true {
            self.pages.push_back(space.pages.pop_back().unwrap());
            self.size += self.pages.back().get().unwrap().size;
        }
        self.allocated_size = space.allocated_size;
        let page = self.pages.back().get().unwrap();
        self.top = Address::from_ptr(&page.top);
        self.limit = Address::from_ptr(&page.limit);
    }

    pub fn clear(&mut self) {
        self.size = 0;
        while let Some(_) = self.pages.pop_back() {}
    }
}

pub struct Page {
    pub data: Address,
    pub top: Address,
    pub limit: Address,
    pub size: usize,
    pub hook: LinkedListLink,
}

impl Page {
    pub fn new(size: usize) -> Self {
        let data = commit(size, false);
        let top = data;
        let limit = data.offset(size);
        Self {
            top,
            data,
            limit,
            size,
            hook: LinkedListLink::new(),
        }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        uncommit(self.data, self.size);
    }
}
