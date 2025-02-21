use std::{cell::RefCell, collections::HashMap, rc::Rc};
use log::{debug, error, info, warn};

use super::{storage::Storage, UpdatableAction};

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
pub struct UpdatableActionStorage {
    _data: Rc<RefCell<HashMap<String, Box<dyn UpdatableAction>>>>,
    _index: Rc<RefCell<usize>>,
}

impl UpdatableActionStorage {
    pub fn new() -> Self {
        UpdatableActionStorage {
            _data: Rc::new(RefCell::new(HashMap::new())),
            _index: Rc::new(RefCell::new(0)),
        }
    }
}

impl Drop for UpdatableActionStorage {
    fn drop(&mut self) {
        self.dispose();
    }
}

#[cfg(target_arch = "wasm32")]
impl Storage for UpdatableActionStorage {
    fn insert(&mut self, key: String, action: impl UpdatableAction + 'static) {
        let mut data = self._data.borrow_mut();
        
        if data.contains_key(&key) == false {
            data.insert(key.clone(), Box::new(action));
            debug!("Inserting key {} into actions, count: {}", key, data.len());
        } else {
            warn!("The key already exists in storage: {}. Dropping...", &key);
        }
    }

    fn contains(&self, key: String) -> bool {
        self._data.borrow_mut().contains_key(&key)
    }

    fn update(&mut self, key: String, mut f: impl FnMut(&mut Box<dyn UpdatableAction>)) {
        let mut data = self._data.borrow_mut();

        if data.contains_key(&key) {
            let action = data.get_mut(&key).unwrap();
            (f)(action);
        } else {
            error!("Key {} is not found in {} registered actions", key, data.len());
        }
    }

    fn remove(&mut self, key: String) {
        let mut data = self._data.borrow_mut();

        if data.contains_key(&key) {
            let mut removed = data.remove(&key);

            if removed.is_some() {
                debug!("Removed key {} from actions, count: {}", key, data.len());

                info!("Dropping item at key {}", key);
                let data = removed.take().unwrap();
                drop(data);
            } else {
                warn!("Data at key {} is an empty action.", key);
            }
        } else {
            warn!("Cannot remove key {} from actions, count: {}. The key does not exist.", key, data.len());
        }
    }
    
    fn dispose(&mut self) {
        let count = Rc::strong_count(&self._data);

        if count == 1 {
            info!("Clearing storage...");
            let mut data = self._data.borrow_mut();

            data.clear();
        }
    }

    fn increment(&mut self) -> usize {
        let mut index = self._index.borrow_mut();

        *index += 1;

        *index
    }
}


