use channel::rpc::RpcClient;
use controller::{ExclusiveConnection, SharedConnection};

use dataflow::backlog::{self, ReadHandle};
use dataflow::prelude::*;
use dataflow::{self, checktable, LocalBypass, Readers};

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::rc::Rc;

pub(crate) type GetterRpc = Rc<RefCell<RpcClient<LocalOrNot<ReadQuery>, LocalOrNot<ReadReply>>>>;

/// A request to read a specific key.
#[derive(Serialize, Deserialize, Debug)]
pub enum ReadQuery {
    /// Read normally
    Normal {
        /// Where to read from
        target: (NodeIndex, usize),
        /// Keys to read with
        keys: Vec<Vec<DataType>>,
        /// Whether to block if a partial replay is triggered
        block: bool,
    },
    /// Read and also get a checktable token
    WithToken {
        /// Where to read from
        target: (NodeIndex, usize),
        /// Keys to read with
        keys: Vec<Vec<DataType>>,
    },
    /// Read the size of a leaf view
    Size {
        /// Where to read from
        target: (NodeIndex, usize),
    },
}

#[derive(Serialize, Deserialize)]
pub(crate) enum LocalOrNot<T> {
    Local(LocalBypass<T>),
    Not(T),
}

impl<T> LocalOrNot<T> {
    pub(crate) fn is_local(&self) -> bool {
        if let LocalOrNot::Local(..) = *self {
            true
        } else {
            false
        }
    }

    pub(crate) fn make(t: T, local: bool) -> Self {
        if local {
            LocalOrNot::Local(LocalBypass::make(Box::new(t)))
        } else {
            LocalOrNot::Not(t)
        }
    }

    pub(crate) unsafe fn take(self) -> T {
        match self {
            LocalOrNot::Local(l) => *l.take(),
            LocalOrNot::Not(t) => t,
        }
    }
}

/// The contents of a specific key
#[derive(Serialize, Deserialize, Debug)]
pub enum ReadReply {
    /// Read normally.
    /// Errors if view isn't ready yet.
    Normal(Result<Vec<Datas>, ()>),
    /// Read and got checktable tokens.
    /// Errors if view isn't ready yet.
    WithToken(Result<Vec<(Datas, checktable::Token)>, ()>),
    /// Read size of view
    Size(usize),
}

/// Serializeable version of a `RemoteGetter`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteGetterBuilder {
    pub(crate) node: NodeIndex,
    pub(crate) columns: Vec<String>,
    pub(crate) shards: Vec<(SocketAddr, bool)>,
    pub(crate) local_port: Option<u16>,
}

impl RemoteGetterBuilder {
    /// Build a `RemoteGetter` out of a `RemoteGetterBuilder`
    pub(crate) fn build_exclusive(self) -> RemoteGetter<ExclusiveConnection> {
        let conns = self.shards
            .iter()
            .map(move |&(ref addr, is_local)| {
                Rc::new(RefCell::new(RpcClient::connect(addr, is_local).unwrap()))
            })
            .collect();

        RemoteGetter {
            node: self.node,
            columns: self.columns,
            shard_addrs: self.shards,
            shards: conns,
            exclusivity: ExclusiveConnection,
        }
    }

    /// Set the local port to bind to when making the shared connection.
    pub(crate) fn with_local_port(mut self, port: u16) -> RemoteGetterBuilder {
        self.local_port = Some(port);
        self
    }

    /// Build a `RemoteGetter` out of a `RemoteGetterBuilder`
    pub(crate) fn build(
        mut self,
        rpcs: &mut HashMap<SocketAddr, GetterRpc>,
    ) -> RemoteGetter<SharedConnection> {
        let sport = &mut self.local_port;
        let conns = self.shards
            .iter()
            .map(move |&(ref addr, is_local)| {
                use std::collections::hash_map::Entry;

                match rpcs.entry(*addr) {
                    Entry::Occupied(e) => Rc::clone(e.get()),
                    Entry::Vacant(h) => {
                        let c = RpcClient::connect_from(*sport, addr, is_local).unwrap();
                        if sport.is_none() {
                            *sport = Some(c.local_addr().unwrap().port());
                        }

                        let c = Rc::new(RefCell::new(c));
                        h.insert(Rc::clone(&c));
                        c
                    }
                }
            })
            .collect();

        RemoteGetter {
            node: self.node,
            columns: self.columns,
            shard_addrs: self.shards,
            shards: conns,
            exclusivity: SharedConnection,
        }
    }
}

/// Struct to query the contents of a materialized view.
pub struct RemoteGetter<E = SharedConnection> {
    node: NodeIndex,
    columns: Vec<String>,
    shards: Vec<GetterRpc>,
    shard_addrs: Vec<(SocketAddr, bool)>,

    #[allow(dead_code)]
    exclusivity: E,
}

impl Clone for RemoteGetter<SharedConnection> {
    fn clone(&self) -> Self {
        RemoteGetter {
            node: self.node,
            columns: self.columns.clone(),
            shards: self.shards.clone(),
            shard_addrs: self.shard_addrs.clone(),
            exclusivity: SharedConnection,
        }
    }
}

unsafe impl Send for RemoteGetter<ExclusiveConnection> {}

impl RemoteGetter<SharedConnection> {
    /// Change this getter into one with a dedicated connection the server so it can be sent across
    /// threads.
    pub fn into_exclusive(self) -> RemoteGetter<ExclusiveConnection> {
        RemoteGetterBuilder {
            node: self.node,
            local_port: None,
            columns: self.columns,
            shards: self.shard_addrs,
        }.build_exclusive()
    }
}

impl<E> RemoteGetter<E> {
    /// Get the local address this getter is bound to.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.shards[0].borrow().local_addr()
    }

    /// Return the column schema of the view this getter is associated with.
    pub fn columns(&self) -> &[String] {
        self.columns.as_slice()
    }

    /// Query for the size of a specific view.
    pub fn len(&mut self) -> Result<usize, ()> {
        if self.shards.len() == 1 {
            let mut shard = self.shards[0].borrow_mut();
            let is_local = shard.is_local();
            let reply = shard
                .send(&LocalOrNot::make(
                    ReadQuery::Size {
                        target: (self.node, 0),
                    },
                    is_local,
                ))
                .unwrap();
            match unsafe { reply.take() } {
                ReadReply::Size(rows) => Ok(rows),
                _ => unreachable!(),
            }
        } else {
            let shard_queries = 0..self.shards.len();

            let len = shard_queries.into_iter().fold(0, |acc, shardi| {
                let mut shard = self.shards[shardi].borrow_mut();
                let is_local = shard.is_local();
                let reply = shard
                    .send(&LocalOrNot::make(
                        ReadQuery::Size {
                            target: (self.node, shardi),
                        },
                        is_local,
                    ))
                    .unwrap();

                match unsafe { reply.take() } {
                    ReadReply::Size(rows) => acc + rows,
                    _ => unreachable!(),
                }
            });
            Ok(len)
        }
    }

    /// Query for the results for the given keys, optionally blocking if it is not yet available.
    pub fn multi_lookup(
        &mut self,
        keys: Vec<Vec<DataType>>,
        block: bool,
    ) -> Result<Vec<Datas>, ()> {
        if self.shards.len() == 1 {
            let mut shard = self.shards[0].borrow_mut();
            let is_local = shard.is_local();
            let reply = shard
                .send(&LocalOrNot::make(
                    ReadQuery::Normal {
                        target: (self.node, 0),
                        keys,
                        block,
                    },
                    is_local,
                ))
                .unwrap();
            match unsafe { reply.take() } {
                ReadReply::Normal(rows) => rows,
                _ => unreachable!(),
            }
        } else {
            assert!(keys.iter().all(|k| k.len() == 1));
            let mut shard_queries = vec![Vec::new(); self.shards.len()];
            for key in keys {
                let shard = dataflow::shard_by(&key[0], self.shards.len());
                shard_queries[shard].push(key);
            }

            let mut err = false;
            let rows = shard_queries
                .into_iter()
                .enumerate()
                .filter(|&(_, ref keys)| !keys.is_empty())
                .flat_map(|(shardi, keys)| {
                    let mut shard = self.shards[shardi].borrow_mut();
                    let is_local = shard.is_local();
                    let reply = shard
                        .send(&LocalOrNot::make(
                            ReadQuery::Normal {
                                target: (self.node, shardi),
                                keys,
                                block,
                            },
                            is_local,
                        ))
                        .unwrap();

                    match unsafe { reply.take() } {
                        ReadReply::Normal(Ok(rows)) => rows,
                        ReadReply::Normal(Err(())) => {
                            err = true;
                            Vec::new()
                        }
                        _ => unreachable!(),
                    }
                })
                .collect();

            if err {
                Err(())
            } else {
                Ok(rows)
            }
        }
    }

    /// Query for the results for the given keys, optionally blocking if it is not yet available.
    pub fn transactional_multi_lookup(
        &mut self,
        keys: Vec<Vec<DataType>>,
    ) -> Result<Vec<(Datas, checktable::Token)>, ()> {
        if self.shards.len() == 1 {
            let mut shard = self.shards[0].borrow_mut();
            let is_local = shard.is_local();
            let reply = shard
                .send(&LocalOrNot::make(
                    ReadQuery::WithToken {
                        target: (self.node, 0),
                        keys,
                    },
                    is_local,
                ))
                .unwrap();
            match unsafe { reply.take() } {
                ReadReply::WithToken(rows) => rows,
                _ => unreachable!(),
            }
        } else {
            assert!(keys.iter().all(|k| k.len() == 1));
            let mut shard_queries = vec![Vec::new(); self.shards.len()];
            for key in keys {
                let shard = dataflow::shard_by(&key[0], self.shards.len());
                shard_queries[shard].push(key);
            }

            let mut err = false;
            let rows = shard_queries
                .into_iter()
                .enumerate()
                .flat_map(|(shardi, keys)| {
                    let mut shard = self.shards[shardi].borrow_mut();
                    let is_local = shard.is_local();
                    let reply = shard
                        .send(&LocalOrNot::make(
                            ReadQuery::WithToken {
                                target: (self.node, shardi),
                                keys,
                            },
                            is_local,
                        ))
                        .unwrap();

                    match unsafe { reply.take() } {
                        ReadReply::WithToken(Ok(rows)) => rows,
                        ReadReply::WithToken(Err(())) => {
                            err = true;
                            Vec::new()
                        }
                        _ => unreachable!(),
                    }
                })
                .collect();
            if err {
                Err(())
            } else {
                Ok(rows)
            }
        }
    }

    /// Lookup a single key.
    pub fn lookup(&mut self, key: &[DataType], block: bool) -> Result<Datas, ()> {
        // TODO: Optimized version of this function?
        self.multi_lookup(vec![Vec::from(key)], block)
            .map(|rs| rs.into_iter().next().unwrap())
    }

    /// Do a transactional lookup for a single key.
    pub fn transactional_lookup(
        &mut self,
        key: &[DataType],
    ) -> Result<(Datas, checktable::Token), ()> {
        // TODO: Optimized version of this function?
        self.transactional_multi_lookup(vec![Vec::from(key)])
            .map(|rs| rs.into_iter().next().unwrap())
    }
}

/// A handle for looking up results in a materialized view.
pub struct Getter {
    pub(crate) generator: Option<checktable::TokenGenerator>,
    pub(crate) handle: backlog::ReadHandle,
    last_ts: i64,
}

#[allow(unused)]
impl Getter {
    pub(crate) fn new(
        node: NodeIndex,
        sharded: bool,
        readers: &Readers,
        ingredients: &Graph,
    ) -> Option<Self> {
        let rh = if sharded {
            let vr = readers.lock().unwrap();

            let shards = ingredients[node].sharded_by().shards();
            let mut getters = Vec::with_capacity(shards);
            for shard in 0..shards {
                match vr.get(&(node, shard)).cloned() {
                    Some((rh, _)) => getters.push(Some(rh)),
                    None => return None,
                }
            }
            ReadHandle::Sharded(getters)
        } else {
            let vr = readers.lock().unwrap();
            match vr.get(&(node, 0)).cloned() {
                Some((rh, _)) => ReadHandle::Singleton(Some(rh)),
                None => return None,
            }
        };

        let gen = ingredients[node]
            .with_reader(|r| r)
            .ok()
            .and_then(|r| r.token_generator().cloned());
        assert_eq!(ingredients[node].is_transactional(), gen.is_some());
        Some(Getter {
            generator: gen,
            handle: rh,
            last_ts: i64::min_value(),
        })
    }

    /// Returns the number of populated keys
    pub fn len(&self) -> usize {
        self.handle.len()
    }

    /// Returns true if this getter supports transactional reads.
    pub fn supports_transactions(&self) -> bool {
        self.generator.is_some()
    }

    /// Query for the results for the given key, and apply the given callback to matching rows.
    ///
    /// If `block` is `true`, this function will block if the results for the given key are not yet
    /// available.
    ///
    /// If you need to clone values out of the returned rows, make sure to use
    /// `DataType::deep_clone` to avoid contention on internally de-duplicated strings!
    pub fn lookup_map<F, T>(&self, q: &[DataType], mut f: F, block: bool) -> Result<Option<T>, ()>
    where
        F: FnMut(&[Vec<DataType>]) -> T,
    {
        self.handle.find_and(q, |rs| f(&rs[..]), block).map(|r| r.0)
    }

    /// Query for the results for the given key, optionally blocking if it is not yet available.
    pub fn lookup(&self, q: &[DataType], block: bool) -> Result<Datas, ()> {
        self.lookup_map(
            q,
            |rs| {
                rs.into_iter()
                    .map(|r| r.iter().map(|v| v.deep_clone()).collect())
                    .collect()
            },
            block,
        ).map(|r| r.unwrap_or_else(Vec::new))
    }

    /// Transactionally query for the given key, blocking if it is not yet available.
    pub fn transactional_lookup(
        &mut self,
        q: &[DataType],
    ) -> Result<(Datas, checktable::Token), ()> {
        match self.generator {
            None => Err(()),
            Some(ref g) => {
                loop {
                    let res = self.handle.find_and(
                        q,
                        |rs| {
                            rs.into_iter()
                                .map(|v| (&**v).into_iter().map(|v| v.deep_clone()).collect())
                                .collect()
                        },
                        true,
                    );
                    match res {
                        Ok((_, ts)) if ts < self.last_ts => {
                            // we must have read from a different shard that is not yet up-to-date
                            // to our last read. this is *extremely* unlikely: you would have to
                            // issue two reads to different shards *between* the barrier and swap
                            // inside Reader nodes, which is only a span of a handful of
                            // instructions. But it is *possible*.
                        }
                        Ok((res, ts)) => {
                            self.last_ts = ts;
                            let token = g.generate(ts, q.clone());
                            break Ok((res.unwrap_or_else(Vec::new), token));
                        }
                        Err(e) => break Err(e),
                    }
                }
            }
        }
    }
}
