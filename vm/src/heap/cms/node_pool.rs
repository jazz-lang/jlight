use crate::util::ptr::Ptr;

pub trait List {
    type NodeType: Sized;
    fn pop_back_(&mut self) -> Ptr<Self::NodeType>;
    fn pop_front_(&mut self) -> Ptr<Self::NodeType>;
    fn push_front_(&mut self, st: Ptr<Self::NodeType>);
    fn push_back_(&mut self, st: Ptr<Self::NodeType>);
}

pub struct NodePool<T: List> {
    pool: T,
}
