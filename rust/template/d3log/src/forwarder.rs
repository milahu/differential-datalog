// fowarder takes a batch and uses the hddlog interface to extract those facts with
// locality annotations, groups them by destination, and calls the registered send
// method for that destination

use crate::{Batch, BatchBody, Evaluator, Node, Port, Properties, Transport, ValueSet};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
// an Entry is a record of a peer adjacency, for which there may or ay not be
// an output port stablished
struct Entry {
    port: Option<Port>,
    batches: VecDeque<Batch>,
}

pub struct Forwarder {
    eval: Evaluator,
    // a map from node id to a port (thats been wrapped with some partial support for incremental updates)
    fib: Arc<Mutex<HashMap<Node, Arc<Mutex<Entry>>>>>,
}

impl Forwarder {
    pub fn new(eval: Evaluator) -> Arc<Forwarder> {
        let forwarder = Arc::new(Forwarder {
            eval: eval.clone(),
            fib: Arc::new(Mutex::new(HashMap::new())),
        });
        forwarder
    }

    fn lookup(&self, n: Node) -> Arc<Mutex<Entry>> {
        self.fib
            .lock()
            .expect("lock")
            .entry(n)
            .or_insert_with(|| {
                Arc::new(Mutex::new(Entry {
                    port: None,
                    batches: VecDeque::new(),
                }))
            })
            .clone()
    }

    pub fn register(&self, n: Node, p: Port) {
        // overwrite warning?
        let entry = self.lookup(n);
        {
            entry.lock().expect("lock").port = Some(p.clone());
        }

        while let Some(b) = entry.lock().expect("lock").batches.pop_front() {
            p.clone().send(b);
        }
    }
}

impl Transport for Forwarder {
    fn send(&self, batch: Batch) {
        let mut output = HashMap::<Node, ValueSet>::new();
        for (rel, value, weight) in
            &ValueSet::from(self.eval.clone(), batch.clone()).expect("iterator")
        {
            if let Some((loc_id, in_rel, inner_val)) = self.eval.localize(rel, value.clone()) {
                output
                    .entry(loc_id)
                    .or_insert_with(|| ValueSet::new(self.eval.clone()))
                    .insert(in_rel, inner_val, weight);
            }
        }

        for (nid, localized_batch) in output.drain() {
            let port = {
                match self.lookup(nid).lock() {
                    Ok(mut x) => match &x.port {
                        Some(x) => x.clone(),
                        None => {
                            x.batches.push_front(Batch {
                                metadata: batch.metadata.clone(),
                                body: BatchBody::Value(localized_batch),
                            });
                            break;
                        }
                    },
                    Err(_) => panic!("lock"),
                }
            };
            port.send(Batch {
                metadata: Properties::new(),
                body: BatchBody::Value(localized_batch.clone()),
            })
        }
    }
}