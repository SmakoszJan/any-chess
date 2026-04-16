use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use futures::lock::Mutex;

use crate::Table;

#[derive(Default)]
pub struct StateSet {
    data: HashMap<i32, Weak<Mutex<Table>>>,
}

impl StateSet {
    // #[must_use]
    // pub fn contains(&mut self, room: i32) -> bool {
    //     self.data.contains_key(room)
    // }
    //
    pub fn get(&self, room: i32) -> Option<Arc<Mutex<Table>>> {
        self.data.get(&room).and_then(Weak::upgrade)
    }

    pub fn maybe_insert(&mut self, room: i32, table: Arc<Mutex<Table>>) -> Arc<Mutex<Table>> {
        let t = self.data.get(&room).cloned();
        if t.is_none() {
            self.data.remove(&room);
        };
        let t = t.as_ref().and_then(Weak::upgrade);
        let t = if let Some(t) = t {
            t
        } else {
            self.data.insert(room, Arc::downgrade(&table));
            table
        };

        t
    }

    pub fn collect(&mut self) {
        self.data.retain(|_, v| Weak::strong_count(v) > 0);
    }
}
