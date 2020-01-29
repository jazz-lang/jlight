use super::*;
use crate::config::Config;
use crate::object::*;
use crate::object_value::*;
use crate::ptr::*;
use crate::state::*;
use block::*;
use bucket::*;
use copy_object::*;
use generation_config::*;
use global_allocator::*;
use histograms::*;
use remembered::*;
/// The maximum age of a bucket in the young generation.
pub const YOUNG_MAX_AGE: i8 = 2;

/// Structure containing the state of a process-local allocator.
pub struct LocalAllocator {
    /// The global allocated from which to request blocks of memory and return
    /// unused blocks to.
    pub global_allocator: RcGlobalAllocator,

    /// The buckets to use for the eden and young survivor spaces.
    pub young_generation: [Bucket; YOUNG_MAX_AGE as usize + 1],

    /// The histograms to use for collecting the young generation.
    pub young_histograms: Histograms,

    /// The histograms to use for collecting the mature generation.
    pub mature_histograms: Histograms,

    /// The position of the eden bucket in the young generation.
    ///
    /// This is a u8 to conserve space, as we'll never have more than 255
    /// buckets to choose from.
    pub eden_index: u8,

    /// A collection of mature objects that contain pointers to young objects.
    pub remembered_set: RememberedSet,

    /// The bucket to use for the mature generation.
    pub mature_generation: Bucket,

    /// The configuration for the young generation.
    pub young_config: GenerationConfig,

    /// The configuration for the mature generation.
    pub mature_config: GenerationConfig,

    /// A boolean indicating if we should evacuate objects in the young
    /// generation.
    evacuate_young: bool,

    /// A boolean indicating if we should evacuate objects in the mature
    /// generation.
    evacuate_mature: bool,
}

impl LocalAllocator {
    pub fn new(global_allocator: RcGlobalAllocator, config: &Config) -> LocalAllocator {
        LocalAllocator {
            global_allocator,
            young_generation: [
                Bucket::with_age(0),
                Bucket::with_age(-1),
                Bucket::with_age(-2),
            ],
            young_histograms: Histograms::new(),
            mature_histograms: Histograms::new(),
            eden_index: 0,
            mature_generation: Bucket::with_age(MATURE),
            young_config: GenerationConfig::new(config.young_threshold),
            mature_config: GenerationConfig::new(config.mature_threshold),
            remembered_set: RememberedSet::new(),
            evacuate_young: false,
            evacuate_mature: false,
        }
    }

    pub fn global_allocator(&self) -> RcGlobalAllocator {
        self.global_allocator.clone()
    }

    pub fn eden_space(&self) -> &Bucket {
        &self.young_generation[self.eden_index as usize]
    }

    pub fn eden_space_mut(&mut self) -> &mut Bucket {
        &mut self.young_generation[self.eden_index as usize]
    }

    pub fn should_collect_young(&self) -> bool {
        self.young_config.allocation_threshold_exceeded()
    }

    pub fn should_collect_mature(&self) -> bool {
        self.mature_config.allocation_threshold_exceeded()
    }

    /// Prepares for a garbage collection.
    ///
    /// Returns true if objects have to be moved around.
    pub fn prepare_for_collection(&mut self, mature: bool) -> bool {
        let mut move_objects = self.evacuate_young;

        for bucket in &mut self.young_generation {
            bucket.prepare_for_collection(&mut self.young_histograms, self.evacuate_young);

            if bucket.age == YOUNG_MAX_AGE {
                move_objects = true;
            }
        }

        if mature {
            self.mature_generation
                .prepare_for_collection(&mut self.mature_histograms, self.evacuate_mature);

            if self.evacuate_mature {
                move_objects = true;
            }
        }

        move_objects
    }

    /// Reclaims blocks in the young (and mature) generation.
    pub fn reclaim_blocks(&mut self, state: &State, mature: bool) {
        self.reclaim_young_blocks(&state);

        if mature {
            self.reclaim_mature_blocks(&state);
        }
    }

    fn reclaim_young_blocks(&mut self, state: &State) {
        self.young_histograms.reset();

        let mut blocks = 0;

        for bucket in &mut self.young_generation {
            blocks += bucket.reclaim_blocks(state, &mut self.young_histograms);
        }

        self.increment_young_ages();

        self.evacuate_young = self
            .young_config
            .update_after_collection(&state.config, blocks);
    }

    fn reclaim_mature_blocks(&mut self, state: &State) {
        self.mature_histograms.reset();

        let blocks = self
            .mature_generation
            .reclaim_blocks(state, &mut self.mature_histograms);

        self.evacuate_mature = self
            .mature_config
            .update_after_collection(&state.config, blocks);
    }

    pub fn allocate_with_prototype(
        &mut self,
        value: ObjectValue,
        proto: ObjectPointer,
    ) -> ObjectPointer {
        let object = Object::with_prototype(value, proto);

        self.allocate_eden(object)
    }

    pub fn allocate_without_prototype(&mut self, value: ObjectValue) -> ObjectPointer {
        let object = Object::new(value);

        self.allocate_eden(object)
    }

    /// Allocates an empty object without a prototype.
    pub fn allocate_empty(&mut self) -> ObjectPointer {
        self.allocate_without_prototype(ObjectValue::None)
    }

    pub fn allocate_eden(&mut self, object: Object) -> ObjectPointer {
        let (new_block, pointer) = self.young_generation[self.eden_index as usize]
            .allocate(&self.global_allocator, object);

        if new_block {
            self.young_config.increment_allocations();
        }

        pointer
    }

    pub fn allocate_mature(&mut self, object: Object) -> ObjectPointer {
        let (new_block, pointer) = self
            .mature_generation
            .allocate(&self.global_allocator, object);

        if new_block {
            self.mature_config.increment_allocations();
        }

        pointer
    }

    /// Increments the age of all buckets in the young generation
    pub fn increment_young_ages(&mut self) {
        for (index, bucket) in self.young_generation.iter_mut().enumerate() {
            if bucket.age == YOUNG_MAX_AGE {
                bucket.reset_age();
            } else {
                bucket.increment_age();
            }

            if bucket.age == 0 {
                self.eden_index = index as u8;
            }
        }
    }

    pub fn remember_object(&mut self, pointer: ObjectPointer) {
        if pointer.is_remembered() {
            return;
        }

        pointer.mark_as_remembered();

        self.remembered_set.remember(pointer);
    }

    pub fn prune_remembered_objects(&mut self) {
        self.remembered_set.prune();
    }

    pub fn each_remembered_pointer<F>(&mut self, mut callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        if self.remembered_set.is_empty() {
            return;
        }

        for pointer in self.remembered_set.iter() {
            // In a young collection we want to (re-)trace all remembered
            // objects. Mark values for the mature space are only updated during
            // a mature collection. We don't care about nested mature objects,
            // as those will be in the remembered set if they contain any young
            // pointers.
            //
            // Line mark states should remain as-is, so we don't promote mature
            // objects into already used mature lines.
            //
            // Because of this, all we need to do here is unmark the mature
            // objects we have remembered; ensuring we will trace them for any
            // young pointers.
            pointer.unmark();

            callback(pointer.pointer());
        }
    }
}

impl CopyObject for LocalAllocator {
    fn allocate_copy(&mut self, object: Object) -> ObjectPointer {
        self.allocate_eden(object)
    }
}
