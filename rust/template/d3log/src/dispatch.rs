// dispatch is an external placeholder for a timely sink. It allows ports to be registered against
// relations, and like forwarder, groups up sub-batches based on relation id and routes them
// out the correct port. Used to hang management relation update ports off the broadcast tree

use crate::{Batch, BatchBody, Error, Port, Properties, RecordSet, Transport};

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

type DispatchMap = HashMap<String, Vec<(u64, Port)>>;

#[derive(Clone)]
pub struct Dispatch {
    count: Arc<AtomicUsize>,
    handlers: Arc<RwLock<DispatchMap>>,
}

impl Transport for Dispatch {
    fn send(&self, b: Batch) {
        let mut output = HashMap::<u64, (Port, RecordSet)>::new();

        for (rel, v, weight) in &RecordSet::from(b.clone()).expect("batch") {
            if let Some(ports) = self.handlers.read().expect("lock").get(&rel) {
                for (i, p) in ports {
                    output
                        .entry(*i)
                        .or_insert_with(|| (p.clone(), RecordSet::new()))
                        .1
                        .insert(v.clone(), weight);
                }
            }
        }
        for (_, (p, b)) in output {
            p.send(Batch {
                metadata: Properties::new(),
                body: BatchBody::Record(b),
            });
        }
    }
}

impl Dispatch {
    pub fn new() -> Dispatch {
        Dispatch {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            count: Arc::new(AtomicUsize::new(0)),
        }
    }

    // deregstration? return a handle?
    // we should validate the relation_name? incl dynamic schema
    pub fn register(&self, relation_name: &str, p: Port) -> Result<(), Error> {
        let id = self.count.fetch_add(1, Ordering::SeqCst);

        self.handlers
            .write()
            .expect("lock")
            .entry(relation_name.to_string())
            .or_insert_with(Vec::new)
            .push((id as u64, p));
        Ok(())
    }
}