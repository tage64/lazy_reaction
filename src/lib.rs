//! An implementation of a semi-reactive system, similar to
//! [reactive_graph](https://docs.rs/reactive_graph) but much simpler. In particular, unlike
//! reactive_graph, this implementation does not require a global runtime. Instead, updates become
//! visible in [derived signals](DerivedSignal) only after a call to [`ReactiveGraph::tick()`].
//!
//! However, the most notable difference from reactive_graph is that this implementation does not
//! include any side effects such as the `Effect` type. You must manually query nodes in the graph
//! after calling [`tick()`](ReactiveGraph::tick). Consequently, the entire graph does not need to
//! be reevaluated at every tick; only the relevant parts of the graph are updated when you call
//! [`Source::get()`].
//!
//! In short, the system operates as follows:
//!
//! - You create a set of signals using the [`signal()`] function. These act as sources for the
//!   reactive graph and receive values from the outside world. Each signal comes in a pair:
//!   ([`ReadSignal`], [`WriteSignal`]). The [`ReadSignal`] type implements the [`Source`] trait,
//!   which provides the following methods:
//!
//!     - [`Source::get()`]: Returns `Some(val)` if the signal has been updated since the last call
//!       to this method, or [`None`] otherwise. For [`ReadSignal`], it returns [`Some`] on the
//!       first call with the initial value; this is not the case for [`DerivedSignal`] (see below).
//!
//!     - [`Source::get_existing()`]: Returns the current value. This requires that values
//!       implement [`Clone`]. If you are working with large objects, consider wrapping them in a
//!       shared pointer such as [`Arc`].
//!
//! - You create a graph for all derived signals and memos with [`ReactiveGraph::new()`].
//!
//! - You add [derived signals](DerivedSignal) using [`ReactiveGraph::derived_signal()`] or
//!   [`ReactiveGraph::memo()`]. These are nodes that depend on one or more sources within the
//!   reactive graph. The sources may be [pure signals](ReadSignal) or other [derived signals]
//!   (`DerivedSignal`). Any type implementing the [`Source`] trait can serve as a source.
//!
//!     [`DerivedSignal`] also implements the [`Source`] trait, with one notable distinction from
//!     [`ReadSignal`]: the [`get()`-function](Source::get) may return a new value (`Some`) only
//!     after a call to [`ReactiveGraph::tick()`]. Subsequent calls to [`Source::get()`] will return
//!     [`None`] until the next tick. This also applies after its creation—`get()` will return
//!     [`None`] until the first tick.
//!
//! - You update the system by pushing new values to signals using [`WriteSignal::set()`].
//!
//! - When you want to observe changes, you call [`ReactiveGraph::tick()`].
//!
//! - You use [`DerivedSignal::get()`] to retrieve the value of a derived signal. If it returns
//!   [`None`], the value has not changed since you (or another derived signal) last called `get()`.
//!   You can always retrieve the latest value (as of the last tick) with
//!   [`DerivedSignal::get_existing()`], regardless of whether it has changed.
//!
//! So, why is it lazy? Because derived values are only computed on demand. In reactive_graph,
//! derived signals are reevaluated on every tick of the runtime, and relevant side effects are
//! triggered. Here, you decide when and which parts of the graph should be reevaluated. Only when
//! you call [`DerivedSignal::get()`] are the necessary parts of the graph recomputed.
//!
//! ### Example
//!
//! ```
//! use lazy_reaction::{ReactiveGraph, Source, signal};
//!
//! let (mut get_year, set_year) = signal(1989);
//! let (get_month, set_month) = signal(11);
//! let (get_day, set_day) = signal(9);
//!
//! // Initial call to get() will return the values but subsequent will not.
//! assert_eq!(get_year.get(), Some(1989));
//! assert_eq!(get_year.get(), None);
//! assert_eq!(get_year.get(), None);
//! // However get_existing() works.
//! assert_eq!(get_year.get_existing(), 1989);
//! assert_eq!(get_month.get_existing(), 11);
//!
//! // Let's create derived signals that format these items.
//! // We begin by constructing a reactive graph:
//! let rgraph = ReactiveGraph::new();
//!
//! // And then we add the derived signals.
//! // get_year is the source, and is cheaply clonable.
//! let mut formatted_year = rgraph.derived_signal(get_year.clone(), |year| year.to_string());
//!
//! assert_eq!(formatted_year.get(), Some("1989".to_string()));
//! assert_eq!(formatted_year.get(), None);
//! assert_eq!(formatted_year.get_existing(), "1989");
//!
//! // It is usually better to use a memo. Then the value will only be considered updated if it has
//! // changed.
//! let mut formatted_month = rgraph.memo(get_month, |m| format!("{m:02}"));
//! let mut formatted_day = rgraph.memo(get_day, |d| format!("{d:02}"));
//!
//! // Now we show the real power of the system! We agrigate these derived signals to a formatted
//! // date. This will only be reevaluated if any of the input signals changed.
//! let mut formatted_date = rgraph.memo(
//!     (formatted_year, formatted_month, formatted_day),
//!     |(yyyy, mm, dd)| format!("{yyyy}-{mm}-{dd}"),
//! );
//!
//! // A value is immediately available based on the get_existing() calls of the sources.
//! assert_eq!(formatted_date.get(), Some("1989-11-09".to_string()));
//! assert_eq!(formatted_date.get(), None);
//!
//! // Let's make a tick of the graph!
//! rgraph.tick();
//!
//! // Nothing has changed so get() still returns None.
//! assert_eq!(formatted_date.get(), None);
//! assert_eq!(formatted_date.get_existing(), "1989-11-09");
//!
//! // Let's change dates.
//! set_year.set(1991);
//! assert_eq!(get_year.get(), Some(1991));
//! set_month.set(12);
//! set_day.set(8);
//! // We haven't called tick() yet so nothing has been updated.
//! assert_eq!(formatted_date.get(), None);
//!
//! rgraph.tick();
//! assert_eq!(formatted_date.get(), Some("1991-12-08".to_string()));
//!
//! set_year.set(2024);
//!
//! rgraph.tick();
//! assert_eq!(formatted_date.get(), Some("2024-12-08".to_string()));
//! ```

mod source;
use std::mem;
use std::num::NonZero;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::{RwLock, RwLockReadGuard};
pub use source::Source;

/// A graph of signals and some kind of lazy evaluation.
///
/// This is a graph for signals and derived computations. You should use this graph to create nodes
/// in the graph and to signal a tick in the graph.
#[derive(Debug, Clone)]
pub struct ReactiveGraph(Arc<ReactiveGraphInner>);

#[derive(Debug)]
struct ReactiveGraphInner {
    /// A value that will be incremented at every reactive step.
    ///
    /// Starts at 1 so that there is always a lower value for new or reset nodes.
    generation: AtomicU64,
}

impl Default for ReactiveGraph {
    fn default() -> Self {
        ReactiveGraph(Arc::new(ReactiveGraphInner {
            generation: AtomicU64::new(1),
        }))
    }
}

impl ReactiveGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// A call to this method propagates updated values through the graph. Call this when you will
    /// be able to observe the changes of any updated signals since the last tick.
    pub fn tick(&self) {
        self.0.generation.fetch_add(1, Ordering::AcqRel);
    }
}

#[derive(Debug)]
struct WriteSignalInner<T> {
    /// The most recent value.
    value: T,

    /// A generation that will be incremented every time the value is updated.
    ///
    /// Starts at 1 so that a fresh reader could set its generation to a lower value.
    val_generation: NonZero<u64>,
}

/// The writing end of a signal.
///
/// This is a source to the reactive graph. You can write values to this signal, but the change
/// will not be propagated through the reactive graph until the
/// [`tick`-method](ReactiveGraph::tick) has been called. Only the reading end of this signal is
/// able to observe the changes instantly.
#[derive(Debug)]
pub struct WriteSignal<T>(Arc<RwLock<WriteSignalInner<T>>>);

/// The reading end of a signal.
///
/// You typically use this as input to other [derived signals](DerivedSignal), but it also
/// implements the [`Source`] trait, so you could read the value immediately. Beaware that this
/// does not respect the [tick method](ReactiveGraph::tick) of the reactive graph. That is, if you
/// update the value of this signal with [`WriteSignal::set()`], it won't propagate through the
/// graph unless [`ReactiveGraph::tick`] is called, but it will be reflected on this [`ReadSignal`]
/// immediately.
#[derive(Debug)]
pub struct ReadSignal<T> {
    source: WriteSignal<T>,

    /// The generation of the value when it was last read.
    val_generation: u64,
}

/// Create a new (reader, writer) signal pair.
pub fn signal<T>(initial_value: T) -> (ReadSignal<T>, WriteSignal<T>) {
    let write_signal = WriteSignal::new(initial_value);
    (write_signal.subscribe(), write_signal)
}

impl<T> WriteSignal<T> {
    pub fn new(initial_value: T) -> Self {
        // We set the generation of the `WriteSignal` to 1 and of the `ReadSignal` to 0 to make sure
        // the `ReadSignal` returns the first value on the first call to `Source::get()`.
        WriteSignal(Arc::new(RwLock::new(WriteSignalInner {
            value: initial_value,
            val_generation: NonZero::new(1).unwrap(),
        })))
    }

    /// Get a reader to this signal.
    pub fn subscribe(&self) -> ReadSignal<T> {
        ReadSignal {
            source: self.clone(),
            val_generation: 0,
        }
    }

    /// Get a reference to the value without having to subscribe a reader.
    ///
    /// Note that this returns a read-lock of the value, so you shouldn't update the value
    /// simultaneously.
    pub fn value<'a>(&'a self) -> impl Deref<Target = T> + use<'a, T> {
        RwLockReadGuard::map(self.0.read(), |x| &x.value)
    }

    /// Update the value of the signal if the new value is not equal to the existing. Returns the
    /// old value if updated, otherwise returns `Err(value)`.
    pub fn set_if_changed(&self, value: T) -> Result<T, T>
    where
        T: PartialEq,
    {
        self.update_with(|x| {
            if value == *x {
                (false, Err(value))
            } else {
                let old_value = mem::replace(x, value);
                (true, Ok(old_value))
            }
        })
    }

    /// Get a mutable reference to the value and possibly update it. The change will be propagated
    /// if the old value is not equal to the new.
    ///
    /// Returns [`Ok`] with the old value if it did change or [`Err`] if it was unchanged.
    ///
    /// Note that this requires an extra clone of the inner value. You may also use
    /// [`Self::set_if_changed()`], [`Self::update_with()`] or [`Self::update()`] if this is
    /// undesirable.
    pub fn update_if_changed<U>(&self, f: impl FnOnce(&mut T) -> U) -> Result<(T, U), U>
    where
        T: PartialEq + Clone,
    {
        self.update_with(|x| {
            let old_value = x.clone();
            let ret = f(x);
            if *x == old_value {
                (false, Err(ret)) // Not updated.
            } else {
                (true, Ok((old_value, ret)))
            }
        })
    }

    /// Update the value of the signal, returning the old value.
    pub fn set(&self, value: T) -> T {
        self.update(|x| mem::replace(x, value))
    }

    /// Update the value by a mutable reference.
    pub fn update<U>(&self, f: impl FnOnce(&mut T) -> U) -> U {
        self.update_with(|x| (true, f(x)))
    }

    /// You get a mutable reference to the value and return a `(changed, ret)` tuple where
    /// `changed` is a bool indicating if the value did change and readers should be updated.
    ///
    /// Note that it is possible but not recommended to change the value and not notify the
    /// readers. This has the consequence that the call to [`Source::get()`] will not see the
    /// update and still return [`None`]. The update will be reflected in
    /// [`Source::get_existing()`] of the immediate [readers](ReadSignal), but it won't be
    /// propagated in the graph. The main motivation of using this method is if `T` doesn't
    /// implement [`PartialEq`], otherwise you should really use [`Self::set_if_changed()`] or
    /// [`Self::update_if_changed()`] instead.
    #[inline]
    pub fn update_with<U>(&self, f: impl FnOnce(&mut T) -> (bool, U)) -> U {
        let mut self_ = self.0.write();
        let (changed, res) = f(&mut self_.value);

        if changed {
            // Increment the generation.
            // Pssst: the syntax for updating `NonZero` types needs some refinement :)
            self_.val_generation = self_
                .val_generation
                .checked_add(1)
                .expect("WriteSignal generation overflow");
        }

        res
    }
}

impl<T> Clone for WriteSignal<T> {
    fn clone(&self) -> Self {
        WriteSignal(self.0.clone())
    }
}

impl<T: Default> Default for WriteSignal<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> Clone for ReadSignal<T> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            val_generation: self.val_generation.clone(),
        }
    }
}

impl<T: Clone> Source<T> for ReadSignal<T> {
    fn get(&mut self) -> Option<T> {
        let source = &self.source.0.read();

        let source_generation = source.val_generation.get();
        let my_generation = mem::replace(&mut self.val_generation, source_generation);
        debug_assert!(my_generation <= source_generation);

        if my_generation < source_generation {
            Some(source.value.clone())
        } else {
            None
        }
    }

    fn get_existing(&self) -> T {
        self.source.0.read().value.clone()
    }

    fn reset(&mut self) {
        self.val_generation = 0;
    }
}

struct Node<T> {
    /// The [`ReactiveGraph`] for this node.
    graph: ReactiveGraph,

    /// The [`ReactiveGraphInner::generation`] when this was last evaluated, or 0 if it never has been
    /// generated.
    graph_generation: u64,

    /// The most recently evaluated value.
    value: T,

    /// The function to compute a new value. Takes a reference to the `last_value` and returns
    /// `None` if the value is unchanged.
    func: Box<dyn FnMut(&T) -> Option<T> + Send + Sync>,

    /// The generation of this value.
    val_generation: NonZero<u64>,
}

/// A node in the reactive graph computing its value based on other nodes in the graph.
///
/// Remember that new values will be propagated through the graph only after a call to
/// [`ReactiveGraph::tick()`]. This means that subsequent calls to [`Source::get()`] will return
/// [`None`] till the next call to [`ReactiveGraph::tick()`].
#[derive(Clone)]
pub struct DerivedSignal<T: Clone> {
    node: Arc<RwLock<Node<T>>>,

    /// The value generation when this reader last called [`Source::get()`], or 0 if it has never
    /// been called or if reset.
    val_generation: u64,
}

impl<T: Clone> DerivedSignal<T> {
    /// Convenience method to avoid code duplication.
    fn new<S, U>(
        graph: ReactiveGraph,
        mut source: S,
        mut f: impl FnMut(Option<&T>, U) -> T + Send + Sync + 'static,
        mut has_changed: impl FnMut(&T, &T) -> bool + Send + Sync + 'static,
    ) -> Self
    where
        S: Source<U> + Send + Sync + 'static,
    {
        source.reset();

        let value = f(None, source.get_existing());
        let func = Box::new(move |prev_val: &T| {
            source
                .get()
                .map(|x| f(Some(prev_val), x))
                .filter(|x| has_changed(prev_val, x))
        });
        Self {
            node: Arc::new(RwLock::new(Node {
                value,
                func,
                val_generation: NonZero::new(1).unwrap(),
                graph,
                graph_generation: 0,
            })),
            val_generation: 0,
        }
    }
}

impl ReactiveGraph {
    /// Create a [`DerivedSignal`]: a node in the reactive graph with an evaluation based on other
    /// sources.
    ///
    /// The source could for instance be a [`ReadSignal`], a [`DerivedSignal`], or a tuple of
    /// sources. The function `f` then excepts the output of this source and computes the new
    /// value.
    ///
    /// Note that calls to [`Source::get()`] on this signal will return [`None`] before the next
    /// call to [`ReactiveGraph::tick()`]. But calls to [`Source::get_existing()`] will return the
    /// existing value of this signal based on the current value of the sources.
    pub fn derived_signal<S, U, T>(
        self: &Self,
        source: S,
        mut f: impl FnMut(U) -> T + Send + Sync + 'static,
    ) -> DerivedSignal<T>
    where
        S: Source<U> + Send + Sync + 'static,
        T: Clone,
    {
        self.derived_signal_with_old_val(source, move |_old_val, x| f(x))
    }

    /// Like [`derived_signal()`](Self::derived_signal) but the evaluation function gets a reference to the previous
    /// value as well (except for the first time).
    pub fn derived_signal_with_old_val<S, U, T>(
        self: &Self,
        source: S,
        f: impl FnMut(Option<&T>, U) -> T + Send + Sync + 'static,
    ) -> DerivedSignal<T>
    where
        S: Source<U> + Send + Sync + 'static,
        T: Clone,
    {
        DerivedSignal::new(self.clone(), source, f, |_, _| true)
    }

    /// Create a [`DerivedSignal`] that only propagates a new value if it has changed according
    /// to the [`PartialEq`] implementation.
    pub fn memo<S, U, T>(
        self: &Self,
        source: S,
        mut f: impl FnMut(U) -> T + Send + Sync + 'static,
    ) -> DerivedSignal<T>
    where
        S: Source<U> + Send + Sync + 'static,
        T: Clone + PartialEq,
    {
        self.memo_with_comparator(source, move |_old_val, x| f(x), |lhs, rhs| lhs != rhs)
    }

    /// Like [`memo()`](Self::memo) but the evaluation function takes a mutable reference to the previous value
    /// as well (except for the first evaluation).
    pub fn memo_with_old_val<S, U, T>(
        self: &Self,
        source: S,
        mut f: impl FnMut(Option<&T>, U) -> T + Send + Sync + 'static,
    ) -> DerivedSignal<T>
    where
        S: Source<U> + Send + Sync + 'static,
        T: Clone + PartialEq,
    {
        self.memo_with_comparator(
            source,
            move |old_val, x| f(old_val, x),
            |lhs, rhs| lhs != rhs,
        )
    }

    /// Create a [`DerivedSignal`] that only propagate a new value if it has changed according
    /// to a comparison function.
    pub fn memo_with_comparator<S, U, T>(
        self: &Self,
        source: S,
        f: impl FnMut(Option<&T>, U) -> T + Send + Sync + 'static,
        has_changed: impl FnMut(&T, &T) -> bool + Send + Sync + 'static,
    ) -> DerivedSignal<T>
    where
        S: Source<U> + Send + Sync + 'static,
        T: Clone,
    {
        DerivedSignal::new(self.clone(), source, f, has_changed)
    }
}

impl<T: Clone> Source<T> for DerivedSignal<T> {
    fn get(&mut self) -> Option<T> {
        let mut node = self.node.upgradable_read();

        // Check if a tick has occurred on the graph and we need to compute a new value.
        let graph_generation = node.graph.0.generation.load(Ordering::Acquire);
        debug_assert!(node.graph_generation <= graph_generation);
        if node.graph_generation < graph_generation {
            let maybe_new_value = node.with_upgraded(|node| {
                // Update the graph generation.
                node.graph_generation = graph_generation;

                // Check if the source has a newer value.
                if let Some(x) = (node.func)(&node.value) {
                    // A new value is computed!
                    node.value = x.clone();
                    // Bump the value generation.
                    // This is the bad `NonZero +=`-syntax again :)
                    node.val_generation = node
                        .val_generation
                        .checked_add(1)
                        .expect("DerivedSignal: val_generation overflow");

                    // Update the value generation of this reader as well.
                    self.val_generation = node.val_generation.get();

                    Some(x)
                } else {
                    None
                }
            });

            if let Some(x) = maybe_new_value {
                return Some(x);
            }
        }

        // Check if self.val_generation is lagging behind self.node.val_generation.
        let val_generation = node.val_generation.get();
        let my_val_generation = mem::replace(&mut self.val_generation, val_generation);
        debug_assert!(my_val_generation <= val_generation);
        if my_val_generation < val_generation {
            return Some(node.value.clone());
        }

        None
    }

    fn get_existing(&self) -> T {
        self.node.read().value.clone()
    }

    fn reset(&mut self) {
        self.val_generation = 0;
    }
}
