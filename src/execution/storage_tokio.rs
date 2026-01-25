use std::{collections::HashMap, sync::{Arc, Mutex}};
use log::{error, info};
use super::{Storage, UpdatableAction};

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct UpdatableActionStorage {
    _data: Arc<Mutex<HashMap<String, Mutex<Box<dyn UpdatableAction>>>>>,
    _index: Arc<Mutex<usize>>,
}

impl UpdatableActionStorage {
    pub fn new() -> Self {
        UpdatableActionStorage {
            _data: Arc::new(Mutex::new(HashMap::new())),
            _index: Arc::new(Mutex::new(0)),
        }
    }
}

impl Drop for UpdatableActionStorage {
    fn drop(&mut self) {
        self.dispose();
    }
}

impl Storage for UpdatableActionStorage {
    fn insert(&mut self, key: String, action: impl UpdatableAction + 'static) {
        if let Ok(mut data) = self._data.lock() {
            if data.contains_key(&key) == false {
                data.insert(key, Mutex::new(Box::new(action)));
            } else {
                error!("Key {} is already registered as an action", key);
            }
        } else {
            error!("Cannot lock storage");
        }
    }

    fn contains(&self, key: String) -> bool {
        if let Ok(data) = self._data.lock() {
            let  res = data.contains_key(&key);

            res    
        } else {
            error!("Cannot lock storage");

            false
        }
    }

    fn update(&mut self, key: String, mut f: impl FnMut(&mut Box<dyn UpdatableAction>)) {
        if let Ok(mut data) = self._data.lock() {
            if data.contains_key(&key) {
                if let Some(action) = data.get_mut(&key) {
                    if let Ok(mut a) = action.lock() {
                        (f)(&mut a);
                    } else {
                        error!("Cannot unlock action");
                    }
                } else {
                    error!("Cannot get out action from storage");
                }
            } else {
                error!("Key {} is not found in registered actions", key);
            }
        } else {
            error!("Cannot lock storage");
        }
    }

    fn remove(&mut self, key: String) {
        if let Ok(mut data) = self._data.lock() {
            if let Some(ret) = data.remove(&key) {
                let r = ret.into_inner();
    
                if r.is_ok() {
                    drop(r.unwrap());
                }
            }    
        } else {
            error!("Cannot lock storage");
        }
    }

    fn dispose(&mut self) {
        let count = Arc::strong_count(&self._data);

        if count == 1 {
            info!("Clearing storage...");
            if let Ok(mut data) = self._data.lock() {
                data.clear();
            } else {
                error!("Cannot lock storage");
            }
        }
    }

    fn increment(&mut self) -> usize {
        let mut index = self._index.lock().unwrap();

        *index += 1;

        *index
    }
}