use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
    time::Instant,
};

use futures::lock::Mutex;

use crate::Table;

const CAPACITY: usize = 100;

struct State {
    last_modified: Instant,
    table: Arc<Mutex<Table>>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
struct StateInfo {
    time: Instant,
    room: i32,
}

#[derive(Default)]
pub struct StateSet {
    data: HashMap<i32, State>,
    recent: BTreeSet<StateInfo>,
}

impl StateSet {
    // #[must_use]
    // pub fn contains(&mut self, room: i32) -> bool {
    //     self.data.contains_key(room)
    // }

    pub async fn insert_or_bump<F: Future<Output = Table>>(
        &mut self,
        room: i32,
        with: impl FnOnce() -> F,
    ) -> Arc<Mutex<Table>> {
        // If exists, bump
        // If not, insert

        let mut new = false;

        // This is really ugly, but that's the effect of async here
        // I might consider changing the structure to something more async-friendly
        let v = if let Some(v) = self.data.get_mut(&room) {
            // Bump the room
            assert!(self.recent.remove(&StateInfo {
                time: v.last_modified,
                room: room
            }));
            v.last_modified = Instant::now();
            self.recent.insert(StateInfo {
                time: v.last_modified,
                room,
            });

            v
        } else {
            new = true;
            self.data.insert(
                room,
                State {
                    last_modified: Instant::now(),
                    table: Arc::new(Mutex::new(with().await)),
                },
            );

            self.data.get_mut(&room).unwrap()
        };

        let ret = v.table.clone();

        if new {
            self.recent.insert(StateInfo {
                time: v.last_modified,
                room,
            });

            if self.data.len() > CAPACITY
                && let Some(min) = self.recent.first().copied()
            {
                self.recent.remove(&min);
                self.data.remove(&min.room);
            }
        }

        ret
    }
}
