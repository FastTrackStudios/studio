//! Outgoing edit coalescing. A drag produces a value per pointer event
//! (often 120+/s); the engine needs ~25/s. Changes to one parameter within
//! the interval collapse to the latest; gesture begin/end are never dropped
//! and always flush, so the engine always ends on the exact final value.

use std::collections::BTreeMap;

use crate::surface::proto::{EditPhase, ParamAddr};

#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    pub addr: ParamAddr,
    pub value: f64,
    pub phase: EditPhase,
}

#[derive(Debug, Clone)]
pub struct Coalescer {
    interval_ms: f64,
    pending: BTreeMap<ParamAddr, f64>,
    last_sent: BTreeMap<ParamAddr, f64>,
}

impl Default for Coalescer {
    fn default() -> Self {
        Self::new(40.0)
    }
}

impl Coalescer {
    #[must_use]
    pub fn new(interval_ms: f64) -> Self {
        Self {
            interval_ms,
            pending: BTreeMap::new(),
            last_sent: BTreeMap::new(),
        }
    }

    /// Queue an edit at time `t` (ms); returns what to send now.
    pub fn push(&mut self, addr: ParamAddr, value: f64, phase: EditPhase, t: f64) -> Vec<Edit> {
        match phase {
            EditPhase::Begin | EditPhase::End => {
                self.pending.remove(&addr);
                self.last_sent.insert(addr.clone(), t);
                vec![Edit { addr, value, phase }]
            }
            EditPhase::Change => {
                let due = self
                    .last_sent
                    .get(&addr)
                    .is_none_or(|last| t - last >= self.interval_ms);
                if due {
                    self.pending.remove(&addr);
                    self.last_sent.insert(addr.clone(), t);
                    vec![Edit { addr, value, phase }]
                } else {
                    self.pending.insert(addr, value);
                    Vec::new()
                }
            }
        }
    }

    /// Send whatever has waited out the interval.
    pub fn poll(&mut self, t: f64) -> Vec<Edit> {
        let due: Vec<ParamAddr> = self
            .pending
            .keys()
            .filter(|a| {
                self.last_sent
                    .get(*a)
                    .is_none_or(|last| t - last >= self.interval_ms)
            })
            .cloned()
            .collect();
        due.into_iter()
            .filter_map(|addr| {
                let value = self.pending.remove(&addr)?;
                self.last_sent.insert(addr.clone(), t);
                Some(Edit {
                    addr,
                    value,
                    phase: EditPhase::Change,
                })
            })
            .collect()
    }
}
