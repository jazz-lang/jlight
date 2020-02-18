use crate::util::ptr::{DerefPointer, Ptr};
use std::sync::atomic::*;

pub struct ListNode<T> {
    pub data: T,
    pub next: Ptr<Self>,
}

pub struct ListNodeIter<T> {
    focus: Ptr<ListNode<T>>,
}

impl<T: 'static> Iterator for ListNodeIter<T> {
    type Item = DerefPointer<T>;
    fn next(&mut self) -> Option<DerefPointer<T>> {
        if self.focus.next.is_null() {
            return None;
        }
        let p = self.focus.next;
        self.focus = p;
        Some(DerefPointer::new(&p.get().data))
    }
}

pub struct List<T> {
    pub head: Ptr<ListNode<T>>,
    pub tail: Ptr<ListNode<T>>,
}
