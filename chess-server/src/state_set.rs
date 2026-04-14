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

    pub async fn insert_or_bump<F: Future<Output = Result<Table, crate::Error>>>(
        &mut self,
        room: i32,
        with: impl FnOnce() -> F,
    ) -> Result<Arc<Mutex<Table>>, crate::Error> {
        let table = self.data.get(&room).cloned();
        if table.is_none() {
            self.data.remove(&room);
        };
        let table = table.as_ref().and_then(Weak::upgrade);
        let table = if let Some(table) = table {
            table
        } else {
            let table = Arc::new(Mutex::new(with().await?));
            self.data.insert(room, Arc::downgrade(&table));
            table
        };

        Ok(table)
    }

    pub fn collect(&mut self) {
        self.data.retain(|_, v| Weak::strong_count(v) > 0);
    }
}
